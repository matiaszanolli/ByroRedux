//! Diffuse sky convolution into nine RGB spherical-harmonic coefficients.
use super::*;
use crate::shader_constants::SKY_SH_COEFFICIENTS;
use crate::vulkan::descriptors::compute_binding;
use crate::vulkan::descriptors::write_storage_buffer;
use crate::vulkan::descriptors::{sampled_image_info};

const SPV: &[u8] = include_bytes!("../../../shaders/sky_irradiance.comp.spv");
/// The share of light an exterior's ground reflects back up into the diffuse sky term. 0.2 is a middling
/// albedo for soil, rock and pavement, chosen rather than measured from the scene's terrain.
pub const SKY_GROUND_ALBEDO: f32 = 0.2;

/// `sky_irradiance.comp`'s push constant: the sun's light on flat ground (`sun_illuminance`, the same value
/// the terrain receives, times the cosine to the vertical) and the ground albedo. Albedo 0 projects the sky
/// cube unchanged.
pub(super) fn ground_params(sun_illuminance: [f32; 4], sun_dir_y: f32, albedo: f32) -> [f32; 4] {
    let cosine = sun_dir_y.max(0.0);
    [
        sun_illuminance[0] * cosine,
        sun_illuminance[1] * cosine,
        sun_illuminance[2] * cosine,
        albedo,
    ]
}

const GROUND_PARAMS_BYTES: u32 = std::mem::size_of::<[f32; 4]>() as u32;

/// `SKY_SH_COEFFICIENTS` RGB `vec4` coefficients.
pub const SH_BYTES: u64 = SKY_SH_COEFFICIENTS as u64 * 16;

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
            compute_binding(0, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
            compute_binding(1, vk::DescriptorType::STORAGE_BUFFER),
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
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&[self.set_layout])
                    .push_constant_ranges(&[vk::PushConstantRange::default()
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                        .size(GROUND_PARAMS_BYTES)]),
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
            let input = [sampled_image_info(sampler, view)];
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
    pub unsafe fn record(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        ground: [f32; 4],
    ) {
        // SAFETY: this is the whole body of the `unsafe fn`; each operation in it is
        // covered by the caller contract documented on the function.
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.layout,
                0,
                &[self.sets[frame]],
                &[],
            );
            let mut bytes = [0u8; GROUND_PARAMS_BYTES as usize];
            for (chunk, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(ground) {
                *chunk = value.to_ne_bytes();
            }
            device.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::COMPUTE, 0, &bytes);
            device.cmd_dispatch(cmd, SKY_SH_COEFFICIENTS, 1, 1);
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
    }

    /// Safety: no pending commands reference these handles or their descriptors.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // SAFETY: this is the whole body of the `unsafe fn`; each operation in it is
        // covered by the caller contract documented on the function.
        unsafe {
            // SAFETY: the caller's contract (no in-flight work references the pass's pipeline or pool);
            // `destroy_pass_objects` skips null handles and nulls what it destroys (#6371).
            crate::vulkan::descriptors::destroy_pass_objects(
                device,
                crate::vulkan::descriptors::PassHandles {
                    pipeline: &mut self.pipeline,
                    layout: &mut self.layout,
                    pool: &mut self.pool,
                    set_layout: &mut self.set_layout,
                },
            );
            for view in self.views.drain(..) {
                device.destroy_image_view(view, None);
            }
            for buffer in &mut self.buffers {
                buffer.destroy(device, allocator);
            }
            *self = Self::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_takes_the_sun_by_its_height_and_none_from_below_the_horizon() {
        let sun = [0.4, 0.2, 0.1, 0.0];
        assert_eq!(ground_params(sun, 0.5, 0.2), [0.2, 0.1, 0.05, 0.2]);
        assert_eq!(ground_params(sun, -0.3, 0.2), [0.0, 0.0, 0.0, 0.2]);
        // No ground: the shader reads only the zero albedo.
        assert_eq!(ground_params(sun, 1.0, 0.0)[3], 0.0);
    }
}
