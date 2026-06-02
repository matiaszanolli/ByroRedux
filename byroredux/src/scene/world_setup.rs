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
    CloudSimState, GameTimeRes, SkyParamsRes, WeatherDataRes, WeatherTransitionRes,
};
use crate::streaming::{self, LoadedCell, WorldStreamingState};

/// Reference cloud sprite width — Bethesda's typical authoring
/// resolution. Per-layer baselines (`CLOUD_TILE_SCALE_*`) assume this
/// width; an authored cloud DDS at any other resolution is rescaled
/// inversely so a 1024² cloud tiles half as often as a 512² and a
/// 256² tiles twice as often, preserving on-screen blob density across
/// WTHR records that ship sharper or coarser cloud layers. See #529.
const CLOUD_REF_WIDTH: f32 = 512.0;

/// Build the initial [`GameTimeRes`], honoring the `BYRO_HOUR` env var for
/// offline / cinematic renders. When `BYRO_HOUR` is set to a value in
/// `[0, 24)`, the starting hour is overridden and `time_scale` is frozen to
/// `0.0` so the requested time-of-day stays put for a deterministic capture
/// (e.g. a golden-hour screenshot via `--bench-frames`/`--screenshot`).
/// Unset or unparseable → the normal advancing default. Env-var convention
/// matches `BYRO_DEBUG_PORT` / `RUST_LOG`; no extra CLI threading needed.
fn initial_game_time() -> GameTimeRes {
    match std::env::var("BYRO_HOUR")
        .ok()
        .and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|h| (0.0..24.0).contains(h))
    {
        Some(hour) => {
            log::info!("BYRO_HOUR override: starting hour {hour:.2}, time frozen (time_scale=0)");
            GameTimeRes {
                hour,
                time_scale: 0.0,
            }
        }
        None => GameTimeRes::default(),
    }
}

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
/// Which prior-worldspace sky-texture handles [`apply_worldspace_weather`]
/// must `drop_texture` when it re-acquires a new set (#1339). Takes the
/// previous `SkyParamsRes::texture_indices()` (the 4 cloud layers + CLMT
/// sun sprite) or `None` on the first worldspace entry. Skips `0`
/// (procedural / absent layer) and the registry `fallback` slot — the same
/// skip rule as the cell-unload texture sweep. Pure so the release set is
/// unit-testable without a `VulkanContext`.
fn sky_textures_to_release(prev: Option<[u32; 5]>, fallback: u32) -> Vec<u32> {
    prev.into_iter()
        .flatten()
        .filter(|&h| h != 0 && h != fallback)
        .collect()
}

