//! G-Buffer attachments for SVGF denoising.
//!
//! Phase 1 adds three auxiliary render targets that the main render pass
//! writes alongside the HDR color intermediate. These carry the
//! per-pixel geometric data that downstream SVGF passes need:
//!
//! | Attachment | Format          | Contents                                   |
//! |------------|-----------------|--------------------------------------------|
//! | normal     | RGBA16_SNORM    | World-space surface normal (xyz), unused w |
//! | motion     | R16G16_SFLOAT   | Screen-space motion vector (current→prev)  |
//! | mesh_id    | R16_UINT        | Per-instance ID (disocclusion detection)   |
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

pub const NORMAL_FORMAT: vk::Format = vk::Format::R16G16B16A16_SNORM;
pub const MOTION_FORMAT: vk::Format = vk::Format::R16G16_SFLOAT;
pub const MESH_ID_FORMAT: vk::Format = vk::Format::R16_UINT;

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

/// Owns the G-buffer attachment images (normal, motion, mesh_id) + their
/// views and allocations. One image per frame-in-flight for each attachment.
pub struct GBuffer {
    normal: Attachment,
    motion: Attachment,
    mesh_id: Attachment,
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
            width,
            height,
        };

        // If any allocation fails, clean up everything allocated so far.
        let r1 = gb.normal.allocate(device, allocator, NORMAL_FORMAT, width, height, "gb_normal");
        let r2 = gb.motion.allocate(device, allocator, MOTION_FORMAT, width, height, "gb_motion");
        let r3 = gb.mesh_id.allocate(device, allocator, MESH_ID_FORMAT, width, height, "gb_mesh_id");
        if let Err(e) = r1.and(r2).and(r3) {
            unsafe { gb.destroy(device, allocator) };
            return Err(e);
        }

        log::info!(
            "G-buffer created: {}x{} (normal + motion + mesh_id, {} frames)",
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
        }
        self.width = width;
        self.height = height;
        self.normal
            .allocate(device, allocator, NORMAL_FORMAT, width, height, "gb_normal")?;
        self.motion
            .allocate(device, allocator, MOTION_FORMAT, width, height, "gb_motion")?;
        self.mesh_id
            .allocate(device, allocator, MESH_ID_FORMAT, width, height, "gb_mesh_id")?;
        Ok(())
    }

    /// Destroy all images, views, and allocations. Safe to call multiple times.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        unsafe {
            self.normal.destroy(device, allocator);
            self.motion.destroy(device, allocator);
            self.mesh_id.destroy(device, allocator);
        }
    }
}
