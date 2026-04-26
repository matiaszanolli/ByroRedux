//! Temporal Antialiasing (TAA) — reproject-and-resolve compute pass.
//!
//! Runs between SVGF and composite:
//!
//!   main render pass → raw HDR + motion + mesh_id  (per frame-in-flight)
//!   SVGF temporal    → denoised indirect           (per frame-in-flight)
//!   TAA dispatch     → anti-aliased HDR            (THIS module)
//!   composite        → tone-mapped swapchain
//!
//! Owns a per-frame-in-flight RGBA16F history image. Each frame reads
//! the OTHER slot (the previous frame's history) via motion-vector
//! reprojection, neighborhood-clamps it against the 3×3 current-pixel
//! stats in YCoCg, and writes the resolved color to its own slot. The
//! composite pass then samples the same slot as this frame's HDR input.
//!
//! Projection-matrix jitter (Halton 2,3) is applied CPU-side in the
//! camera UBO construction; this shader just consumes the result.
//!
//! ## Descriptor set layout
//!
//! | Binding | Resource        | Type                                |
//! |---------|-----------------|-------------------------------------|
//! | 0       | curr HDR        | combined image sampler              |
//! | 1       | motion vector   | combined image sampler              |
//! | 2       | curr mesh_id    | combined image sampler (usampler2D) |
//! | 3       | prev mesh_id    | combined image sampler (usampler2D) |
//! | 4       | prev history    | combined image sampler              |
//! | 5       | out TAA         | storage image (rgba16f)             |
//! | 6       | params UBO      | uniform buffer                      |

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

const TAA_COMP_SPV: &[u8] = include_bytes!("../../shaders/taa.comp.spv");

/// History format. RGBA16F matches the HDR render target so no precision
/// is lost on reprojection. Alpha is always 1.0 and is never read.
const HISTORY_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TaaParams {
    /// xy = screen dimensions (pixels), zw = 1/screen.
    pub screen: [f32; 4],
    /// x = current-frame weight α in [0, 1] (0.1 is canonical, meaning 10%
    /// current + 90% history per frame → ~10-frame rolling average).
    /// y = first_frame flag (1.0 while history is not yet valid).
    /// zw = reserved.
    pub params: [f32; 4],
}

struct HistorySlot {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
}

pub struct TaaPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    shader_module: vk::ShaderModule,

    /// Per-frame-in-flight RGBA16F images. Used simultaneously as
    /// "this frame's output" (storage write) and "next frame's history"
    /// (sampled read).
    history: Vec<HistorySlot>,

    /// Sampler used for history (linear for Catmull-Rom) and for the
    /// integer mesh_id attachments (filtering ignored for usampler).
    linear_sampler: vk::Sampler,
    point_sampler: vk::Sampler,

    param_buffers: Vec<GpuBuffer>,

    pub width: u32,
    pub height: u32,

    frames_since_creation: u32,
}

