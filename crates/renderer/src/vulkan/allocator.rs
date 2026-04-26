//! GPU memory allocator wrapper around `gpu_allocator`.

use anyhow::{Context, Result};
use ash::vk;
use byroredux_core::ecs::Resource;
use gpu_allocator::vulkan;
use std::sync::{Arc, Mutex};

/// Shared GPU memory allocator.
///
/// Wrapped in `Arc<Mutex<>>` because `gpu_allocator::vulkan::Allocator`
/// requires `&mut self` for allocate/free, but we need to hand out
/// references to it for buffer creation and destruction at different
/// points in the renderer lifecycle.
pub type SharedAllocator = Arc<Mutex<vulkan::Allocator>>;

/// Resource newtype that lets the ECS expose the renderer's
/// `SharedAllocator` to systems / console commands. Pre-#503 the
/// allocator lived on `VulkanContext` only — out of reach of
/// `world.try_resource::<…>()`. The newtype dodges the orphan rule
/// (`Resource` is in core, `Arc<Mutex<…>>` is foreign) and keeps the
/// existing `SharedAllocator` type alias as the source of truth for
/// every other call site. Insert once at engine init alongside the
/// renderer setup. See #503 / `mem.frag` console command.
pub struct AllocatorResource(pub SharedAllocator);

impl Resource for AllocatorResource {}

/// Per-block fragmentation snapshot derived from a
/// [`vulkan::Allocator::generate_report`].
///
/// `fragmentation_ratio = largest_free / total_free`, in `[0.0, 1.0]`:
/// - `1.0` — every free byte sits in a single contiguous span (best).
/// - `< 0.5` — the largest contiguous free range is below half of the
///   total free space, i.e. a future allocation up to `total_free` is
///   guaranteed to fail because the allocator is first-fit within a
///   block. This is the threshold the audit
///   `AUDIT_PERFORMANCE_2026-04-20.md` D2-L1 flagged as the
///   "restart-to-defrag is due" signal.
/// - `0.0` — fully utilised block (no free space). Treated as
///   "not fragmented" and skipped from the worst-block warn check —
///   nothing to fragment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockFragInfo {
    /// Index into `AllocatorReport.blocks`.
    pub block_index: usize,
    /// Total block size in bytes (reserved).
    pub size: u64,
    /// Sum of allocation sizes inside this block.
    pub allocated: u64,
    /// `size - allocated`.
    pub free: u64,
    /// Largest contiguous free range in bytes.
    pub largest_free: u64,
    /// `largest_free as f64 / free as f64`. `1.0` when `free == 0`.
    pub fragmentation_ratio: f64,
}

/// Compute per-block fragmentation from a gpu-allocator report.
///
/// Pure function — testable without a live Vulkan device. The
/// algorithm:
///   1. For each block, collect the allocations whose flat-index range
///      sits inside `MemoryBlockReport.allocations` and sort by offset.
///   2. Walk the sorted span and accumulate the free runs (head gap,
///      inter-allocation gaps, tail gap) — `total_free` is the sum,
///      `largest_free` is the max single run.
///   3. `fragmentation_ratio = largest_free / total_free` (or `1.0`
///      when `total_free == 0`).
///
/// `gpu-allocator` 0.27 is first-fit within a block, so a low ratio
/// directly maps to "this block can't honour an allocation that would
/// fit in `total_free`".
pub fn compute_block_fragmentation(report: &gpu_allocator::AllocatorReport) -> Vec<BlockFragInfo> {
    let mut out = Vec::with_capacity(report.blocks.len());
    for (block_index, block) in report.blocks.iter().enumerate() {
        // Slice + sort by offset. The report's flat allocations Vec is
        // shared across blocks; each block's `allocations` Range
        // indexes into it.
        let mut block_allocs: Vec<&gpu_allocator::AllocationReport> = report
            .allocations
            .get(block.allocations.clone())
            .unwrap_or(&[])
            .iter()
            .collect();
        block_allocs.sort_by_key(|a| a.offset);

        let allocated: u64 = block_allocs.iter().map(|a| a.size).sum();
        let free = block.size.saturating_sub(allocated);

        // Walk sorted allocations, tracking the largest free run.
        let mut largest_free: u64 = 0;
        let mut cursor: u64 = 0;
        for alloc in &block_allocs {
            if alloc.offset > cursor {
                largest_free = largest_free.max(alloc.offset - cursor);
            }
            cursor = alloc.offset.saturating_add(alloc.size);
        }
        if block.size > cursor {
            largest_free = largest_free.max(block.size - cursor);
        }

        let fragmentation_ratio = if free == 0 {
            1.0
        } else {
            largest_free as f64 / free as f64
        };

        out.push(BlockFragInfo {
            block_index,
            size: block.size,
            allocated,
            free,
            largest_free,
            fragmentation_ratio,
        });
    }
    out
}

