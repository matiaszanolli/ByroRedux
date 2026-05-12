//! Scene setup and NIF loading logic.

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    ActiveCamera, Billboard, BillboardMode, Camera, GlobalTransform, LocalBound, Material,
    MeshHandle, Name, Parent, ParticleEmitter, SceneFlags, SkinnedMesh, TextureHandle, Transform,
    World, WorldBound, MAX_BONES_PER_MESH,
};
use byroredux_core::math::{Mat4, Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_renderer::{cube_vertices, quad_vertices, triangle_vertices, Vertex, VulkanContext};
use byroredux_ui::UiManager;

use crate::anim_convert::convert_nif_clip;
use crate::asset_provider::{
    build_material_provider, build_texture_provider, merge_bgsm_into_mesh, parse_grid_coords,
    resolve_texture, MaterialProvider, TextureProvider,
};
use crate::cell_loader;
use crate::components::{
    AlphaBlend, CellLightingRes, CloudSimState, DarkMapHandle, ExtraTextureMaps, GameTimeRes,
    InputState, NormalMapHandle, SkyParamsRes, Spinning, TwoSided, WeatherDataRes,
    WeatherTransitionRes,
};
use crate::helpers::add_child;
use crate::streaming::{self, LoadedCell, WorldStreamingState};

/// Parse the `--radius` CLI argument into a clamped grid radius for
/// [`cell_loader::load_exterior_cells`]. Falls back to `3` (7×7 = 49
/// cells, ~28K terrain units view distance) on any parse failure so
/// an unparseable value loads the default rather than silently
/// bailing. Clamped to `1..=7` — below 1 the center cell alone isn't
/// useful, above 7 the cell count (15×15 = 225) already exceeds the
/// streaming budget today.
///
/// Pulled out as a free function so a unit test can pin the bounds
/// contract without standing up a whole App / World. See #531.
pub(crate) fn parse_exterior_radius(s: &str) -> i32 {
    match s.trim().parse::<i32>() {
        Ok(r) => r.clamp(1, 7),
        Err(_) => 3,
    }
}

/// Reference cloud sprite width — Bethesda's typical authoring
/// resolution. Per-layer baselines (`CLOUD_TILE_SCALE_*`) assume this
/// width; an authored cloud DDS at any other resolution is rescaled
/// inversely so a 1024² cloud tiles half as often as a 512² and a
/// 256² tiles twice as often, preserving on-screen blob density across
/// WTHR records that ship sharper or coarser cloud layers. See #529.
const CLOUD_REF_WIDTH: f32 = 512.0;

/// Cloud layer tile-scale baselines for a 512² authored sprite. Higher
/// indices = higher-altitude, finer-grained cloud decks. Pre-#529 these
/// were inline literals at every WTHR layer site.
const CLOUD_TILE_SCALE_LAYER_0: f32 = 0.15;
const CLOUD_TILE_SCALE_LAYER_1: f32 = 0.20;
const CLOUD_TILE_SCALE_LAYER_2: f32 = 0.25;
const CLOUD_TILE_SCALE_LAYER_3: f32 = 0.30;

/// Derive a per-WTHR cloud tile scale from the authored DDS width.
///
/// `cloud_tile_scale = baseline * CLOUD_REF_WIDTH / authored_width`
///
/// Falls back to `baseline` when the DDS header is unparseable or the
/// width comes back as zero — keeps the visual identical to pre-#529
/// behaviour for any cloud sprite at the 512² reference resolution.
///
/// Pure helper so the math has a unit test without standing up Vulkan.
fn cloud_tile_scale_for_dds(dds_bytes: &[u8], baseline: f32) -> f32 {
    match byroredux_renderer::vulkan::dds::parse_dds(dds_bytes) {
        Ok(meta) if meta.width > 0 => baseline * CLOUD_REF_WIDTH / meta.width as f32,
        _ => baseline,
    }
}

/// Diagnostic snapshot of an authored cloud DDS — width × height,
/// compressed/uncompressed, mip-chain depth — for the
/// `resolve_cloud_layer` log line. Returned as a pre-formatted string
/// so the log site can stay terse.
///
/// Added for #730 / EXT-RENDER-2: the user reported visible texel
/// boundaries on FNV WastelandNV clouds despite the bindless sampler
/// being LINEAR/LINEAR/REPEAT/anisotropic, and asked for the cloud
/// sprite's actual dimensions in the bootstrap log so the next
/// streaming session reveals whether the artefact is "tiny DDS
/// magnified hard" vs. "missing mip chain in `from_rgba`" (which
/// hard-codes `mip_levels(1)`).
fn cloud_dds_diag(dds_bytes: &[u8]) -> String {
    match byroredux_renderer::vulkan::dds::parse_dds(dds_bytes) {
        Ok(meta) => format!(
            "{}×{} {} mips={}",
            meta.width,
            meta.height,
            if meta.compressed { "BC" } else { "RGBA" },
            meta.mip_count,
        ),
        Err(_) => "unparseable DDS".to_string(),
    }
}

/// Resolve a single WTHR cloud layer end-to-end:
///   path → archive extract → DDS upload → (handle, tile_scale).
///
/// Returns `(0, 0.0)` (texture handle 0 = fallback, scale 0.0 = shader
/// branch-skips the layer) when the path is absent, the texture isn't
/// in any loaded archive, or the DDS upload fails. The tile scale is
/// derived per-WTHR from the authored DDS width via
/// [`cloud_tile_scale_for_dds`] so cloud density tracks the sprite's
/// authored resolution rather than a fixed per-layer constant. See #529.
///
/// Collapses 4 near-identical match blocks (one per layer) that were
/// drifting in log message wording and error handling.
fn resolve_cloud_layer(
    path: Option<&str>,
    baseline_scale: f32,
    layer_label: &str,
    tex_provider: &TextureProvider,
    ctx: &mut VulkanContext,
) -> (u32, f32) {
    let Some(path) = path else {
        return (0, 0.0);
    };
    // Peek the DDS to derive cloud_tile_scale from the authored width
    // (#529). The handle itself is resolved through `resolve_texture`
    // below — sharing the same `strip_build_prefix` + `acquire_by_path`
    // canonicalization every other texture consumer uses (#528 /
    // FNV-CELL-2). Pre-fix the cloud path called `texture_registry.load_dds`
    // directly with the raw archive path, so a future TOD-crossfade
    // system resolving the same cloud sprite through `resolve_texture`
    // would key on the stripped path and miss the cache — re-uploading
    // every cloud layer on the crossfade tick.
    let Some(dds_bytes) = tex_provider.extract(path) else {
        log::debug!(
            "Cloud layer {} texture '{}' not in archives",
            layer_label,
            path
        );
        return (0, 0.0);
    };
    let scale = cloud_tile_scale_for_dds(&dds_bytes, baseline_scale);
    let diag = cloud_dds_diag(&dds_bytes);
    // Drop the peeked bytes — `resolve_texture` will re-extract on the
    // cache-miss path (cloud loads run once per cell transition, so the
    // duplicate extract is irrelevant). On the cache-hit path (e.g. a
    // future TOD crossfade re-entering the same WTHR) the registry
    // bumps the existing slot's refcount via `acquire_by_path` without
    // re-extracting.
    drop(dds_bytes);
    let h = crate::asset_provider::resolve_texture(ctx, tex_provider, Some(path));
    if h == ctx.texture_registry.fallback() {
        log::warn!(
            "Cloud layer {} '{}' resolved to fallback — disabling layer",
            layer_label,
            path,
        );
        return (0, 0.0);
    }
    log::info!(
        "Cloud layer {} '{}' → handle {} (tile_scale {:.3}, {})",
        layer_label,
        path,
        h,
        scale,
        diag,
    );
    (h, scale)
}

/// Per-climate sunrise/sunset breakpoints in hours. CLMT TNAM bytes
/// are in 10-min units (`hour = byte / 6`); the valid authored range is
/// `1..=144` (`1` = 0:10, `144` = 24:00). Returns the pre-#463 hardcoded
/// `[6.0, 10.0, 18.0, 22.0]` fallback when:
///   * the worldspace has no climate (stub or unresolved record),
///   * the CLMT TNAM is all-zero (a stub field, not authored data),
///   * any of the four bytes lies outside `1..=144` — corruption guard
///     for modded ESMs that ship out-of-range bytes (e.g.
///     `[0, 0, 0, 0xFF]` would otherwise pass the pre-#530 OR-of-bytes
///     filter and produce a sunset_end of 42.5h, breaking the TOD
///     interpolator). See #530 / FNV-CELL-8.
pub(crate) fn climate_tod_hours(
    climate: Option<&byroredux_plugin::esm::records::ClimateRecord>,
) -> [f32; 4] {
    const FALLBACK: [f32; 4] = [6.0, 10.0, 18.0, 22.0];
    let Some(c) = climate else {
        return FALLBACK;
    };
    let valid = |b: u8| (1..=144).contains(&b);
    if valid(c.sunrise_begin)
        && valid(c.sunrise_end)
        && valid(c.sunset_begin)
        && valid(c.sunset_end)
    {
        [
            c.sunrise_begin as f32 / 6.0,
            c.sunrise_end as f32 / 6.0,
            c.sunset_begin as f32 / 6.0,
            c.sunset_end as f32 / 6.0,
        ]
    } else {
        FALLBACK
    }
}

/// Insert exterior worldspace lighting + sky resources into the world,
/// driven by the (already-resolved) climate + default-weather sitting
/// on the streaming context. Worldspace-wide concern, run once at
/// streaming bootstrap rather than per cell load.
///
/// Falls back to a procedural Mojave-style warm desert sky when the
/// worldspace has no climate / no default weather (common for stub
/// worldspaces and bare-DLC parses). Pre-#M40 this was inlined in the
/// `--grid` CLI arm next to the bulk loader; factoring it out lets
/// the streaming system bootstrap reuse it.
fn apply_worldspace_weather(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    wctx: &cell_loader::ExteriorWorldContext,
) {
    let sun_dir: [f32; 3] = [-0.4, 0.8, -0.45];
    if let Some(ref wthr) = wctx.default_weather {
        use byroredux_plugin::esm::records::weather::*;
        // Day-slot snapshot for the initial CellLightingRes / SkyParamsRes —
        // the per-frame `weather_system` interpolator advances through the
        // stored NAM0 table over the in-game day. Raw monitor-space colors
        // (commit 0e8efc6) — sRGB decode would darken every warm hue.
        let ambient = wthr.sky_colors[SKY_AMBIENT][TOD_DAY].to_rgb_f32();
        let sunlight = wthr.sky_colors[SKY_SUNLIGHT][TOD_DAY].to_rgb_f32();
        let fog_col = wthr.sky_colors[SKY_FOG][TOD_DAY].to_rgb_f32();
        let zenith = wthr.sky_colors[SKY_UPPER][TOD_DAY].to_rgb_f32();
        let horizon = wthr.sky_colors[SKY_HORIZON][TOD_DAY].to_rgb_f32();
        let sun_col = wthr.sky_colors[SKY_SUN][TOD_DAY].to_rgb_f32();
        // #541 — `SKY_LOWER` (real `Sky-Lower` per nif.xml NAM0
        // schema, slot 7 post-#729) drives `composite.frag`'s
        // below-horizon branch. Pre-fix the shader faked it as
        // `horizon * 0.3`, dropping the authored colour entirely.
        let lower = wthr.sky_colors[SKY_LOWER][TOD_DAY].to_rgb_f32();
        log::info!(
            "WTHR '{}': zenith={:?} horizon={:?} sun={:?} ambient={:?} sunlight={:?} fog_color={:?} fog_day={:.0}\u{2013}{:.0}",
            wthr.editor_id,
            zenith,
            horizon,
            sun_col,
            ambient,
            sunlight,
            fog_col,
            wthr.fog_day_near,
            wthr.fog_day_far,
        );
        world.insert_resource(CellLightingRes {
            ambient,
            directional_color: sunlight,
            directional_dir: sun_dir,
            is_interior: false,
            fog_color: fog_col,
            fog_near: wthr.fog_day_near,
            fog_far: wthr.fog_day_far,
            // WTHR-driven exterior lighting; the XCLL extended block
            // applies only to interior cells (and exterior cells with
            // overridden lighting templates, not yet wired). See #861.
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
        });
        // Resolve all 4 WTHR cloud layers via the shared per-WTHR
        // helper (#529 — derives tile_scale from authored DDS width).
        let (cloud_tex_index, cloud_tile_scale) = resolve_cloud_layer(
            wthr.cloud_textures[0].as_deref(),
            CLOUD_TILE_SCALE_LAYER_0,
            "0",
            tex_provider,
            ctx,
        );
        let (cloud_tex_index_1, cloud_tile_scale_1) = resolve_cloud_layer(
            wthr.cloud_textures[1].as_deref(),
            CLOUD_TILE_SCALE_LAYER_1,
            "1",
            tex_provider,
            ctx,
        );
        let (cloud_tex_index_2, cloud_tile_scale_2) = resolve_cloud_layer(
            wthr.cloud_textures[2].as_deref(),
            CLOUD_TILE_SCALE_LAYER_2,
            "2",
            tex_provider,
            ctx,
        );
        let (cloud_tex_index_3, cloud_tile_scale_3) = resolve_cloud_layer(
            wthr.cloud_textures[3].as_deref(),
            CLOUD_TILE_SCALE_LAYER_3,
            "3",
            tex_provider,
            ctx,
        );
        // CLMT FNAM sun-sprite resolution (#478). 0 = procedural disc fallback.
        let sun_tex_index: u32 = wctx
            .climate
            .as_ref()
            .and_then(|c| c.sun_texture.as_deref())
            .filter(|s| !s.is_empty())
            .and_then(|path| {
                let dds = tex_provider.extract(path)?;
                let alloc = ctx.allocator.as_ref().unwrap();
                match ctx.texture_registry.load_dds(
                    &ctx.device,
                    alloc,
                    &ctx.graphics_queue,
                    ctx.transfer_pool,
                    path,
                    &dds,
                ) {
                    Ok(h) => {
                        log::info!("Sun texture '{}' → handle {}", path, h);
                        Some(h)
                    }
                    Err(e) => {
                        log::warn!(
                            "Sun DDS load failed '{}': {} — using procedural disc",
                            path,
                            e
                        );
                        None
                    }
                }
            })
            .unwrap_or(0);
        world.insert_resource(SkyParamsRes {
            zenith_color: zenith,
            horizon_color: horizon,
            lower_color: lower,
            sun_direction: sun_dir,
            sun_color: sun_col,
            sun_size: 0.9995,
            sun_intensity: 4.0,
            is_exterior: true,
            cloud_tile_scale,
            cloud_texture_index: cloud_tex_index,
            sun_texture_index: sun_tex_index,
            cloud_tile_scale_1,
            cloud_texture_index_1: cloud_tex_index_1,
            cloud_tile_scale_2,
            cloud_texture_index_2: cloud_tex_index_2,
            cloud_tile_scale_3,
            cloud_texture_index_3: cloud_tex_index_3,
        });
        // #803 — cloud scroll lives on `CloudSimState`, which survives
        // cell transitions. Insert a default-zero state on first
        // exterior load only; subsequent loads reuse the existing
        // accumulator so clouds resume drift across interior visits.
        if world.try_resource::<CloudSimState>().is_none() {
            world.insert_resource(CloudSimState::default());
        }
        // Full NAM0 color table for per-frame TOD interpolation.
        let mut sky_colors = [[[0.0f32; 3]; 6]; 10];
        for (dst_group, src_group) in sky_colors.iter_mut().zip(wthr.sky_colors.iter()) {
            for (dst, src) in dst_group.iter_mut().zip(src_group.iter()) {
                *dst = src.to_rgb_f32();
            }
        }
        // #463 — per-climate sunrise/sunset breakpoints.
        let tod_hours = climate_tod_hours(wctx.climate.as_ref());
        let new_weather = WeatherDataRes {
            sky_colors,
            fog: [
                wthr.fog_day_near,
                wthr.fog_day_far,
                wthr.fog_night_near,
                wthr.fog_night_far,
            ],
            tod_hours,
        };
        // First-time bootstrap: insert directly. A subsequent worldspace
        // change (door-walking interior↔exterior, M40 Phase 2) will
        // trigger the 8-second crossfade via WeatherTransitionRes.
        if world.try_resource::<WeatherDataRes>().is_some() {
            world.insert_resource(WeatherTransitionRes {
                target: new_weather,
                elapsed_secs: 0.0,
                duration_secs: 8.0,
            });
        } else {
            world.insert_resource(new_weather);
            world.insert_resource(GameTimeRes::default());
        }
    } else {
        // Procedural fallback — warm Mojave desert sky. Same defaults
        // the bulk loader used pre-#M40 when a worldspace had no
        // climate / weather. Factored out (#542 / M33-10) so the
        // procedural-fallback branch also installs `GameTimeRes` +
        // a synthetic `WeatherDataRes` — without those,
        // `weather_system` early-returns and the fallback sun stays
        // pinned at its initial direction forever, freezing exterior
        // lighting on any cell whose worldspace failed to resolve a
        // climate / weather (corrupt ESM, broken plugin, bespoke
        // synthetic test cell).
        insert_procedural_fallback_resources(world, sun_dir);
    }
}

/// Procedural Mojave-style sky + lighting + game-time resources for a
/// worldspace that has no resolved climate / weather record. Mirrors
/// the per-WTHR insert above but with hardcoded warm-desert defaults.
///
/// Crucially also installs `GameTimeRes` (default hour=10 / time_scale
/// 30×) and a synthetic `WeatherDataRes` whose 6 TOD slots all carry
/// the same fallback colours, so `weather_system` runs the sun arc
/// each frame instead of early-returning. The TOD-slot lerp of two
/// identical endpoints reproduces the procedural colours unchanged
/// while letting `sun_direction` and `sun_intensity` animate
/// across the simulated day. Synthetic NAM0 groups outside the six
/// `weather_system` reads (`SKY_UPPER`, `SKY_FOG`, `SKY_AMBIENT`,
/// `SKY_SUNLIGHT`, `SKY_SUN`, `SKY_HORIZON`) stay at zero — those
/// slots aren't sampled in the fallback path.
fn insert_procedural_fallback_resources(world: &mut World, sun_dir: [f32; 3]) {
    use byroredux_plugin::esm::records::weather as wthr;
    const AMBIENT: [f32; 3] = [0.15, 0.14, 0.12];
    const SUNLIGHT: [f32; 3] = [1.0, 0.95, 0.8];
    const FOG_COLOR: [f32; 3] = [0.65, 0.7, 0.8];
    const ZENITH: [f32; 3] = [0.15, 0.3, 0.65];
    const HORIZON: [f32; 3] = [0.55, 0.5, 0.42];
    // Pre-#541 the `compute_sky` below-horizon branch faked the
    // ground tint as `horizon * 0.3`; matching that scaling here
    // keeps the procedural look unchanged when no WTHR is present.
    const LOWER: [f32; 3] = [HORIZON[0] * 0.3, HORIZON[1] * 0.3, HORIZON[2] * 0.3];
    const SUN_COLOR: [f32; 3] = [1.0, 0.95, 0.8];
    const FOG_NEAR: f32 = 15000.0;
    const FOG_FAR: f32 = 80000.0;

    world.insert_resource(CellLightingRes {
        ambient: AMBIENT,
        directional_color: SUNLIGHT,
        directional_dir: sun_dir,
        is_interior: false,
        fog_color: FOG_COLOR,
        fog_near: FOG_NEAR,
        fog_far: FOG_FAR,
        // Engine-default fallback (no plugin data) — extended XCLL
        // fields stay None. See #861.
        directional_fade: None,
        fog_clip: None,
        fog_power: None,
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
    });
    world.insert_resource(SkyParamsRes {
        zenith_color: ZENITH,
        horizon_color: HORIZON,
        lower_color: LOWER,
        sun_direction: sun_dir,
        sun_color: SUN_COLOR,
        sun_size: 0.9995,
        sun_intensity: 4.0,
        is_exterior: true,
        cloud_tile_scale: 0.0,
        cloud_texture_index: 0,
        sun_texture_index: 0,
        cloud_tile_scale_1: 0.0,
        cloud_texture_index_1: 0,
        cloud_tile_scale_2: 0.0,
        cloud_texture_index_2: 0,
        cloud_tile_scale_3: 0.0,
        cloud_texture_index_3: 0,
    });
    // #803 — same survives-transitions pattern as the WTHR-driven
    // path: the procedural fallback also seeds CloudSimState only on
    // the first exterior load.
    if world.try_resource::<CloudSimState>().is_none() {
        world.insert_resource(CloudSimState::default());
    }

    // Synthetic NAM0 table — every TOD slot gets the same procedural
    // colour for the six groups `weather_system` reads. The lerp of
    // two equal endpoints is a no-op for colour, so the TOD pass
    // re-writes the same procedural values each frame while still
    // refreshing `sun_direction` / `sun_intensity` from the advancing
    // game hour. See #542 / M33-10.
    let mut sky_colors = [[[0.0f32; 3]; wthr::SKY_TIME_SLOTS]; wthr::SKY_COLOR_GROUPS];
    let synthetic = [
        (wthr::SKY_UPPER, ZENITH),
        (wthr::SKY_FOG, FOG_COLOR),
        (wthr::SKY_AMBIENT, AMBIENT),
        (wthr::SKY_SUNLIGHT, SUNLIGHT),
        (wthr::SKY_SUN, SUN_COLOR),
        // #541 — `weather_system` now also reads SKY_LOWER for the
        // below-horizon branch. Synthetic value matches the
        // procedural `LOWER` constant so the lerp re-writes the same
        // ground tint each frame.
        (wthr::SKY_LOWER, LOWER),
        (wthr::SKY_HORIZON, HORIZON),
    ];
    for (group, color) in synthetic {
        sky_colors[group].fill(color);
    }
    world.insert_resource(WeatherDataRes {
        sky_colors,
        // Day/night fog distances kept identical — no authored night
        // distance to interpolate toward.
        fog: [FOG_NEAR, FOG_FAR, FOG_NEAR, FOG_FAR],
        // Pre-#463 hardcoded TOD breakpoints — sunrise 6h, day 10h,
        // sunset 18h, night 22h.
        tod_hours: [6.0, 10.0, 18.0, 22.0],
    });
    world.insert_resource(GameTimeRes::default());
}

/// Stream the initial radius around the player's spawn cell. Returns
/// the camera-spawn point (center cell terrain mid-height + 200 units,
/// or `Vec3::ZERO` when there's no center cell).
///
/// Dispatches the initial radius via the streaming worker and blocks
/// (with `payload_rx.recv()`, not `try_recv`) until every pending
/// load resolves — bench harness expects the world fully populated
/// before measurement starts. Each payload is consumed via the same
/// `finish_partial_import` + `load_one_exterior_cell` pipeline the
/// per-frame `step_streaming` uses.
///
/// Phase 1b: worker still does the heavy CPU work off-thread while
/// the bootstrap thread blocks on the `recv()`. The win vs. Phase 1a
/// sync is bounded — the bootstrap is single-threaded by design — but
/// it keeps the post-bootstrap streaming loop using exactly one code
/// path for cell load (no separate sync vs async branches).
fn stream_initial_radius(
    world: &mut World,
    ctx: &mut VulkanContext,
    state: &mut WorldStreamingState,
    cx: i32,
    cy: i32,
) -> Vec3 {
    let deltas = streaming::compute_streaming_deltas(
        &state.loaded,
        (cx, cy),
        state.radius_load,
        state.radius_unload,
    );

    // Dispatch every cell in the initial radius. Generation counter
    // ticks per request so a future re-load on the same cell (e.g.
    // after a scripted teleport in M40 Phase 2) can distinguish stale
    // payloads from the new one.
    //
    // Snapshot the NifImportRegistry's cached keys once for the
    // batch so the worker can skip already-cached models (#862). On
    // initial-radius dispatch the cache is normally empty, so this
    // typically returns an empty set and the worker parses
    // everything — but the same plumbing handles a warm cache after
    // a future M40 Phase 2 hot-reload.
    let cached_keys = world
        .resource::<crate::cell_loader::NifImportRegistry>()
        .snapshot_keys();
    for (gx, gy) in &deltas.to_load {
        let coord = (*gx, *gy);
        let generation = state.next_generation;
        state.next_generation = state.next_generation.wrapping_add(1);
        state.pending.insert(coord, generation);
        let req = streaming::LoadCellRequest {
            gx: *gx,
            gy: *gy,
            generation,
            wctx: state.wctx.clone(),
            tex_provider: state.tex_provider.clone(),
            cached_keys: cached_keys.clone(),
        };
        if state.request_tx.send(req).is_err() {
            log::error!(
                "Streaming worker channel closed during initial-radius dispatch \
                 — cell ({},{}) cannot be loaded",
                gx,
                gy
            );
            state.pending.remove(&coord);
        }
    }

    // Block on the receiver until every pending load resolves. Loads
    // arrive in worker-completion order, not dispatch order, so we
    // consume any payload that arrives and only stop when `pending`
    // is empty.
    let mut center = Vec3::ZERO;
    let wctx = state.wctx.clone();
    while !state.pending.is_empty() {
        let payload = match state.payload_rx.recv() {
            Ok(p) => p,
            Err(_) => {
                log::error!(
                    "Streaming worker disconnected mid-bootstrap with {} pending cells",
                    state.pending.len()
                );
                break;
            }
        };
        let coord = (payload.gx, payload.gy);
        // Stale-load gate via the testable `classify_payload` helper.
        match streaming::classify_payload(&state.pending, coord, payload.generation) {
            streaming::PayloadDecision::Apply => {
                state.pending.remove(&coord);
            }
            streaming::PayloadDecision::StaleNewerPending { .. }
            | streaming::PayloadDecision::StaleNoPending => continue,
        }
        for (model_path, partial_opt) in payload.parsed {
            match partial_opt {
                Some(partial) => cell_loader::finish_partial_import(
                    world,
                    Some(&mut state.mat_provider),
                    Some(state.tex_provider.as_ref()),
                    &model_path,
                    partial,
                ),
                None => {
                    let cache_key = model_path.to_ascii_lowercase();
                    let freed = {
                        let mut reg = world.resource_mut::<cell_loader::NifImportRegistry>();
                        reg.insert(cache_key, None)
                    };
                    // #863 — release LRU-evicted clip handles.
                    if !freed.is_empty() {
                        let mut clip_reg = world
                            .resource_mut::<byroredux_core::animation::AnimationClipRegistry>();
                        for h in freed {
                            clip_reg.release(h);
                        }
                    }
                }
            }
        }
        match cell_loader::load_one_exterior_cell(
            wctx.as_ref(),
            payload.gx,
            payload.gy,
            world,
            ctx,
            state.tex_provider.as_ref(),
            Some(&mut state.mat_provider),
            None,
        ) {
            Ok(Some(info)) => {
                if coord == (cx, cy) {
                    center = info.center;
                }
                state.loaded.insert(
                    coord,
                    LoadedCell {
                        cell_root: info.cell_root,
                    },
                );
            }
            Ok(None) => {
                // Worldspace hole — common at edges.
            }
            Err(e) => {
                log::warn!(
                    "Initial cell ({},{}) spawn failed: {:#}",
                    coord.0,
                    coord.1,
                    e
                );
            }
        }
    }
    center
}

/// Called once after the renderer is ready — uploads meshes and spawns entities.
pub(crate) fn setup_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    ui_manager: &mut Option<UiManager>,
    ui_texture_handle: &mut Option<u32>,
    camera_pos_override: Option<(f32, f32, f32)>,
    camera_forward_override: Option<(f32, f32, f32)>,
    streaming_slot: &mut Option<WorldStreamingState>,
) {
    // Load content from CLI: cell, loose NIF, or BSA NIF.
    let args: Vec<String> = std::env::args().collect();
    let mut cam_center = Vec3::ZERO;
    let mut has_nif_content = false;
    let mut nif_root: Option<EntityId> = None;

    // Cell loading mode: --esm <path> --cell <editor_id> OR --wrld <name> --grid <x>,<y>
    if let Some(esm_idx) = args.iter().position(|a| a == "--esm") {
        let esm_path = args.get(esm_idx + 1).cloned();
        let cell_id = args
            .iter()
            .position(|a| a == "--cell")
            .and_then(|i| args.get(i + 1))
            .cloned();
        let grid_str = args
            .iter()
            .position(|a| a == "--grid")
            .and_then(|i| args.get(i + 1))
            .cloned();
        // #444 — explicit worldspace EDID override. Used with --grid
        // when the ESM defines multiple exterior worldspaces (e.g.
        // FO3 + DLC masters ship Wasteland, PointLookout, Zeta, Pitt,
        // Anchorage) and the automatic pick lands on the wrong one.
        // Case-insensitive EDID match inside load_exterior_cells.
        let wrld_name = args
            .iter()
            .position(|a| a == "--wrld")
            .and_then(|i| args.get(i + 1))
            .cloned();
        // #531 — optional `--radius N` override for the exterior grid.
        // Defaults to 3 (7×7 grid, ~28K terrain units view distance)
        // to preserve pre-fix behaviour. Clamped to 1..=7 by
        // [`parse_exterior_radius`] so an accidental 100 doesn't try
        // to load 40 401 cells.
        let radius = args
            .iter()
            .position(|a| a == "--radius")
            .and_then(|i| args.get(i + 1))
            .map(|s| parse_exterior_radius(s))
            .unwrap_or(3);

        // #561 — repeatable `--master <path>` arg. Order matters:
        // base masters first, then any required intermediate masters
        // (Update.esm before Dawnguard.esm), and finally the main
        // ESM via `--esm`. Each `--master` is collected in CLI order;
        // the cell loader's `_with_masters` entry points compose the
        // global load order as `[masters…, esm]` and parse each plugin
        // with the appropriate FormID remap so a DLC interior REFR
        // placing a base-game STAT resolves cleanly. Without this,
        // Dawnguard / HearthFires / Dragonborn interiors render
        // empty silently. See M46.0.
        let masters: Vec<String> = args
            .iter()
            .enumerate()
            .filter_map(|(i, a)| {
                if a == "--master" {
                    args.get(i + 1).cloned()
                } else {
                    None
                }
            })
            .collect();
        if !masters.is_empty() {
            log::info!("Load order: masters={:?}, main='{:?}'", masters, esm_path);
        }

        if let (Some(ref esm_path), Some(ref cell_id)) = (&esm_path, &cell_id) {
            // Interior cell mode
            let tex_provider = build_texture_provider(&args);
            let mut mat_provider = build_material_provider(&args);
            match cell_loader::load_cell_with_masters(
                &masters,
                esm_path,
                cell_id,
                world,
                ctx,
                &tex_provider,
                Some(&mut mat_provider),
            ) {
                Ok(result) => {
                    cam_center = result.center;
                    has_nif_content = true;
                    // Store cell lighting for the renderer.
                    if let Some(ref lit) = result.lighting {
                        let (rx, ry) = (lit.directional_rotation[0], lit.directional_rotation[1]);
                        // Route the authored XCLL Euler angles through
                        // `euler_zup_to_quat_yup` — the same
                        // CW-convention helper REFR placements use —
                        // then apply the resulting Y-up quaternion to
                        // Gamebryo's NiDirectionalLight model
                        // direction `(1, 0, 0)` (per the 2.3
                        // `NiDirectionalLight.h` comment: "The model
                        // direction of the light is (1,0,0)"). The
                        // Z-up → Y-up coord swap leaves +X invariant,
                        // so the Y-up model vector is also `(1, 0, 0)`.
                        // Pre-#380 an inline spherical formula treated
                        // ry as elevation-from-horizon and drifted
                        // from the authored intent as ry grew. See
                        // audit F3-09.
                        let quat = cell_loader::euler_zup_to_quat_yup(rx, ry, 0.0);
                        let dir_v = quat * Vec3::new(1.0, 0.0, 0.0);
                        let dir = [dir_v.x, dir_v.y, dir_v.z];
                        // load_cell() only handles interior cells —
                        // `is_interior: true` skips the directional as
                        // a scene light to prevent wall light leakage.
                        // The 9 extended XCLL fields (`fog_clip`,
                        // `directional_ambient`, etc.) are propagated
                        // by `from_cell_lighting` even though the
                        // renderer doesn't yet consume them — #861
                        // establishes the data plumbing; #865 + a
                        // future Skyrim ambient-cube uniform are the
                        // shader-side follow-ups.
                        world.insert_resource(CellLightingRes::from_cell_lighting(lit, dir, true));
                        log::info!(
                            "Cell lighting: ambient={:?} directional={:?} dir={:?} fog={:?} near={:.0} far={:.0}",
                            lit.ambient,
                            lit.directional_color,
                            dir,
                            lit.fog_color,
                            lit.fog_near,
                            lit.fog_far,
                        );
                    }
                    log::info!(
                        "Cell '{}' ready: {} entities",
                        result.cell_name,
                        result.entity_count
                    );
                }
                Err(e) => log::error!("Failed to load cell: {:#}", e),
            }
        } else if let (Some(ref esm_path), Some(ref grid)) = (&esm_path, &grid_str) {
            // Exterior cell mode: --esm <path> --grid <x>,<y> — driven
            // through `WorldStreamingState` (M40 Phase 1a). The bulk
            // loader has been retired from this path; cells stream in
            // around the player via `step_streaming` from frame 1.
            // Initial-radius cells are loaded synchronously here so
            // the first rendered frame has a populated world.
            let (cx, cy) = parse_grid_coords(grid);
            let tex_provider = build_texture_provider(&args);
            let mat_provider = build_material_provider(&args);
            match cell_loader::build_exterior_world_context(
                &masters,
                esm_path,
                cx,
                cy,
                radius,
                wrld_name.as_deref(),
            ) {
                Ok(wctx) => {
                    has_nif_content = true;
                    apply_worldspace_weather(world, ctx, &tex_provider, &wctx);
                    let mut state =
                        WorldStreamingState::new(wctx, tex_provider, mat_provider, radius);
                    state.last_player_grid = Some((cx, cy));
                    cam_center = stream_initial_radius(world, ctx, &mut state, cx, cy);
                    log::info!(
                        "Streaming context ready: worldspace '{}', radius {} (load), {} (unload), {} cells loaded initially",
                        state.wctx.worldspace_key,
                        state.radius_load,
                        state.radius_unload,
                        state.loaded.len(),
                    );
                    *streaming_slot = Some(state);
                }
                Err(e) => log::error!("Failed to build exterior world context: {:#}", e),
            }
        } else {
            log::error!("--esm requires either --cell <editor_id> or --grid <x>,<y>");
        }
    } else {
        // NIF loading mode: loose file or BSA extraction.
        let (nif_count, loaded_root) = load_nif_from_args(world, ctx);
        has_nif_content = nif_count > 0;
        nif_root = loaded_root;
    }

    // Animation: --kf <path> loads a .kf file and starts playback.
    // Tries BSA extraction first (KF files live in mesh BSAs), falls back to loose file.
    if let Some(kf_idx) = args.iter().position(|a| a == "--kf") {
        if let Some(kf_path) = args.get(kf_idx + 1).cloned() {
            let kf_provider = build_texture_provider(&args);
            let kf_data = kf_provider
                .extract_mesh(&kf_path)
                .inspect(|_| {
                    log::info!("Extracted KF from BSA: '{}'", kf_path);
                })
                .or_else(|| {
                    std::fs::read(&kf_path)
                        .map_err(|e| log::error!("Failed to read KF '{}': {}", kf_path, e))
                        .ok()
                });
            if let Some(kf_data) = kf_data {
                match byroredux_nif::parse_nif(&kf_data) {
                    Ok(kf_scene) => {
                        let nif_clips = byroredux_nif::anim::import_kf(&kf_scene);
                        if nif_clips.is_empty() {
                            log::warn!("No animation clips found in '{}'", kf_path);
                        } else {
                            let first_handle;
                            {
                                let mut registry = world.resource_mut::<AnimationClipRegistry>();
                                let mut pool = world.resource_mut::<StringPool>();
                                for nif_clip in &nif_clips {
                                    let clip = convert_nif_clip(nif_clip, &mut pool);
                                    let handle = registry.add(clip);
                                    log::info!(
                                        "Loaded animation clip '{}' ({:.2}s, {} channels) → handle {}",
                                        nif_clip.name,
                                        nif_clip.duration,
                                        nif_clip.channels.len(),
                                        handle,
                                    );
                                }
                                first_handle = registry.len() as u32 - nif_clips.len() as u32;
                            }

                            // Spawn an AnimationPlayer scoped to the NIF subtree.
                            let player_entity = world.spawn();
                            let mut player = AnimationPlayer::new(first_handle);
                            if let Some(root) = nif_root {
                                player.root_entity = Some(root);
                            }
                            world.insert(player_entity, player);
                            log::info!("Animation playback started (clip handle {})", first_handle);
                        }
                    }
                    Err(e) => log::error!("Failed to parse KF '{}': {}", kf_path, e),
                }
            }
        }
    }

    // Only spawn demo primitives when no NIF content was loaded.
    if !has_nif_content {
        let alloc = ctx.allocator.as_ref().unwrap();
        let (verts, idxs) = cube_vertices();
        let queue = &ctx.graphics_queue;
        let pool = ctx.transfer_pool;
        let rt = ctx.device_caps.ray_query_supported;
        let cube_handle = ctx
            .mesh_registry
            .upload(&ctx.device, alloc, queue, pool, &verts, &idxs, rt, None)
            .expect("Failed to upload cube mesh");

        let (quad_verts, quad_idxs) = quad_vertices();
        let quad_handle = ctx
            .mesh_registry
            .upload(
                &ctx.device,
                alloc,
                queue,
                pool,
                &quad_verts,
                &quad_idxs,
                rt,
                None,
            )
            .expect("Failed to upload quad mesh");

        let (red_verts, red_idxs) = triangle_vertices([1.0, 0.2, 0.2]);
        let red_handle = ctx
            .mesh_registry
            .upload(
                &ctx.device,
                alloc,
                queue,
                pool,
                &red_verts,
                &red_idxs,
                rt,
                None,
            )
            .expect("Failed to upload red triangle mesh");

        let (blue_verts, blue_idxs) = triangle_vertices([0.2, 0.2, 1.0]);
        let blue_handle = ctx
            .mesh_registry
            .upload(
                &ctx.device,
                alloc,
                queue,
                pool,
                &blue_verts,
                &blue_idxs,
                rt,
                None,
            )
            .expect("Failed to upload blue triangle mesh");

        // Batched BLAS build for RT shadows on demo meshes.
        let (cv, ci) = (verts.len() as u32, idxs.len() as u32);
        let (qv, qi) = (quad_verts.len() as u32, quad_idxs.len() as u32);
        let (rv, ri) = (red_verts.len() as u32, red_idxs.len() as u32);
        let (bv, bi) = (blue_verts.len() as u32, blue_idxs.len() as u32);
        ctx.build_blas_batched(&[
            (cube_handle, cv, ci),
            (quad_handle, qv, qi),
            (red_handle, rv, ri),
            (blue_handle, bv, bi),
        ]);

        let cube = world.spawn();
        world.insert(cube, Transform::from_translation(Vec3::new(-1.5, 0.0, 0.0)));
        world.insert(cube, GlobalTransform::IDENTITY);
        world.insert(cube, MeshHandle(cube_handle));
        world.insert(cube, Spinning);

        let quad = world.spawn();
        world.insert(quad, Transform::from_translation(Vec3::new(0.0, 0.0, -1.0)));
        world.insert(quad, GlobalTransform::IDENTITY);
        world.insert(quad, MeshHandle(quad_handle));
        world.insert(quad, Spinning);

        let red_tri = world.spawn();
        world.insert(
            red_tri,
            Transform::from_translation(Vec3::new(1.5, 0.0, 0.5)),
        );
        world.insert(red_tri, GlobalTransform::IDENTITY);
        world.insert(red_tri, MeshHandle(red_handle));
        world.insert(red_tri, Spinning);

        let blue_tri = world.spawn();
        world.insert(
            blue_tri,
            Transform::from_translation(Vec3::new(1.8, 0.0, -0.3)),
        );
        world.insert(blue_tri, GlobalTransform::IDENTITY);
        world.insert(blue_tri, MeshHandle(blue_handle));
        world.insert(blue_tri, Spinning);
    }

    // Spawn camera entity looking at the scene center — unless CLI
    // overrides are supplied (`--camera-pos` / `--camera-forward`),
    // in which case the requested pose wins. Useful for offline
    // diagnostic renders without needing interactive WASD.
    let cam = world.spawn();
    let cam_pos = match camera_pos_override {
        Some((x, y, z)) => Vec3::new(x, y, z),
        None if has_nif_content => cam_center + Vec3::new(0.0, 100.0, 200.0),
        None => Vec3::new(0.0, 1.5, 4.0),
    };
    let cam_target = cam_center;
    let forward = match camera_forward_override {
        Some((x, y, z)) => {
            let v = Vec3::new(x, y, z);
            if v.length_squared() > 1e-8 {
                v.normalize()
            } else {
                log::warn!("--camera-forward 0,0,0 is invalid; using computed look-at");
                (cam_target - cam_pos).normalize()
            }
        }
        None => (cam_target - cam_pos).normalize(),
    };
    let cam_rotation = Quat::from_rotation_arc(-Vec3::Z, forward);
    world.insert(cam, Transform::new(cam_pos, cam_rotation, 1.0));
    world.insert(cam, GlobalTransform::new(cam_pos, cam_rotation, 1.0));
    world.insert(cam, Camera::default());
    // M44 Phase 1: the camera entity doubles as the audio listener
    // ("ears at the eyes"). M28.5 character controller will likely
    // split listener onto a head joint of the player capsule, but
    // for fly-cam fidelity this is canonical.
    world.insert(cam, byroredux_audio::AudioListener);
    // M44 Phase 3.5: opt the camera into footstep dispatch. Stride
    // threshold + per-footstep volume are read from `FootstepConfig`
    // (engine-wide resource set up in `App::new`).
    world.insert(cam, crate::components::FootstepEmitter::new());
    // Submersion state is recomputed each frame by `submersion_system`
    // from active `WaterPlane` / `WaterVolume` entities. Pre-inserting
    // the default keeps the system on the pure-mutation path (no
    // structural inserts mid-frame).
    world.insert(
        cam,
        byroredux_core::ecs::components::water::SubmersionState::default(),
    );
    world.insert_resource(ActiveCamera(cam));

    // NOTE: M28 Phase 1 attached a `PlayerBody::HUMAN` capsule to the
    // camera so the fly cam would collide with world geometry. That
    // path doesn't actually work as a camera rig — physics_sync_system
    // Phase 4 clobbers the rotation the fly camera writes each frame
    // (locking the view to the body's initial yaw), and setting linvel
    // directly overrides gravity so the player can't fall. A proper
    // kinematic character controller lands in M28.5; until then, the
    // fly camera stays free-fly and physics runs only on world +
    // clutter bodies spawned by the cell loader.

    // Initialize fly camera yaw/pitch from the initial look direction.
    {
        let mut input = world.resource_mut::<InputState>();
        input.yaw = forward.x.atan2(-forward.z);
        input.pitch = forward.y.asin();
    }

    // Build the global geometry SSBO for RT reflection ray UV lookups.
    if let Err(e) = ctx.mesh_registry.build_geometry_ssbo(
        &ctx.device,
        ctx.allocator.as_ref().unwrap(),
        &ctx.graphics_queue,
        ctx.transfer_pool,
        None, // TODO: thread StagingPool through scene load (#242)
    ) {
        log::warn!("Failed to build geometry SSBO: {e}");
    }
    // Write global geometry buffers to scene descriptor sets for RT reflection UV lookups.
    if let (Some(ref vb), Some(ref ib)) = (
        &ctx.mesh_registry.global_vertex_buffer,
        &ctx.mesh_registry.global_index_buffer,
    ) {
        for f in 0..2 {
            ctx.scene_buffers.write_geometry_buffers(
                &ctx.device,
                f,
                vb.buffer,
                vb.size,
                ib.buffer,
                ib.size,
            );
        }
    }

    let total_entities = world.next_entity_id();
    log::info!(
        "Scene ready: {} entities, 1 camera. Press Escape to capture mouse for fly camera.",
        total_entities
    );

    // Register the fullscreen quad mesh for UI overlay.
    if let Err(e) = ctx.register_ui_quad() {
        log::error!("Failed to register UI quad: {e:#}");
    }
    // Register the unit XY quad used by the CPU particle billboard path
    // (#401). One DrawCommand per live particle references this handle.
    if let Err(e) = ctx.register_particle_quad() {
        log::error!("Failed to register particle quad: {e:#}");
    }

    // UI: --swf <path> loads a SWF menu overlay.
    if let Some(swf_idx) = args.iter().position(|a| a == "--swf") {
        if let Some(swf_path) = args.get(swf_idx + 1) {
            match std::fs::read(swf_path) {
                Ok(swf_data) => {
                    let (w, h) = ctx.swapchain_extent();
                    let mut ui = UiManager::new(w, h);
                    match ui.load_swf(&swf_data, swf_path) {
                        Ok(()) => {
                            // Create the initial UI texture (transparent black).
                            let pixels = vec![0u8; (w * h * 4) as usize];
                            let allocator = ctx.allocator.as_ref().unwrap();
                            match ctx.texture_registry.register_rgba(
                                &ctx.device,
                                allocator,
                                &ctx.graphics_queue,
                                ctx.transfer_pool,
                                w,
                                h,
                                &pixels,
                            ) {
                                Ok(handle) => {
                                    *ui_texture_handle = Some(handle);
                                    log::info!("UI texture registered (handle {})", handle);
                                }
                                Err(e) => log::error!("Failed to register UI texture: {e:#}"),
                            }
                            *ui_manager = Some(ui);
                        }
                        Err(e) => log::error!("Failed to load SWF '{}': {e:#}", swf_path),
                    }
                }
                Err(e) => log::error!("Failed to read SWF file '{}': {e}", swf_path),
            }
        } else {
            log::error!("--swf requires a file path");
        }
    }
}

