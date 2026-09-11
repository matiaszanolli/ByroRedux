//! Per-FIF R32_UINT accumulator image for water-side caustics
//! (#1255 / Phase C of #1210).
//!
//! The existing `caustic_splat.comp` accumulator (owned by
//! [`super::caustic::CausticPipeline`]) handles glass / MultiLayerParallax
//! refractors via a post-render-pass compute splat. That image
//! cannot be shared with `water.frag` because the caustic pipeline's
//! pre-clear barrier — the moving-camera arm of
//! [`super::caustic::CausticPipeline::dispatch`], and the skip-streak
//! [`super::caustic::CausticPipeline::clear_for_skip`] — runs AFTER the
//! main render pass ends; any in-render-pass writes from `water.frag`
//! would be wiped before `caustic_splat.comp` accumulates.
//!
//! This module owns a dedicated sibling image with the inverse pass
//! ordering: cleared BEFORE the main render pass begins (so
//! `water.frag`'s `imageAtomicAdd` accumulates), then sampled by
//! `composite.frag` alongside the existing `causticTex`.
//!
//! Scope (Phase C — #1255):
//!   * image + view + allocation per FIF slot
//!   * pre-clear command + TRANSFER → FRAGMENT_SHADER barrier
//!   * post-water FRAGMENT_SHADER write → FRAGMENT_SHADER read barrier
//!     (for composite)
//!   * resize / destroy lifecycle
//!
//! Descriptor wiring (write-side at WaterPipeline + read-side at
//! composite) is intentionally OUT of this module — each pipeline
//! owns its own descriptor sets. This module exposes the
//! `image()` / `storage_view()` / `sampled_view()` accessors so
//! callers can wire their own descriptor writes against the
//! correct per-FIF resource.

use super::allocator::SharedAllocator;
use super::caustic::CAUSTIC_FORMAT;
use super::descriptors::{
    color_subresource_single_mip, image_barrier_general_write_to_read,
    image_barrier_undef_to_general,
};
use super::image::{GpuImage, GpuImageDesc};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::Result;
use ash::vk;

/// One per-FIF accumulator slot. Layout follows the same shape as
/// `caustic::CausticSlot` so the two paths stay reviewer-friendly:
/// `image` is the GPU resource, `storage_view` is the `r32ui`
/// view bound to `water.frag` for `imageAtomicAdd`, `sampled_view`

/// #3860 — an accumulator slot is an owned image plus its view, so it is a
/// [`GpuImage`].
///
/// **This collapses two byte-identical views into one, which is #2779's fix
/// applied to the file that sibling missed.** `storage_view` and
/// `sampled_view` were built by the *same closure* here — same image, view
/// type, format and subresource range — and the code said as much ("the same
/// handle backs both the storage and sampled views (legal because they
/// specify identical subresources + format)"). #2779 established for
/// `caustic.rs` that a view carries no usage or layout state, since the
/// descriptor type and the barrier's `image_layout` supply both, so nothing
/// distinguished the pair; the same reasoning holds verbatim here. Both
/// accessors are kept — their call sites mean different things even though
/// the handle is now the same one — and each frame in flight stops paying for
/// a redundant `VkImageView` and a second destroy.
type Slot = GpuImage;

/// Per-frame water-side caustic accumulator (Phase C of #1210).
///
/// Owns one image per frame-in-flight in `vk::ImageLayout::GENERAL`
/// throughout (compatible with `cmd_clear_color_image`,
/// `imageAtomicAdd`, and `usampler2D` reads — same convention as
/// `CausticPipeline`).
pub struct WaterCausticAccum {
    slots: Vec<Slot>,
    pub width: u32,
    pub height: u32,
}