impl TaaPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        hdr_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        debug_assert_eq!(hdr_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(motion_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(mesh_id_views.len(), MAX_FRAMES_IN_FLIGHT);

        let result = Self::new_inner(
            device,
            allocator,
            pipeline_cache,
            hdr_views,
            motion_views,
            mesh_id_views,
            width,
            height,
        );
        if let Err(ref e) = result {
            log::debug!("TAA pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        hdr_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            shader_module: vk::ShaderModule::null(),
            history: Vec::new(),
            linear_sampler: vk::Sampler::null(),
            point_sampler: vk::Sampler::null(),
            param_buffers: Vec::new(),
            width,
            height,
            frames_since_creation: 0,
        };

        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        unsafe { partial.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let slot = try_or_cleanup!(Self::create_history_image(
                device,
                allocator,
                width,
                height,
                &format!("taa_history_{i}"),
            ));
            partial.history.push(slot);
        }

        partial.linear_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("TAA linear sampler")
        });
        partial.point_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::NEAREST)
                        .min_filter(vk::Filter::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("TAA point sampler")
        });

        let param_size = std::mem::size_of::<TaaParams>() as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buf);
        }

        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "taa.comp",
                spirv: TAA_COMP_SPV,
            }],
            "taa",
            &[],
        )
        .expect("taa descriptor layout drifted against taa.comp (see #427)");
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("TAA descriptor set layout")
        });

        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout)),
                    None,
                )
                .context("TAA pipeline layout")
        });

        partial.shader_module = try_or_cleanup!(super::pipeline::load_shader_module(
            device,
            TAA_COMP_SPV
        ));
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(partial.shader_module)
            .name(c"main");
        partial.pipeline = match unsafe {
            device
                .create_compute_pipelines(
                    pipeline_cache,
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(stage)
                        .layout(partial.pipeline_layout)],
                    None,
                )
                .map_err(|(_, e)| e)
                .context("TAA compute pipeline")
        } {
            Ok(p) => p[0],
            Err(e) => {
                unsafe { partial.destroy(device, allocator) };
                return Err(e);
            }
        };

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 5) as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
        ];
        partial.descriptor_pool = try_or_cleanup!(unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .pool_sizes(&pool_sizes)
                        .max_sets(MAX_FRAMES_IN_FLIGHT as u32),
                    None,
                )
                .context("TAA descriptor pool")
        });

        let set_layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("TAA descriptor sets")
        });

        partial.write_descriptor_sets(device, hdr_views, motion_views, mesh_id_views);

        log::info!("TAA pipeline created: {}x{}", width, height);
        Ok(partial)
    }

    fn create_history_image(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
        name: &str,
    ) -> Result<HistorySlot> {
        let img_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(HISTORY_FORMAT)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe {
            device
                .create_image(&img_info, None)
                .with_context(|| format!("create {name}"))?
        };

        let alloc = match allocator
            .lock()
            .expect("allocator lock")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name,
                requirements: unsafe { device.get_image_memory_requirements(image) },
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .with_context(|| format!("allocate {name}"))
        {
            Ok(a) => a,
            Err(e) => {
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        if let Err(e) = unsafe {
            device
                .bind_image_memory(image, alloc.memory(), alloc.offset())
                .with_context(|| format!("bind {name}"))
        } {
            allocator.lock().expect("allocator lock").free(alloc).ok();
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }

        let view = match unsafe {
            device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(HISTORY_FORMAT)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )
                .with_context(|| format!("view {name}"))
        } {
            Ok(v) => v,
            Err(e) => {
                allocator.lock().expect("allocator lock").free(alloc).ok();
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        Ok(HistorySlot {
            image,
            view,
            allocation: Some(alloc),
        })
    }

    fn write_descriptor_sets(
        &self,
        device: &ash::Device,
        hdr_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
    ) {
        let param_size = std::mem::size_of::<TaaParams>() as vk::DeviceSize;
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let prev = (f + 1) % MAX_FRAMES_IN_FLIGHT;

            let curr_hdr = [vk::DescriptorImageInfo::default()
                .sampler(self.linear_sampler)
                .image_view(hdr_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let motion = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(motion_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let curr_mid = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(mesh_id_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let prev_mid = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(mesh_id_views[prev])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let prev_hist = [vk::DescriptorImageInfo::default()
                .sampler(self.linear_sampler)
                .image_view(self.history[prev].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let out_taa = [vk::DescriptorImageInfo::default()
                .image_view(self.history[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let params = [vk::DescriptorBufferInfo {
                buffer: self.param_buffers[f].buffer,
                offset: 0,
                range: param_size,
            }];

            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&curr_hdr),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&motion),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&curr_mid),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&prev_mid),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(4)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&prev_hist),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(5)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&out_taa),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(6)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&params),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// View for this frame's resolved TAA output (= composite's input).
    pub fn output_view(&self, frame: usize) -> vk::ImageView {
        self.history[frame].view
    }

    /// UNDEFINED → GENERAL for every history slot. Call once after `new()`.
    /// # Safety
    /// device / queue / pool must be valid; queue must support graphics.
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let mut barriers = Vec::with_capacity(self.history.len());
            for slot in &self.history {
                barriers.push(
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::empty())
                        .dst_access_mask(
                            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                        )
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::GENERAL)
                        .image(slot.image)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                );
            }
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            }
            Ok(())
        })
    }

    /// Dispatch TAA. Must run after the main render pass (so HDR / motion /
    /// mesh_id are in SHADER_READ_ONLY_OPTIMAL) and before composite
    /// (which samples `output_view(frame)` in GENERAL).
    ///
    /// # Safety
    /// `cmd` must be a recording command buffer. `frame < MAX_FRAMES_IN_FLIGHT`.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) -> Result<()> {
        let first_frame = if self.frames_since_creation < MAX_FRAMES_IN_FLIGHT as u32 {
            1.0
        } else {
            0.0
        };
        let params = TaaParams {
            screen: [
                self.width as f32,
                self.height as f32,
                1.0 / self.width as f32,
                1.0 / self.height as f32,
            ],
            // 0.1 = 10% current, 90% history — canonical TAA blend.
            params: [0.1, first_frame, 0.0, 0.0],
        };
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&params))?;

        let ubo_barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::HOST_WRITE)
            .dst_access_mask(vk::AccessFlags::UNIFORM_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::HOST,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[ubo_barrier],
            &[],
            &[],
        );

        // Order this frame's write after any lingering sample of the same
        // slot (layout stays GENERAL). The in-flight fence already
        // guarantees the previous GPU work on this slot has completed.
        let out_img = self.history[frame].image;
        let pre = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(out_img)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[pre],
        );

        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[frame]],
            &[],
        );

        let gx = (self.width + 7) / 8;
        let gy = (self.height + 7) / 8;
        device.cmd_dispatch(cmd, gx, gy, 1);

        // Expose the result to composite's fragment shader read.
        let post = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(out_img)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            // dst = FRAGMENT for composite's read this frame + COMPUTE for
            // next frame's TAA dispatch reading this slot as
            // `prev_history`. Per-frame fence currently serialises both
            // consumers, but the dst-stage mask still has to cover both
            // for correctness once the fence wait is relaxed. #653.
            vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[post],
        );

        self.frames_since_creation = self.frames_since_creation.saturating_add(1);
        Ok(())
    }

    /// Recreate history images at a new extent after swapchain resize.
    /// Caller must pass the freshly-recreated G-buffer and composite HDR views.
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        hdr_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<()> {
        for slot in self.history.drain(..) {
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }

        self.width = width;
        self.height = height;
        self.frames_since_creation = 0;

        let result = (|| -> Result<()> {
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                self.history.push(Self::create_history_image(
                    device,
                    allocator,
                    width,
                    height,
                    &format!("taa_history_{i}"),
                )?);
            }
            Ok(())
        })();
        if let Err(ref e) = result {
            log::error!("TAA recreate partial failure: {e} — destroying partial state");
            unsafe { self.destroy(device, allocator) };
            return result;
        }

        self.write_descriptor_sets(device, hdr_views, motion_views, mesh_id_views);
        Ok(())
    }

    /// Destroy all Vulkan objects. Safe to call on partially-initialized state.
    /// # Safety
    /// device and allocator must be valid; no GPU work may reference these images.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        self.param_buffers.clear();
        if self.pipeline != vk::Pipeline::null() {
            unsafe { device.destroy_pipeline(self.pipeline, None) };
        }
        if self.shader_module != vk::ShaderModule::null() {
            unsafe { device.destroy_shader_module(self.shader_module, None) };
        }
        if self.pipeline_layout != vk::PipelineLayout::null() {
            unsafe { device.destroy_pipeline_layout(self.pipeline_layout, None) };
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            unsafe { device.destroy_descriptor_pool(self.descriptor_pool, None) };
        }
        if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
            unsafe { device.destroy_descriptor_set_layout(self.descriptor_set_layout, None) };
        }
        if self.linear_sampler != vk::Sampler::null() {
            unsafe { device.destroy_sampler(self.linear_sampler, None) };
        }
        if self.point_sampler != vk::Sampler::null() {
            unsafe { device.destroy_sampler(self.point_sampler, None) };
        }
        for slot in self.history.drain(..) {
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
    }
}
