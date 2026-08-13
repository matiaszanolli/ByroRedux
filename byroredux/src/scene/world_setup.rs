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
use crate::streaming::{self, WorldStreamingState};
use crate::streaming_helpers::{
    consume_streaming_payload, reconcile_lod_rings, StreamingPayloadOutcome,
};

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
            GameTimeRes::frozen_at(hour)
        }
        None => GameTimeRes::default(),
    }
}

pub(crate) fn ensure_game_time(world: &mut World) {
    if world.try_resource::<GameTimeRes>().is_none() {
        world.insert_resource(initial_game_time());
    }
}

fn bootstrap_game_hour(world: &World) -> f32 {
    world
        .try_resource::<GameTimeRes>()
        .map_or_else(|| initial_game_time().hour, |time| time.hour)
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
    // The clock survives worldspace transitions. Seed the replacement sky
    // from that live hour rather than flashing back to the process default
    // until `weather_system` runs on the next frame.
    let bootstrap_hour = bootstrap_game_hour(world);
    // #1339 / #1770 — capture the prior worldspace's sky-texture handles BEFORE
    // either branch re-acquires (the WTHR path bumps a refcount per cloud/sun
    // layer; the procedural fallback installs a texture-less sky). Released
    // after the new `SkyParamsRes` is installed on BOTH branches, so handles
    // shared with the new worldspace stay resident (acquire-new-then-release-
    // old). Worldspace-scoped, survive per-cell unload (#1199); this re-acquire
    // is the only release point. First call: `None`.
    let prev_sky_textures = world
        .try_resource::<SkyParamsRes>()
        .map(|s| s.texture_indices());
    // #2511 / REN-D18-NEW-02 — collapse any transition still in flight
    // from a PRIOR worldspace change before this one installs a new
    // transition (WTHR branch) or replaces `WeatherDataRes` outright
    // (procedural-fallback branch). See the function doc for the two
    // failure modes this prevents.
    collapse_weather_transition(world);
    if let Some(ref wthr) = wctx.default_weather {
        let sun_dir = compute_sun_arc(
            bootstrap_hour,
            crate::env_translate::climate_tod_hours(wctx.climate.as_ref()),
        )
        .0;
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
            ensure_game_time(world);
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
        // #1770 — the procedural fallback installs a texture-less sky, so it
        // must release the prior worldspace's sky handles too. Without this, a
        // transition into a climateless worldspace (corrupt/partial ESM, mod
        // worldspace with no CLMT, synthetic cell) leaked up to 5 textures
        // (4 cloud layers + CLMT sun sprite). Mirrors the WTHR branch's
        // acquire-new-then-release-old; the procedural sky has all-zero indices
        // so every prior unique handle correctly drops to 0 and frees.
        let sky_fallback = ctx.texture_registry.fallback();
        for handle in sky_textures_to_release(prev_sky_textures, sky_fallback) {
            ctx.texture_registry.drop_texture(&ctx.device, handle);
        }
    }

    install_ground_cover(world, wctx);
}

/// EXAL ground-cover translate site (#2369, design §7).
///
/// Runs after both weather branches so the wind field reads whichever
/// `WeatherDataRes` was actually installed — WTHR-derived or procedural
/// fallback — rather than duplicating the translation per branch.
///
/// Both resources are *derived*: the palette from the worldspace identity plus
/// (in Phase 5) its `GRAS` records, the wind from the live weather. Neither is
/// saved; both are re-resolved on load, which is why they sit in the save
/// guard's `NOT_SAVED_BY_DESIGN` list rather than the registry.
///
/// `authored` is empty for every game today. That is the designed steady state
/// for content with no vegetation data, not a stub — `GroundCoverPalette::
/// resolve` substitutes the built-in species so the scatter pass never sees an
/// empty palette. Phase 5 fills it from `GRAS`.
fn install_ground_cover(world: &mut World, wctx: &cell_loader::ExteriorWorldContext) {
    use crate::groundcover_translate::{resolve_palette_for_chain, resolve_wind};

    let wind_speed = world
        .try_resource::<WeatherDataRes>()
        .map(|w| w.wind_speed)
        .unwrap_or(0);
    // Classify over the full WNAM ancestry, not the leaf name: FO3's
    // `MegatonWorld` carries no geographic signal of its own and reads as
    // temperate, when Megaton is a Capital Wasteland settlement.
    let chain = crate::env_translate::worldspace_name_chain(
        &wctx.record_index.cells.worldspaces,
        &wctx.worldspace_key,
    );
    let palette = resolve_palette_for_chain(&chain, Vec::new());
    let wind = resolve_wind(&wctx.worldspace_key, wind_speed);
    log::info!(
        target: "engine::groundcover",
        "Ground cover for '{}' (chain {:?}): climate {:?}, {} species, \
         wind {:.1} u/s along [{:.2}, {:.2}]",
        wctx.worldspace_key,
        chain,
        palette.climate,
        palette.species.len(),
        wind.speed,
        wind.direction[0],
        wind.direction[1],
    );
    world.insert_resource(palette);
    world.insert_resource(wind);
}

