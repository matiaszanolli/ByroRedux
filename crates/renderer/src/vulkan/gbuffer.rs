//! G-Buffer attachments for SVGF denoising.
//!
//! The auxiliary render targets written by the main render pass alongside
//! the HDR color intermediate are enumerated below. They carry the per-pixel
//! geometric and material data that downstream SVGF, FSR, and composite
//! passes need:
//!
//! | Attachment    | Format             | Contents                                     |
//! |---------------|--------------------|----------------------------------------------|
//! | normal        | RG16_SNORM         | Octahedral-encoded world-space normal (#275)  |
//! | motion        | RG16_SFLOAT        | Screen-space motion vector (current→prev)     |
//! | mesh_id       | R32_UINT           | Stable surface ID / alpha draw lookup         |
//! | raw_indirect  | B10G11R11_UFLOAT   | Pre-denoise indirect light (albedo-demod)     |
//! | albedo        | B10G11R11_UFLOAT   | Surface color for composite re-multiplication |
//! | reactive      | R8_UNORM           | FSR reactive mask (transparent coverage)      |
//! | transparency  | R8_UNORM           | FSR transparency & composition mask           |
//!
//! ## Per-frame-in-flight
//!
//! Like the HDR color image, each G-buffer attachment has one image per
//! frame-in-flight slot. This eliminates cross-frame read-after-write
//! hazards when later SVGF compute passes sample these attachments.
//!
//! ## Layout
//!
//! Created with COLOR_ATTACHMENT | SAMPLED usage. After the main render
//! pass ends, they are in SHADER_READ_ONLY_OPTIMAL layout (set by the
//! render pass's final_layout), ready to be sampled by SVGF compute
//! shaders in later phases.

use super::allocator::SharedAllocator;
use super::image::{GpuImage, GpuImageDesc};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::Result;
use ash::vk;

/// Octahedral-encoded normal (2 channels). RGBA16_SNORM→RG16_SNORM saves
/// 50% bandwidth (4B vs 8B/pixel). The fragment shader encodes via
/// octahedral projection; consumers decode with the inverse. See #275.
/// Render-extent VRAM this G-buffer holds, per pixel, across all frames in
/// flight (#3992).
///
/// Five 4-byte attachments (normal `R16G16_SNORM`, motion `R16G16_SFLOAT`,
/// mesh-id `R32_UINT`, raw-indirect and albedo `B10G11R11_UFLOAT_PACK32`) plus
/// the two 1-byte FSR masks (`R8_UNORM`) = 22 B/px, doubled per frame in
/// flight. Published so `screen_scaled_reservation_bytes` can bill it from the
/// owning pass rather than a hand-copied figure, the discipline
/// `SVGF_BYTES_PER_PIXEL` already sets. `memory-budget.md`'s roll-up row
/// carries the same 22 B/px x 2 FIF derivation.
// `5 * 4`: the five 4-byte attachments. `2`: the two 1-byte FSR masks
// (written as a bare `2`, not `2 * 1`, per clippy::identity_op — the doc
// comment above states the "two 1-byte" derivation this constant encodes).
pub const GBUFFER_BYTES_PER_PIXEL: u32 = (5 * 4 + 2) * super::sync::MAX_FRAMES_IN_FLIGHT as u32;

