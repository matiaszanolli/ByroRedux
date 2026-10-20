//! SKYAL — cloud-shadow map: direct-sun transmittance through the sky's
//! volumetric deck, per world position around the camera (`cloud_shadow.comp`;
//! the authored upper plane casts none — see the shader header).
//!
//! Baked from the sky cube's own per-frame parameter UBO and noise volumes,
//! so the shadows are cast by exactly the deck the sky shows.
//! Surfaces sample it at scene set 1 / binding 24
//! (`exteriorCloudSunTransmittance` in `bindings.glsl`); passes that light by
//! one sun value read the camera's own column back on the host
//! ([`CloudShadow::camera_transmittance`]).
use super::*;
use crate::shader_constants::CLOUD_SHADOW_MAP_SIZE;
use crate::vulkan::descriptors::write_storage_buffer;

const SPV: &[u8] = include_bytes!("../../../shaders/cloud_shadow.comp.spv");

/// One transmittance per texel; 16-bit float holds the soft range a cloud
/// shadow needs without visible banding at a quarter of RGBA16F's size.
const FORMAT: vk::Format = vk::Format::R16_SFLOAT;

/// The camera-column readback: one `float`, padded to a `vec4`.
const READBACK_BYTES: vk::DeviceSize = 16;

/// VRAM the map holds per frame in flight, in bytes.
pub const fn cloud_shadow_bytes_per_frame() -> u64 {
    CLOUD_SHADOW_MAP_SIZE as u64 * CLOUD_SHADOW_MAP_SIZE as u64 * 2
}

#[derive(Default)]
pub(super) struct CloudShadow {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
    /// Per frame in flight, for the reason the sky cube is: frame N's bake
    /// must not race frame N-1's fragment reads.
    pub maps: Vec<GpuImage>,
    readbacks: Vec<GpuBuffer>,
    /// Linear, clamped to the edge: a receiver whose sun ray crosses the deck
    /// beyond the map takes the edge texel rather than a wrapped one.
    pub sampler: vk::Sampler,
}

