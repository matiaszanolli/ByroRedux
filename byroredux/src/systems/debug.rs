//! Debug / demo systems — spinning cube, stats logging.

use byroredux_core::ecs::{
    CpuFrameTimings, DebugStats, DeltaTime, SkinCoverageStats, TotalTime, Transform, World,
};
use byroredux_core::math::Quat;

use crate::components::Spinning;

/// Rotates only entities marked with the Spinning component.
pub(crate) fn spin_system(world: &World, dt: f32) {
    if let Some((sq, mut tq)) = world.query_2_mut::<Spinning, Transform>() {
        for (entity, _) in sq.iter() {
            if let Some(transform) = tq.get_mut(entity) {
                let rotation = Quat::from_rotation_y(dt * 1.0) * Quat::from_rotation_x(dt * 0.3);
                transform.rotation = rotation * transform.rotation;
            }
        }
    }
}

/// A single frame slower than this (milliseconds) is logged at WARN with
/// the GPU per-pass breakdown — a frame anywhere near the ~2 s OS/NVIDIA
/// GPU watchdog (TDR) is one device-loss away from killing the renderer,
/// so surfacing the heavy frames *before* the fatal one is how we catch
/// the culprit pass. 200 ms ≈ 5 fps: well below the watchdog but far above
/// any healthy frame, so this only fires on genuine hitches (cell-load
/// spikes, RT-cost balloons), not steady state. See `docs/engine/watal.md`
/// §0 (the "near water" device-loss was a TDR, not a water fault).
const SLOW_FRAME_WARN_MS: f32 = 200.0;

/// Did this frame cross a whole-wall-clock-second boundary? `total` is
/// `TotalTime` AFTER this frame's `dt` was added; `prev = total - dt` is
/// what it was last frame. Shared by every once-per-second diagnostic
/// cadence: `log_stats_system`'s summary line below, and `about_to_wait`'s
/// `meshes_in_use` / `textures_in_use` dedup-walk throttle (PERF-D1-NEW-01
/// / #1801) — both want "roughly 1 Hz" rather than a frame-count modulo
/// that drifts with framerate. Note this is a per-*wall-clock*-second
/// cadence, not per-frame: from a fresh `TotalTime` start it doesn't fire
/// until `total` first reaches 1.0, same as it always has for
/// `log_stats_system`'s first summary line.
pub(crate) fn crosses_one_second_boundary(total: f32, dt: f32) -> bool {
    let prev = total - dt;
    prev < 0.0 || total.floor() != prev.floor()
}

/// Format the per-pass GPU timer breakdown from [`SkinCoverageStats`] (the
/// canonical landing pad for every `GpuPerFrameTimers` bracket, #1194).
/// Values lag the live frame by `MAX_FRAMES_IN_FLIGHT` (≈2 frames) — so on
/// a *sudden* single-frame spike the numbers reflect the moments just
/// before it; the diagnostic value is the trend as a heavy area is
/// approached. `main_render` is the per-fragment RT loop (shadow/GI/
/// reflection rays + ReSTIR), `tlas` the acceleration-structure rebuild on
/// cell-load frames, `svgf` the à-trous denoiser.
fn gpu_breakdown(cov: &SkinCoverageStats) -> String {
    format!(
        "main_render={:.1} tlas={:.1} svgf={:.1} composite={:.1} cluster_cull={:.1} \
         ssao={:.1} bloom={:.1} caustic={:.1} volumetrics={:.1} skin={:.1} blas_refit={:.1} taa={:.1}",
        cov.gpu_main_render_ms, cov.gpu_tlas_build_ms, cov.gpu_svgf_ms,
        cov.gpu_composite_ms, cov.gpu_cluster_cull_ms, cov.gpu_ssao_ms,
        cov.gpu_bloom_ms, cov.gpu_caustic_splat_ms, cov.gpu_volumetrics_ms,
        cov.gpu_skin_dispatch_ms, cov.gpu_skin_blas_refit_ms, cov.gpu_taa_ms,
    )
}