pub const NORMAL_FORMAT: vk::Format = vk::Format::R16G16_SNORM;
pub const MOTION_FORMAT: vk::Format = vk::Format::R16G16_SFLOAT;
/// Stable surface ID for opaque SVGF / TAA disocclusion, and current-frame
/// instance index for alpha-blended caustic-source lookups. Alpha draws set
/// bit 31 and bypass temporal history, so the two low-bit meanings cannot be
/// confused by a consumer. Pre-#992 this was `R16_UINT` — with bit 15 reserved for
/// the `ALPHA_BLEND_NO_HISTORY` flag, the encoding capped at 32767
/// addressable instances (`0x7FFF`). Dense Skyrim/FO4 city cells
/// (Solitude, Whiterun draw distance, Diamond City) exceed that
/// ceiling and would silently wrap to meshId 0 (the sky sentinel),
/// misrouting every shadow / reflection / SVGF query against the
/// wrapped instance. Now `R32_UINT`: bit 31 carries the alpha-blend
/// flag, bits 0..30 carry the stable ID or instance ID + 1, capping the encoding
/// at `0x7FFFFFFF` (~2.1G — effectively unbounded). VRAM cost is
/// modest (+4.15 MB at 1080p × 2 frames = +8.3 MB on a 6 GB target).
pub const MESH_ID_FORMAT: vk::Format = vk::Format::R32_UINT;
/// Raw (pre-denoise) indirect light, albedo-demodulated. Written by the
/// main render pass, sampled by SVGF temporal pass (Phase 3+) and the
/// composite pass. R11G11B10F = 4 bytes/pixel, plenty of precision for
/// HDR diffuse bounce without alpha.
pub const RAW_INDIRECT_FORMAT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;
/// Surface albedo (diffuse color × vertex color). Written by the main
/// render pass and re-multiplied in the composite pass to recover
/// texture detail after SVGF blurs the demodulated indirect light.
pub const ALBEDO_FORMAT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;
/// FSR reactive and transparency-and-composition masks (AMD's FSR 3.1
/// upscaler integration contract). Both are single-channel 0..1 coverage
/// signals cleared to zero and MAX-blended by transparent draws, so a stack
/// of overlapping translucent surfaces reports the most reactive one rather
/// than the last one drawn.
///
/// `R8_UNORM` is what the SDK's own samples use; the mask is a hint that
/// biases history rejection, so 8 bits of coverage is ample.
pub const FSR_MASK_FORMAT: vk::Format = vk::Format::R8_UNORM;

/// A single G-buffer attachment slot (one image per frame-in-flight).
/// #3860 — was three parallel `Vec`s (`images`, `views`, `allocations`) whose
/// indices had to be kept in step by hand; #2178 is what happens when they
/// come apart (a sub-allocation stranded on bind failure). One
/// `Vec<GpuImage>` makes the correspondence structural.
struct Attachment {
    slots: Vec<GpuImage>,
}

impl Attachment {
    fn new_empty() -> Self {
        Self { slots: Vec::new() }
    }

    fn allocate(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        format: vk::Format,
        width: u32,
        height: u32,
        name_prefix: &str,
    ) -> Result<()> {
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            // #3860 — was ~85 lines of create → allocate → bind → view with
            // its own three-arm cleanup. #2178 (PERF-D3-03) was diagnosed in
            // this copy: on bind failure the sub-allocation was stranded. Its
            // comment read "Same shape as the sibling site in
            // `frame_upscaler.rs::create_outputs` and the established pattern
            // in `exposure.rs`" — three copies hand-checked for one fix.
            self.slots.push(GpuImage::create(
                device,
                allocator,
                &GpuImageDesc::color_2d(
                    &format!("{name_prefix}{i}"),
                    width,
                    height,
                    format,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                ),
            )?);
        }
        Ok(())
    }

    /// # Safety
    ///
    /// No in-flight command buffer or descriptor set may still reference
    /// any image, image view, or allocation owned by this `Attachment` —
    /// i.e. the caller must have fenced/idled the device (or otherwise
    /// proven no outstanding GPU work touches them) before calling this.
    /// `device` must be the same logical device the images/views/
    /// allocations were created against.
    unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for mut slot in self.slots.drain(..) {
            // #3860 — view, image and slab in one call, in that order.
            slot.destroy(device, allocator);
        }
    }
}

// #3860 — `Attachment` no longer needs a `Drop` of its own.
//
// The old one could only *scream*: its doc said it "doesn't stash device or
// allocator handles internally (the parent `GBuffer::destroy` passes them
// in), so the safety net can't clean up by itself; it can only scream so the
// leak surfaces in tests and dev logs" (REN-D2-NEW-01). The leak it was
// screaming about was the largest in the renderer — up to 42 Vulkan handles
// per `GBuffer` (7 attachments × 2 frames in flight × image + view + alloc).
//
// Each `GpuImage` now carries its own cheap Arc-backed device and allocator
// clones, so the same escape is *recovered* rather than merely reported, and
// the warning still fires once per leaked image.

/// Owns the G-buffer attachment images (normal, motion, mesh_id,
/// raw_indirect, albedo, reactive, transparency) + their views and
/// allocations. One image per frame-in-flight slot for each attachment.
pub struct GBuffer {
    normal: Attachment,
    motion: Attachment,
    mesh_id: Attachment,
    raw_indirect: Attachment,
    albedo: Attachment,
    reactive: Attachment,
    transparency: Attachment,
    pub width: u32,
    pub height: u32,
}

