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
        allocation_sizes: Default::default(),
    })
    .context("Failed to create GPU allocator")?;

    log::info!("GPU allocator created");
    Ok(Arc::new(Mutex::new(allocator)))
}
