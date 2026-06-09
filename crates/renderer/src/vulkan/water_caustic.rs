//! Per-FIF R32_UINT accumulator image for water-side caustics
//! (#1255 / Phase C of #1210).
//!
//! The existing `caustic_splat.comp` accumulator (owned by
//! [`super::caustic::CausticPipeline`]) handles glass / MultiLayerParallax
//! refractors via a post-render-pass compute splat. That image
//! cannot be shared with `water.frag` because the caustic pipeline's
//! pre-clear barrier at `caustic.rs:720-735` runs AFTER the main
//! render pass ends — any in-render-pass writes from `water.frag`
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
use super::descriptors::{color_subresource_single_mip, image_barrier_undef_to_general};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

/// One per-FIF accumulator slot. Layout follows the same shape as
/// `caustic::CausticSlot` so the two paths stay reviewer-friendly:
/// `image` is the GPU resource, `storage_view` is the `r32ui`
/// view bound to `water.frag` for `imageAtomicAdd`, `sampled_view`
/// is the view bound to `composite.frag` as `usampler2D`.
struct Slot {
    image: vk::Image,
    storage_view: vk::ImageView,
    sampled_view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
}

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

    fn create_slot(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
        slot_idx: usize,
    ) -> Result<Slot> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(CAUSTIC_FORMAT)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        // SAFETY: `info` fully populated above (TYPE_2D, R32_UINT,
        // STORAGE | SAMPLED | TRANSFER_DST). On Err, no resource is
        // returned and no follow-on allocator state is touched.
        let image = unsafe {
            device
                .create_image(&info, None)
                .context("water-caustic image")?
        };

        let alloc = match allocator
            .lock()
            .expect("allocator lock")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name: &format!("water_caustic_accum_{slot_idx}"),
                // SAFETY: `image` just created above.
                requirements: unsafe { device.get_image_memory_requirements(image) },
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .context("water-caustic image allocate")
        {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: alloc failed; image was created but never bound.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        // SAFETY: `image` matches the memory requirements that produced
        // `alloc`; bound once per image.
        if let Err(e) = unsafe {
            device
                .bind_image_memory(image, alloc.memory(), alloc.offset())
                .context("water-caustic bind image memory")
        } {
            allocator.lock().expect("allocator lock").free(alloc).ok();
            // SAFETY: bind failed; free alloc first, then destroy unbound image.
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }

        let make_view = |img: vk::Image| -> Result<vk::ImageView> {
            // SAFETY: `img` is the bound `image` above; the same handle
            // backs both the storage and sampled views (legal because
            // they specify identical subresources + format).
            Ok(unsafe {
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(CAUSTIC_FORMAT)
                            .subresource_range(color_subresource_single_mip()),
                        None,
                    )
                    .context("water-caustic image view")?
            })
        };
        let storage_view = match make_view(image) {
            Ok(v) => v,
            Err(e) => {
                allocator.lock().expect("allocator lock").free(alloc).ok();
                // SAFETY: storage view creation failed; free alloc first,
                // destroy bound image.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };
        let sampled_view = match make_view(image) {
            Ok(v) => v,
            Err(e) => {
                // SAFETY: sampled view creation failed; tear down the
                // already-created storage view, free alloc, destroy image.
                unsafe { device.destroy_image_view(storage_view, None) };
                allocator.lock().expect("allocator lock").free(alloc).ok();
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        Ok(Slot {
            image,
            storage_view,
            sampled_view,
            allocation: Some(alloc),
        })
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

        // ── Pre-clear barrier: FRAGMENT_SHADER (prior-frame writes /
        // composite reads — both possible) → TRANSFER ──────────────
        let pre_clear = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            // First use of this slot is UNDEFINED → GENERAL via a
            // discarding layout transition. Subsequent frames go
            // GENERAL → GENERAL (no discard, no data preserved by
            // the clear that immediately follows). Either way the
            // clear writes every texel.
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot.image)
            .subresource_range(color_subresource_single_mip());
        // SAFETY: caller's unsafe-fn contract — `cmd` recording, slot
        // index valid. Pipeline-barrier args are well-formed.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[pre_clear],
            );
        }

        // ── Clear to zero (R32_UINT all-zero starting accumulator) ──
        let clear_value = vk::ClearColorValue {
            uint32: [0, 0, 0, 0],
        };
        let clear_range = color_subresource_single_mip();
        unsafe {
            device.cmd_clear_color_image(
                cmd,
                slot.image,
                vk::ImageLayout::GENERAL,
                &clear_value,
                &[clear_range],
            );
        }

        // ── Post-clear barrier: TRANSFER → FRAGMENT_SHADER ─────────
        // water.frag's `imageAtomicAdd` is FRAGMENT-stage SHADER_WRITE;
        // it must see the zeroed image after the clear retires.
        let post_clear = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot.image)
            .subresource_range(clear_range);
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[post_clear],
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
        let bar = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot.image)
            .subresource_range(color_subresource_single_mip());
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
        self.slots[frame].storage_view
    }

    /// Sampled view for the per-FIF slot — bound by composite as
    /// `usampler2D` (NEAREST sampler, per composite.rs's existing
    /// integer-format-sampling rule).
    pub fn sampled_view(&self, frame: usize) -> vk::ImageView {
        self.slots[frame].sampled_view
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
    unsafe fn destroy_slot(device: &ash::Device, allocator: &SharedAllocator, slot: Slot) {
        // SAFETY: caller's contract — no in-flight refs.
        unsafe {
            device.destroy_image_view(slot.storage_view, None);
            device.destroy_image_view(slot.sampled_view, None);
            device.destroy_image(slot.image, None);
        }
        if let Some(a) = slot.allocation {
            allocator.lock().expect("allocator lock").free(a).ok();
        }
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
