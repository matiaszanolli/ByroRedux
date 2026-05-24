//! Aggregated runtime metrics snapshot — drives the debug-UI overlay
//! and the `Metrics` debug-protocol request.
//!
//! Populated by the binary's `metrics_sample_system` at a fixed cadence
//! (default 2 Hz). Reads are cheap RwLock-guarded resource accesses
//! exactly like every other ECS resource — callers don't need to know
//! about the throttle.
//!
//! Why a pure-data Resource lives in core: the protocol crate and the
//! eventual egui overlay both need to read these fields without taking
//! a dependency on the renderer (where the sampling actually happens).
//! Core has no `sysinfo` / `ash` deps — only the binary that owns the
//! renderer + sysinfo can fill the snapshot.

use std::collections::BTreeMap;

use super::resource::Resource;

/// Snapshot of runtime engine metrics, refreshed at ~2 Hz by the
/// `metrics_sample_system`.
///
/// All `_mb` fields are in mebibytes. `gpu_pass_ms` maps a per-pass
/// label (`"skin"`, `"skin_blas_refit"`, `"taa"`, …) to the elapsed
/// GPU time read out of `SkinCoverageStats` — one
/// `MAX_FRAMES_IN_FLIGHT` cycle behind the live frame because that's
/// the pipeline lag for timestamp readback (see #1194).
#[derive(Debug, Default, Clone)]
pub struct MetricsSnapshot {
    /// Unix-epoch seconds at which the snapshot was filled. Zero
    /// before the first sample. Used by the debug UI to display
    /// "stale snapshot" warnings when the renderer hangs.
    pub sampled_at_secs: u64,
    /// Whole-process CPU usage as reported by sysinfo (0..N*100 across
    /// N logical cores). Zero before sysinfo has settled — its CPU
    /// reading needs a prior sample to differentiate against.
    pub cpu_pct: f32,
    /// System-wide used physical memory in MB.
    pub ram_used_mb: u64,
    /// System-wide total physical memory in MB.
    pub ram_total_mb: u64,
    /// This process's resident set size in MB.
    pub process_ram_mb: u64,
    /// GPU memory currently allocated by gpu-allocator (sum of
    /// in-flight allocations). Zero before renderer boot.
    pub vram_used_mb: u64,
    /// GPU memory currently reserved by gpu-allocator (sum of block
    /// sizes — `vram_used_mb` is always ≤ this). Zero before boot.
    pub vram_reserved_mb: u64,
    /// Total VRAM exposed by the physical device — sum of
    /// `DEVICE_LOCAL` heap capacities. Constant after device pick;
    /// zero when no physical device has been selected yet.
    pub vram_budget_mb: u64,
    /// Per-pass GPU elapsed time in milliseconds. Map keys are
    /// the pass-name strings (`"skin"`, `"skin_blas_refit"`,
    /// `"taa"` today; extensible without breaking the wire
    /// protocol).
    pub gpu_pass_ms: BTreeMap<String, f32>,
    /// CPU-side wall-clock breakdown of `draw_frame`, in
    /// milliseconds. Surfaces operations the GPU TIMESTAMP
    /// brackets in `gpu_pass_ms` can't see — the fence wait at
    /// the top of `draw_frame`, the queue submit + present at
    /// the bottom, and CPU work for cmd-buffer recording. When
    /// the bracketed GPU work sums to less than wall frame time,
    /// the missing time hides here. Phase 8 of the debug-UI plan
    /// added this to diagnose a 311 ms gap between summed GPU
    /// pass times (78 ms) and wall frame time (389 ms) on a
    /// Skyrim interior.
    pub cpu_pass_ms: BTreeMap<String, f32>,
    /// Per-system wall-time of the most recent `Scheduler::run`,
    /// sorted descending by milliseconds. Phase 11 — added to
    /// localize the system inside `atw_scheduler_ms` that
    /// dominates the frame budget when the scheduler reads ~500 ms.
    /// Entries with `0.0` ms (didn't run or sub-microsecond) are
    /// included for completeness; consumers can filter by their
    /// own threshold.
    pub top_systems_ms: Vec<(String, f32)>,
}

impl Resource for MetricsSnapshot {}
