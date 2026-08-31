//! Synchronization primitives for frame rendering.

use anyhow::{Context, Result};
use ash::vk;

pub const MAX_FRAMES_IN_FLIGHT: usize = 2;

// Issue #870 (REN-D4-NEW-01): `VulkanContext::depth_image`
// (`vulkan::context`) is a single VkImage shared across all
// frames-in-flight (NOT per-frame like the G-buffer / TAA / SVGF /
// caustic / SSAO attachments). Frame N+1's main-render-pass
// LOAD_OP_CLEAR on depth would race against frame N's consumers of
// that shared image UNLESS frame N's work has retired before frame
// N+1 begins. The consumer list is:
//   * the SSAO sampler,
//   * the SVGF depth read,
//   * FSR (#2485) — `context/post_passes.rs::record_upscale_pass`
//     passes `depth: self.depth_image` into `UpscaleDispatchInputs`
//     with no frame index, so `frame_upscaler.rs` reads the same
//     single image,
//   * `copy_depth_to_history`'s transfer read.
// Treat that list as load-bearing rather than illustrative: whoever
// next evaluates making the depth image per-frame-in-flight must size
// the work off all of them, and a future MAX_FRAMES_IN_FLIGHT bump
// review must not read a short list as exhaustive. The safety
// argument itself is unchanged — the both-slots fence wait covers
// every consumer at 2 slots. The double-fence
// wait in `VulkanContext::draw_frame` (#282) guarantees this *only*
// while waiting on both `in_flight[frame]` and `in_flight[(frame+1)
// % MAX_FRAMES_IN_FLIGHT]` is equivalent to device-idle for prior
// frames — which is true at MAX_FRAMES_IN_FLIGHT == 2 because two
// fences cover both slots. At 3+ slots the both-fences pattern
// would only cover 2 of N, leaving frame N-2's compute possibly in
// flight when frame N+1's render pass clears depth.
//
// Bumping this constant requires either:
//   (a) making the depth image per-frame-in-flight
//       (`Vec<vk::Image>` indexed by frame_index, mirroring
//       `GBuffer`'s own per-frame `images` vec), AND THEN STILL (b), OR
//   (b) extending the fence wait to cover all in-flight slots
//       (currently 2; would become MAX_FRAMES_IN_FLIGHT - 1 fences).
//
// #3643 — read that as written: **(a) alone is NOT sufficient.** The
// depth image is the resource this assert is named after, not the only
// one riding on the both-slots wait. Per-FIF-ing depth would let the
// assert be deleted while these five other non-per-FIF resources
// silently lose their only guarantee:
//
//   1. `acceleration/blas_skinned.rs`'s `blas_scratch_buffer` —
//      destroyed IMMEDIATELY (deliberately, and correctly today) on
//      growth, mid-`draw_frame`, while the *other* slot's recorded
//      `cmd_build_acceleration_structures` still holds its device
//      address. See that site's SAFETY comment.
//   2. `context/depth_capture.rs`'s `depth_capture_staging` —
//      destroyed and reallocated during frame recording, with a SAFETY
//      comment asserting no command buffer can still reference it.
//   3. `scene_buffer/upload.rs`'s `terrain_tile_buffer` — one shared
//      DEVICE_LOCAL buffer overwritten by a blocking staged copy from
//      inside `draw_frame`.
//   4. `context/screenshot.rs`'s `screenshot_staging` and
//      `depth_capture.rs`'s `depth_capture_pending_readback` —
//      single-slot host readbacks gated purely on the top-of-frame wait.
//   5. `morph_compute.rs`'s mapped `weight_buffer`, host-written by
//      `flush_pending_morph_weights` (#3244); its own regression test
//      pins "flush after the wait", and the wait it finds is this one.
//
// `FrameSync::images_in_flight` (below) carries its own version of the
// warning and is the sixth. So option (b) — or per-FIF-ing every one of
// them — is mandatory on any bump; (a) on its own only removes the
// tripwire. Treat this list the same way the depth-consumer list above
// asks to be treated: load-bearing, and re-derived rather than trusted
// as exhaustive.
//
// The const_assert below fails the workspace build if anyone
// raises the value without addressing the depth-image hazard.
const _: () = assert!(
    MAX_FRAMES_IN_FLIGHT == 2,
    "the shared VulkanContext::depth_image requires \
     MAX_FRAMES_IN_FLIGHT == 2; see #870 for the safety contract. \
     Per-FIF-ing the depth image is NOT enough to delete this assert: \
     the skinned BLAS scratch free, the depth-capture/screenshot \
     staging destroys, the terrain tile buffer and the mapped morph \
     weight buffer all rest on the same both-slots wait (#3643)"
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
    /// by the time `draw_frame` next reads it (the post-acquire
    /// image-fence wait). This is upheld upstream by the *both-slots*
    /// `wait_for_fences` at the top of `draw_frame`, which blocks on BOTH
    /// frame-in-flight fences before any image-fence read — so by the time
    /// we reach the guard, every fence in this vec is either null (image
    /// never used) or matches one of the two frame slots we just waited on.
    ///
    /// The aliasing guard `image_fence != in_flight[frame]` then skips the
    /// case where this vec already holds the current slot's own fence.
    /// That is a **redundant-wait skip, not a deadlock preventer** (#3645):
    /// since #952 moved `reset_fences` to immediately before
    /// `queue_submit`, `in_flight[frame]` is still SIGNALED here — the
    /// top-of-frame wait signalled it and nothing has reset it yet — so
    /// waiting on it would simply return immediately. Do not read this
    /// guard as what makes the early reset safe; #952 established the
    /// opposite direction (the reset is late *because* an early one
    /// stranded the fence across ~2,000 lines of fallible work).
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

        // `in_flight` is fixed-size (per frame-in-flight slot, not per
        // swapchain image), so unlike `render_finished` above it can't be
        // `clear()`d and rebuilt with `push`. Mirror the same destroy-
        // before-fallible-recreate safety instead: null out every handle
        // in its own pass first, so if `create_fence` fails partway
        // through the second pass, no `in_flight` entry can be left
        // pointing at an already-destroyed fence. `destroy_fence` on
        // `vk::Fence::null()` is a spec-defined no-op, so the null'd
        // entries are safe to destroy again (e.g. from `Drop`) even if
        // this function returns early.
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        for fence in &mut self.in_flight {
            device.destroy_fence(*fence, None);
            *fence = vk::Fence::null();
        }
        for fence in &mut self.in_flight {
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
    /// at the top of `draw_frame` would block forever.
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

    /// #2783 (REN-D4-02) — the per-swapchain-image `render_finished`
    /// contract (`548c1b69`) was prose only: six mentions across three
    /// files and not one assertion, on a rule that has already been
    /// reverted once. #906 moved these semaphores from per-image to
    /// per-frame-in-flight on a pre-2023 reading of the MAILBOX-discard
    /// rules; that replacement tripped
    /// `VUID-vkQueueSubmit-pSignalSemaphores-00067` in the Skyrim
    /// Riverwood run (3 swapchain images > 2 frames in flight under FIFO),
    /// and `548c1b69` moved it back. Nothing would catch the same swap
    /// happening a third time.
    ///
    /// Creating real semaphores needs a device, so this pins the two
    /// halves of the contract that are decidable from source — the same
    /// technique `context/draw.rs`'s untestable paths use. What breaks the
    /// VUID is a *count* keyed off the wrong quantity or a *lookup* keyed
    /// off the wrong index, and both are visible here.
    #[test]
    fn render_finished_is_sized_and_indexed_per_swapchain_image() {
        let sync_src = include_str!("sync.rs");
        let draw_src = include_str!("context/draw.rs");

        // This test scans its OWN file, so every needle searched in
        // `sync_src` is composed at runtime — a literal written here would
        // match itself and keep the assertion green after the production
        // code it guards was deleted.
        let count = "swapchain_image".to_string() + "_count";

        // (1) Sizing: both the create and the resize path must loop over
        // the swapchain image count. `MAX_FRAMES_IN_FLIGHT` here would be
        // the #906 regression exactly.
        let sizing_loop = format!("for _ in 0..{count} {{");
        let sized_from_image_count = sync_src.matches(&sizing_loop).count();
        assert_eq!(
            sized_from_image_count, 2,
            "exactly two loops must size a per-image vec from {count} \
             (create_sync_objects and recreate_for_swapchain); sizing either \
             from MAX_FRAMES_IN_FLIGHT is the #906 regression"
        );

        // (2) Indexing: the submit signals — and the present waits on —
        // the semaphore for the ACQUIRED IMAGE, never the frame slot.
        // `img` is the `image_index as usize` binding in `draw_frame`.
        assert!(
            draw_src.contains("self.frame_sync.render_finished[img]"),
            "the render submit must signal render_finished for the acquired \
             image index; keying it on the frame-in-flight slot is what \
             VUID-vkQueueSubmit-pSignalSemaphores-00067 rejects when \
             swapchain_image_count > MAX_FRAMES_IN_FLIGHT"
        );
        assert!(
            !draw_src.contains("render_finished[frame]"),
            "render_finished must never be indexed by the frame-in-flight \
             slot — that is the #906 pattern 548c1b69 reverted"
        );

        // (3) And the two must not silently diverge in size: the fence
        // aliasing tracker is per-image too, so a resize that rebuilt one
        // and not the other would leave `render_finished[img]` able to
        // index out of bounds for a grown swapchain.
        let tracker_resize = format!("self.images_in_flight = vec![vk::Fence::null(); {count}]");
        assert!(
            sync_src.contains(&tracker_resize),
            "images_in_flight is indexed by the same image index and must be \
             resized from the same count"
        );
    }

    /// #2783 — the sizing rule itself, as a value rather than a source
    /// scan: `render_finished` tracks the swapchain image count, which is
    /// independent of (and on every real device larger than)
    /// `MAX_FRAMES_IN_FLIGHT`. Pinning the two apart is the point — the
    /// bug being guarded is precisely someone reusing the frame-in-flight
    /// count because it "looks like" the right size.
    #[test]
    fn swapchain_image_count_is_not_the_frames_in_flight_count() {
        // The counts a real driver reports for FIFO / MAILBOX triple
        // buffering — the configuration that exposed VUID-00067.
        for swapchain_image_count in [2usize, 3, 4] {
            let render_finished_len = swapchain_image_count;
            let images_in_flight_len = swapchain_image_count;
            assert_eq!(render_finished_len, images_in_flight_len);
            if swapchain_image_count > MAX_FRAMES_IN_FLIGHT {
                assert!(
                    render_finished_len > MAX_FRAMES_IN_FLIGHT,
                    "with {swapchain_image_count} images the per-image vec must \
                     outgrow the {MAX_FRAMES_IN_FLIGHT} frame slots — this is the \
                     case where the per-frame pattern aliases a live semaphore"
                );
            }
        }
    }

    /// #3643 — the #870 const-assert is the project's designated tripwire
    /// for the both-slots-wait class, but its remediation list named only
    /// the shared depth image. Anyone who follows option (a), per-FIF-es
    /// depth and deletes the assert would silently break five other
    /// non-per-FIF resources that rest on the same identity.
    ///
    /// Pin the enumeration to the resources themselves: each name below is
    /// grepped straight out of the code it refers to, so deleting or
    /// renaming one of those sites without revisiting this block fails the
    /// build rather than leaving a comment that reads correct and is not.
    #[test]
    fn frames_in_flight_contract_names_every_dependent_resource() {
        const SYNC_RS: &str = include_str!("sync.rs");

        let block = SYNC_RS
            .split_once("// Bumping this constant requires either:")
            .expect("the #870 remediation block")
            .1
            .split_once("const _: () = assert!(")
            .expect("the #870 const-assert");
        let (prose, assert_message) = block;

        for (resource, owner) in [
            (
                "blas_scratch_buffer",
                include_str!("acceleration/blas_skinned.rs"),
            ),
            (
                "depth_capture_staging",
                include_str!("context/depth_capture.rs"),
            ),
            (
                "depth_capture_pending_readback",
                include_str!("context/depth_capture.rs"),
            ),
            (
                "terrain_tile_buffer",
                include_str!("scene_buffer/upload.rs"),
            ),
            ("screenshot_staging", include_str!("context/screenshot.rs")),
            ("weight_buffer", include_str!("morph_compute.rs")),
        ] {
            assert!(
                owner.contains(resource),
                "`{resource}` no longer exists at the site the #870 block \
                 points at — re-derive the list rather than deleting the \
                 entry (#3643)",
            );
            assert!(
                prose.contains(resource),
                "the #870 remediation block does not name `{resource}`, a \
                 non-per-frame-in-flight resource whose safety rests on the \
                 both-slots wait (#3643)",
            );
        }

        assert!(
            assert_message.contains("NOT enough"),
            "the #870 const-assert message must say that per-FIF-ing the \
             depth image alone is not enough to delete it (#3643)",
        );
        assert!(
            include_str!("acceleration/blas_skinned.rs").contains("*both-slots*"),
            "blas_skinned's immediate scratch free must name draw_frame's \
             BOTH-slots wait as its guarantee — the slot-local argument \
             alone is insufficient and would keep reading correct at 3+ \
             slots (#3643)",
        );
    }
}
