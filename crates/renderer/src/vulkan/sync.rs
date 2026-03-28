//! Synchronization primitives for frame rendering.

use anyhow::{Context, Result};
use ash::vk;

pub const MAX_FRAMES_IN_FLIGHT: usize = 2;

/// Per-frame synchronization objects.
pub struct FrameSync {
    pub image_available: Vec<vk::Semaphore>,
    pub render_finished: Vec<vk::Semaphore>,
    pub in_flight: Vec<vk::Fence>,
}

pub fn create_sync_objects(device: &ash::Device) -> Result<FrameSync> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    let mut image_available = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut render_finished = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut in_flight = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);

    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        unsafe {
            image_available.push(
                device
                    .create_semaphore(&semaphore_info, None)
                    .context("Failed to create image_available semaphore")?,
            );
            render_finished.push(
                device
                    .create_semaphore(&semaphore_info, None)
                    .context("Failed to create render_finished semaphore")?,
            );
            in_flight.push(
                device
                    .create_fence(&fence_info, None)
                    .context("Failed to create in_flight fence")?,
            );
        }
    }

    log::info!("Sync objects created ({} frames in flight)", MAX_FRAMES_IN_FLIGHT);

    Ok(FrameSync {
        image_available,
        render_finished,
        in_flight,
    })
}

impl FrameSync {
    pub unsafe fn destroy(&self, device: &ash::Device) {
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            device.destroy_semaphore(self.image_available[i], None);
            device.destroy_semaphore(self.render_finished[i], None);
            device.destroy_fence(self.in_flight[i], None);
        }
    }
}
