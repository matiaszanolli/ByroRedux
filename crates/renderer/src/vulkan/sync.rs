//! Synchronization primitives for frame rendering.

use anyhow::{Context, Result};
use ash::vk;

pub const MAX_FRAMES_IN_FLIGHT: usize = 2;

// Issue #870 (REN-D4-NEW-01): the depth image at
// `context/mod.rs:580-582` is a single VkImage shared across all
// frames-in-flight (NOT per-frame like the G-buffer / TAA / SVGF /
// caustic / SSAO attachments). Frame N+1's main-render-pass
// LOAD_OP_CLEAR on depth would race against frame N's compute
// consumers (SSAO sampler, SVGF depth read) UNLESS frame N's
// compute work has retired before frame N+1 begins. The double-fence
// wait at `context/draw.rs:108-120` (#282) guarantees this *only*
// while waiting on both `in_flight[frame]` and `in_flight[(frame+1)
// % MAX_FRAMES_IN_FLIGHT]` is equivalent to device-idle for prior
// frames — which is true at MAX_FRAMES_IN_FLIGHT == 2 because two
// fences cover both slots. At 3+ slots the both-fences pattern
// would only cover 2 of N, leaving frame N-2's compute possibly in
// flight when frame N+1's render pass clears depth.
//
// Bumping this constant requires either:
//   (a) making the depth image per-frame-in-flight
//       (`Vec<vk::Image>` indexed by frame_index, mirroring the
//       G-buffer pattern at `gbuffer.rs:52`), OR
//   (b) extending the fence wait to cover all in-flight slots
//       (currently 2; would become MAX_FRAMES_IN_FLIGHT - 1 fences).
//
// The const_assert below fails the workspace build if anyone
// raises the value without addressing the depth-image hazard.
const _: () = assert!(
    MAX_FRAMES_IN_FLIGHT == 2,
    "shared depth image at context/mod.rs:580 requires \
     MAX_FRAMES_IN_FLIGHT == 2; see #870 for the safety contract"
);

/// Per-frame synchronization objects.
///
/// `image_available` semaphores are per frame-in-flight — one is signaled
/// per `acquire_next_image` call and waited on by the same frame's submit.
///
/// `render_finished` semaphores are per frame-in-flight — signaled by the
/// frame's render submit, waited on by `vkQueuePresentKHR`. Per-frame (not
/// per-image) keys off the frame slot's `in_flight[frame]` fence — when the
/// CPU reuses a slot via `wait_for_fences`, the submit (which signaled
/// `render_finished[frame]`) has retired and is safe to re-signal.
///
/// Pre-#906 / REN-D1-NEW-02 this was per swapchain image, which had a
/// validation hazard under MAILBOX present mode: if the present engine
/// REPLACED a queued present without waiting on its semaphore (per VK
/// spec — MAILBOX is allowed to discard the queued entry), the
/// `render_finished[image]` stayed signaled, and the next frame
/// re-acquiring that image would submit with a still-signaled semaphore
/// in `pSignalSemaphores` (VUID-vkQueueSubmit-pSignalSemaphores-00067).
/// Per-frame-in-flight matches the canonical Khronos / Vulkan-Tutorial
/// pattern and sidesteps the MAILBOX discard race entirely.
///
/// Fences are per frame-in-flight for CPU-side throttling.
pub struct FrameSync {
    /// One per frame-in-flight — signaled when an image is acquired.
    pub image_available: Vec<vk::Semaphore>,
    /// One per frame-in-flight — signaled by the frame's render submit,
    /// waited on by the matching frame's present. See type-level doc
    /// for the MAILBOX-discard rationale (#906).
    pub render_finished: Vec<vk::Semaphore>,
    /// One per frame-in-flight — CPU waits on these to throttle submission.
    pub in_flight: Vec<vk::Fence>,
    /// Maps swapchain image index → which `in_flight` fence was last used.
    /// Prevents submitting work for an image that's still being rendered.
    ///
    /// # Invariant (#953 / REN-D1-NEW-05)
    ///
    /// Any handle stored here is guaranteed SIGNALED (or `vk::Fence::null()`)
    /// by the time `draw_frame` next reads it at `context/draw.rs:179-186`.
    /// This is upheld upstream by the *both-slots* `wait_for_fences` at
    /// `context/draw.rs:144-156`, which blocks on BOTH frame-in-flight
    /// fences before any image-fence read — so by the time we reach the
    /// guard, every fence in this vec is either null (image never used)
    /// or matches one of the two frame slots we just waited on.
    ///
    /// The aliasing guard `image_fence != in_flight[frame]` at draw.rs:180
    /// then prevents waiting on the just-reset fence belonging to the
    /// current frame slot. Reusing the slot's own fence would block on
    /// an UNSIGNALED handle (it's reset at draw.rs:191) and deadlock.
    ///
    /// **If `draw_frame` ever drops to a single-slot fence wait** at the
    /// top of frame (e.g. as a perf optimization), this invariant breaks
    /// silently: the OTHER slot's fence handle could still be stored
    /// here from a prior frame in an UNSIGNALED state. Update both call
    /// sites in lockstep or this vec stops being safe to read.
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