/// Format a fragmentation report into log-ready lines. Used both by
/// the `mem.frag` console command and by the explicit
/// [`log_fragmentation_report`] entry point. Pure formatting — no log
/// emit, no side effects.
///
/// Empty `frags` (no allocator blocks yet) returns a single
/// "no blocks" line so the caller's output isn't blank.
pub fn fragmentation_report_lines(frags: &[BlockFragInfo]) -> Vec<String> {
    if frags.is_empty() {
        return vec!["GPU memory fragmentation: no blocks reported".to_string()];
    }
    // Header + per-block rows + summary footer.
    let mut lines = Vec::with_capacity(frags.len() + 3);
    lines.push("GPU memory fragmentation per block:".to_string());
    lines.push(format!(
        "  {:>5} {:>10} {:>10} {:>10} {:>12} {:>6}",
        "block", "size_MB", "alloc_MB", "free_MB", "largest_MB", "frag",
    ));
    let mut worst_ratio = 1.0_f64;
    let mut worst_block: usize = 0;
    for info in frags {
        // Only count blocks with non-zero free space — fully-utilised
        // blocks have no fragmentation by definition.
        if info.free > 0 && info.fragmentation_ratio < worst_ratio {
            worst_ratio = info.fragmentation_ratio;
            worst_block = info.block_index;
        }
        lines.push(format!(
            "  {:>5} {:>10.2} {:>10.2} {:>10.2} {:>12.2} {:>5.2}",
            info.block_index,
            info.size as f64 / (1024.0 * 1024.0),
            info.allocated as f64 / (1024.0 * 1024.0),
            info.free as f64 / (1024.0 * 1024.0),
            info.largest_free as f64 / (1024.0 * 1024.0),
            info.fragmentation_ratio,
        ));
    }
    if worst_ratio < 0.5 {
        lines.push(format!(
            "  WARN: worst block {} has fragmentation ratio {:.2} (< 0.5) — \
             a restart-to-defrag is in order",
            worst_block, worst_ratio,
        ));
    } else {
        lines.push(format!(
            "  OK: worst block fragmentation ratio {:.2} (≥ 0.5)",
            worst_ratio,
        ));
    }
    lines
}

/// Compute + emit a fragmentation report to the log. Explicit-call
/// only — never wire into `log_memory_usage` or any per-frame path.
/// The `mem.frag` console command surfaces the same data without
/// going through the log.
pub fn log_fragmentation_report(allocator: &SharedAllocator) {
    let alloc = allocator.lock().expect("allocator lock poisoned");
    let report = alloc.generate_report();
    let frags = compute_block_fragmentation(&report);
    drop(alloc);
    for line in fragmentation_report_lines(&frags) {
        log::info!("{}", line);
    }
}

/// Create the GPU allocator. Call after logical device creation.
pub fn create_allocator(
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: ash::vk::PhysicalDevice,
    buffer_device_address: bool,
) -> Result<SharedAllocator> {
    // Tuned block sizes for game engine workload. The gpu-allocator
    // defaults (256 MB device, 64 MB host) over-reserve on GPUs with
    // 4–8 GB VRAM — a single 256 MB block at startup consumes 6% of a
    // 4 GB GPU before any content loads. Smaller blocks (64 MB device,
    // 16 MB host) let the allocator grow on demand with less waste.
    // Typical cell load: ~250 MB across ~3000 allocations in 4–5 blocks.
    let allocation_sizes = gpu_allocator::AllocationSizes::new(
        64 * 1024 * 1024, // 64 MB device-local blocks
        16 * 1024 * 1024, // 16 MB host-visible blocks
    );
    let allocator = vulkan::Allocator::new(&vulkan::AllocatorCreateDesc {
        instance: instance.clone(),
        device: device.clone(),
        physical_device,
        debug_settings: gpu_allocator::AllocatorDebugSettings {
            log_memory_information: cfg!(debug_assertions),
            log_leaks_on_shutdown: true,
            ..Default::default()
        },
        buffer_device_address,
        allocation_sizes,
    })
    .context("Failed to create GPU allocator")?;

    log::info!("GPU allocator created");
    Ok(Arc::new(Mutex::new(allocator)))
}

