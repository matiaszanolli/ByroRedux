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
/// `render_finished` semaphores are PER SWAPCHAIN IMAGE — signaled by the
/// frame's render submit, waited on by `vkQueuePresentKHR`. Per-image
/// keys off the acquire boundary: when `acquire_next_image` returns image
/// index `i`, the implementation guarantees the previous present of image
/// `i` has completed and the matching `render_finished[i]` semaphore is
/// fully consumed. Re-signaling is therefore always safe.
///
/// Pre-#906 we used per-image; #906 moved to per-frame-in-flight citing a
/// MAILBOX-discard race (semaphore signal would survive a discarded present).
/// That premise was based on the pre-2023 spec text; current spec (clarified
/// via Khronos issue 2007) requires the implementation to consume / reset
/// wait semaphores even on MAILBOX discard. The per-frame pattern that
/// replaced it has its OWN hazard, observed in the Skyrim Riverwood run:
/// with swapchain_image_count (3) > MAX_FRAMES_IN_FLIGHT (2) under FIFO,
/// a slot's submit re-signals `render_finished[slot]` while the prior
/// present of some other image still holds the same handle in its
/// pSignalSemaphores tracking, tripping
/// VUID-vkQueueSubmit-pSignalSemaphores-00067 (
/// "Swapchain image N was presented but was not re-acquired, so semaphore
/// may still be in use and cannot be safely reused with image index M").
/// Per-image flips this back to the canonical pattern used by the current
/// Vulkan-Samples HelloTriangle and avoids both races.
///
/// Fences are per frame-in-flight for CPU-side throttling.
pub struct FrameSync {
    /// One per frame-in-flight — signaled when an image is acquired.
    pub image_available: Vec<vk::Semaphore>,
    /// One per SWAPCHAIN IMAGE — signaled by the frame's render submit
    /// (indexed by the acquired image_index), waited on by the matching
    /// `queue_present`. See type-level doc for the per-image rationale.
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
            // SAFETY: `device` is the live logical device; `semaphore_info` is a
            // valid (default) create info; the returned semaphore is owned by
            // `FrameSync` and destroyed in its teardown.
            image_available.push(
                device
                    .create_semaphore(&semaphore_info, None)
                    .context("Failed to create image_available semaphore")?,
            );
        }
    }

    // One render-finished semaphore per SWAPCHAIN IMAGE. See `FrameSync`
    // doc for the per-image rationale (canonical Khronos pattern;
    // avoids VUID-00067 across both FIFO and MAILBOX).
    let mut render_finished = Vec::with_capacity(swapchain_image_count);
    for _ in 0..swapchain_image_count {
        unsafe {
            // SAFETY: `device` is the live logical device; `semaphore_info` is a
            // valid (default) create info; the returned semaphore is owned by
            // `FrameSync` and destroyed in its teardown.
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
            // SAFETY: `device` is the live logical device; `fence_info` is a valid
            // create info (SIGNALED so the first frame's wait passes); the returned
            // fence is owned by `FrameSync` and destroyed in its teardown.
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
    /// Resize the per-image fence-aliasing tracker AND the per-image
    /// `render_finished` semaphore Vec for a new swapchain image count,
    /// then recreate `in_flight` fences as SIGNALED. Must be called
    /// after `device_wait_idle` so no previous image-fence / image-
    /// semaphore reference is in use.
    ///
    /// `render_finished` semaphore recreation: post the revert to
    /// per-image (`render_finished[image_index]`), the Vec length must
    /// track `swapchain_image_count`. The CALLER passes the swapchain
    /// recreation through `device_wait_idle` before reaching this
    /// function, so destroy + recreate is safe (no in-flight present
    /// holds any of the old handles). This was the path that #906
    /// originally removed; the per-frame replacement turned out to have
    /// VUID-00067 issues of its own, so we're back. See `FrameSync` doc.
    ///
    /// `in_flight` fence recreation (added in #908 / REN-D1-NEW-01):
    /// `draw_frame` calls `reset_fences` immediately before
    /// `queue_submit`. Any `?`-propagated error between those two
    /// points leaves the fence UNSIGNALED with no submit queued to
    /// ever signal it. The preceding `device_wait_idle` doesn't
    /// transition UNSIGNALED fences back to SIGNALED, so the next
    /// `wait_for_fences` (the both-slots wait at the top of each
    /// frame) would deadlock at `u64::MAX` timeout. Destroying +
    /// recreating the fences with `SIGNALED` here is safe because
    /// `device_wait_idle` guarantees no command buffer is referencing
    /// them, and it sidesteps the missing `vkSignalFence` API. Cost
    /// is two `vkDestroyFence` + two `vkCreateFence` per resize —
    /// negligible.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` is valid and live, the device is not lost,
    /// and that the existing semaphores/fences being recreated are not in use
    /// by any in-flight command buffer or pending present.
    pub unsafe fn recreate_for_swapchain(
        &mut self,
        device: &ash::Device,
        swapchain_image_count: usize,
    ) -> Result<()> {
        self.images_in_flight = vec![vk::Fence::null(); swapchain_image_count];

        // Per-image render_finished — destroy old, create N fresh ones
        // for the new image count. device_wait_idle (caller-side, before
        // entering this function) guarantees no present is still using
        // any of the old handles.
        let sem_info = vk::SemaphoreCreateInfo::default();
        for sem in &self.render_finished {
            device.destroy_semaphore(*sem, None);
        }
        self.render_finished.clear();
        self.render_finished.reserve(swapchain_image_count);
        for _ in 0..swapchain_image_count {
            self.render_finished.push(
                device
                    .create_semaphore(&sem_info, None)
                    .context("Failed to recreate render_finished semaphore after resize")?,
            );
        }

        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        for fence in &mut self.in_flight {
            device.destroy_fence(*fence, None);
            *fence = device
                .create_fence(&fence_info, None)
                .context("Failed to recreate in_flight fence after resize")?;
        }

        log::info!(
            "Sync objects recreated for {} swapchain images ({} render_finished semaphores, {} in_flight fences re-signaled)",
            swapchain_image_count,
            self.render_finished.len(),
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

    /// Destroy + recreate the `in_flight[frame]` fence in place. Used by
    /// `draw_frame`'s submit-failure path: once `reset_fences` runs (now
    /// immediately before `queue_submit`, post-#952), the fence is
    /// UNSIGNALED with no pending submit. If `vkQueueSubmit` then fails,
    /// the fence stays stuck — there is no `vkSignalFence` to flip it
    /// back. The next frame's both-slots `wait_for_fences(..., u64::MAX)`
    /// at `draw.rs:174-183` would block forever.
    ///
    /// Recreating destroys the unsignaled fence and replaces it with a
    /// fresh `SIGNALED`-flagged one, mirroring the
    /// `recreate_for_swapchain` pattern that handles the resize-path
    /// leak (#908). #952 / REN-D1-NEW-04.
    ///
    /// # Safety
    ///
    /// - Caller guarantees no in-flight submit references the existing
    ///   fence. `draw_frame`'s submit-failure arm sits in that window
    ///   by construction (the submit that would have referenced it
    ///   just failed; nothing else can be pending against this slot).
    /// - `frame` must be `< MAX_FRAMES_IN_FLIGHT`.
    /// - `device` must be the same one that allocated the existing
    ///   fence.
    pub unsafe fn recreate_in_flight_for_frame(
        &mut self,
        device: &ash::Device,
        frame: usize,
    ) -> Result<()> {
        let info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let new_fence = device
            .create_fence(&info, None)
            .context("Failed to recreate in_flight fence on submit-failure path")?;
        let old = std::mem::replace(&mut self.in_flight[frame], new_fence);
        // #1188 / REN-D1-NEW-05 — `draw_frame` writes
        // `images_in_flight[img] = in_flight[frame]` BEFORE the submit
        // that can fail. After we destroy `old` below, any matching
        // `images_in_flight` slot would point at a destroyed handle;
        // the next acquire returning the same image index then calls
        // `wait_for_fences` on a dangling fence. Null those entries
        // here — same shape as `recreate_for_swapchain`'s line-182
        // whole-table wipe, scaled to the single-frame case.
        invalidate_images_in_flight_for_fence(&mut self.images_in_flight, old);
        device.destroy_fence(old, None);
        log::warn!(
            "draw_frame error-recovery: recreated in_flight[{}] after reset_fences \
             left the fence unsignaled with no pending submit",
            frame,
        );
        Ok(())
    }

    /// Destroy all semaphores and fences.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` is valid and live, the device is not lost,
    /// and that none of the semaphores or fences are still in use by an
    /// in-flight command buffer or pending present.
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

/// Pure-Rust slot walk that nulls every `images_in_flight` entry equal
/// to `old`. Factored out so the cross-reference invalidation can be
/// unit-tested without a real Vulkan device — the destroy/create calls
/// in `recreate_in_flight_for_frame` need a live `ash::Device`, but
/// this loop is pointer-comparison only. #1188 / REN-D1-NEW-05.
fn invalidate_images_in_flight_for_fence(slots: &mut [vk::Fence], old: vk::Fence) {
    for slot in slots {
        if *slot == old {
            *slot = vk::Fence::null();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;

    fn sentinel(v: u64) -> vk::Fence {
        vk::Fence::from_raw(v)
    }

    #[test]
    fn invalidate_clears_matching_slots() {
        let f_old = sentinel(0xDEAD_BEEF);
        let f_other = sentinel(0xCAFE_F00D);
        let null = vk::Fence::null();
        let mut slots = vec![null, f_old, f_other, f_old, null];
        invalidate_images_in_flight_for_fence(&mut slots, f_old);
        assert_eq!(slots, vec![null, null, f_other, null, null]);
    }

    #[test]
    fn invalidate_is_noop_when_old_is_absent() {
        let f_keep = sentinel(0xAAAA_BBBB);
        let f_old = sentinel(0xDEAD_BEEF);
        let mut slots = vec![f_keep, vk::Fence::null(), f_keep];
        let before = slots.clone();
        invalidate_images_in_flight_for_fence(&mut slots, f_old);
        assert_eq!(slots, before);
    }

    #[test]
    fn invalidate_does_not_touch_null_slots_when_old_is_null() {
        // Defensive: if `old` is `vk::Fence::null()` (impossible on the
        // real submit-failure path — `in_flight[frame]` is always a
        // live handle there — but worth pinning), null slots stay null.
        let null = vk::Fence::null();
        let f_live = sentinel(0xAAAA_BBBB);
        let mut slots = vec![null, f_live, null];
        invalidate_images_in_flight_for_fence(&mut slots, null);
        // `null == null` so the null slots are "matched" and re-written
        // to null — net effect identity. The live slot is untouched.
        assert_eq!(slots, vec![null, f_live, null]);
    }
}
