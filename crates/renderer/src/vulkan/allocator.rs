//! GPU memory allocator wrapper around `gpu_allocator`.

use anyhow::{Context, Result};
use gpu_allocator::vulkan;
use std::sync::{Arc, Mutex};

/// Shared GPU memory allocator.
///
/// Wrapped in `Arc<Mutex<>>` because `gpu_allocator::vulkan::Allocator`
/// requires `&mut self` for allocate/free, but we need to hand out
/// references to it for buffer creation and destruction at different
/// points in the renderer lifecycle.
pub type SharedAllocator = Arc<Mutex<vulkan::Allocator>>;

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
        64 * 1024 * 1024,  // 64 MB device-local blocks
        16 * 1024 * 1024,  // 16 MB host-visible blocks
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

/// Log current GPU memory allocation statistics.
///
/// Queries the gpu_allocator report for total allocated/reserved bytes.
/// Logs at INFO if usage is normal, WARN if allocated > budget_warn_threshold.
pub fn log_memory_usage(allocator: &SharedAllocator) {
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

    // Warn if allocated exceeds a conservative threshold (2 GB).
    // Real budget tracking via VK_EXT_memory_budget deferred to streaming milestone.
    const WARN_THRESHOLD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
    if report.total_allocated_bytes > WARN_THRESHOLD_BYTES {
        log::warn!(
            "GPU memory usage high: {:.1} MB allocated (threshold: {} MB)",
            allocated_mb,
            WARN_THRESHOLD_BYTES / (1024 * 1024)
        );
    }
}
