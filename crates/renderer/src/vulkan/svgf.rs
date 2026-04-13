//! SVGF (spatiotemporal variance-guided filtering) denoiser for 1-SPP GI.
//!
//! Phase 3 only implements the temporal accumulation pass: reproject the
//! previous frame's accumulated indirect via motion vectors, test
//! consistency against the previous frame's mesh_id, and blend with the
//! current 1-SPP indirect at α=0.2. The moments image tracks luminance
//! μ₁/μ₂ for Phase 4's variance estimation.
//!
//! ## Resource layout
//!
//! - **indirect_history[frame]** (RGBA16F, STORAGE|SAMPLED, per frame-in-flight):
//!   the accumulated demodulated indirect light. Read as history by the
//!   NEXT frame, written by the CURRENT frame, sampled by composite.
//! - **moments_history[frame]** (RGBA16F, STORAGE|SAMPLED, per frame-in-flight):
//!   rg = running (μ₁, μ₂) luminance moments, b = history length.
//!
//! Both live in VK_IMAGE_LAYOUT_GENERAL throughout their lifetime — simpler
//! than flipping layouts between passes, and sampling in GENERAL is legal
//! per the Vulkan spec for combined image samplers.
//!
//! ## Ping-pong scheme
//!
//! With MAX_FRAMES_IN_FLIGHT = 2, each frame slot owns its own pair of
//! history images. Frame N writes to slot N and reads slot (N+1) mod 2 as
//! its history input. Submission ordering + in-flight fence guarantee that
//! the read target was fully written by the previous frame before the new
//! dispatch begins.
//!
//! ## Descriptor set (binding layout)
//!
//! | Binding | Resource            | Type                       |
//! |---------|---------------------|----------------------------|
//! | 0       | curr raw indirect   | sampler2D (G-buffer)       |
//! | 1       | motion vector       | sampler2D (G-buffer)       |
//! | 2       | curr mesh_id        | usampler2D (G-buffer)      |
//! | 3       | prev mesh_id        | usampler2D (G-buffer prev) |
//! | 4       | prev indirect hist  | sampler2D (this module)    |
//! | 5       | prev moments hist   | sampler2D (this module)    |
//! | 6       | out indirect        | image2D (rgba16f, storage) |
//! | 7       | out moments         | image2D (rgba16f, storage) |
//! | 8       | SvgfTemporalParams  | uniform buffer             |

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

/// Accumulated indirect light format. R11G11B10F saves 50% vs RGBA16F
/// (4B vs 8B/pixel). Alpha is always 1.0 and never read. Storage image
/// support for R11G11B10 is required on all desktop GPUs since 2014
/// (Maxwell/GCN/Gen9). See #275.
const INDIRECT_HIST_FORMAT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;
/// Moments format (μ1, μ2, history_length, unused). Kept as RGBA16F for
/// precision — luminance² values up to 100+ need 10+ bit mantissa.
const MOMENTS_HIST_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SvgfTemporalParams {
    /// xy = screen size (pixels), zw = 1/screen size.
    pub screen: [f32; 4],
    /// x = α color blend, y = α moments blend, z = first_frame flag
    /// (1.0 = reset history), w = unused.
    pub params: [f32; 4],
}

struct HistorySlot {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
}

pub struct SvgfPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    shader_module: vk::ShaderModule,

    indirect_history: Vec<HistorySlot>,
    moments_history: Vec<HistorySlot>,

    /// Linear sampler used to sample history + raw indirect (for fetches).
    /// All lookups in the shader are texelFetch so filtering does not
    /// matter, but we still need a sampler object.
    point_sampler: vk::Sampler,

    param_buffers: Vec<GpuBuffer>,

    pub width: u32,
    pub height: u32,

    /// True until the first frame has written both slots — forces the
    /// shader to take the "no history" branch and reset moments.
    frames_since_creation: u32,
}

