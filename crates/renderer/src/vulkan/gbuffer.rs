//! G-Buffer attachments for SVGF denoising.
//!
//! Five auxiliary render targets written by the main render pass alongside
//! the HDR color intermediate. These carry the per-pixel geometric and
//! material data that downstream SVGF and composite passes need:
//!
//! | Attachment    | Format             | Contents                                     |
//! |---------------|--------------------|----------------------------------------------|
//! | normal        | RG16_SNORM         | Octahedral-encoded world-space normal (#275)  |
//! | motion        | RG16_SFLOAT        | Screen-space motion vector (current→prev)     |
//! | mesh_id       | R32_UINT           | Per-instance ID (disocclusion detection)      |
//! | raw_indirect  | B10G11R11_UFLOAT   | Pre-denoise indirect light (albedo-demod)     |
//! | albedo        | B10G11R11_UFLOAT   | Surface color for composite re-multiplication |
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
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

/// Octahedral-encoded normal (2 channels). RGBA16_SNORM→RG16_SNORM saves
/// 50% bandwidth (4B vs 8B/pixel). The fragment shader encodes via
/// octahedral projection; consumers decode with the inverse. See #275.
pub const NORMAL_FORMAT: vk::Format = vk::Format::R16G16_SNORM;
pub const MOTION_FORMAT: vk::Format = vk::Format::R16G16_SFLOAT;
/// Per-instance ID for SVGF / TAA disocclusion + caustic source
/// lookups. Pre-#992 this was `R16_UINT` — with bit 15 reserved for
/// the `ALPHA_BLEND_NO_HISTORY` flag, the encoding capped at 32767
/// addressable instances (`0x7FFF`). Dense Skyrim/FO4 city cells
/// (Solitude, Whiterun draw distance, Diamond City) exceed that
/// ceiling and would silently wrap to meshId 0 (the sky sentinel),
/// misrouting every shadow / reflection / SVGF query against the
/// wrapped instance. Now `R32_UINT`: bit 31 carries the alpha-blend
/// flag, bits 0..30 carry the instance ID + 1, capping the encoding
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

/// A single G-buffer attachment slot (one image per frame-in-flight).
struct Attachment {
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    allocations: Vec<Option<vk_alloc::Allocation>>,
}

impl Attachment {
    fn new_empty() -> Self {
        Self {
            images: Vec::new(),
            views: Vec::new(),
            allocations: Vec::new(),
        }
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
            let img_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            // SAFETY: `img_info` is a fully-populated builder with extent /
            // format / usage set above. `device` is the engine's live ash
            // device. On Ok, `img` becomes a fresh handle owned by this
            // attachment vec and freed by `destroy()` below.
            let img = unsafe {
                device
                    .create_image(&img_info, None)
                    .with_context(|| format!("Failed to create {name_prefix} image"))?
            };
            self.images.push(img);

            let alloc = allocator
                .lock()
                .expect("allocator lock")
                .allocate(&vk_alloc::AllocationCreateDesc {
                    name: &format!("{name_prefix}_{i}"),
                    // SAFETY: `img` was just created on the previous line
                    // and pushed into `self.images`; the handle is live
                    // until `destroy()` releases it.
                    requirements: unsafe { device.get_image_memory_requirements(img) },
                    location: gpu_allocator::MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                })
                .with_context(|| format!("Failed to allocate {name_prefix} memory"))?;
            // SAFETY: `img` is the freshly-created image; `alloc.memory()`
            // is the matching allocation gpu-allocator returned for `img`'s
            // memory requirements. Bound once — the `Drop` path on `alloc`
            // is the only thing that releases the memory, and we own `alloc`
            // in `self.allocations` until `destroy()` runs.
            unsafe {
                device
                    .bind_image_memory(img, alloc.memory(), alloc.offset())
                    .with_context(|| format!("bind {name_prefix} image memory"))?;
            }
            self.allocations.push(Some(alloc));

            // SAFETY: `img` is bound to backing memory (line above) and
            // the view-create-info references its format / aspect / mips.
            // The resulting view is owned by `self.views` until `destroy()`.
            let view = unsafe {
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(format)
                            .subresource_range(super::descriptors::color_subresource_single_mip()),
                        None,
                    )
                    .with_context(|| format!("Failed to create {name_prefix} image view"))?
            };
            self.views.push(view);
        }
        Ok(())
    }

    unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for &view in &self.views {
            // SAFETY: caller of `destroy` (an `unsafe fn`) guarantees no
            // in-flight command buffer or descriptor set references `view`.
            unsafe { device.destroy_image_view(view, None) };
        }
        self.views.clear();
        for &img in &self.images {
            // SAFETY: same caller contract as `destroy_image_view` — the
            // view-destroy above already broke any descriptor-bound
            // references, and the caller fences the queue separately.
            unsafe { device.destroy_image(img, None) };
        }
        self.images.clear();
        for alloc in self.allocations.drain(..) {
            if let Some(a) = alloc {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
    }
}

