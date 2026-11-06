//! Per-frame stats logging and the debug-UI metrics sampler.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use crate::systems::{log_stats_system, metrics_sample_system, MetricsState};
use byroredux_core::ecs::{
    Access, DebugStats, DeltaTime, MetricsSnapshot, Scheduler, SkinCoverageStats, Stage, TotalTime,
};

/// Per-frame stats logging and the debug-UI metrics sampler.
pub(super) fn register_diagnostic_systems(scheduler: &mut Scheduler) {
    scheduler.add_to_with_access(
        Stage::Late,
        log_stats_system,
        Access::new()
            .reads_resource::<TotalTime>()
            .reads_resource::<DeltaTime>()
            .reads_resource::<DebugStats>()
            // #2389 / ECS-D5-01 — the `want_breakdown` arm of the body
            // reads two more resources (`systems/debug.rs`). The runtime
            // gate is invisible to the analyzer, so both must be declared
            // unconditionally — same shape as #1787's `ContactConfig`.
            .reads_resource::<SkinCoverageStats>()
            .reads_resource::<byroredux_core::ecs::CpuFrameTimings>()
            .reads_resource::<byroredux_core::ecs::SchedulerSystemTimings>(),
    );
    // Debug-UI metrics sampler — throttles itself to ~2 Hz, so the
    // per-frame cost is a single resource read + compare. On a
    // sample tick it walks sysinfo + the gpu-allocator block list
    // and writes the snapshot read by the protocol / TUI / egui
    // overlay.
    scheduler.add_to_with_access(
        Stage::Late,
        metrics_sample_system,
        Access::new()
            .reads_resource::<TotalTime>()
            .reads_resource::<SkinCoverageStats>()
            .reads_resource::<byroredux_renderer::vulkan::allocator::AllocatorResource>()
            .reads_resource::<byroredux_renderer::vulkan::allocator::GpuMemoryBudget>()
            // #2389 / ECS-D5-01 — the sample tick also reads the CPU
            // frame timings and the per-system scheduler timings
            // (`systems/metrics.rs`); both were missing. The latter is
            // the likeliest future write target in this stage (an
            // in-ECS profiler), which is exactly the pairing the
            // analyzer would have waved through.
            .reads_resource::<byroredux_core::ecs::CpuFrameTimings>()
            .reads_resource::<byroredux_core::ecs::SchedulerSystemTimings>()
            .writes_resource::<MetricsState>()
            .writes_resource::<MetricsSnapshot>(),
    );
}
