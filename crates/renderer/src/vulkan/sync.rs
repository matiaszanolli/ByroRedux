//! Synchronization primitives for frame rendering.

use anyhow::{Context, Result};
use ash::vk;

pub const MAX_FRAMES_IN_FLIGHT: usize = 2;

/// Per-frame synchronization objects.
///
/// `image_available` semaphores are per frame-in-flight — one is signaled
/// per `acquire_next_image` call and waited on by the same frame's submit.
///
/// `render_finished` semaphores are per swapchain image — signaled when
/// rendering to that image finishes, waited on by the present engine.
/// Indexing per-image avoids reusing a semaphore the present engine still holds.
///
/// Fences are per frame-in-flight for CPU-side throttling.
pub struct FrameSync {
    /// One per frame-in-flight — signaled when an image is acquired.
    pub image_available: Vec<vk::Semaphore>,
    /// One per swapchain image — signaled when rendering to that image finishes.
    pub render_finished: Vec<vk::Semaphore>,
    /// One per frame-in-flight — CPU waits on these to throttle submission.
    pub in_flight: Vec<vk::Fence>,
    /// Maps swapchain image index → which in_flight fence was last used.
    /// Prevents submitting work for an image that's still being rendered.
    pub images_in_flight: Vec<vk::Fence>,
}

pub fn create_sync_objects(
    device: &ash::Device,
    swapchain_image_count: usize,
) -> Result<FrameSync> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    // One acquire semaphore per frame-in-flight.
    let mut image_available = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        unsafe {
            image_available.push(
                device
                    .create_semaphore(&semaphore_info, None)
                    .context("Failed to create image_available semaphore")?,
            );
        }
    }

    // One render-finished semaphore per swapchain image.
    let mut render_finished = Vec::with_capacity(swapchain_image_count);
    for _ in 0..swapchain_image_count {
        unsafe {
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
    })
}

impl FrameSync {
    /// Recreate per-image sync state for a new swapchain image count.
    ///
    /// Destroys and recreates `render_finished` semaphores and resets
    /// `images_in_flight` tracking. Must be called after `device_wait_idle`
    /// to ensure no semaphore is in use.
    pub unsafe fn recreate_for_swapchain(
        &mut self,
        device: &ash::Device,
        swapchain_image_count: usize,
    ) -> Result<()> {
        // Destroy old per-image semaphores.
        for &sem in &self.render_finished {
            device.destroy_semaphore(sem, None);
        }

        // Create new ones matching the new image count.
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let mut render_finished = Vec::with_capacity(swapchain_image_count);
        for _ in 0..swapchain_image_count {
            render_finished.push(
                device
                    .create_semaphore(&semaphore_info, None)
                    .context("Failed to create render_finished semaphore")?,
            );
        }
        self.render_finished = render_finished;
        self.images_in_flight = vec![vk::Fence::null(); swapchain_image_count];

        log::info!(
            "Sync objects recreated for {} swapchain images",
            swapchain_image_count,
        );
        Ok(())
    }

    pub unsafe fn destroy(&self, device: &ash::Device) {
        for &sem in &self.image_available {
            device.destroy_semaphore(sem, None);
        }
        for &sem in &self.render_finished {
            device.destroy_semaphore(sem, None);
        }
        for &fence in &self.in_flight {
            device.destroy_fence(fence, None);
        }
    }
}