impl Drop for Attachment {
    /// Safety net for `Attachment`'s manual `destroy(device, allocator)`
    /// contract. Mirrors the `GpuBuffer::Drop` pattern (#656) without
    /// the recovery branch — `Attachment` doesn't stash device or
    /// allocator handles internally (the parent `GBuffer::destroy`
    /// passes them in), so the safety net can't clean up by itself;
    /// it can only scream so the leak surfaces in tests and dev logs.
    ///
    /// `debug_assert!` fires in tests + dev builds the moment any
    /// path drops a populated `Attachment` without calling
    /// `destroy()` first; the `log::error!` carries the same signal
    /// into release builds. Pre-fix release builds silently leaked
    /// (5 attachments × 2 FIF slots × image + view + alloc per
    /// attachment = up to 30 leaked Vulkan handles per `GBuffer`).
    /// See REN-D2-NEW-01 (audit 2026-05-09).
    fn drop(&mut self) {
        if self.images.is_empty() && self.views.is_empty() && self.allocations.is_empty() {
            return;
        }
        log::error!(
            "Attachment leaked into Drop: {} images, {} views, {} allocations — \
             destroy(device, allocator) was not called. See REN-D2-NEW-01.",
            self.images.len(),
            self.views.len(),
            self.allocations.len(),
        );
        debug_assert!(false, "Attachment dropped without destroy()");
    }
}

/// Owns the G-buffer attachment images (normal, motion, mesh_id,
/// raw_indirect, albedo) + their views and allocations. One image per
/// frame-in-flight slot for each attachment.
pub struct GBuffer {
    normal: Attachment,
    motion: Attachment,
    mesh_id: Attachment,
    raw_indirect: Attachment,
    albedo: Attachment,
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
        if let Err(e) = r1.and(r2).and(r3).and(r4).and(r5) {
            // SAFETY: `gb` is local to this function; no command buffer or
            // descriptor set has had a chance to reference it yet because
            // we never returned the partial result. Cleanup path on
            // partial-allocate failure.
            unsafe { gb.destroy(device, allocator) };
            return Err(e);
        }

        log::info!(
            "G-buffer created: {}x{} (normal + motion + mesh_id + raw_indirect + albedo, {} frames)",
            width, height, MAX_FRAMES_IN_FLIGHT
        );
        Ok(gb)
    }

    /// Image view for the normal attachment in the given frame-in-flight slot.
    pub fn normal_view(&self, frame: usize) -> vk::ImageView {
        self.normal.views[frame]
    }
    /// Image view for the motion vector attachment in the given frame slot.
    pub fn motion_view(&self, frame: usize) -> vk::ImageView {
        self.motion.views[frame]
    }
    /// Image view for the mesh ID attachment in the given frame slot.
    pub fn mesh_id_view(&self, frame: usize) -> vk::ImageView {
        self.mesh_id.views[frame]
    }
    /// Image view for the raw (pre-denoise) indirect light, per frame.
    pub fn raw_indirect_view(&self, frame: usize) -> vk::ImageView {
        self.raw_indirect.views[frame]
    }
    /// Image view for the albedo attachment in the given frame slot.
    pub fn albedo_view(&self, frame: usize) -> vk::ImageView {
        self.albedo.views[frame]
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
            ];
            let mut barriers = Vec::with_capacity(attachments.len() * MAX_FRAMES_IN_FLIGHT);
            for att in &attachments {
                for &img in &att.images {
                    barriers.push(
                        vk::ImageMemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::empty())
                            .dst_access_mask(vk::AccessFlags::SHADER_READ)
                            .old_layout(vk::ImageLayout::UNDEFINED)
                            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                            .image(img)
                            .subresource_range(super::descriptors::color_subresource_single_mip()),
                    );
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
        }
    }
}