pub(crate) fn apply_worldspace_weather(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    wctx: &cell_loader::ExteriorWorldContext,
) {
    // Bootstrap sun direction from the canonical sun model (EXAL step 4):
    // `tod_hours` + the engine south-tilt drive `compute_sun_arc` — the same
    // model `weather_system` runs every frame — so the initial resources seed
    // consistently instead of with an arbitrary vector. `weather_system`
    // overwrites this on frame 1 from the live game hour. Q1 settled that the
    // sun-path is engine-defined (no authored latitude exists); see
    // docs/engine/exal.md §9.
    use crate::systems::weather::{compute_sun_arc, DEFAULT_TOD_HOURS};
    let bootstrap_hour = initial_game_time().hour;
    if let Some(ref wthr) = wctx.default_weather {
        let sun_dir = compute_sun_arc(bootstrap_hour, climate_tod_hours(wctx.climate.as_ref())).0;
        // #1339 — capture the prior worldspace's sky-texture handles BEFORE
        // the cloud/sun resolution below re-acquires (each bumps a refcount).
        // Released after the new `SkyParamsRes` is installed, so handles
        // shared with the new worldspace stay resident (acquire-new-then-
        // release-old). Worldspace-scoped, survive per-cell unload (#1199);
        // this re-acquire is the only release point. First call: `None`.
        let prev_sky_textures = world
            .try_resource::<SkyParamsRes>()
            .map(|s| s.texture_indices());
        // Canonical day-slot lighting (EXAL boundary). The per-frame
        // `weather_system` then advances through the stored NAM0 table.
        let lighting = crate::env_translate::translate_exterior_cell_lighting(wthr, sun_dir);
        // Resolve the 4 WTHR cloud layers + CLMT sun sprite — the only
        // VulkanContext-coupled step (#529 derives tile_scale from the
        // authored DDS width; #478 resolves the FNAM sun sprite). The
        // translate stays pure (EXAL §3): handles in, canonical out.
        let cloud_layers = [
            resolve_cloud_layer(
                wthr.cloud_textures[0].as_deref(),
                CLOUD_TILE_SCALE_LAYER_0,
                "0",
                tex_provider,
                ctx,
            ),
            resolve_cloud_layer(
                wthr.cloud_textures[1].as_deref(),
                CLOUD_TILE_SCALE_LAYER_1,
                "1",
                tex_provider,
                ctx,
            ),
            resolve_cloud_layer(
                wthr.cloud_textures[2].as_deref(),
                CLOUD_TILE_SCALE_LAYER_2,
                "2",
                tex_provider,
                ctx,
            ),
            resolve_cloud_layer(
                wthr.cloud_textures[3].as_deref(),
                CLOUD_TILE_SCALE_LAYER_3,
                "3",
                tex_provider,
                ctx,
            ),
        ];
        let sun_sprite = resolve_sun_sprite(wctx.climate.as_ref(), tex_provider, ctx);
        let sky = crate::env_translate::translate_sky(
            wthr,
            sun_dir,
            crate::env_translate::SkyTextures {
                cloud_layers,
                sun_sprite,
            },
        );
        log::info!(
            "WTHR '{}': zenith={:?} horizon={:?} sun={:?} ambient={:?} sunlight={:?} fog_color={:?} fog_day={:.0}\u{2013}{:.0}",
            wthr.editor_id,
            sky.zenith_color,
            sky.horizon_color,
            sky.sun_color,
            lighting.ambient,
            lighting.directional_color,
            lighting.fog_color,
            lighting.fog_near,
            lighting.fog_far,
        );
        world.insert_resource(lighting);
        world.insert_resource(sky);
        // #1339 — release the prior worldspace's sky textures now that the
        // new set is acquired + installed. `drop_texture` decrements the
        // refcount: a handle shared with the new worldspace drops back to
        // its prior count (stays resident); one unique to the old worldspace
        // hits 0 and frees its bindless slot + VkImage. Without this, every
        // interior→exterior / exterior→exterior worldspace transition leaked
        // up to 5 textures (4 cloud layers + 1 CLMT sun sprite).
        let sky_fallback = ctx.texture_registry.fallback();
        for handle in sky_textures_to_release(prev_sky_textures, sky_fallback) {
            ctx.texture_registry.drop_texture(&ctx.device, handle);
        }
        // #803 — cloud scroll lives on `CloudSimState`, which survives
        // cell transitions. Insert a default-zero state on first
        // exterior load only; subsequent loads reuse the existing
        // accumulator so clouds resume drift across interior visits.
        if world.try_resource::<CloudSimState>().is_none() {
            world.insert_resource(CloudSimState::default());
        }
        // Full NAM0 table + per-climate TOD breakpoints + Skyrim DALC cube
        // (Z-up→Y-up once), all resolved at the EXAL boundary.
        let new_weather = crate::env_translate::translate_weather(wthr, wctx.climate.as_ref());
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
            world.insert_resource(initial_game_time());
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
        let sun_dir = compute_sun_arc(bootstrap_hour, DEFAULT_TOD_HOURS).0;
        insert_procedural_fallback_resources(world, sun_dir);
    }
}

