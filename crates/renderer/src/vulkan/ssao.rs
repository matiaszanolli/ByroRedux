//! Screen-space ambient occlusion (SSAO) compute pipeline.
//!
//! Runs after the render pass to produce an R8 occlusion texture from the
//! depth buffer. The fragment shader reads this texture the next frame to
//! darken corners, crevices, and contact shadows.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use anyhow::{Context, Result};
use ash::vk;

/// SSAO parameters uploaded as a UBO.
#[repr(C)]
#[derive(Clone, Copy)]
struct SsaoParams {
    /// Projection matrix (for view-space reconstruction from depth).
    projection: [[f32; 4]; 4],
    /// x = radius, y = bias, z = intensity, w = unused.
    params: [f32; 4],
    /// x = width, y = height, z = 1/width, w = 1/height.
    screen_size: [f32; 4],
}

pub struct SsaoPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per-frame parameter UBOs.
    param_buffers: Vec<GpuBuffer>,
    /// AO output image (full-resolution R8).
    pub ao_image: vk::Image,
    pub ao_image_view: vk::ImageView,
    ao_allocation: Option<gpu_allocator::vulkan::Allocation>,
    /// Sampler for the AO texture (used by the fragment shader).
    pub ao_sampler: vk::Sampler,
    /// Depth sampler for reading the depth buffer.
    depth_sampler: vk::Sampler,
    pub width: u32,
    pub height: u32,
}

impl SsaoPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        depth_image_view: vk::ImageView,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let max_frames = 2;

        // Create AO output image (R8, full resolution).
        let ao_image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8_UNORM)
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

        let ao_image = unsafe {
            device
                .create_image(&ao_image_info, None)
                .context("Failed to create AO image")?
        };

        let requirements = unsafe { device.get_image_memory_requirements(ao_image) };
        let ao_allocation = allocator
            .lock()
            .expect("allocator lock")
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "ssao_output",
                requirements,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate AO image memory")?;

        unsafe {
            device
                .bind_image_memory(ao_image, ao_allocation.memory(), ao_allocation.offset())
                .context("Failed to bind AO image memory")?;
        }

        let ao_image_view = unsafe {
            device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(ao_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::R8_UNORM)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )
                .context("Failed to create AO image view")?
        };

        // Samplers.
        let depth_sampler = unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::NEAREST)
                        .min_filter(vk::Filter::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("depth sampler")?
        };

        let ao_sampler = unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("ao sampler")?
        };

        // Parameter UBOs.
        let param_size = std::mem::size_of::<SsaoParams>() as vk::DeviceSize;
        let mut param_buffers = Vec::with_capacity(max_frames);
        for _ in 0..max_frames {
            param_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )?);
        }

        // Descriptor set layout.
        let bindings = [
            // binding 0: depth texture (sampled)
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // binding 1: AO output (storage image)
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // binding 2: SSAO params UBO
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        let layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .context("SSAO descriptor set layout")?
        };

        // Pipeline layout (no push constants).
        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&descriptor_set_layout)),
                    None,
                )
                .context("SSAO pipeline layout")?
        };

        // Compute pipeline.
        let comp_spv = include_bytes!("../../shaders/ssao.comp.spv");
        let shader_module = super::pipeline::load_shader_module(device, comp_spv)?;
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader_module)
            .name(c"main");
        let pipeline = unsafe {
            device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(stage)
                        .layout(pipeline_layout)],
                    None,
                )
                .map_err(|(_, e)| e)
                .context("SSAO compute pipeline")?[0]
        };
        unsafe { device.destroy_shader_module(shader_module, None) };

        // Descriptor pool + sets.
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: max_frames as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: max_frames as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: max_frames as u32,
            },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(max_frames as u32);
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("SSAO descriptor pool")?
        };

        let layouts = vec![descriptor_set_layout; max_frames];
        let descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("SSAO descriptor sets")?
        };

        // Write descriptor sets.
        for i in 0..max_frames {
            let depth_info = [vk::DescriptorImageInfo::default()
                .sampler(depth_sampler)
                .image_view(depth_image_view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
            let ao_info = [vk::DescriptorImageInfo::default()
                .image_view(ao_image_view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let param_info = [vk::DescriptorBufferInfo {
                buffer: param_buffers[i].buffer,
                offset: 0,
                range: param_size,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&depth_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&ao_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&param_info),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        log::info!("SSAO pipeline created: {}x{}", width, height);

        Ok(Self {
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_sets,
            param_buffers,
            ao_image,
            ao_image_view,
            ao_allocation: Some(ao_allocation),
            ao_sampler,
            depth_sampler,
            width,
            height,
        })
    }

    /// Upload SSAO parameters and dispatch the compute shader.
    ///
    /// Call AFTER the render pass (depth buffer must be written and
    /// transitioned to READ_ONLY). The AO output image is written in
    /// GENERAL layout and can be sampled by the next frame's fragment shader.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        projection: &[[f32; 4]; 4],
    ) -> Result<()> {
        // Upload parameters.
        let params = SsaoParams {
            projection: *projection,
            params: [50.0, 0.05, 1.5, 0.0], // radius=50, bias=0.05, intensity=1.5
            screen_size: [
                self.width as f32,
                self.height as f32,
                1.0 / self.width as f32,
                1.0 / self.height as f32,
            ],
        };
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&params))?;

        // Transition AO image to GENERAL for compute write.
        let ao_barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.ao_image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[ao_barrier],
        );

        // Bind and dispatch.
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[frame]],
            &[],
        );

        let groups_x = (self.width + 7) / 8;
        let groups_y = (self.height + 7) / 8;
        device.cmd_dispatch(cmd, groups_x, groups_y, 1);

        // Transition AO image to SHADER_READ_ONLY for fragment shader sampling.
        let ao_barrier_read = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(self.ao_image)
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
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[ao_barrier_read],
        );

        Ok(())
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        device.destroy_image_view(self.ao_image_view, None);
        device.destroy_image(self.ao_image, None);
        if let Some(alloc) = self.ao_allocation.take() {
            allocator
                .lock()
                .expect("allocator lock")
                .free(alloc)
                .expect("free AO allocation");
        }
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        device.destroy_sampler(self.ao_sampler, None);
        device.destroy_sampler(self.depth_sampler, None);
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}