impl CloudShadow {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        cache: vk::PipelineCache,
        bindless_layout: vk::DescriptorSetLayout,
        params: &[GpuBuffer],
        noise: CloudNoiseViews,
    ) -> Result<Self> {
        let mut shadow = Self::default();
        if let Err(err) = shadow.init(device, allocator, cache, bindless_layout, params, noise) {
            // SAFETY: constructor-owned resources have never been submitted.
            unsafe { shadow.destroy(device, allocator) };
            return Err(err);
        }
        Ok(shadow)
    }

    fn init(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cache: vk::PipelineCache,
        bindless_layout: vk::DescriptorSetLayout,
        params: &[GpuBuffer],
        noise: CloudNoiseViews,
    ) -> Result<()> {
        let bindings = [
            compute_binding(0, vk::DescriptorType::STORAGE_IMAGE),
            compute_binding(1, vk::DescriptorType::UNIFORM_BUFFER),
            compute_binding(2, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
            compute_binding(3, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
            compute_binding(4, vk::DescriptorType::STORAGE_BUFFER),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "cloud_shadow.comp",
                spirv: SPV,
            }],
            "cloud shadow",
            &[],
        )?;
        // SAFETY: reflected bindings; each handle is retained for error cleanup.
        unsafe {
            self.set_layout = device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?;
            // Set 1 is the bindless array `include/sky.glsl` declares. The bake
            // includes the sky for `SkyDome` and the cloud field, and is bound
            // like the sky-cube bake so a future plane term needs no new
            // layout (see its `set_layouts`).
            let set_layouts = [self.set_layout, bindless_layout];
            self.layout = device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts),
                None,
            )?;
        }
        // SAFETY: `device` is live; the handle is freed by `destroy`.
        self.sampler = unsafe {
            crate::vulkan::descriptors::clamp_sampler(
                device,
                vk::Filter::LINEAR,
                vk::SamplerMipmapMode::NEAREST,
            )?
        };
        self.pipeline = crate::vulkan::pipeline::create_compute_pipeline(
            device,
            cache,
            SPV,
            self.layout,
            "cloud shadow",
        )?;
        let frames = params.len() as u32;
        self.pool = DescriptorPoolBuilder::from_layout_bindings(&bindings, frames)
            .max_sets(frames)
            .build(device, "cloud shadow")?;
        // SAFETY: newly created pool has capacity for all requested sets.
        self.sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.pool)
                    .set_layouts(&vec![self.set_layout; params.len()]),
            )?
        };
        for (frame, param) in params.iter().enumerate() {
            self.maps.push(GpuImage::create(
                device,
                allocator,
                &GpuImageDesc::color_2d(
                    "cloud shadow map",
                    CLOUD_SHADOW_MAP_SIZE,
                    CLOUD_SHADOW_MAP_SIZE,
                    FORMAT,
                    vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
                ),
            )?);
            self.readbacks.push(GpuBuffer::create_host_readback(
                device,
                allocator,
                READBACK_BYTES,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            // A clear sky until the first bake in this slot lands, so the
            // first read never sees uninitialised memory.
            self.readbacks[frame].mapped_slice_mut()?[..4].copy_from_slice(&1.0f32.to_ne_bytes());
            let image_info = [vk::DescriptorImageInfo::default()
                .image_view(self.maps[frame].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let param_info = [vk::DescriptorBufferInfo::default()
                .buffer(param.buffer)
                .range(std::mem::size_of::<SkyCubeParams>() as vk::DeviceSize)];
            let base_info = [vk::DescriptorImageInfo::default()
                .sampler(noise.sampler)
                .image_view(noise.base)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let detail_info = [vk::DescriptorImageInfo::default()
                .sampler(noise.sampler)
                .image_view(noise.detail)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let readback_info = [vk::DescriptorBufferInfo::default()
                .buffer(self.readbacks[frame].buffer)
                .range(READBACK_BYTES)];
            let set = self.sets[frame];
            // SAFETY: private descriptor sets are not yet in use.
            unsafe {
                device.update_descriptor_sets(
                    &[
                        write_storage_image(set, 0, &image_info),
                        write_uniform_buffer(set, 1, &param_info),
                        write_combined_image_sampler(set, 2, &base_info),
                        write_combined_image_sampler(set, 3, &detail_info),
                        write_storage_buffer(set, 4, &readback_info),
                    ],
                    &[],
                )
            };
        }
        Ok(())
    }

    /// Record the bake with both layout transitions and the readback's host
    /// visibility. `UNDEFINED` discards the previous contents: every texel is
    /// rewritten.
    ///
    /// Safety: frame slot fenced; this slot's parameter UBO uploaded; command
    /// buffer recording; all owned resources live.
    pub unsafe fn record(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        bindless_set: vk::DescriptorSet,
    ) {
        let image = self.maps[frame].image;
        let to_general =
            image_barrier_to_general_write_layers(image, vk::ImageLayout::UNDEFINED, vk::AccessFlags::empty(), 1);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_general],
        );
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.layout,
            0,
            &[self.sets[frame], bindless_set],
            &[],
        );
        device.cmd_dispatch(
            cmd,
            CLOUD_SHADOW_MAP_SIZE.div_ceil(WORKGROUP_X),
            CLOUD_SHADOW_MAP_SIZE.div_ceil(WORKGROUP_Y),
            1,
        );
        let to_read = image_barrier_general_to_shader_read_layers(image, 1);
        // A fence alone does not make device writes host-visible (#2740):
        // the readback needs its own SHADER_WRITE -> HOST_READ dependency.
        let host_visible = vk::BufferMemoryBarrier::default()
            .buffer(self.readbacks[frame].buffer)
            .size(READBACK_BYTES)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_read],
        );
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[],
            &[host_visible],
            &[],
        );
    }

    /// The camera column's transmittance the last bake in slot `frame`
    /// wrote, or `None` if it cannot be read. Call only after that slot's
    /// fence has signalled.
    pub fn camera_transmittance(&mut self, device: &ash::Device, frame: usize) -> Option<f32> {
        let readback = self.readbacks.get_mut(frame)?;
        readback.invalidate_if_needed(device).ok()?;
        let bytes = readback.mapped_slice_mut().ok()?;
        let value = f32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
        (value.is_finite()).then(|| value.clamp(0.0, 1.0))
    }

    /// Safety: no pending commands reference these handles or their descriptors.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        device.destroy_descriptor_pool(self.pool, None);
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_set_layout(self.set_layout, None);
        device.destroy_sampler(self.sampler, None);
        for mut map in self.maps.drain(..) {
            map.destroy(device, allocator);
        }
        for buffer in &mut self.readbacks {
            buffer.destroy(device, allocator);
        }
        *self = Self::default();
    }
}
