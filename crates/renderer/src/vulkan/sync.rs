//! Synchronization primitives for frame rendering.

use anyhow::{Context, Result};
use ash::vk;

pub const MAX_FRAMES_IN_FLIGHT: usize = 2;

/// Per-frame synchronization objects.
///
/// Semaphores are indexed by swapchain image (not frame-in-flight) to
/// avoid reusing a semaphore that the present engine still holds.
/// Fences are indexed by frame-in-flight for CPU-side throttling.
pub struct FrameSync {
    /// One per swapchain image — signaled when image is acquired.
    pub image_available: Vec<vk::Semaphore>,
    /// One per swapchain image — signaled when rendering to that image finishes.
    pub render_finished: Vec<vk::Semaphore>,
    /// One per frame-in-flight — CPU waits on these to throttle submission.
    pub in_flight: Vec<vk::Fence>,
    /// Maps swapchain image index → which in_flight fence was last used.
    /// Prevents submitting work for an image that's still being rendered.
    pub images_in_flight: Vec<vk::Fence>,
    /// Number of semaphores (= swapchain image count).
    semaphore_count: usize,
}

pub fn create_sync_objects(
    device: &ash::Device,
    swapchain_image_count: usize,
) -> Result<FrameSync> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    // One semaphore pair per swapchain image.
    let mut image_available = Vec::with_capacity(swapchain_image_count);
    let mut render_finished = Vec::with_capacity(swapchain_image_count);
    for _ in 0..swapchain_image_count {
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
        }
    }

    // One fence per frame-in-flight.
    let mut in_flight = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        unsafe {
            in_flight.push(
                device
                    .create_fence(&fence_info, None)
                    .context("Failed to create in_flight fence")?,
            );
        }
    }

    let images_in_flight = vec![vk::Fence::null(); swapchain_image_count];

    log::info!(
        "Sync objects created ({} frames in flight, {} swapchain images)",
        MAX_FRAMES_IN_FLIGHT,
        swapchain_image_count,
    );

    Ok(FrameSync {
        image_available,
        render_finished,
        in_flight,
        images_in_flight,
        semaphore_count: swapchain_image_count,
    })
}

impl FrameSync {
    pub fn reset_image_fences(&mut self, swapchain_image_count: usize) {
        self.images_in_flight = vec![vk::Fence::null(); swapchain_image_count];
    }

    pub unsafe fn destroy(&self, device: &ash::Device) {
        for i in 0..self.semaphore_count {
            device.destroy_semaphore(self.image_available[i], None);
            device.destroy_semaphore(self.render_finished[i], None);
        }
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            device.destroy_fence(self.in_flight[i], None);
        }
    }
}