/// Resolve the CLMT FNAM sun-sprite path to a bindless handle. `0` = use
/// the composite shader's procedural disc (no climate / no path / load
/// failure). The only `VulkanContext`-coupled half of sky setup besides
/// the cloud layers; kept here so `env_translate::translate_sky` stays
/// pure. See #478.
fn resolve_sun_sprite(
    climate: Option<&byroredux_plugin::esm::records::ClimateRecord>,
    tex_provider: &TextureProvider,
    ctx: &mut VulkanContext,
) -> u32 {
    climate
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
        .unwrap_or(0)
}

/// Procedural fallback sky + lighting + game-time resources for a
/// worldspace with no resolved climate / weather record. The canonical
/// values live behind the EXAL boundary
/// ([`crate::env_translate::procedural_fallback_cell_lighting`] /
/// `_sky` / `_weather`); this function is the orchestration that installs
/// them plus `GameTimeRes` and the survives-transitions `CloudSimState`,
/// so `weather_system` runs the sun arc each frame instead of
/// early-returning. See #542 / M33-10.
pub(crate) fn insert_procedural_fallback_resources(world: &mut World, sun_dir: [f32; 3]) {
    world.insert_resource(crate::env_translate::procedural_fallback_cell_lighting(sun_dir));
    world.insert_resource(crate::env_translate::procedural_fallback_sky(sun_dir));
    // #803 — same survives-transitions pattern as the WTHR path: seed
    // CloudSimState only on the first exterior load.
    if world.try_resource::<CloudSimState>().is_none() {
        world.insert_resource(CloudSimState::default());
    }
    world.insert_resource(crate::env_translate::procedural_fallback_weather());
    world.insert_resource(initial_game_time());
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
pub(crate) fn stream_initial_radius(
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
        if state.send_request(req).is_err() {
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

    // Distant-terrain LOD ring (#view-dist). With every full-detail cell
    // now loaded, build the coarse LOD blocks that extend view distance
    // ~10× beyond the streamed ring. Cells inside `radius_load` are holed
    // out so the LOD never overlaps the near terrain. Slice 2 (#1373):
    // populate `state.lod_blocks` so the ring is tracked and re-centered as
    // the player walks (the initial call runs against an empty map → spawns
    // the whole ring around the spawn cell).
    let lod_tex = state.tex_provider.clone();
    cell_loader::stream_lod_blocks(
        world,
        ctx,
        lod_tex.as_ref(),
        wctx.as_ref(),
        (cx, cy),
        state.radius_load,
        &mut state.lod_blocks,
    );
    // Distant object LOD (Skyrim+/FO4 `.bto`) — no-op on other games.
    cell_loader::stream_object_lod_blocks(
        world,
        ctx,
        lod_tex.as_ref(),
        wctx.as_ref(),
        (cx, cy),
        state.radius_load,
        &mut state.object_lod_blocks,
    );

    center
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1339 / D3-03 — on a worldspace re-acquire, every real prior sky
    /// texture (cloud 0-3 + CLMT sun) must be released, but `0` (absent /
    /// procedural) and the shared `fallback` slot must be skipped so we
    /// don't over-release a slot the renderer still points at.
    #[test]
    fn sky_release_keeps_only_real_handles() {
        let fallback = 99u32;
        // [cloud0, cloud1, cloud2, cloud3, sun]; 0 = absent layer.
        let prev = Some([10, 0, 11, fallback, 12]);
        let mut got = sky_textures_to_release(prev, fallback);
        got.sort_unstable();
        assert_eq!(
            got,
            vec![10, 11, 12],
            "only real, non-fallback sky handles are released"
        );
    }

    /// First worldspace entry (startup) — no prior `SkyParamsRes`, so
    /// nothing to release. Guards against an over-release / panic on boot.
    #[test]
    fn sky_release_none_is_empty() {
        assert!(sky_textures_to_release(None, 99).is_empty());
    }

    /// A worldspace whose WTHR authored no cloud/sun textures (all-zero
    /// indices) releases nothing.
    #[test]
    fn sky_release_all_absent_is_empty() {
        assert!(sky_textures_to_release(Some([0, 0, 0, 0, 0]), 99).is_empty());
    }
}