/// Collapse any `WeatherTransitionRes` still in flight from a prior
/// worldspace change. `WeatherTransitionRes` is a one-shot cross-fade —
/// nothing removes it once installed; `weather_system` only latches
/// `done = true` on completion (never `remove_resource`, since systems
/// only get `&World`) and `cell_loader/unload.rs` deliberately leaves it
/// worldspace-scoped (#1199). Without this collapse, a second worldspace
/// transition landing inside the 8-second fade window hits one of two
/// bugs:
///
/// - **WTHR branch retarget**: installing a fresh `WeatherTransitionRes`
///   clobbers the in-flight one while the live `WeatherDataRes` is still
///   the *original* pre-fade source (it's never progressively blended,
///   only swapped on completion) — the rendered weather pops backward
///   from `lerp(src, oldTarget, t)` to `src` on the switch frame.
/// - **Procedural-fallback branch**: replacing `WeatherDataRes` with the
///   procedural sky leaves the in-flight transition installed;
///   `weather_system` keeps blending the procedural sky toward the
///   *previous* worldspace's target and, on completion, permanently
///   overwrites the procedural fallback with that stale target.
///
/// Called at the top of [`apply_worldspace_weather`], before either
/// branch touches `WeatherDataRes` / `WeatherTransitionRes`. If a
/// transition is genuinely in flight (`done == false`), finishes it
/// early by promoting `target` into `WeatherDataRes` via the same field
/// copy `weather_system` uses on natural completion — a same-frame snap
/// rather than an indefinite stale blend — then removes the transition
/// resource outright (a real `remove_resource`, not a `done` latch,
/// since this call site holds `&mut World`). A no-op when no transition
/// is present. See REN-D18-NEW-02 / #2511.
fn collapse_weather_transition(world: &mut World) {
    let in_flight = world
        .try_resource::<WeatherTransitionRes>()
        .is_some_and(|tr| !tr.done);
    if in_flight {
        crate::systems::weather::promote_weather_transition_target(world);
    }
    world.remove_resource::<WeatherTransitionRes>();
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
///
/// REN-D18-01: `GameTimeRes` is seeded only on the first call (mirroring
/// `CloudSimState` immediately below and the WTHR branch's own first-load
/// guard in `apply_worldspace_weather`) — an unconditional insert here
/// snapped the global clock back to `initial_game_time()`'s fixed hour on
/// every transition into a climateless worldspace, popping time-of-day /
/// sun direction / fog mid-session on corrupt ESMs, climate-less mod
/// worldspaces, and synthetic test cells (vanilla content always resolves
/// a CLMT and never re-enters this branch after first load).
pub(crate) fn insert_procedural_fallback_resources(world: &mut World, sun_dir: [f32; 3]) {
    world.insert_resource(crate::env_translate::procedural_fallback_cell_lighting(
        sun_dir,
    ));
    world.insert_resource(crate::env_translate::procedural_fallback_sky(sun_dir));
    // #803 — same survives-transitions pattern as the WTHR path: seed
    // CloudSimState only on the first exterior load.
    if world.try_resource::<CloudSimState>().is_none() {
        world.insert_resource(CloudSimState::default());
    }
    world.insert_resource(crate::env_translate::procedural_fallback_weather());
    ensure_game_time(world);
}

/// How much of the initial exterior radius must be ready before control
/// returns to the render loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExteriorBootstrapMode {
    /// Wait only for the arrival cell. The rest of the radius stays queued
    /// and is applied by the normal per-frame streaming budget.
    ForegroundFirst,
    /// Wait for every requested cell. Used by deterministic benchmarks so
    /// measurement never includes initial-radius population.
    FullRadius,
}