/// Parse CLI arguments and load NIF data accordingly.
///
/// Supported flags:
///   `cargo run -- path/to/file.nif` — loose NIF file
///   `cargo run -- --bsa meshes.bsa --mesh meshes\foo.nif` — extract from BSA
///   `cargo run -- --bsa meshes.bsa --mesh meshes\foo.nif --textures-bsa textures.bsa`
///   `cargo run -- --bsa meshes.bsa --tree trees\joshua01.spt` — direct
///       SpeedTree visualiser (Phase 1.6). Renders the placeholder billboard
///       per the SpeedTree compatibility plan; useful for one-tree
///       reverse-engineering iteration without spinning up a whole cell.
fn load_nif_from_args(world: &mut World, ctx: &mut VulkanContext) -> (usize, Option<EntityId>) {
    let args: Vec<String> = std::env::args().collect();

    // Collect BSA/BA2 archives (auto-detects format).
    let tex_provider = build_texture_provider(&args);
    let mut mat_provider = build_material_provider(&args);

    if let Some(bsa_idx) = args.iter().position(|a| a == "--bsa") {
        // BSA mode: --bsa <archive> {--mesh|--tree} <path_in_archive>.
        let bsa_path = match args.get(bsa_idx + 1) {
            Some(p) => p,
            None => {
                log::error!("--bsa requires an archive path");
                return (0, None);
            }
        };
        // `--tree` is shorthand for `--mesh` that documents the
        // user's intent to visualise a SpeedTree binary. The
        // routing inside `parse_import_and_merge` branches on the
        // path's `.spt` extension regardless of which flag was
        // used, so `--mesh foo.spt` works equivalently — `--tree`
        // exists for discoverability via `--help` / docs.
        let asset_path = match args
            .iter()
            .position(|a| a == "--mesh" || a == "--tree")
            .and_then(|i| args.get(i + 1))
        {
            Some(p) => p,
            None => {
                log::error!("--bsa requires --mesh <path> (or --tree <path> for `.spt`)");
                return (0, None);
            }
        };

        let archive = match byroredux_bsa::BsaArchive::open(bsa_path) {
            Ok(a) => a,
            Err(e) => {
                log::error!("Failed to open BSA '{}': {}", bsa_path, e);
                return (0, None);
            }
        };
        let data = match archive.extract(asset_path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to extract '{}': {}", asset_path, e);
                return (0, None);
            }
        };
        log::info!("Extracted {} bytes from BSA '{}'", data.len(), asset_path);
        load_nif_bytes(
            world,
            ctx,
            &data,
            asset_path,
            &tex_provider,
            Some(&mut mat_provider),
        )
    } else if let Some(nif_path) = args.get(1) {
        if nif_path.starts_with("--") {
            return (0, None); // Skip flags that aren't NIF paths
        }
        // Loose file mode: <path.nif>
        let data = match std::fs::read(nif_path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to read NIF file '{}': {}", nif_path, e);
                return (0, None);
            }
        };
        load_nif_bytes(
            world,
            ctx,
            &data,
            nif_path,
            &tex_provider,
            Some(&mut mat_provider),
        )
    } else {
        (0, None)
    }
}