    // One render-finished semaphore per frame-in-flight (#906 /
    // REN-D1-NEW-02). See `FrameSync` doc for the MAILBOX rationale.
    let mut render_finished = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
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
    /// Resize the per-image fence-aliasing tracker for a new swapchain
    /// image count AND recreate `in_flight` fences as SIGNALED. Must
    /// be called after `device_wait_idle` so no previous image-fence
    /// reference is in use.
    ///
    /// Post-#906 `render_finished` is per frame-in-flight (constant
    /// size), so the per-image-semaphore destroy/recreate dance is
    /// gone. What stays is the `in_flight` fence recreation, added in
    /// #908 / REN-D1-NEW-01: `draw_frame` calls `reset_fences` at
    /// `context/draw.rs:191` *before* `queue_submit`. Any `?`-
    /// propagated error between those two points leaves the fence
    /// UNSIGNALED with no submit queued to ever signal it. The
    /// preceding `device_wait_idle` doesn't transition UNSIGNALED
    /// fences back to SIGNALED, so the next `wait_for_fences`
    /// (`draw.rs:147` — the both-slots wait at the top of each frame)
    /// would deadlock at `u64::MAX` timeout. Destroying + recreating
    /// the fences with `SIGNALED` here is safe because
    /// `device_wait_idle` guarantees no command buffer is referencing
    /// them, and it sidesteps the missing `vkSignalFence` API. Cost
    /// is two `vkDestroyFence` + two `vkCreateFence` per resize —
    /// negligible.
    pub unsafe fn recreate_for_swapchain(
        &mut self,
        device: &ash::Device,
        swapchain_image_count: usize,
    ) -> Result<()> {
        self.images_in_flight = vec![vk::Fence::null(); swapchain_image_count];

        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        for fence in &mut self.in_flight {
            device.destroy_fence(*fence, None);
            *fence = device
                .create_fence(&fence_info, None)
                .context("Failed to recreate in_flight fence after resize")?;
        }

        log::info!(
            "Sync objects recreated for {} swapchain images ({} in_flight fences re-signaled)",
            swapchain_image_count,
            self.in_flight.len(),
        );
        Ok(())
    }

    /// Destroy + recreate the `image_available[frame]` semaphore in
    /// place. Used by `draw_frame`'s error-recovery path: if any `?`-
    /// propagated error fires between a successful `acquire_next_image`
    /// (which signals this semaphore) and `queue_submit`'s
    /// `wait_semaphores` consumption, the signal stays pending. Per
    /// VUID-vkAcquireNextImageKHR-semaphore-01779 the next
    /// `acquire_next_image` on the same slot would then trip the
    /// validation layer ("semaphore must not be currently signaled or
    /// in a wait operation"). Sibling to `recreate_for_swapchain`'s
    /// `in_flight` fence recovery — same shape of leak, same shape of
    /// fix. #910 / REN-D5-NEW-01.
    ///
    /// # Safety
    ///
    /// - Caller guarantees no command buffer that waits on this
    ///   semaphore is currently submitted (i.e. the only ops referring
    ///   to it are the failed acquire's signal and the failed-or-
    ///   skipped submit's wait). `draw_frame`'s error sites all fall
    ///   in that window: between the acquire and the `queue_submit`,
    ///   no batch has been launched yet.
    /// - `frame` must be `< MAX_FRAMES_IN_FLIGHT`.
    /// - `device` must be the same one that allocated the existing
    ///   semaphore.
    pub unsafe fn recreate_image_available_for_frame(
        &mut self,
        device: &ash::Device,
        frame: usize,
    ) -> Result<()> {
        let info = vk::SemaphoreCreateInfo::default();
        let new_sem = device
            .create_semaphore(&info, None)
            .context("Failed to recreate image_available semaphore on error path")?;
        let old = std::mem::replace(&mut self.image_available[frame], new_sem);
        device.destroy_semaphore(old, None);
        log::warn!(
            "draw_frame error-recovery: recreated image_available[{}] to clear leaked acquire signal",
            frame,
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