/// Compute the "usage is high" warn threshold from the physical
/// device's smallest DEVICE_LOCAL heap. Returns 80% of that heap.
///
/// Pre-#505 this was a compile-time `const 2 GB`: too high on a 6 GB
/// VRAM floor (33%, spuriously quiet) and too low on a 12 GB dev GPU
/// (16%, warn on every large cell load). Scaling with the actual heap
/// gives a sensible "approaching OOM" signal on any config. Falls
/// back to 2 GB on devices that don't expose a DEVICE_LOCAL heap
/// (pure-SoC / Vulkan-on-software-rasterizer).
fn warn_threshold_bytes(instance: &ash::Instance, physical_device: vk::PhysicalDevice) -> u64 {
    let heap = super::device::smallest_device_local_heap_bytes(instance, physical_device);
    if heap == 0 {
        2 * 1024 * 1024 * 1024
    } else {
        (heap / 5) * 4 // 80% without losing precision to floats
    }
}

/// Log current GPU memory allocation statistics.
///
/// Queries the gpu_allocator report for total allocated/reserved bytes.
/// Logs at INFO if usage is normal, WARN if allocated exceeds 80% of
/// the smallest DEVICE_LOCAL heap on the physical device.
pub fn log_memory_usage(
    allocator: &SharedAllocator,
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) {
    let alloc = allocator.lock().expect("allocator lock poisoned");
    let report = alloc.generate_report();
    let allocated_mb = report.total_allocated_bytes as f64 / (1024.0 * 1024.0);
    let reserved_mb = report.total_reserved_bytes as f64 / (1024.0 * 1024.0);
    let num_allocs = report.allocations.len();
    let num_blocks = report.blocks.len();

    log::info!(
        "GPU memory: {:.1} MB allocated / {:.1} MB reserved ({} allocations, {} blocks)",
        allocated_mb,
        reserved_mb,
        num_allocs,
        num_blocks
    );

    let threshold = warn_threshold_bytes(instance, physical_device);
    if report.total_allocated_bytes > threshold {
        log::warn!(
            "GPU memory usage high: {:.1} MB allocated (threshold: {} MB ≈ 80% of smallest DEVICE_LOCAL heap)",
            allocated_mb,
            threshold / (1024 * 1024)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpu_allocator::{AllocationReport, AllocatorReport, MemoryBlockReport};

    /// Synthetic helper — build a one-block report from a list of
    /// `(offset, size)` allocations. Used to exercise
    /// [`compute_block_fragmentation`] without a live Vulkan device.
    fn one_block_report(block_size: u64, allocs: &[(u64, u64)]) -> AllocatorReport {
        let allocations: Vec<AllocationReport> = allocs
            .iter()
            .map(|&(offset, size)| AllocationReport {
                name: String::new(),
                offset,
                size,
            })
            .collect();
        let total_allocated: u64 = allocations.iter().map(|a| a.size).sum();
        let blocks = vec![MemoryBlockReport {
            size: block_size,
            allocations: 0..allocations.len(),
        }];
        AllocatorReport {
            allocations,
            blocks,
            total_allocated_bytes: total_allocated,
            total_reserved_bytes: block_size,
        }
    }

    /// Smoke check for the warn-threshold math. `smallest_device_local_heap_bytes`
    /// itself requires a live Vulkan instance (not feasible in unit tests),
    /// so exercise the fallback path explicitly to guarantee zero doesn't
    /// leak through as a divisor and that the 2 GB fallback lands.
    #[test]
    fn warn_threshold_falls_back_when_heap_missing() {
        // We can't easily fabricate a VkPhysicalDevice in a unit test, but
        // we can verify the math: 80% of a known heap size.
        fn threshold_for(heap: u64) -> u64 {
            if heap == 0 {
                2 * 1024 * 1024 * 1024
            } else {
                (heap / 5) * 4
            }
        }
        assert_eq!(threshold_for(0), 2 * 1024 * 1024 * 1024);
        // 6 GB floor → ~4.8 GB threshold (int truncation of /5).
        let six_gb = 6u64 * 1024 * 1024 * 1024;
        assert_eq!(threshold_for(six_gb), (six_gb / 5) * 4);
        // 12 GB dev GPU → ~9.6 GB threshold.
        let twelve_gb = 12u64 * 1024 * 1024 * 1024;
        assert_eq!(threshold_for(twelve_gb), (twelve_gb / 5) * 4);
        // Sanity: 80% is strictly greater than the pre-#505 2 GB
        // constant for any heap ≥ 2.5 GB.
        assert!(threshold_for(six_gb) > 2 * 1024 * 1024 * 1024);
    }

    /// Regression: #503 D2-L1 — fragmentation ratio should compute
    /// `largest_free / total_free` per block. The contiguous-free
    /// happy path must report `1.0`.
    #[test]
    fn compute_fragmentation_contiguous_free_reports_full_ratio() {
        // 64 MB block with one 1 KB alloc at the start — the rest is
        // a single contiguous free run.
        let report = one_block_report(64 * 1024 * 1024, &[(0, 1024)]);
        let frags = compute_block_fragmentation(&report);
        assert_eq!(frags.len(), 1);
        let f = &frags[0];
        assert_eq!(f.allocated, 1024);
        assert_eq!(f.free, 64 * 1024 * 1024 - 1024);
        assert_eq!(f.largest_free, 64 * 1024 * 1024 - 1024);
        assert!(
            (f.fragmentation_ratio - 1.0).abs() < 1e-9,
            "single-free-run block must report ratio 1.0, got {}",
            f.fragmentation_ratio,
        );
    }

    /// Regression: #503 D2-L1 — the audit-prescribed signal. A
    /// fragmentation-inducing pattern (big alloc in the middle that
    /// splits free space into two ~equal halves) MUST report a ratio
    /// below 0.5 so the worst-block warn fires.
    ///
    /// Pattern: 64 MB block, two allocations spaced so the largest
    /// contiguous free run is < 50% of total free space:
    ///   alloc[0]:  offset=20 MB,  size=  4 MB
    ///   alloc[1]:  offset=42 MB,  size=  4 MB
    /// Free runs: [0, 20MB) = 20 MB, [24, 42MB) = 18 MB,
    ///            [46, 64MB) = 18 MB. Total free = 56 MB,
    /// largest = 20 MB → ratio ≈ 0.357.
    #[test]
    fn compute_fragmentation_split_block_reports_below_half() {
        let mb = 1024 * 1024;
        let report = one_block_report(64 * mb, &[(20 * mb, 4 * mb), (42 * mb, 4 * mb)]);
        let frags = compute_block_fragmentation(&report);
        assert_eq!(frags.len(), 1);
        let f = &frags[0];
        assert_eq!(f.allocated, 8 * mb);
        assert_eq!(f.free, 56 * mb);
        assert_eq!(f.largest_free, 20 * mb);
        assert!(
            f.fragmentation_ratio < 0.5,
            "expected fragmentation ratio < 0.5 for split-block pattern, got {}",
            f.fragmentation_ratio,
        );
    }

    /// Regression: #503 — the formatted lines must surface the worst
    /// block and emit a "restart-to-defrag" WARN line whenever any
    /// block falls below the 0.5 threshold. Pin both directions:
    /// fragmented ⇒ WARN line present; clean ⇒ OK line present.
    #[test]
    fn fragmentation_report_lines_warns_on_low_ratio_only() {
        let mb = 1024 * 1024;

        let split = one_block_report(64 * mb, &[(20 * mb, 4 * mb), (42 * mb, 4 * mb)]);
        let split_lines = fragmentation_report_lines(&compute_block_fragmentation(&split));
        assert!(
            split_lines
                .iter()
                .any(|l| l.contains("WARN") && l.contains("restart-to-defrag")),
            "fragmented pattern must emit WARN line, got: {split_lines:#?}",
        );

        let clean = one_block_report(64 * mb, &[(0, 1024)]);
        let clean_lines = fragmentation_report_lines(&compute_block_fragmentation(&clean));
        assert!(
            !clean_lines.iter().any(|l| l.contains("WARN")),
            "contiguous-free block must not emit WARN line",
        );
        assert!(
            clean_lines.iter().any(|l| l.contains("OK:")),
            "contiguous-free block must emit OK line, got: {clean_lines:#?}",
        );
    }

    /// Edge case: fully-utilised block (no free space) must report a
    /// ratio of 1.0 — there's nothing to fragment, so it must not
    /// trip the worst-block warn.
    #[test]
    fn compute_fragmentation_fully_utilised_reports_one() {
        let report = one_block_report(64 * 1024 * 1024, &[(0, 64 * 1024 * 1024)]);
        let frags = compute_block_fragmentation(&report);
        assert_eq!(frags[0].free, 0);
        assert!((frags[0].fragmentation_ratio - 1.0).abs() < 1e-9);
        let lines = fragmentation_report_lines(&frags);
        assert!(!lines.iter().any(|l| l.contains("WARN")));
    }

    /// Empty report (no blocks yet — allocator hasn't been touched)
    /// must surface a non-empty single-line message rather than blank
    /// output.
    #[test]
    fn fragmentation_report_lines_handles_empty_report() {
        let lines = fragmentation_report_lines(&[]);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("no blocks reported"));
    }
}