impl ExteriorBootstrapMode {
    pub(crate) fn from_cli_args(args: &[String]) -> Self {
        if args.iter().any(|arg| arg == "--bench-frames") {
            Self::FullRadius
        } else {
            Self::ForegroundFirst
        }
    }
}

fn bootstrap_waiting(
    mode: ExteriorBootstrapMode,
    pending: &std::collections::HashMap<(i32, i32), u64>,
    center: (i32, i32),
) -> bool {
    match mode {
        ExteriorBootstrapMode::ForegroundFirst => pending.contains_key(&center),
        ExteriorBootstrapMode::FullRadius => !pending.is_empty(),
    }
}

/// Stream the initial radius around the player's spawn cell. Returns the
/// camera-spawn point (center cell terrain mid-height + 200 units, or
/// `Vec3::ZERO` when there's no center cell).
///
/// Every cell is dispatched through the streaming worker. In
/// [`ExteriorBootstrapMode::ForegroundFirst`] this blocks only for the
/// center cell—the exterior equivalent of one coherent interior-cell
/// transaction—then leaves the surrounding radius to the steady-state
/// per-frame budget. [`ExteriorBootstrapMode::FullRadius`] preserves the
/// old deterministic benchmark behavior.
///
/// Both bootstrap modes consume payloads through
/// [`consume_streaming_payload`]. Its synchronous driver and steady-state's
/// resumable driver share the underlying import helper, exterior apply job,
/// reference pipeline, cache semantics, temporal invalidation, and generation
/// gate.
pub(crate) fn stream_initial_radius(
    world: &mut World,
    ctx: &mut VulkanContext,
    state: &mut WorldStreamingState,
    cx: i32,
    cy: i32,
    mode: ExteriorBootstrapMode,
) -> Vec3 {
    if state.persistent_root.is_none() && state.persistent_apply.is_none() {
        let wctx = state.wctx.clone();
        if let Some(job) = crate::cell_loader::begin_worldspace_persistent_cell(
            wctx.as_ref(),
            (cx, cy),
            state.radius_load,
            world,
        ) {
            state.persistent_root = Some(job.cell_root());
            state.persistent_apply = Some(job);
        }
    }
    if mode == ExteriorBootstrapMode::FullRadius {
        if let Some(job) = state.persistent_apply.take() {
            let wctx = state.wctx.clone();
            let mut budget = crate::cell_loader::FrameTimeBudget::unlimited();
            match job.advance(
                wctx.as_ref(),
                world,
                ctx,
                state.tex_provider.as_ref(),
                Some(&mut state.mat_provider),
                &mut budget,
            ) {
                crate::cell_loader::PersistentCellApplyProgress::Complete => {}
                crate::cell_loader::PersistentCellApplyProgress::Pending(_) => {
                    unreachable!("an unlimited persistent-CELL budget cannot yield")
                }
            }
        }
    }

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
    state.queue_loads(deltas.to_load, cached_keys);

    // Block for the selected foreground contract. Loads normally arrive
    // closest-first because the delta list is ordered and the worker is FIFO,
    // but the loop keys its stop condition to the center coordinate rather
    // than relying on that implementation detail.
    let mut center = Vec3::ZERO;
    let center_coord = (cx, cy);
    while bootstrap_waiting(mode, &state.pending, center_coord) {
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
        if let StreamingPayloadOutcome::Applied {
            coord,
            center: Some(cell_center),
        } = consume_streaming_payload(world, ctx, state, payload)
        {
            if coord == center_coord {
                center = cell_center;
            }
        }
    }
    if mode == ExteriorBootstrapMode::ForegroundFirst && !state.pending.is_empty() {
        log::info!(
            "Exterior foreground ready at ({cx},{cy}); {} peripheral cells continue streaming",
            state.pending.len(),
        );
    }

    // EXAL progressive bootstrap: interactive loads return as soon as the
    // foreground cell is coherent, then the normal per-frame driver fills
    // terrain and the game-specific object LOD ring on idle frames. Bench
    // mode preserves the deterministic contract by settling every ring here.
    match mode {
        ExteriorBootstrapMode::ForegroundFirst => {
            state.lod_reconcile_pending = true;
        }
        ExteriorBootstrapMode::FullRadius => {
            // Bootstrap is the deterministic full-radius contract: no attempt
            // cap and no deadline, so the ring is complete before the first frame.
            let progress = reconcile_lod_rings(world, ctx, state, (cx, cy), usize::MAX, None);
            debug_assert!(
                progress.complete,
                "unlimited LOD bootstrap must settle every provider"
            );
            state.lod_reconcile_pending = false;
        }
    }

    center
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_bootstrap_waits_only_for_foreground_cell() {
        let center = (4, -2);
        let mut pending = std::collections::HashMap::from([(center, 1), ((5, -2), 2)]);
        assert!(bootstrap_waiting(
            ExteriorBootstrapMode::ForegroundFirst,
            &pending,
            center,
        ));

        pending.remove(&center);
        assert!(
            !bootstrap_waiting(ExteriorBootstrapMode::ForegroundFirst, &pending, center,),
            "peripheral requests must not extend the interactive loading gate"
        );
        assert_eq!(pending.len(), 1, "peripheral work remains queued");
    }

    #[test]
    fn deterministic_bootstrap_waits_for_the_full_radius() {
        let center = (4, -2);
        let mut pending = std::collections::HashMap::from([(center, 1), ((5, -2), 2)]);
        pending.remove(&center);
        assert!(bootstrap_waiting(
            ExteriorBootstrapMode::FullRadius,
            &pending,
            center,
        ));
        pending.clear();
        assert!(!bootstrap_waiting(
            ExteriorBootstrapMode::FullRadius,
            &pending,
            center,
        ));
    }

    #[test]
    fn bench_cli_selects_full_radius_bootstrap() {
        let interactive = vec!["byroredux".to_string(), "--grid".to_string()];
        assert_eq!(
            ExteriorBootstrapMode::from_cli_args(&interactive),
            ExteriorBootstrapMode::ForegroundFirst
        );

        let bench = vec![
            "byroredux".to_string(),
            "--bench-frames".to_string(),
            "60".to_string(),
        ];
        assert_eq!(
            ExteriorBootstrapMode::from_cli_args(&bench),
            ExteriorBootstrapMode::FullRadius
        );
    }

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

    /// #1770 — climateless-worldspace transition: the `else` (procedural
    /// fallback) branch of `apply_worldspace_weather` installs a texture-less
    /// sky, so NONE of the prior worldspace's handles are shared with the new
    /// one and every real prior handle must be released. Pins the release set
    /// the else-branch fix now drops (the branch previously released nothing,
    /// leaking up to 5 textures per such transition). The GPU call site is a
    /// structural mirror of the WTHR branch covered above.
    #[test]
    fn sky_release_climateless_transition_frees_all_real_handles() {
        let fallback = 99u32;
        let prev = Some([10, 11, 12, 13, 14]); // a fully-authored prior worldspace sky
        let mut got = sky_textures_to_release(prev, fallback);
        got.sort_unstable();
        assert_eq!(
            got,
            vec![10, 11, 12, 13, 14],
            "a transition into a texture-less procedural sky must release every prior sky handle",
        );
    }

    /// REN-D18-01 — a second (or later) call to `insert_procedural_fallback_resources`
    /// within the same session (re-entering a climateless worldspace mid-session)
    /// must NOT reset `GameTimeRes` back to its initial hour. Pre-fix, the insert
    /// was unconditional, popping time-of-day every such transition even though
    /// the neighboring `CloudSimState` insert already used the correct
    /// survives-transitions guard.
    #[test]
    fn insert_procedural_fallback_resources_preserves_advanced_game_time() {
        let mut world = World::new();
        let mut expected = GameTimeRes::new(18.0, 90.0);
        expected.advance_hours(24.0);
        expected.pause();
        world.insert_resource(expected);

        insert_procedural_fallback_resources(&mut world, [0.0, 1.0, 0.0]);

        let time = world
            .try_resource::<GameTimeRes>()
            .expect("insert_procedural_fallback_resources must leave a GameTimeRes installed");
        assert_eq!(
            *time, expected,
            "a later call must not reset an already-advanced game clock (REN-D18-01)"
        );
    }

    #[test]
    fn bootstrap_hour_prefers_the_persistent_live_clock() {
        let mut world = World::new();
        world.insert_resource(GameTimeRes::new(21.75, 30.0));
        assert_eq!(bootstrap_game_hour(&world), 21.75);
    }

    /// The very first call (no prior `GameTimeRes`) must still seed one —
    /// otherwise `weather_system` early-returns forever on a climateless
    /// worldspace with no clock to read.
    #[test]
    fn insert_procedural_fallback_resources_seeds_game_time_on_first_call() {
        let mut world = World::new();
        assert!(world.try_resource::<GameTimeRes>().is_none());

        insert_procedural_fallback_resources(&mut world, [0.0, 1.0, 0.0]);

        assert!(
            world.try_resource::<GameTimeRes>().is_some(),
            "first entry into a climateless worldspace must still seed GameTimeRes"
        );
    }

    /// Minimal `WeatherDataRes` fixture for `collapse_weather_transition`
    /// tests, distinguished by `fog[1]` (day-far) and `wind_speed` so a
    /// wrongly-applied or wrongly-skipped promotion is easy to detect.
    fn mk_weather(day_far: f32, wind_speed: u8) -> WeatherDataRes {
        WeatherDataRes {
            sky_colors: [[[0.0_f32; 3]; 6]; 10],
            fog: [100.0, day_far, 200.0, 30000.0],
            fog_media: [
                crate::fog::FogMedium::from_legacy_ramp(100.0, day_far, None),
                crate::fog::FogMedium::from_legacy_ramp(200.0, 30000.0, None),
            ],
            tod_hours: [6.0, 10.0, 18.0, 22.0],
            skyrim_dalc_per_tod: None,
            wind_speed,
        }
    }

    /// #2511 / REN-D18-NEW-02 — a transition still in flight (`done ==
    /// false`) when a second worldspace change lands must be finished
    /// early: `target` promotes into the live `WeatherDataRes` and the
    /// transition resource is fully removed (not just latched `done`),
    /// so neither of the two documented failure modes (backward colour
    /// pop, or blending toward a since-replaced target forever) can
    /// occur.
    #[test]
    fn collapse_weather_transition_promotes_in_flight_target_and_removes_resource() {
        let mut world = World::new();
        world.insert_resource(mk_weather(60000.0, 0));
        world.insert_resource(WeatherTransitionRes {
            target: mk_weather(12345.0, 7),
            elapsed_secs: 4.0, // mid-fade, well short of duration_secs
            duration_secs: 8.0,
            done: false,
        });

        collapse_weather_transition(&mut world);

        let wd = world
            .try_resource::<WeatherDataRes>()
            .expect("WeatherDataRes must survive collapse");
        assert_eq!(
            wd.fog[1], 12345.0,
            "in-flight target must be promoted into WeatherDataRes, not left at the stale source"
        );
        assert_eq!(wd.wind_speed, 7, "wind_speed must promote alongside fog");
        assert!(
            world.try_resource::<WeatherTransitionRes>().is_none(),
            "an in-flight transition must be fully removed, not left resident as a dormant \
             `done` latch — otherwise the NEXT worldspace change collapses this stale copy again"
        );
    }

    /// A transition already latched `done == true` (a natural completion
    /// that already promoted its target on a prior frame) must be
    /// removed WITHOUT re-promoting — re-applying it would silently
    /// clobber whatever the live `WeatherDataRes` has moved on to since.
    #[test]
    fn collapse_weather_transition_does_not_reapply_a_done_transition() {
        let mut world = World::new();
        world.insert_resource(mk_weather(60000.0, 0));
        world.insert_resource(WeatherTransitionRes {
            target: mk_weather(99999.0, 9), // stale target — must NOT reappear
            elapsed_secs: 8.0,
            duration_secs: 8.0,
            done: true,
        });

        collapse_weather_transition(&mut world);

        let wd = world
            .try_resource::<WeatherDataRes>()
            .expect("WeatherDataRes must survive collapse");
        assert_eq!(
            wd.fog[1], 60000.0,
            "a dormant `done` transition must not be re-promoted over the live weather"
        );
        assert!(
            world.try_resource::<WeatherTransitionRes>().is_none(),
            "a dormant transition must still be removed so it can't be collapsed again later"
        );
    }

    /// No transition present (the common case, or first-ever worldspace
    /// load with no `WeatherDataRes` yet either) — collapse must be a
    /// silent no-op, not a panic.
    #[test]
    fn collapse_weather_transition_is_a_noop_without_a_transition() {
        let mut world = World::new();
        collapse_weather_transition(&mut world);
        assert!(world.try_resource::<WeatherDataRes>().is_none());
        assert!(world.try_resource::<WeatherTransitionRes>().is_none());

        world.insert_resource(mk_weather(60000.0, 0));
        collapse_weather_transition(&mut world);
        let wd = world.try_resource::<WeatherDataRes>().unwrap();
        assert_eq!(
            wd.fog[1], 60000.0,
            "an untouched WeatherDataRes must be left as-is"
        );
    }
}