/// Format the CPU-side per-phase wall-clock breakdown from
/// [`CpuFrameTimings`] (populated unconditionally every frame). This is the
/// decisive localizer for a multi-second frame whose GPU passes are cheap:
/// `fence_wait` large ⇒ the GPU is hung on a PRIOR submission (the host is
/// blocked in `wait_for_fences` until the driver resets); `atw_post` large
/// with small `fence_wait`/`rof_*` ⇒ the stall is CPU cell-load
/// (`step_streaming` + uploads, which `atw_post` brackets); `acquire` /
/// `submit_present` large ⇒ compositor / present stall. (WATAL §0 hunt.)
fn cpu_breakdown(t: &CpuFrameTimings) -> String {
    format!(
        "fence_wait={:.0} acquire={:.0} submit_present={:.0} ssbo_build={:.0} \
         tlas_build={:.0} cmd_record={:.0} rof_pre_draw={:.0} rof_draw_call={:.0} \
         rof_post_draw={:.0} atw_pre={:.0} atw_scheduler={:.0} atw_post={:.0}",
        t.fence_wait_ms, t.acquire_ms, t.submit_present_ms, t.ssbo_build_ms,
        t.tlas_build_ms, t.cmd_record_ms, t.rof_pre_draw_ms, t.rof_draw_call_ms,
        t.rof_post_draw_ms, t.atw_pre_ms, t.atw_scheduler_ms, t.atw_post_ms,
    )
}