/// Parse NIF bytes, import meshes with hierarchy, upload to GPU, and spawn ECS entities.
/// Returns (entity_count, root_entity).
pub(crate) fn load_nif_bytes(
    world: &mut World,
    ctx: &mut VulkanContext,
    data: &[u8],
    label: &str,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
) -> (usize, Option<EntityId>) {
    let (count, root, _local_map) = load_nif_bytes_with_skeleton(
        world,
        ctx,
        data,
        label,
        tex_provider,
        mat_provider,
        None,
        None,
    );
    (count, root)
}

/// Parse + import + BGSM-merge a NIF scene from raw bytes. Shared
/// helper for [`load_nif_bytes_with_skeleton`]'s cache-miss path
/// (where the result is wrapped in `Arc` and inserted into
/// [`crate::scene_import_cache::SceneImportCache`]) and its
/// hook-bypass path (where the per-NPC `pre_spawn_hook` then mutates
/// the result before spawn). Returns `None` on parse failure so the
/// caller can record a negative cache entry. See #880 / CELL-PERF-02.
///
/// Branches on `label`'s extension to route SpeedTree `.spt` bytes
/// through `byroredux_spt::parse_spt + import_spt_scene` instead of
/// the NIF parser. This is the loose-file / `--tree` direct-visualiser
/// path; cell loader REFRs go through `cell_loader::parse_and_import_spt`
/// which can also pull TREE record metadata for sizing + texture
/// override.
fn parse_import_and_merge(
    world: &mut World,
    data: &[u8],
    label: &str,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
) -> Option<byroredux_nif::import::ImportedScene> {
    let is_spt = label
        .rsplit('.')
        .next()
        .map(|ext| ext.eq_ignore_ascii_case("spt"))
        .unwrap_or(false);
    if is_spt {
        let scene = match byroredux_spt::parse_spt(data) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to parse SPT '{}': {}", label, e);
                return None;
            }
        };
        let mut pool = world.resource_mut::<StringPool>();
        // Direct-visualiser path has no TREE record context — the
        // importer's default params produce a 256×512 placeholder
        // textured with whatever leaf path the `.spt` itself
        // authored (tag 4003). Cell-loader REFRs hit the parallel
        // `cell_loader::parse_and_import_spt` path which threads
        // the TREE record's ICON / OBND through.
        let imported = byroredux_spt::import_spt_scene(
            &scene,
            &byroredux_spt::SptImportParams::default(),
            &mut pool,
        );
        // BGSM merge doesn't apply — `.spt` doesn't carry BGSM/BGEM
        // material refs. Drop the mat_provider unused for this path.
        let _ = mat_provider;
        return Some(imported);
    }
    let scene = match byroredux_nif::parse_nif(data) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to parse NIF '{}': {}", label, e);
            return None;
        }
    };
    let mut pool = world.resource_mut::<StringPool>();
    let mut imported = byroredux_nif::import::import_nif_scene_with_resolver(
        &scene,
        &mut pool,
        Some(tex_provider),
    );
    // FO4+ external material resolution (#493). NIF fields take
    // precedence; only empty slots fill in from the resolved
    // BGSM/BGEM chain. The merge interns through the same pool so
    // REFR overlays and per-mesh imports share the dedup table (#609).
    if let Some(provider) = mat_provider {
        for mesh in &mut imported.meshes {
            merge_bgsm_into_mesh(mesh, provider, &mut pool);
        }
    }
    Some(imported)
}

