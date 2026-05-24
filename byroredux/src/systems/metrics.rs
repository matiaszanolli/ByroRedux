//! Aggregates CPU / RAM / VRAM / GPU pass-time metrics into
//! [`MetricsSnapshot`] at a fixed cadence (2 Hz default).
//!
//! The system is registered in the Late stage. It returns immediately
//! when called inside the throttle window, so per-frame overhead is a
//! single resource read + compare. On the sample tick it walks the
//! sysinfo `System` (one `/proc` traversal on Linux, one snapshot
//! enumeration on Windows) and the gpu-allocator block list (cheap
//! O(blocks) iteration), so the cost is bounded.
//!
//! Why a resource, not a thread-local: rayon-scheduled systems can
//! land on any thread in the pool, and sysinfo's CPU reading needs the
//! prior tick to compute the delta. A thread-local would reset CPU% to
//! zero whenever the scheduler moved the system to a fresh worker
//! thread. The resource keeps the state pinned to the world.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use byroredux_core::ecs::{
    CpuFrameTimings, MetricsSnapshot, Resource, SchedulerSystemTimings, SkinCoverageStats,
    TotalTime, World,
};
use byroredux_renderer::vulkan::allocator::{AllocatorResource, GpuMemoryBudget};
use sysinfo::{Pid, System};

/// Default 2 Hz cadence. Tuned so the debug UI updates fast enough to
/// read like a live dashboard without burning CPU on per-frame
/// `/proc` walks. Operators wanting tighter / looser sampling can
/// adjust by editing this constant — the system has no runtime config
/// hook today because there's no use case yet.
const SAMPLE_PERIOD_SECS: f32 = 0.5;

/// Persistent state for the metrics sampler. Held in the world as a
/// resource so the cross-frame `sysinfo::System` retains the prior CPU
/// sample needed to compute `%`-since-last-tick.
pub struct MetricsState {
    sys: Mutex<System>,
    pid: Option<Pid>,
    last_sample_secs: f32,
}

impl Resource for MetricsState {}

impl MetricsState {
    pub fn new() -> Self {
        Self {
            sys: Mutex::new(System::new()),
            // PID lookup is infallible on every supported OS; the
            // `Option` is sysinfo's idiom, not a real failure mode.
            pid: sysinfo::get_current_pid().ok(),
            last_sample_secs: f32::NEG_INFINITY,
        }
    }
}

impl Default for MetricsState {
    fn default() -> Self {
        Self::new()
    }
}