/// Logs engine stats once per second using DebugStats; additionally warns
/// on any single frame slower than [`SLOW_FRAME_WARN_MS`] with the GPU
/// per-pass breakdown (TDR-hunting instrumentation, WATAL §0).
///
/// Writes at `log::info!` / target `engine::stats`, so `env_logger`'s
/// default filter (info) surfaces it in release builds without needing
/// `--debug`. Users who want a quieter console can set
/// `RUST_LOG=warn` or target-filter
/// `RUST_LOG=info,engine::stats=warn`. See #366.
pub(crate) fn log_stats_system(world: &World, _dt: f32) {
    let total = world.resource::<TotalTime>().0;
    let dt = world.resource::<DeltaTime>().0;

    let stats = world.resource::<DebugStats>();
    // GPU per-pass timers live on `SkinCoverageStats` (absent on the
    // headless / no-renderer demo path — then the breakdown is skipped).
    let gpu = world
        .try_resource::<SkinCoverageStats>()
        .map(|cov| gpu_breakdown(&cov));
    // CPU per-phase wall-clock — the decisive localizer for a slow frame
    // whose GPU passes are cheap (is the stall a GPU-hang fence_wait or
    // CPU cell-load atw_post?).
    let cpu = world
        .try_resource::<CpuFrameTimings>()
        .map(|t| cpu_breakdown(&t));

    // Per-frame slow-frame warning — fires on hitches regardless of the
    // once-per-second boundary, so a cell-load / RT-cost spike that walks
    // the frame toward the GPU watchdog is visible right before a crash.
    // Skip the first few frames: the initial scene load (parse ESM + load
    // the startup cell ring) is counted as one giant "frame 0" delta
    // (seconds), with all per-pass timers still zero — a benign
    // false-positive, not an in-game hitch.
    if stats.frame_time_ms > SLOW_FRAME_WARN_MS && stats.frame_index() > 3 {
        log::warn!(
            target: "engine::stats",
            "SLOW FRAME dt={:.1}ms (watchdog ~2000ms)\n  gpu[lag~2f]: {}\n  cpu_ms: {}",
            stats.frame_time_ms,
            gpu.as_deref().unwrap_or("unavailable"),
            cpu.as_deref().unwrap_or("unavailable"),
        );
    }

    if crosses_one_second_boundary(total, dt) {
        // #1258 — `draws=N/Mb/Kc` = N input DrawCommands / M post-merge
        // batches / K actual GPU calls. Pre-fix this was a single `draws=N`
        // that read like a GPU call count but stored the batcher input.
        // #1284 — `skin=L/M+S` exposes SkinSlotPool live-slot count,
        // cap, and cumulative overflow demand so cap-sizing decisions
        // are data-driven. `S=0` is the healthy state.
        log::info!(
            target: "engine::stats",
            "fps={:.0} avg={:.0} dt={:.2}ms entities={} meshes={} textures={} draws={}/{}b/{}c skin={}/{}+{}",
            stats.fps, stats.avg_fps(), stats.frame_time_ms,
            stats.entity_count, stats.mesh_count, stats.texture_count,
            stats.draw_command_count, stats.batch_count, stats.indirect_call_count,
            stats.skin_pool_live, stats.skin_pool_max, stats.skin_pool_overflow_attempts,
        );
        // GPU per-pass breakdown on its own line so the primary stats line
        // stays parseable by existing log scrapers. Surfaces the
        // RT-loop / TLAS / denoiser cost trend that the WATAL §0 TDR
        // analysis needs to confirm the device-loss culprit.
        if let Some(ref gpu) = gpu {
            log::info!(target: "engine::stats", "gpu_ms: {gpu}");
        }
        // CPU per-phase breakdown on the once-per-second line too (not just
        // the per-frame SLOW-FRAME warn, which can be missed). `fence_wait`
        // large ⇒ a GPU submission hung; `atw_post`/`ssbo_build` large with
        // small `fence_wait` ⇒ CPU cell-load stall. WATAL §0 device-loss hunt.
        if let Some(ref cpu) = cpu {
            log::info!(target: "engine::stats", "cpu_ms: {cpu}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The very first frame from a fresh `TotalTime` start (`total ==
    /// dt`, so `prev == 0.0`) does NOT cross a boundary — matches
    /// `log_stats_system`'s existing behavior of not printing its first
    /// summary line until wall-clock time actually reaches 1.0s.
    #[test]
    fn first_frame_does_not_refresh() {
        assert!(!crosses_one_second_boundary(0.1, 0.1));
    }

    /// Mid-second: two frames that don't cross a whole-second boundary
    /// must not refresh.
    #[test]
    fn same_second_does_not_refresh() {
        assert!(!crosses_one_second_boundary(1.2, 0.1));
        assert!(!crosses_one_second_boundary(1.9, 0.05));
    }

    /// Crossing from e.g. 0.95s to 1.02s must refresh exactly once, on
    /// the frame that lands past the boundary.
    #[test]
    fn crossing_a_second_boundary_refreshes() {
        assert!(crosses_one_second_boundary(1.02, 0.07));
    }

    /// A frame landing exactly ON a whole second must refresh (matches
    /// `log_stats_system`'s pre-existing `!=` comparison semantics).
    #[test]
    fn landing_exactly_on_the_boundary_refreshes() {
        assert!(crosses_one_second_boundary(2.0, 0.03));
    }

    /// PERF-D1-NEW-01 / #1801 — over many frames at a plausible
    /// variable-dt cadence, the boundary must fire exactly once per
    /// whole second crossed, not zero and not more than once (which
    /// would defeat the point of throttling to ~1 Hz).
    #[test]
    fn fires_once_per_second_over_many_frames() {
        let mut total = 0.0f32;
        let dt = 1.0 / 63.0; // an irregular frame time, not a clean divisor
        let mut refresh_count = 0u32;
        let seconds_to_simulate = 5;
        while total < seconds_to_simulate as f32 {
            let new_total = total + dt;
            if crosses_one_second_boundary(new_total, dt) {
                refresh_count += 1;
            }
            total = new_total;
        }
        assert_eq!(
            refresh_count, seconds_to_simulate,
            "expected exactly one refresh per whole second crossed"
        );
    }
}