/// Variant of [`load_nif_bytes`] used by NPC spawn (M41.0 Phase 1b)
/// when assembling skeleton + body + head from three separate NIFs.
///
/// `external_skeleton`: when `Some(map)`, every skinning-bone name
/// lookup tries the external map first, falling back to this NIF's
/// local nodes. The body and head NIFs each spawn their own
/// (orphaned) copy of the skeleton's node hierarchy, but their
/// `SkinnedMesh.bones` references point at the SHARED skeleton
/// entities so all three palettes draw from one bone palette. Pre-fix
/// the body and head would each resolve against their own local
/// skeleton copies, leaving the head detached from the animated
/// skeleton.
///
/// Returns the local `node_by_name` map alongside the count and root
/// so the caller can chain it forward into the next NIF's external
/// skeleton parameter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn load_nif_bytes_with_skeleton(
    world: &mut World,
    ctx: &mut VulkanContext,
    data: &[u8],
    label: &str,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    external_skeleton: Option<&std::collections::HashMap<std::sync::Arc<str>, EntityId>>,
    // M41.0 Phase 3b — optional callback invoked once after the
    // import returns and before the per-mesh GPU upload loop runs.
    // Lets the caller mutate `imported.meshes[i].positions`
    // (for FaceGen morph deformation: head NIF + EGM sliders) or
    // any other field the renderer reads from `ImportedMesh`.
    // `&mut dyn FnMut` (rather than a generic) keeps the function
    // monomorphisation-cheap; static-dispatch isn't worth a
    // generic parameter for a load-time call.
    pre_spawn_hook: Option<&mut dyn FnMut(&mut byroredux_nif::import::ImportedScene)>,
) -> (
    usize,
    Option<EntityId>,
    std::collections::HashMap<std::sync::Arc<str>, EntityId>,
) {
    // #880 / CELL-PERF-02 — cache the parse + import + BGSM-merge
    // pipeline by lowercased path. Pre-fix every NPC spawn re-parsed
    // skeleton + body + hand NIFs from BSA bytes (~280 redundant
    // parses for Megaton-scale interiors). The cache is bypassed
    // when a `pre_spawn_hook` is provided (head-with-FaceGen-morph
    // path) because each NPC's morph is unique — caching the
    // already-morphed scene would hand the same face to every NPC.
    // The skeleton/body/hand calls all pass `pre_spawn_hook: None`,
    // so they hit the cache.
    let cache_key = label.to_ascii_lowercase();
    let cached_arc: Option<std::sync::Arc<byroredux_nif::import::ImportedScene>>;
    let mut owned_for_hook: Option<byroredux_nif::import::ImportedScene> = None;

    if let Some(hook) = pre_spawn_hook {
        // M41.0 Phase 3b — pre-spawn hook bypass. NPC head spawn
        // uses this hook to apply FaceGen FGGS / FGGA slider deltas
        // to `imported.meshes[head].positions` so the per-NPC unique
        // face shape lands in the GPU upload below. Recorded as a
        // bypass-parse so the cache's `parses` counter still
        // reflects total parse_nif invocations.
        {
            let mut cache = world.resource_mut::<crate::scene_import_cache::SceneImportCache>();
            cache.record_bypass_parse();
        }
        let mut imported =
            match parse_import_and_merge(world, data, label, tex_provider, mat_provider) {
                Some(s) => s,
                None => return (0, None, std::collections::HashMap::new()),
            };
        hook(&mut imported);
        owned_for_hook = Some(imported);
        cached_arc = None;
    } else {
        // Cache routing: read-lock probe → parse + import + insert
        // on miss. Three-tier shape mirrors `cell_loader::load_references`
        // (#523). Negative-cache entries (failed parses) short-circuit
        // subsequent NPC spawns of the same path so the warning log
        // doesn't spam.
        let cached = {
            let mut cache = world.resource_mut::<crate::scene_import_cache::SceneImportCache>();
            cache.get(&cache_key)
        };
        cached_arc = match cached {
            Some(Some(arc)) => Some(arc),
            Some(None) => {
                // Negative-cached parse failure — propagate the empty
                // result without re-parsing.
                return (0, None, std::collections::HashMap::new());
            }
            None => {
                let imported_opt =
                    parse_import_and_merge(world, data, label, tex_provider, mat_provider);
                let arc_opt = imported_opt.map(std::sync::Arc::new);
                let mut cache = world.resource_mut::<crate::scene_import_cache::SceneImportCache>();
                let stored = cache.insert(cache_key, arc_opt);
                match stored {
                    Some(arc) => Some(arc),
                    None => return (0, None, std::collections::HashMap::new()),
                }
            }
        };
    }

    // Bind a single `&ImportedScene` reference for the rest of the
    // function — the spawn loops only read. The borrow is anchored
    // in either `cached_arc` (cache hit / cache-miss insert) or
    // `owned_for_hook` (per-NPC FaceGen morph path); whichever one
    // is `Some` holds the live data.
    let imported: &byroredux_nif::import::ImportedScene = if let Some(ref s) = owned_for_hook {
        s
    } else {
        cached_arc
            .as_ref()
            .expect("either hook bypass or cache lookup must populate one branch")
            .as_ref()
    };

    // Phase 1: Spawn node entities (NiNode hierarchy).
    // node_index → EntityId mapping.
    // Also build a name → EntityId map so Phase 3 can resolve skinning
    // bone names to the entities they should drive. Skeleton nodes are
    // the only entities with unique names in a typical NIF, so collisions
    // (multiple nodes sharing a name) are rare; on collision we keep the
    // first spawn (root-most in depth-first order).
    let mut node_entities: Vec<EntityId> = Vec::with_capacity(imported.nodes.len());
    let mut node_by_name: std::collections::HashMap<std::sync::Arc<str>, EntityId> =
        std::collections::HashMap::with_capacity(imported.nodes.len());
    for node in &imported.nodes {
        let quat = Quat::from_xyzw(
            node.rotation[0],
            node.rotation[1],
            node.rotation[2],
            node.rotation[3],
        );
        let translation = Vec3::new(
            node.translation[0],
            node.translation[1],
            node.translation[2],
        );

        let entity = world.spawn();
        world.insert(entity, Transform::new(translation, quat, node.scale));
        world.insert(entity, GlobalTransform::IDENTITY);

        if let Some(ref name) = node.name {
            let mut pool = world.resource_mut::<StringPool>();
            let sym = pool.intern(name);
            drop(pool);
            world.insert(entity, Name(sym));
            node_by_name.entry(name.clone()).or_insert(entity);
        }

        // Attach collision data if present.
        if let Some((ref shape, ref body)) = node.collision {
            log::info!(
                "Collision attached to '{}': {:?} motion={:?} mass={:.1}",
                node.name.as_deref().unwrap_or("?"),
                std::mem::discriminant(shape),
                body.motion_type,
                body.mass,
            );
            world.insert(entity, shape.clone());
            world.insert(entity, body.clone());
        }

        // Attach Billboard component for NiBillboardNode-derived entities.
        // See #225 — nif import normalizes pre/post 10.1.0.0 mode layouts
        // into a single u16 before we map it to BillboardMode.
        if let Some(raw) = node.billboard_mode {
            world.insert(entity, Billboard::new(BillboardMode::from_nif(raw)));
        }

        // Attach raw NiAVObject flags so gameplay systems can branch on
        // DISABLE_SORTING, SELECTIVE_UPDATE, IS_NODE, DISPLAY_OBJECT,
        // etc. without re-reading the source NIF. APP_CULLED (bit 0) is
        // already consumed by the import-time visibility filter in
        // `walk.rs`, so every spawned node arrives with that bit clear.
        // We still emit the component unconditionally (not gated on
        // `flags != 0`) so a future toggle-visible system can just flip
        // the bit on the existing component. See #222.
        if node.flags != 0 {
            world.insert(entity, SceneFlags::from_nif(node.flags));
        }

        node_entities.push(entity);
    }

    // Phase 2: Set up Parent/Children relationships for nodes.
    for (node_idx, node) in imported.nodes.iter().enumerate() {
        if let Some(parent_idx) = node.parent_node {
            let child_entity = node_entities[node_idx];
            let parent_entity = node_entities[parent_idx];
            world.insert(child_entity, Parent(parent_entity));
            add_child(world, parent_entity, child_entity);
        }
    }

    // Phase 2.5: Particle emitters. The NIF importer surfaces every
    // NiParticleSystem / NiParticles / NiBSPArrayController as an
    // [`ImportedParticleEmitter`] tagged with its host node index, but
    // it doesn't carry per-emitter values — `NiPSysBlock` discards
    // every parsed field. We pick a heuristic ParticleEmitter preset
    // (torch_flame / smoke / magic_sparkles / generic flame fallback)
    // by scanning the host node's name. Every emitter is attached
    // directly to the host entity so the simulation sources its
    // world-space spawn origin from the host's GlobalTransform. See
    // #401 / audit OBL-D6-2.
    for emitter in &imported.particle_emitters {
        let Some(host_idx) = emitter.parent_node else {
            continue;
        };
        let Some(&host_entity) = node_entities.get(host_idx) else {
            continue;
        };
        let host_name = imported.nodes[host_idx]
            .name
            .as_deref()
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        // Embers / sparks check FIRST so a node like "FireSparks" lands
        // on the bright-glint preset rather than the larger torch flame
        // (the `fire` substring would otherwise win).
        let mut preset = if host_name.contains("spark")
            || host_name.contains("ember")
            || host_name.contains("cinder")
        {
            ParticleEmitter::embers()
        } else if host_name.contains("torch")
            || host_name.contains("fire")
            || host_name.contains("flame")
            || host_name.contains("brazier")
            || host_name.contains("candle")
        {
            ParticleEmitter::torch_flame()
        } else if host_name.contains("smoke")
            || host_name.contains("steam")
            || host_name.contains("ash")
        {
            ParticleEmitter::smoke()
        } else if host_name.contains("magic")
            || host_name.contains("enchant")
            || host_name.contains("sparkle")
            || host_name.contains("glow")
        {
            ParticleEmitter::magic_sparkles()
        } else {
            // Fallback — many vanilla NIFs don't name the host node
            // descriptively (e.g. just "EmitterNode"). Default to a
            // visible flame so the audit's "every torch invisible"
            // failure is still resolved end-to-end.
            ParticleEmitter::torch_flame()
        };
        // #707 / FX-2 — when the NIF authored a NiPSysColorModifier ->
        // NiColorData chain, the importer captured the keyframe
        // stream's first/last RGBA into `emitter.color_curve`.
        // Override the heuristic preset's start/end colour so authored
        // Dragonsreach embers / spell-cast colours / geyser steam read
        // distinctly from the generic preset values. Pre-fix the data
        // was parsed and immediately discarded — every torch looked
        // identical. The other preset fields (size_curve, lifetime,
        // emit_rate, etc.) stay at the heuristic preset's defaults
        // because the modifier only authors colour.
        if let Some(curve) = emitter.color_curve {
            preset.start_color = curve.start;
            preset.end_color = curve.end;
        }
        world.insert(host_entity, preset);
    }

    // Phase 3: Spawn mesh entities with parent links.
    let mut count = 0;
    let mut blas_specs: Vec<(u32, u32, u32)> = Vec::new();
    for mesh in &imported.meshes {
        // M41.0 Phase 1b.x temp gate — vanilla FNV / FO3 actor body NIFs
        // ship 4 dismemberment-cap sub-meshes alongside the visible body
        // (`bodycaps`, `limbcaps`, `meatneck01`, `meathead01`). The
        // legacy engine hides them via `BSDismemberSkinInstance.partitions
        // [i].part_flag` until a body part is actually dismembered; we
        // don't honour that flag yet, so they render as inside-the-body
        // bloody geometry that looks like dark ribbons / spikes spilling
        // from the actor. Skipping by name keeps NPCs visually coherent
        // until the partition-flag visibility pipeline lands as its own
        // followup. Match-arm naming is conservative — these are exact
        // vanilla mesh-name conventions and won't false-positive on
        // anything else.
        let mesh_name = mesh.name.as_deref().unwrap_or("");
        if matches!(
            mesh_name,
            "bodycaps" | "limbcaps" | "meatneck01" | "meathead01"
        ) {
            log::debug!(
                "Phase 1b.x: skipping dismemberment cap '{}' until BSDismemberSkinInstance \
                 partition flags are wired",
                mesh_name,
            );
            continue;
        }

        let num_verts = mesh.positions.len();
        // Skinned vertices use the per-vertex bone indices + weights that
        // #151 / #177 extracted from NiSkinData / BSTriShape. Rigid
        // vertices pass zero weights and the shader's rigid-path routes
        // them through `pc.model` instead of the bone palette.
        let skin_vertex_data = mesh
            .skin
            .as_ref()
            .filter(|s| !s.vertex_bone_indices.is_empty() && !s.vertex_bone_weights.is_empty());
        let vertices: Vec<Vertex> = (0..num_verts)
            .map(|i| {
                let position = mesh.positions[i];
                // Drop alpha — current `Vertex` color is 3-channel.
                // Imported colors carry RGBA so alpha is preserved on
                // the import side for a future 4-channel vertex format
                // (#618). Hair-tip / eyelash modulation will become
                // visible once the renderer's Vertex extends.
                let color = if i < mesh.colors.len() {
                    let c = mesh.colors[i];
                    [c[0], c[1], c[2]]
                } else {
                    [1.0, 1.0, 1.0]
                };
                let normal = if i < mesh.normals.len() {
                    mesh.normals[i]
                } else {
                    [0.0, 1.0, 0.0]
                };
                let uv = if i < mesh.uvs.len() {
                    mesh.uvs[i]
                } else {
                    [0.0, 0.0]
                };
                // #783 / M-NORMALS — pull the per-vertex tangent (xyz +
                // bitangent sign) from the imported mesh when authored.
                // Empty `mesh.tangents` falls through to the zero-vec
                // default, which the fragment shader's perturbNormal
                // detects and routes to its screen-space derivative
                // fallback path. This preserves rendering correctness
                // for both Bethesda-with-tangents and synthetic-without
                // content paths.
                let tangent = if i < mesh.tangents.len() {
                    mesh.tangents[i]
                } else {
                    [0.0, 0.0, 0.0, 0.0]
                };
                if let Some(skin) = skin_vertex_data {
                    // Guard against parallel-vector truncation — if the
                    // sparse skin upload filled fewer vertices than the
                    // mesh has positions, fall back to rigid for the
                    // remainder rather than panicking on index.
                    if i < skin.vertex_bone_indices.len() && i < skin.vertex_bone_weights.len() {
                        let idx = skin.vertex_bone_indices[i];
                        let w = skin.vertex_bone_weights[i];
                        let mut v = Vertex::new_skinned(
                            position,
                            color,
                            normal,
                            uv,
                            [idx[0] as u32, idx[1] as u32, idx[2] as u32, idx[3] as u32],
                            w,
                        );
                        v.tangent = tangent;
                        return v;
                    }
                }
                let mut v = Vertex::new(position, color, normal, uv);
                v.tangent = tangent;
                v
            })
            .collect();

        let alloc = ctx.allocator.as_ref().unwrap();
        // upload_scene_mesh registers the vertices/indices into the global
        // geometry SSBO that RT ray queries sample for reflection UVs.
        // See #371.
        let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
            &ctx.device,
            alloc,
            &ctx.graphics_queue,
            ctx.transfer_pool,
            &vertices,
            &mesh.indices,
            ctx.device_caps.ray_query_supported,
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                log::warn!(
                    "Failed to upload NIF mesh '{}': {}",
                    mesh.name.as_deref().unwrap_or("?"),
                    e
                );
                continue;
            }
        };

        // Collect BLAS specs for batched build after the loop.
        blas_specs.push((mesh_handle, num_verts as u32, mesh.indices.len() as u32));

        // Mesh paths are interned `FixedString` handles (#609). Resolve
        // each populated slot to an owned `String` once for the
        // downstream `Material` component + texture-resolve calls. The
        // pool read lock is short-lived; the resolved Strings outlive it.
        let (
            owned_texture_path,
            owned_normal_map,
            owned_glow_map,
            owned_detail_map,
            owned_gloss_map,
            owned_dark_map,
            owned_parallax_map,
            owned_env_map,
            owned_env_mask,
            owned_material_path,
        ) = {
            let pool_read = world.resource::<StringPool>();
            let resolve_owned =
                |sym: Option<byroredux_core::string::FixedString>| -> Option<String> {
                    sym.and_then(|s| pool_read.resolve(s))
                        .map(|s| s.to_string())
                };
            (
                resolve_owned(mesh.texture_path),
                resolve_owned(mesh.normal_map),
                resolve_owned(mesh.glow_map),
                resolve_owned(mesh.detail_map),
                resolve_owned(mesh.gloss_map),
                resolve_owned(mesh.dark_map),
                resolve_owned(mesh.parallax_map),
                resolve_owned(mesh.env_map),
                resolve_owned(mesh.env_mask),
                resolve_owned(mesh.material_path),
            )
        };

        let tex_handle = resolve_texture(ctx, tex_provider, owned_texture_path.as_deref());

        let quat = Quat::from_xyzw(
            mesh.rotation[0],
            mesh.rotation[1],
            mesh.rotation[2],
            mesh.rotation[3],
        );
        let translation = Vec3::new(
            mesh.translation[0],
            mesh.translation[1],
            mesh.translation[2],
        );

        let entity = world.spawn();
        world.insert(entity, Transform::new(translation, quat, mesh.scale));
        world.insert(entity, GlobalTransform::IDENTITY);
        world.insert(entity, MeshHandle(mesh_handle));
        world.insert(entity, TextureHandle(tex_handle));

        // Attach bounding data (#217): LocalBound captures the mesh-local
        // sphere; WorldBound is a placeholder filled in by the bound
        // propagation system once GlobalTransform has been computed.
        world.insert(
            entity,
            LocalBound::new(
                Vec3::new(
                    mesh.local_bound_center[0],
                    mesh.local_bound_center[1],
                    mesh.local_bound_center[2],
                ),
                mesh.local_bound_radius,
            ),
        );
        world.insert(entity, WorldBound::ZERO);
        if mesh.has_alpha {
            world.insert(
                entity,
                AlphaBlend {
                    src_blend: mesh.src_blend_mode,
                    dst_blend: mesh.dst_blend_mode,
                },
            );
        }
        if mesh.two_sided {
            world.insert(entity, TwoSided);
        }
        // #renderlayer — loose-NIF path has no REFR base record, so
        // the base layer defaults to Architecture (zero bias). The
        // per-mesh `is_decal` / `alpha_test_func` escalation still
        // applies — a NIF authored with explicit decal flags or
        // alpha-test cutout fringes gets the Decal layer regardless
        // of how it was spawned. NPC body / head / armor meshes
        // overwrite this with Actor in `npc_spawn::tag_descendants_as_actor`
        // after the spawn returns. Pre-#renderlayer this site also
        // inserted a `Decal` marker — retired in favour of
        // `RenderLayer::Decal`.
        {
            use byroredux_core::ecs::components::{
                escalate_small_static_to_clutter, render_layer_with_decal_escalation, RenderLayer,
            };
            // Loose-NIF spawn: no REFR, so no ref_scale to apply —
            // the mesh's local bound is its world bound. Same small-
            // STAT → Clutter rule as cell_loader so loose-loaded
            // desk papers don't z-fight against the desk loaded
            // alongside them.
            let layer = escalate_small_static_to_clutter(
                RenderLayer::Architecture,
                mesh.local_bound_radius,
            );
            let layer = render_layer_with_decal_escalation(layer, mesh.is_decal, mesh.alpha_test);
            world.insert(entity, layer);
        }
        // Carry `NiAVObject.flags` across — gameplay systems branch on
        // DISABLE_SORTING / SELECTIVE_UPDATE / DISPLAY_OBJECT bits
        // without touching the NIF source. APP_CULLED shapes never
        // reach this point (filtered import-side in walk.rs). See #222.
        if mesh.flags != 0 {
            world.insert(entity, SceneFlags::from_nif(mesh.flags));
        }
        // Attach material data (specular, emissive, glossiness, UV transform, etc.)
        world.insert(
            entity,
            Material {
                emissive_color: mesh.emissive_color,
                emissive_mult: mesh.emissive_mult,
                specular_color: mesh.specular_color,
                specular_strength: mesh.specular_strength,
                diffuse_color: mesh.diffuse_color,
                ambient_color: mesh.ambient_color,
                glossiness: mesh.glossiness,
                uv_offset: mesh.uv_offset,
                uv_scale: mesh.uv_scale,
                alpha: mesh.mat_alpha,
                env_map_scale: mesh.env_map_scale,
                normal_map: owned_normal_map.clone(),
                texture_path: owned_texture_path.clone(),
                material_path: owned_material_path.clone(),
                glow_map: owned_glow_map.clone(),
                detail_map: owned_detail_map.clone(),
                gloss_map: owned_gloss_map.clone(),
                dark_map: owned_dark_map.clone(),
                vertex_color_mode: mesh.vertex_color_mode,
                alpha_test: mesh.alpha_test,
                alpha_threshold: mesh.alpha_threshold,
                alpha_test_func: mesh.alpha_test_func,
                material_kind: mesh.material_kind,
                z_test: mesh.z_test,
                z_write: mesh.z_write,
                z_function: mesh.z_function,
                shader_type_fields: if mesh.shader_type_fields.is_empty() {
                    None
                } else {
                    Some(Box::new(mesh.shader_type_fields.to_core()))
                },
                // #620 — BSEffectShaderProperty falloff cone (Skyrim+)
                // OR BSShaderNoLightingProperty falloff cone (FO3/FNV
                // SIBLING per #451). Either yields an `EffectFalloff`;
                // BSShaderNoLighting fills `soft_falloff_depth = 0.0`
                // since that block has no soft-depth field.
                effect_falloff: mesh
                    .effect_shader
                    .as_ref()
                    .map(
                        |es| byroredux_core::ecs::components::material::EffectFalloff {
                            start_angle: es.falloff_start_angle,
                            stop_angle: es.falloff_stop_angle,
                            start_opacity: es.falloff_start_opacity,
                            stop_opacity: es.falloff_stop_opacity,
                            soft_falloff_depth: es.soft_falloff_depth,
                        },
                    )
                    .or_else(|| {
                        mesh.no_lighting_falloff.as_ref().map(|nl| {
                            byroredux_core::ecs::components::material::EffectFalloff {
                                start_angle: nl.start_angle,
                                stop_angle: nl.stop_angle,
                                start_opacity: nl.start_opacity,
                                stop_opacity: nl.stop_opacity,
                                soft_falloff_depth: 0.0,
                            }
                        })
                    }),
                // #890 Stage 2 — see cell_loader.rs for the
                // identical packing site / explanation.
                effect_shader_flags: crate::cell_loader::pack_effect_shader_flags(
                    mesh.effect_shader.as_ref(),
                ),
            },
        );

        // Load and attach normal map texture handle.
        if let Some(ref nmap_path) = owned_normal_map {
            let h = resolve_texture(ctx, tex_provider, Some(nmap_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, NormalMapHandle(h));
            }
        }
        // Load and attach dark/lightmap texture handle.
        if let Some(ref dark_path) = owned_dark_map {
            let h = resolve_texture(ctx, tex_provider, Some(dark_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, DarkMapHandle(h));
            }
        }
        // #399 — three NiTexturingProperty extra slots packed into one
        // ECS component. Mirrors the cell_loader.rs path; only attached
        // when at least one slot resolved to a real texture handle.
        let mut resolve = |path: &Option<String>| -> u32 {
            path.as_deref()
                .map(|p| resolve_texture(ctx, tex_provider, Some(p)))
                .filter(|&h| h != ctx.texture_registry.fallback())
                .unwrap_or(0)
        };
        let glow_h = resolve(&owned_glow_map);
        let detail_h = resolve(&owned_detail_map);
        let gloss_h = resolve(&owned_gloss_map);
        let parallax_h = resolve(&owned_parallax_map);
        let env_h = resolve(&owned_env_map);
        let env_mask_h = resolve(&owned_env_mask);
        if glow_h != 0
            || detail_h != 0
            || gloss_h != 0
            || parallax_h != 0
            || env_h != 0
            || env_mask_h != 0
        {
            world.insert(
                entity,
                ExtraTextureMaps {
                    glow: glow_h,
                    detail: detail_h,
                    gloss: gloss_h,
                    parallax: parallax_h,
                    env: env_h,
                    env_mask: env_mask_h,
                    parallax_height_scale: mesh.parallax_height_scale.unwrap_or(0.04),
                    parallax_max_passes: mesh.parallax_max_passes.unwrap_or(4.0),
                },
            );
        }

        if let Some(ref name) = mesh.name {
            let mut pool = world.resource_mut::<StringPool>();
            let sym = pool.intern(name);
            drop(pool);
            world.insert(entity, Name(sym));
        }

        // Attach skinning binding if present. Resolves each bone name to
        // the entity spawned for that node in Phase 1. Missing bones are
        // kept as `None`; the palette system substitutes identity for them.
        if let Some(ref skin) = mesh.skin {
            if skin.bones.len() > MAX_BONES_PER_MESH {
                log::warn!(
                    "Skinned mesh '{}' has {} bones (> MAX_BONES_PER_MESH={}); skipping skinning",
                    mesh.name.as_deref().unwrap_or("?"),
                    skin.bones.len(),
                    MAX_BONES_PER_MESH
                );
            } else {
                let mut bones: Vec<Option<EntityId>> = Vec::with_capacity(skin.bones.len());
                let mut binds: Vec<Mat4> = Vec::with_capacity(skin.bones.len());
                let mut unresolved = 0_usize;
                let mut unresolved_names: Vec<&str> = Vec::new();
                for bone in &skin.bones {
                    // M41.0 Phase 1b: prefer the external skeleton
                    // map (set when the spawn function is assembling
                    // skeleton + body + head) so body/head NIF
                    // skinning resolves to the shared skeleton's
                    // entities, not the body/head's own orphaned
                    // local node copies.
                    let resolved = external_skeleton
                        .and_then(|m| m.get(&bone.name).copied())
                        .or_else(|| node_by_name.get(&bone.name).copied());
                    match resolved {
                        Some(e) => bones.push(Some(e)),
                        None => {
                            bones.push(None);
                            unresolved += 1;
                            if unresolved_names.len() < 8 {
                                unresolved_names.push(&bone.name);
                            }
                        }
                    }
                    binds.push(Mat4::from_cols_array_2d(&bone.bind_inverse));
                }
                // M41.0 Phase 1b.x — global_skin_transform investigation
                // resolved (#771 / LC-D3-NEW-01). Per nifly Skin.hpp:49-51,
                // NiSkinData::bones[i].boneTransform IS skin→bone
                // (compose-ready, includes the global offset). The
                // top-level skinTransform is therefore informational
                // only at runtime; `compute_palette_into` does NOT
                // multiply it. The first attempt at right-multiply
                // double-applied the global offset, which is why it
                // looked visually worse. Captured here for diagnostic
                // visibility (Doc Mitchell ships a non-identity cyclic
                // permutation; FO4+ BSSkin paths ship identity — the
                // asymmetry is informative).
                let global_skin_transform = Mat4::from_cols_array_2d(&skin.global_skin_transform);
                let root_entity = skin.skeleton_root.as_ref().and_then(|n| {
                    external_skeleton
                        .and_then(|m| m.get(n).copied())
                        .or_else(|| node_by_name.get(n).copied())
                });
                world.insert(
                    entity,
                    SkinnedMesh::new_with_global(root_entity, bones, binds, global_skin_transform),
                );
                if unresolved > 0 {
                    // M41.0 Phase 1b.x followup — unresolved bones land
                    // as `None` in `SkinnedMesh.bones`, and
                    // `compute_palette_into` substitutes
                    // `Mat4::IDENTITY` for those slots. Vertices weighted
                    // to such a slot end up at `vertex_local` (near NIF
                    // skin-space origin) while neighbours weighted to
                    // resolved bones land at world coords, producing
                    // triangle ribbons stretched from origin to the
                    // actor's placement. Logging the names so we can see
                    // which sub-skeleton convention is mismatched
                    // between the source NIF and the external skeleton
                    // map.
                    log::warn!(
                        "Skinned mesh '{}': {} bones ({} UNRESOLVED — names: {:?}), root={:?}",
                        mesh.name.as_deref().unwrap_or("?"),
                        skin.bones.len(),
                        unresolved,
                        unresolved_names,
                        skin.skeleton_root,
                    );
                } else {
                    log::info!(
                        "Skinned mesh '{}': {} bones (0 unresolved), root={:?}",
                        mesh.name.as_deref().unwrap_or("?"),
                        skin.bones.len(),
                        skin.skeleton_root,
                    );
                }
            }
        }

        // Set up parent relationship.
        if let Some(parent_idx) = mesh.parent_node {
            let parent_entity = node_entities[parent_idx];
            world.insert(entity, Parent(parent_entity));
            add_child(world, parent_entity, entity);
        }

        log::info!(
            "Loaded NIF mesh '{}': {} verts, {} tris, tex={:?}",
            mesh.name.as_deref().unwrap_or("unnamed"),
            num_verts,
            mesh.indices.len() / 3,
            mesh.texture_path,
        );
        count += 1;
    }

    // Batched BLAS build: single GPU submission for all NIF meshes.
    if !blas_specs.is_empty() {
        ctx.build_blas_batched(&blas_specs);
    }

    let root = node_entities.first().copied();

    // #261 — mesh-embedded controller chains (water UV scroll, torch
    // flame visibility, lava emissive pulse). `import_nif_scene`
    // collected every NiObjectNET.controller_ref chain into a single
    // looping clip. Register it and spawn an AnimationPlayer scoped to
    // the NIF root so the subtree-local name lookup works the same way
    // it does for KF clips.
    if let Some(nif_embedded_clip) = imported.embedded_clip.as_ref() {
        let float_ct = nif_embedded_clip.float_channels.len();
        let color_ct = nif_embedded_clip.color_channels.len();
        let bool_ct = nif_embedded_clip.bool_channels.len();
        let duration = nif_embedded_clip.duration;
        let clip_handle = {
            let mut pool = world.resource_mut::<StringPool>();
            let clip = crate::anim_convert::convert_nif_clip(nif_embedded_clip, &mut pool);
            drop(pool);
            let mut registry = world.resource_mut::<AnimationClipRegistry>();
            registry.add(clip)
        };
        let player_entity = world.spawn();
        let mut player = AnimationPlayer::new(clip_handle);
        if let Some(root) = root {
            player.root_entity = Some(root);
        }
        world.insert(player_entity, player);
        log::info!(
            "Embedded animation clip registered from '{}' ({:.2}s, {} float + {} color + {} bool channels) → handle {}",
            label,
            duration,
            float_ct,
            color_ct,
            bool_ct,
            clip_handle,
        );
    }

    log::info!(
        "Imported {} nodes + {} meshes from '{}'",
        imported.nodes.len(),
        count,
        label
    );
    (count + imported.nodes.len(), root, node_by_name)
}


#[cfg(test)]
mod radius_parse_tests;
#[cfg(test)]
mod cloud_tile_scale_tests;
#[cfg(test)]
mod procedural_fallback_tests;
#[cfg(test)]
mod climate_tod_hours_tests;