impl SvgfPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        raw_indirect_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        debug_assert_eq!(raw_indirect_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(motion_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(mesh_id_views.len(), MAX_FRAMES_IN_FLIGHT);

        let result = Self::new_inner(
            device,
            allocator,
            raw_indirect_views,
            motion_views,
            mesh_id_views,
            width,
            height,
        );
        if let Err(ref e) = result {
            log::debug!("SVGF pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        raw_indirect_views: &[vk::ImageView],
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
            indirect_history: Vec::new(),
            moments_history: Vec::new(),
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

        // ── 1. Allocate history images (per frame-in-flight) ──────────
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let ind = try_or_cleanup!(Self::create_history_image(
                device,
                allocator,
                width,
                height,
                INDIRECT_HIST_FORMAT,
                &format!("svgf_indirect_{i}"),
            ));
            partial.indirect_history.push(ind);
            let mom = try_or_cleanup!(Self::create_history_image(
                device,
                allocator,
                width,
                height,
                MOMENTS_HIST_FORMAT,
                &format!("svgf_moments_{i}"),
            ));
            partial.moments_history.push(mom);
        }

        // ── 2. Sampler ────────────────────────────────────────────────
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
                .context("SVGF point sampler")
        });

        // ── 3. Parameter UBOs ─────────────────────────────────────────
        let param_size = std::mem::size_of::<SvgfTemporalParams>() as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buf);
        }

        // ── 4. Descriptor set layout ──────────────────────────────────
        let bindings = [
            // 0: curr indirect (sampler2D)
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 1: motion
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 2: curr mesh_id (usampler2D)
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 3: prev mesh_id
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 4: prev indirect history
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 5: prev moments history
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 6: out indirect
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 7: out moments
            vk::DescriptorSetLayoutBinding::default()
                .binding(7)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 8: params UBO
            vk::DescriptorSetLayoutBinding::default()
                .binding(8)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("SVGF descriptor set layout")
        });

        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout)),
                    None,
                )
                .context("SVGF pipeline layout")
        });

        // ── 5. Compute pipeline ───────────────────────────────────────
        let spv = include_bytes!("../../shaders/svgf_temporal.comp.spv");
        partial.shader_module =
            try_or_cleanup!(super::pipeline::load_shader_module(device, spv));
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(partial.shader_module)
            .name(c"main");
        partial.pipeline = match unsafe {
            device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(stage)
                        .layout(partial.pipeline_layout)],
                    None,
                )
                .map_err(|(_, e)| e)
                .context("SVGF temporal compute pipeline")
        } {
            Ok(p) => p[0],
            Err(e) => {
                unsafe { partial.destroy(device, allocator) };
                return Err(e);
            }
        };

        // ── 6. Descriptor pool + sets ─────────────────────────────────
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 6) as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 2) as u32,
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
                .context("SVGF descriptor pool")
        });

        let set_layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("SVGF descriptor sets")
        });

        // ── 7. Write descriptor sets ──────────────────────────────────
        partial.write_descriptor_sets(
            device,
            raw_indirect_views,
            motion_views,
            mesh_id_views,
        );

        log::info!("SVGF pipeline created: {}x{}", width, height);
        Ok(partial)
    }

    fn create_history_image(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
        format: vk::Format,
        name: &str,
    ) -> Result<HistorySlot> {
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
        raw_indirect_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
    ) {
        let param_size = std::mem::size_of::<SvgfTemporalParams>() as vk::DeviceSize;
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let prev = (f + 1) % MAX_FRAMES_IN_FLIGHT;

            let curr_indirect = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(raw_indirect_views[f])
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
            let prev_ind = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(self.indirect_history[prev].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let prev_mom = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(self.moments_history[prev].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let out_ind = [vk::DescriptorImageInfo::default()
                .image_view(self.indirect_history[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let out_mom = [vk::DescriptorImageInfo::default()
                .image_view(self.moments_history[f].view)
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
                    .image_info(&curr_indirect),
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
                    .image_info(&prev_ind),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(5)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&prev_mom),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(6)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&out_ind),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(7)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&out_mom),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[f])
                    .dst_binding(8)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&params),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// View for the accumulated indirect light (this frame's output), used
    /// by the composite pass to sample the denoised result.
    pub fn indirect_view(&self, frame: usize) -> vk::ImageView {
        self.indirect_history[frame].view
    }

    /// One-time layout transition UNDEFINED → GENERAL for every history
    /// image, so the first dispatch and first descriptor sampling see a
    /// valid layout. Call once after `new()`.
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let mut barriers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT * 2);
            for slot in self.indirect_history.iter().chain(self.moments_history.iter()) {
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

    /// Dispatch the temporal accumulation compute shader.
    ///
    /// Must be called AFTER the main render pass ends (raw_indirect, motion,
    /// mesh_id are in SHADER_READ_ONLY_OPTIMAL via render pass final_layout)
    /// and BEFORE the composite pass (which samples `indirect_view(frame)`).
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
        let params = SvgfTemporalParams {
            screen: [
                self.width as f32,
                self.height as f32,
                1.0 / self.width as f32,
                1.0 / self.height as f32,
            ],
            params: [0.2, 0.2, first_frame, 0.0],
        };
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&params))?;

        // Barrier: host write of params → compute shader uniform read.
        // Required even on HOST_COHERENT memory (execution dependency).
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

        // Barrier: the previous use of this frame's OUT slots (writes in
        // the previous use of this frame-in-flight index, at least two
        // frames ago) finished long before — the in-flight fence guarantees
        // it. We still emit an execution dependency on SHADER_WRITE so
        // descriptor sampling of the same slot in the previous frame is
        // ordered correctly.
        let out_ind_img = self.indirect_history[frame].image;
        let out_mom_img = self.moments_history[frame].image;
        let img_barrier = |img: vk::Image| {
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(img)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
        };
        let img_barriers = [img_barrier(out_ind_img), img_barrier(out_mom_img)];
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &img_barriers,
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

        // Barrier: compute write → composite's fragment shader sampling.
        // Keeps layout in GENERAL — composite's descriptor sets use GENERAL.
        let out_barrier = |img: vk::Image| {
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(img)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
        };
        let out_barriers = [out_barrier(out_ind_img), out_barrier(out_mom_img)];
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &out_barriers,
        );

        self.frames_since_creation = self.frames_since_creation.saturating_add(1);
        Ok(())
    }

    /// Recreate history images at a new extent after a swapchain resize.
    /// Caller must supply the G-buffer's new views (which they just
    /// recreated via `GBuffer::recreate_on_resize`).
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        raw_indirect_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<()> {
        for slot in self.indirect_history.drain(..) {
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
        for slot in self.moments_history.drain(..) {
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
        self.frames_since_creation = 0; // history is meaningless after resize

        for i in 0..MAX_FRAMES_IN_FLIGHT {
            self.indirect_history.push(Self::create_history_image(
                device,
                allocator,
                width,
                height,
                INDIRECT_HIST_FORMAT,
                &format!("svgf_indirect_{i}"),
            )?);
            self.moments_history.push(Self::create_history_image(
                device,
                allocator,
                width,
                height,
                MOMENTS_HIST_FORMAT,
                &format!("svgf_moments_{i}"),
            )?);
        }

        self.write_descriptor_sets(device, raw_indirect_views, motion_views, mesh_id_views);
        Ok(())
    }

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
        if self.point_sampler != vk::Sampler::null() {
            unsafe { device.destroy_sampler(self.point_sampler, None) };
        }
        for slot in self.indirect_history.drain(..) {
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
        for slot in self.moments_history.drain(..) {
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