/// Late-stage system: refresh the host/GPU metrics resources at the
/// throttle cadence.
pub fn metrics_sample_system(world: &World, _dt: f32) {
    let now_secs = world.resource::<TotalTime>().0;

    // Throttle: acquire MetricsState mutably, gate, then collect the
    // sysinfo readings inside the same write scope so the lock window
    // covers exactly one `refresh + read` pair.
    let (cpu_pct, ram_used_b, ram_total_b, process_ram_b) = {
        let mut state = world.resource_mut::<MetricsState>();
        if now_secs - state.last_sample_secs < SAMPLE_PERIOD_SECS {
            return;
        }
        state.last_sample_secs = now_secs;
        let pid = state.pid;
        let mut sys = state.sys.lock().unwrap_or_else(|e| e.into_inner());
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let cpu = sys.global_cpu_info().cpu_usage();
        let ram_used = sys.used_memory();
        let ram_total = sys.total_memory();
        let proc_ram = if let Some(pid) = pid {
            sys.refresh_process(pid);
            sys.process(pid).map(|p| p.memory()).unwrap_or(0)
        } else {
            0
        };
        (cpu, ram_used, ram_total, proc_ram)
    };

    let (vram_used_b, vram_reserved_b) =
        if let Some(alloc_res) = world.try_resource::<AllocatorResource>() {
            let alloc = alloc_res.0.lock().unwrap_or_else(|e| e.into_inner());
            let report = alloc.generate_report();
            // gpu-allocator 0.28: `total_reserved_bytes` →
            // `total_capacity_bytes` (same semantics).
            (report.total_allocated_bytes, report.total_capacity_bytes)
        } else {
            (0, 0)
        };

    let vram_budget_b = world
        .try_resource::<GpuMemoryBudget>()
        .map(|b| b.total_vram_bytes)
        .unwrap_or(0);

    let mut gpu_pass_ms: BTreeMap<String, f32> = BTreeMap::new();
    if let Some(cov) = world.try_resource::<SkinCoverageStats>() {
        // Order in the BTreeMap is alphabetical — the UI renders
        // the rows in that order, which roughly groups related
        // passes (e.g. `skin*` together). Adding the four
        // debug-UI-Phase-6 brackets surfaces the main-render
        // pathology the 540 ms / 1 FPS report flagged.
        gpu_pass_ms.insert("bloom".to_string(), cov.gpu_bloom_ms);
        gpu_pass_ms.insert("caustic_splat".to_string(), cov.gpu_caustic_splat_ms);
        gpu_pass_ms.insert("cluster_cull".to_string(), cov.gpu_cluster_cull_ms);
        gpu_pass_ms.insert("composite".to_string(), cov.gpu_composite_ms);
        gpu_pass_ms.insert("main_render".to_string(), cov.gpu_main_render_ms);
        gpu_pass_ms.insert("skin".to_string(), cov.gpu_skin_dispatch_ms);
        gpu_pass_ms.insert("skin_blas_refit".to_string(), cov.gpu_skin_blas_refit_ms);
        gpu_pass_ms.insert("ssao".to_string(), cov.gpu_ssao_ms);
        gpu_pass_ms.insert("svgf".to_string(), cov.gpu_svgf_ms);
        gpu_pass_ms.insert("taa".to_string(), cov.gpu_taa_ms);
        gpu_pass_ms.insert("tlas_build".to_string(), cov.gpu_tlas_build_ms);
        gpu_pass_ms.insert("volumetrics".to_string(), cov.gpu_volumetrics_ms);
    }

    let mut cpu_pass_ms: BTreeMap<String, f32> = BTreeMap::new();
    if let Some(cpu) = world.try_resource::<CpuFrameTimings>() {
        // Names match the `FrameTimings` field names so a reader
        // can grep one term across both the bench output and the
        // overlay panel. Phase 10 atw_* split `between_frames`
        // into pre / scheduler / post — should sum close to
        // between_frames since about_to_wait runs entirely
        // inside that gap.
        cpu_pass_ms.insert("acquire".to_string(), cpu.acquire_ms);
        cpu_pass_ms.insert("atw_post".to_string(), cpu.atw_post_ms);
        cpu_pass_ms.insert("atw_pre".to_string(), cpu.atw_pre_ms);
        cpu_pass_ms.insert("atw_scheduler".to_string(), cpu.atw_scheduler_ms);
        cpu_pass_ms.insert("between_frames".to_string(), cpu.between_frames_ms);
        cpu_pass_ms.insert("cmd_record".to_string(), cpu.cmd_record_ms);
        cpu_pass_ms.insert("fence_wait".to_string(), cpu.fence_wait_ms);
        cpu_pass_ms.insert("rof_draw_call".to_string(), cpu.rof_draw_call_ms);
        cpu_pass_ms.insert("rof_post_draw".to_string(), cpu.rof_post_draw_ms);
        cpu_pass_ms.insert("rof_pre_draw".to_string(), cpu.rof_pre_draw_ms);
        cpu_pass_ms.insert("ssbo_build".to_string(), cpu.ssbo_build_ms);
        cpu_pass_ms.insert("submit_present".to_string(), cpu.submit_present_ms);
        cpu_pass_ms.insert("tlas_build".to_string(), cpu.tlas_build_ms);
    }

    let sampled_at_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Phase 11 — top-N system timings. Cloning the whole list is
    // fine; the scheduler caps it at the registered system count
    // (~25 today) so this is a few hundred bytes.
    let top_systems_ms: Vec<(String, f32)> = world
        .try_resource::<SchedulerSystemTimings>()
        .map(|t| t.systems.clone())
        .unwrap_or_default();

    let mut snap = world.resource_mut::<MetricsSnapshot>();
    snap.sampled_at_secs = sampled_at_secs;
    snap.cpu_pct = cpu_pct;
    snap.ram_used_mb = ram_used_b / (1024 * 1024);
    snap.ram_total_mb = ram_total_b / (1024 * 1024);
    snap.process_ram_mb = process_ram_b / (1024 * 1024);
    snap.vram_used_mb = vram_used_b / (1024 * 1024);
    snap.vram_reserved_mb = vram_reserved_b / (1024 * 1024);
    snap.vram_budget_mb = vram_budget_b / (1024 * 1024);
    snap.gpu_pass_ms = gpu_pass_ms;
    snap.cpu_pass_ms = cpu_pass_ms;
    snap.top_systems_ms = top_systems_ms;
}