impl GBuffer {
    /// Create all G-buffer attachments at the given extent.
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let mut gb = Self {
            normal: Attachment::new_empty(),
            motion: Attachment::new_empty(),
            mesh_id: Attachment::new_empty(),
            raw_indirect: Attachment::new_empty(),
            albedo: Attachment::new_empty(),
            reactive: Attachment::new_empty(),
            transparency: Attachment::new_empty(),
            width,
            height,
        };

        // If any allocation fails, clean up everything allocated so far.
        let r1 = gb
            .normal
            .allocate(device, allocator, NORMAL_FORMAT, width, height, "gb_normal");
        let r2 = gb
            .motion
            .allocate(device, allocator, MOTION_FORMAT, width, height, "gb_motion");
        let r3 = gb.mesh_id.allocate(
            device,
            allocator,
            MESH_ID_FORMAT,
            width,
            height,
            "gb_mesh_id",
        );
        let r4 = gb.raw_indirect.allocate(
            device,
            allocator,
            RAW_INDIRECT_FORMAT,
            width,
            height,
            "gb_raw_indirect",
        );
        let r5 = gb
            .albedo
            .allocate(device, allocator, ALBEDO_FORMAT, width, height, "gb_albedo");
        let r6 = gb.reactive.allocate(
            device,
            allocator,
            FSR_MASK_FORMAT,
            width,
            height,
            "gb_fsr_reactive",
        );
        let r7 = gb.transparency.allocate(
            device,
            allocator,
            FSR_MASK_FORMAT,
            width,
            height,
            "gb_fsr_transparency",
        );
        if let Err(e) = r1.and(r2).and(r3).and(r4).and(r5).and(r6).and(r7) {
            // SAFETY: `gb` is local to this function; no command buffer or
            // descriptor set has had a chance to reference it yet because
            // we never returned the partial result. Cleanup path on
            // partial-allocate failure.
            unsafe { gb.destroy(device, allocator) };
            return Err(e);
        }

