//! Per-worldspace setup helpers: cloud-sprite resolution, climate TOD
//! breakpoints, weather-pipeline initialization, procedural fallback
//! resources, and exterior streaming radius.
//!
//! Each entry point is called once per worldspace load from
//! [`super::setup_scene`]. Split out of the parent `scene.rs` to keep
//! it under ~1000 lines.

use byroredux_core::ecs::World;
use byroredux_core::math::Vec3;
use byroredux_renderer::VulkanContext;

use crate::asset_provider::TextureProvider;
use crate::cell_loader;
use crate::components::{
    CellLightingRes, CloudSimState, GameTimeRes, SkyParamsRes, WeatherDataRes,
    WeatherTransitionRes,
};
use crate::streaming::{self, LoadedCell, WorldStreamingState};

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
pub(crate) const CLOUD_TILE_SCALE_LAYER_0: f32 = 0.15;
pub(crate) const CLOUD_TILE_SCALE_LAYER_1: f32 = 0.20;
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
pub(crate) fn cloud_tile_scale_for_dds(dds_bytes: &[u8], baseline: f32) -> f32 {
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
pub(super) fn apply_worldspace_weather(
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
            // #993 — populated per-frame by `weather_system` when the
            // WTHR record carried DALC sub-records. Stays `None` for
            // FNV/FO3/Oblivion (different ambient model).
            current_dalc_cube: None,
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
        // #993 — Skyrim WTHR ships a 4-entry DALC cube (sunrise / day
        // / sunset / night). Convert Bethesda Z-up authoring to engine
        // Y-up once here so `weather_system` can lerp on raw f32s
        // without per-frame coord swaps. `None` on FNV / FO3 /
        // Oblivion / FO4+ (different ambient models).
        let skyrim_dalc_per_tod = wthr.skyrim_ambient_cube.as_ref().map(|cubes| {
            [
                crate::components::DalcCubeYup::from_skyrim_zup(&cubes[0]),
                crate::components::DalcCubeYup::from_skyrim_zup(&cubes[1]),
                crate::components::DalcCubeYup::from_skyrim_zup(&cubes[2]),
                crate::components::DalcCubeYup::from_skyrim_zup(&cubes[3]),
            ]
        });
        let new_weather = WeatherDataRes {
            sky_colors,
            fog: [
                wthr.fog_day_near,
                wthr.fog_day_far,
                wthr.fog_night_near,
                wthr.fog_night_far,
            ],
            tod_hours,
            skyrim_dalc_per_tod,
        };
        // First-time bootstrap: insert directly. A subsequent worldspace
        // change (door-walking interior↔exterior, M40 Phase 2) will
        // trigger the 8-second crossfade via WeatherTransitionRes.
        if world.try_resource::<WeatherDataRes>().is_some() {
            world.insert_resource(WeatherTransitionRes {
                target: new_weather,
                elapsed_secs: 0.0,
                duration_secs: 8.0,
                done: false,
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
pub(crate) fn insert_procedural_fallback_resources(world: &mut World, sun_dir: [f32; 3]) {
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
        // Procedural fallback path has no WTHR record, hence no DALC.
        current_dalc_cube: None,
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
        // Procedural fallback has no WTHR DALC — Skyrim cube stays
        // `None`, the renderer falls through to the flat ambient + AO
        // floor path on every fragment.
        skyrim_dalc_per_tod: None,
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
pub(super) fn stream_initial_radius(
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