impl WaterCausticAccum {
    /// Create one image + storage view + sampled view per FIF slot
    /// at `width × height`. On any per-slot failure, all
    /// already-created slots are torn down before returning the
    /// error so no partial resource set leaks.
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let mut slots: Vec<Slot> = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            match Self::create_slot(device, allocator, width, height, i) {
                Ok(slot) => slots.push(slot),
                Err(e) => {
                    // Tear down any already-created slots before
                    // returning. SAFETY: per-slot resources were just
                    // created; no in-flight command buffer references
                    // them yet (we haven't returned `self`).
                    for s in slots.drain(..) {
                        // SAFETY: the slot's image/view handles were created by this
                        // device and are torn down here with the device idle (init
                        // cleanup before returning `self`, or resize after
                        // device_wait_idle), so no in-flight command buffer references them.
                        unsafe { Self::destroy_slot(device, allocator, s) };
                    }
                    return Err(e);
                }
            }
        }
        Ok(Self {
            slots,
            width,
            height,
        })
    }

    /// #3860 — was ~100 lines of create → allocate → bind → view ×2 with a
    /// four-arm cleanup; `GpuImage::create` owns that chain now, and the
    /// second view is gone (see [`Slot`]).
    fn create_slot(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
        slot_idx: usize,
    ) -> Result<Slot> {
        GpuImage::create(
            device,
            allocator,
            &GpuImageDesc::color_2d(
                &format!("water_caustic_accum_{slot_idx}"),
                width,
                height,
                CAUSTIC_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ),
        )
    }

    /// One-time UNDEFINED → GENERAL transition on every per-FIF slot
    /// so the first `clear_pre_render_pass` (which uses
    /// `oldLayout = GENERAL`) doesn't trip
    /// VUID-vkCmdDraw-None-09600. Mirror of
    /// `CausticPipeline::initialize_layouts` — both this and the
    /// caustic accumulator are freshly-created in `UNDEFINED` per
    /// `vk::ImageCreateInfo` spec.
    ///
    /// Call ONCE after [`Self::new`] AND after
    /// [`Self::recreate_on_resize`].
    ///
    /// # Safety
    /// Device + queue + pool must be valid; queue must support
    /// graphics/transfer (for pipeline barriers via the
    /// `with_one_time_commands` fenced submit).
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let mut barriers = Vec::with_capacity(self.slots.len());
            for slot in &self.slots {
                barriers.push(image_barrier_undef_to_general(slot.image));
            }
            // SAFETY: caller's unsafe-fn contract. NONE as srcStageMask
            // on UNDEFINED → GENERAL transitions: there are no prior
            // writes to make visible. dstStage = FRAGMENT_SHADER because
            // water.frag is the first reader/writer (not COMPUTE_SHADER
            // like the caustic version — different consumer pipeline).
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            }
            Ok(())
        })
    }

    /// Clear the per-FIF accumulator + sequence the barriers so
    /// `water.frag` can subsequently `imageAtomicAdd` to it.
    ///
    /// Call ONCE per frame BEFORE `vkCmdBeginRenderPass` on the
    /// main render pass. The output state on the image is
    /// `GENERAL` layout with `SHADER_READ | SHADER_WRITE` access
    /// visible at the `FRAGMENT_SHADER` stage — water.frag can
    /// begin atomic-adding immediately.
    ///
    /// # Safety
    /// Caller guarantees `cmd` is in the recording state and
    /// `frame < MAX_FRAMES_IN_FLIGHT`.
    pub unsafe fn clear_pre_render_pass(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        debug_assert!(frame < self.slots.len(), "frame index out of range");
        let slot = &self.slots[frame];

        // ── Zero the slot ───────────────────────────────────────────
        // #3844 — this used to hand-roll the barrier/clear/barrier sandwich
        // with a `FRAGMENT_SHADER`-only source scope. That is the shape
        // #3646/#3647 corrected on the caustic and volumetrics accumulators
        // five weeks earlier; this copy was not in that commit's field of
        // view, and the pin test written to stop the drift enumerated only
        // those two files. Routing through the shared helper is what makes
        // the omission unrepresentable: it puts `TRANSFER` / `TRANSFER_WRITE`
        // into the source scope structurally, so the prior visit's clear on
        // this slot chains into this one. That case is not exotic here — this
        // runs unconditionally every frame the accumulator exists, so a frame
        // where water.frag never ran leaves clear-then-clear as the normal
        // sequence, not an edge case.
        //
        // Consumers are fragment-only: water.frag's `imageAtomicAdd` and
        // composite.frag's `texelFetch`. No compute stage touches this
        // accumulator — `caustic_splat.comp` writes the *other* one, owned by
        // `CausticPipeline`.
        //
        // The helper never performs the discarding UNDEFINED → GENERAL
        // transition, and could not. [`Self::initialize_layouts`] does that
        // once per FIF slot on a fenced one-time submit before any frame,
        // which is why `oldLayout = GENERAL` is legal on a slot's first use
        // (VUID-vkCmdDraw-None-09600). Do not delete that call believing this
        // covers it (#4037).
        // SAFETY: caller's unsafe-fn contract — `cmd` is recording and
        // `frame` is in range; `slot.image` is device-owned and in GENERAL.
        unsafe {
            super::descriptors::clear_general_accumulator(
                device,
                cmd,
                slot.image,
                color_subresource_single_mip(),
                vk::ClearColorValue {
                    uint32: [0, 0, 0, 0],
                },
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            );
        }
    }

    /// Emit the post-render-pass barrier so `composite.frag` sees
    /// water.frag's atomic-add writes. Call ONCE per frame between
    /// `vkCmdEndRenderPass` on the main pass and the composite-pass
    /// descriptor read.
    ///
    /// # Safety
    /// Same as [`Self::clear_pre_render_pass`].
    pub unsafe fn barrier_post_render_pass(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        debug_assert!(frame < self.slots.len(), "frame index out of range");
        let slot = &self.slots[frame];
        let bar = image_barrier_general_write_to_read(slot.image);
        // SAFETY: caller's unsafe-fn contract — `cmd` recording.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[bar],
            );
        }
    }

    /// Storage view for the per-FIF slot — bound by WaterPipeline as
    /// `r32ui uimage2D` for `imageAtomicAdd`.
    pub fn storage_view(&self, frame: usize) -> vk::ImageView {
        self.slots[frame].view
    }

    /// Sampled view for the per-FIF slot — bound by composite as
    /// `usampler2D` (NEAREST sampler, per composite.rs's existing
    /// integer-format-sampling rule).
    pub fn sampled_view(&self, frame: usize) -> vk::ImageView {
        self.slots[frame].view
    }

    /// Recreate every slot at a new resolution. Caller must have
    /// idled the device (`device.device_wait_idle()`) so no in-flight
    /// command buffer references the old resources. The
    /// `VulkanContext::recreate_swapchain` path already does this
    /// — call from there.
    ///
    /// # Safety
    /// Same as `Self::destroy`. On per-slot recreate failure, the
    /// old slot is freed but a new one is NOT created in its place,
    /// leaving `self.slots` shorter than `MAX_FRAMES_IN_FLIGHT`.
    /// Caller should treat this as fatal for the water-caustic
    /// pipeline.
    pub unsafe fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
    ) -> Result<()> {
        // SAFETY: caller idled the device — no in-flight cmd buffer
        // references the old slots.
        for slot in self.slots.drain(..) {
            // SAFETY: the slot's image/view handles were created by this
            // device and are torn down here with the device idle (init
            // cleanup before returning `self`, or resize after
            // device_wait_idle), so no in-flight command buffer references them.
            unsafe { Self::destroy_slot(device, allocator, slot) };
        }
        self.width = width;
        self.height = height;
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let slot = Self::create_slot(device, allocator, width, height, i)?;
            self.slots.push(slot);
        }
        Ok(())
    }

    /// Tear down a single slot. Used both by [`Self::destroy`] and
    /// by the failure-cleanup paths in [`Self::new`] +
    /// [`Self::recreate_on_resize`].
    ///
    /// # Safety
    /// Caller guarantees no in-flight command buffer references any
    /// resource owned by `slot`.
    /// #3860 — `GpuImage::destroy` frees the allocation before destroying the
    /// image and takes the allocator lock exactly once.
    ///
    /// # Safety
    /// Caller must ensure no in-flight command buffer references the slot.
    unsafe fn destroy_slot(device: &ash::Device, allocator: &SharedAllocator, mut slot: Slot) {
        slot.destroy(device, allocator);
    }

    /// # Safety
    /// Must be called before the device + allocator are dropped, and
    /// after the device has been idled so no in-flight command buffer
    /// references any owned resource.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for slot in self.slots.drain(..) {
            // SAFETY: caller's unsafe-fn contract.
            unsafe { Self::destroy_slot(device, allocator, slot) };
        }
    }
}