        log::info!(
            "G-buffer created: {}x{} (normal + motion + mesh_id + raw_indirect + albedo \
             + fsr reactive/transparency, {} frames)",
            width,
            height,
            MAX_FRAMES_IN_FLIGHT
        );
        Ok(gb)
    }

    /// Image view for the normal attachment in the given frame-in-flight slot.
    pub fn normal_view(&self, frame: usize) -> vk::ImageView {
        self.normal.slots[frame].view
    }
    /// Image view for the motion vector attachment in the given frame slot.
    pub fn motion_view(&self, frame: usize) -> vk::ImageView {
        self.motion.slots[frame].view
    }
    /// Image handle for FSR motion-vector input in the given frame slot.
    pub fn motion_image(&self, frame: usize) -> vk::Image {
        self.motion.slots[frame].image
    }
    /// Image view for the mesh ID attachment in the given frame slot.
    pub fn mesh_id_view(&self, frame: usize) -> vk::ImageView {
        self.mesh_id.slots[frame].view
    }
    /// Image view for the raw (pre-denoise) indirect light, per frame.
    pub fn raw_indirect_view(&self, frame: usize) -> vk::ImageView {
        self.raw_indirect.slots[frame].view
    }
    /// Image view for the albedo attachment in the given frame slot.
    pub fn albedo_view(&self, frame: usize) -> vk::ImageView {
        self.albedo.slots[frame].view
    }
    /// Image view for the FSR reactive mask in the given frame slot.
    pub fn reactive_view(&self, frame: usize) -> vk::ImageView {
        self.reactive.slots[frame].view
    }
    /// Image handle for the FSR reactive-mask dispatch input.
    pub fn reactive_image(&self, frame: usize) -> vk::Image {
        self.reactive.slots[frame].image
    }
    /// Image view for the FSR transparency-and-composition mask.
    pub fn transparency_view(&self, frame: usize) -> vk::ImageView {
        self.transparency.slots[frame].view
    }
    /// Image handle for the FSR transparency-and-composition dispatch input.
    pub fn transparency_image(&self, frame: usize) -> vk::Image {
        self.transparency.slots[frame].image
    }

    /// One-time layout transition UNDEFINED → SHADER_READ_ONLY_OPTIMAL for
    /// every G-buffer image across all frame-in-flight slots. Call once after
    /// `new()` so that the "previous frame" images are in a valid layout on
    /// the very first frame — SVGF's temporal pass binds them for sampling.
    /// Without this, the first frame produces a validation error:
    /// `VkImage expects SHADER_READ_ONLY_OPTIMAL, current layout is UNDEFINED`.
    ///
    /// # Safety
    /// Device, queue and command pool must be valid. The queue must support
    /// graphics operations (for the layout transition pipeline barrier).
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> anyhow::Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let attachments = [
                &self.normal,
                &self.motion,
                &self.mesh_id,
                &self.raw_indirect,
                &self.albedo,
                &self.reactive,
                &self.transparency,
            ];
            let mut barriers = Vec::with_capacity(attachments.len() * MAX_FRAMES_IN_FLIGHT);
            for att in &attachments {
                for &img in att.slots.iter().map(|s| &s.image) {
                    // #4221 — field-for-field identical to the shared
                    // helper; use it instead of a hand-rolled copy.
                    barriers.push(super::descriptors::image_barrier_undef_to_shader_read(img));
                }
            }
            // NONE as srcStageMask: UNDEFINED → SHADER_READ_ONLY transitions
            // discard prior content so there are no previous writes to expose.
            // NONE is the Vulkan 1.3 replacement for the deprecated use of
            // TOP_OF_PIPE as a source stage in memory barriers (#949 / #1100).
            // SAFETY: barriers are well-formed, device and cmd are valid.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER
                        | vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            }
            Ok(())
        })
    }

    /// Recreate all attachments at a new extent (called on swapchain resize).
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
    ) -> Result<()> {
        // SAFETY: caller (`recreate_on_resize`) is invoked from the
        // swapchain-resize path which fences both frames-in-flight first
        // (see `VulkanContext::recreate_swapchain`). No GPU work is
        // referencing the old attachments at this point.
        unsafe {
            self.normal.destroy(device, allocator);
            self.motion.destroy(device, allocator);
            self.mesh_id.destroy(device, allocator);
            self.raw_indirect.destroy(device, allocator);
            self.albedo.destroy(device, allocator);
            self.reactive.destroy(device, allocator);
            self.transparency.destroy(device, allocator);
        }
        self.width = width;
        self.height = height;
        let result = self
            .normal
            .allocate(device, allocator, NORMAL_FORMAT, width, height, "gb_normal")
            .and_then(|()| {
                self.motion
                    .allocate(device, allocator, MOTION_FORMAT, width, height, "gb_motion")
            })
            .and_then(|()| {
                self.mesh_id.allocate(
                    device,
                    allocator,
                    MESH_ID_FORMAT,
                    width,
                    height,
                    "gb_mesh_id",
                )
            })
            .and_then(|()| {
                self.raw_indirect.allocate(
                    device,
                    allocator,
                    RAW_INDIRECT_FORMAT,
                    width,
                    height,
                    "gb_raw_indirect",
                )
            })
            .and_then(|()| {
                self.albedo
                    .allocate(device, allocator, ALBEDO_FORMAT, width, height, "gb_albedo")
            })
            .and_then(|()| {
                self.reactive.allocate(
                    device,
                    allocator,
                    FSR_MASK_FORMAT,
                    width,
                    height,
                    "gb_fsr_reactive",
                )
            })
            .and_then(|()| {
                self.transparency.allocate(
                    device,
                    allocator,
                    FSR_MASK_FORMAT,
                    width,
                    height,
                    "gb_fsr_transparency",
                )
            });
        if let Err(ref e) = result {
            log::error!("G-buffer recreate partial failure: {e} — destroying partial state");
            // SAFETY: same as the destroy at the top of this function —
            // resize path is fenced; the partially-reallocated state is
            // not referenced by any in-flight command buffer.
            unsafe { self.destroy(device, allocator) };
        }
        result
    }

    /// Destroy all images, views, and allocations. Safe to call multiple times.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` and `allocator` are valid and live, the
    /// device is not lost, and that none of the G-buffer images are still in
    /// use by an in-flight command buffer.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // SAFETY: forwarding the unsafe-fn contract — the caller of
        // `GBuffer::destroy` is responsible for ensuring no in-flight
        // command buffer or descriptor binding references any attachment.
        // The per-attachment `destroy` calls below carry the same
        // requirement.
        unsafe {
            self.normal.destroy(device, allocator);
            self.motion.destroy(device, allocator);
            self.mesh_id.destroy(device, allocator);
            self.raw_indirect.destroy(device, allocator);
            self.albedo.destroy(device, allocator);
            self.reactive.destroy(device, allocator);
            self.transparency.destroy(device, allocator);
        }
    }
}
