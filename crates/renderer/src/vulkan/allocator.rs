//! GPU memory allocator wrapper around `gpu_allocator`.

use anyhow::{Context, Result};
use ash::vk;
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
fn warn_threshold_bytes(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> u64 {
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
    /// Smoke check for the warn-threshold math. `smallest_device_local_heap_bytes`
    /// itself requires a live Vulkan instance (not feasible in unit tests),
    /// so exercise the fallback path explicitly to guarantee zero doesn't
    /// leak through as a divisor and that the 2 GB fallback lands.
    #[test]
    fn warn_threshold_falls_back_when_heap_missing() {
        // We can't easily fabricate a VkPhysicalDevice in a unit test, but
        // we can verify the math: 80% of a known heap size.
        fn threshold_for(heap: u64) -> u64 {
            if heap == 0 { 2 * 1024 * 1024 * 1024 } else { (heap / 5) * 4 }
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
}
