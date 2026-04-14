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
//! | mesh_id       | R16_UINT           | Per-instance ID (disocclusion detection)      |
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
pub const MESH_ID_FORMAT: vk::Format = vk::Format::R16_UINT;
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
                    requirements: unsafe { device.get_image_memory_requirements(img) },
                    location: gpu_allocator::MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                })
                .with_context(|| format!("Failed to allocate {name_prefix} memory"))?;
            unsafe {
                device
                    .bind_image_memory(img, alloc.memory(), alloc.offset())
                    .with_context(|| format!("bind {name_prefix} image memory"))?;
            }
            self.allocations.push(Some(alloc));

            let view = unsafe {
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(format)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            }),
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
            unsafe { device.destroy_image_view(view, None) };
        }
        self.views.clear();
        for &img in &self.images {
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
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            }),
                    );
                }
            }
            // SAFETY: barriers are well-formed, device and cmd are valid.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
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
            unsafe { self.destroy(device, allocator) };
        }
        result
    }

    /// Destroy all images, views, and allocations. Safe to call multiple times.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        unsafe {
            self.normal.destroy(device, allocator);
            self.motion.destroy(device, allocator);
            self.mesh_id.destroy(device, allocator);
            self.raw_indirect.destroy(device, allocator);
            self.albedo.destroy(device, allocator);
        }
    }
}
