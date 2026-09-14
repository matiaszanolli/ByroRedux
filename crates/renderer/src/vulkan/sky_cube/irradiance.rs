//! Diffuse sky convolution into nine RGB spherical-harmonic coefficients.
use super::*;
use crate::vulkan::descriptors::write_storage_buffer;

const SPV: &[u8] = include_bytes!("../../../shaders/sky_irradiance.comp.spv");
pub const SH_BYTES: u64 = 9 * 16;

#[derive(Default)]
pub(super) struct SkyIrradiance {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
    views: Vec<vk::ImageView>,
    pub buffers: Vec<GpuBuffer>,
}

impl SkyIrradiance {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        cache: vk::PipelineCache,
        cubes: &[GpuImage],
        sampler: vk::Sampler,
    ) -> Result<Self> {
        let mut projection = Self::default();
        if let Err(err) = projection.init(device, allocator, cache, cubes, sampler) {
            // SAFETY: constructor-owned resources have never been submitted.
            unsafe { projection.destroy(device, allocator) };
            return Err(err);
        }
        Ok(projection)
    }

    fn init(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cache: vk::PipelineCache,
        cubes: &[GpuImage],
        sampler: vk::Sampler,
    ) -> Result<()> {
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "sky_irradiance.comp",
                spirv: SPV,
            }],
            "sky irradiance",
            &[],
        )?;
        // SAFETY: reflected bindings; each handle is retained for error cleanup.
        unsafe {
            self.set_layout = device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?;
            self.layout = device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&[self.set_layout]),
                None,
            )?;
        }
        self.pipeline = crate::vulkan::pipeline::create_compute_pipeline(
            device,
            cache,
            SPV,
            self.layout,
            "sky irradiance",
        )?;
        self.pool = DescriptorPoolBuilder::from_layout_bindings(&bindings, cubes.len() as u32)
            .max_sets(cubes.len() as u32)
            .build(device, "sky irradiance")?;
        // SAFETY: newly created pool has capacity for all requested sets.
        self.sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.pool)
                    .set_layouts(&vec![self.set_layout; cubes.len()]),
            )?
        };
        for (frame, cube) in cubes.iter().enumerate() {
            self.buffers.push(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                SH_BYTES,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
            )?);
            // SAFETY: cube is live, cube-compatible, with six layers; this
            // base-only view excludes concurrently generated specular mips.
            let view = unsafe {
                device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(cube.image)
                        .view_type(vk::ImageViewType::CUBE)
                        .format(SKY_CUBE_FORMAT)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .level_count(1)
                                .layer_count(CUBE_FACES),
                        ),
                    None,
                )?
            };
            self.views.push(view);
            let input = [vk::DescriptorImageInfo::default()
                .sampler(sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let output = [vk::DescriptorBufferInfo::default()
                .buffer(self.buffers[frame].buffer)
                .range(SH_BYTES)];
            // SAFETY: private descriptor sets are not yet in use.
            unsafe {
                device.update_descriptor_sets(
                    &[
                        write_combined_image_sampler(self.sets[frame], 0, &input),
                        write_storage_buffer(self.sets[frame], 1, &output),
                    ],
                    &[],
                )
            };
        }
        Ok(())
    }

    /// Safety: frame slot fenced; base cube bake visible to compute, command
    /// buffer recording, all owned resources live. All coefficients overwritten.
    pub unsafe fn record(&self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.layout,
            0,
            &[self.sets[frame]],
            &[],
        );
        device.cmd_dispatch(cmd, 9, 1, 1);
        let ready = vk::BufferMemoryBarrier::default()
            .buffer(self.buffers[frame].buffer)
            .size(SH_BYTES)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[ready],
            &[],
        );
    }

    /// Safety: no pending commands reference these handles or their descriptors.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        device.destroy_descriptor_pool(self.pool, None);
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_set_layout(self.set_layout, None);
        for view in self.views.drain(..) {
            device.destroy_image_view(view, None);
        }
        for buffer in &mut self.buffers {
            buffer.destroy(device, allocator);
        }
        *self = Self::default();
    }
}
