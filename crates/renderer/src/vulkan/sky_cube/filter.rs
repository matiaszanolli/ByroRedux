//! Per-mip GGX convolution. Reads a base-only cube view while writing
//! disjoint mip views, then exposes the complete chain to fragment shaders.
use super::*;

const SPV: &[u8] = include_bytes!("../../../shaders/sky_prefilter.comp.spv");

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct SkyFilter {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
    views: Vec<vk::ImageView>,
}

impl SkyFilter {
    pub fn new(
        device: &ash::Device,
        cache: vk::PipelineCache,
        cubes: &[GpuImage],
        sampler: vk::Sampler,
    ) -> Result<Self> {
        let mut filter = Self::default();
        if let Err(err) = filter.init(device, cache, cubes, sampler) {
            // SAFETY: constructor-owned handles have never been submitted.
            unsafe { filter.destroy(device) };
            return Err(err);
        }
        Ok(filter)
    }

    fn init(
        &mut self,
        device: &ash::Device,
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
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "sky_prefilter.comp",
                spirv: SPV,
            }],
            "sky prefilter",
            &[],
        )?;
        // SAFETY: valid reflected bindings; all created handles are immediately
        // retained by self and destroyed on any subsequent failure.
        unsafe {
            self.set_layout = device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?;
            self.layout = device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&[self.set_layout])
                    .push_constant_ranges(&[vk::PushConstantRange::default()
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                        .size(4)]),
                None,
            )?;
        }
        self.pipeline = super::super::pipeline::create_compute_pipeline(
            device,
            cache,
            SPV,
            self.layout,
            "sky prefilter",
        )?;
        let count = cubes.len() as u32 * (SKY_CUBE_MIP_LEVELS - 1);
        self.pool = DescriptorPoolBuilder::from_layout_bindings(&bindings, count)
            .max_sets(count)
            .build(device, "sky prefilter")?;
        // SAFETY: pool has exactly enough capacity for these layouts.
        self.sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.pool)
                    .set_layouts(&vec![self.set_layout; count as usize]),
            )?
        };
        for (frame, cube) in cubes.iter().enumerate() {
            let source = self.view(device, cube.image, 0, vk::ImageViewType::CUBE)?;
            for mip in 1..SKY_CUBE_MIP_LEVELS {
                let target =
                    self.view(device, cube.image, mip, vk::ImageViewType::TYPE_2D_ARRAY)?;
                let input = [vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(source)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let output = [vk::DescriptorImageInfo::default()
                    .image_view(target)
                    .image_layout(vk::ImageLayout::GENERAL)];
                let set = self.sets[frame * (SKY_CUBE_MIP_LEVELS - 1) as usize + mip as usize - 1];
                // SAFETY: private sets are not yet referenced by command buffers;
                // source and target views cover disjoint subresources.
                unsafe {
                    device.update_descriptor_sets(
                        &[
                            write_combined_image_sampler(set, 0, &input),
                            write_storage_image(set, 1, &output),
                        ],
                        &[],
                    )
                };
            }
        }
        Ok(())
    }

    fn view(
        &mut self,
        device: &ash::Device,
        image: vk::Image,
        mip: u32,
        kind: vk::ImageViewType,
    ) -> Result<vk::ImageView> {
        // SAFETY: caller-owned cube has all named mip levels and six layers.
        let view = unsafe {
            device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(kind)
                    .format(SKY_CUBE_FORMAT)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(mip)
                            .level_count(1)
                            .layer_count(CUBE_FACES),
                    ),
                None,
            )?
        };
        self.views.push(view);
        Ok(view)
    }

    /// Safety: frame slot fenced; base mip in SHADER_READ_ONLY_OPTIMAL,
    /// its bake visible to compute. Other mips can be discarded.
    pub unsafe fn record(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        frame: usize,
    ) {
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(1)
            .level_count(SKY_CUBE_MIP_LEVELS - 1)
            .layer_count(CUBE_FACES);
        let begin = vk::ImageMemoryBarrier::default()
            .image(image)
            .subresource_range(range)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[begin],
        );
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        for mip in 1..SKY_CUBE_MIP_LEVELS {
            let set = self.sets[frame * (SKY_CUBE_MIP_LEVELS - 1) as usize + mip as usize - 1];
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.layout,
                0,
                &[set],
                &[],
            );
            let roughness = mip as f32 / (SKY_CUBE_MIP_LEVELS - 1) as f32;
            device.cmd_push_constants(
                cmd,
                self.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &roughness.to_ne_bytes(),
            );
            let edge = SKY_CUBE_FACE_SIZE >> mip;
            device.cmd_dispatch(cmd, edge.div_ceil(8), edge.div_ceil(8), CUBE_FACES);
        }
        let end = vk::ImageMemoryBarrier::default()
            .image(image)
            .subresource_range(range)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[end],
        );
    }

    /// Safety: no pending command buffers reference these handles.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        device.destroy_descriptor_pool(self.pool, None);
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_set_layout(self.set_layout, None);
        for view in self.views.drain(..) {
            device.destroy_image_view(view, None);
        }
        *self = Self::default();
    }
}
