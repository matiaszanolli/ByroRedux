//! Caustic scatter pass (#321, Option A).
//!
//! One compute dispatch per frame that splats refracted-light contributions
//! from every caustic-source pixel into a screen-space accumulator. The
//! accumulator is a single R32_UINT storage image so the shader can use
//! `imageAtomicAdd`; composite samples the accumulator as a `usampler2D`,
//! divides out the fixed-point scale, and adds the result to direct lighting.
//!
//! ## Resource layout
//!
//! - **caustic_accum[frame]** (R32_UINT, STORAGE | SAMPLED | TRANSFER_DST,
//!   per frame-in-flight): accumulator written by this module, read by the
//!   composite pass as a 1-channel unsigned texture.
//!
//! Layout lives in `GENERAL` throughout the frame (required for storage
//! image access) — same policy as SVGF's history images. Composite samples
//! it through a `usampler2D` view, which is legal in `GENERAL`.
//!
//! ## Per-frame flow
//!
//! 1. `vkCmdClearColorImage` resets the accumulator to zero.
//! 2. Params UBO uploaded (screen size, IOR, strength, fixed-point scale).
//! 3. HOST→COMPUTE barrier for the UBO + CLEAR→COMPUTE barrier for the image.
//! 4. `vkCmdDispatch` — one invocation per screen pixel; only caustic-source
//!    pixels do work.
//! 5. COMPUTE→FRAGMENT barrier so the composite pass can sample the result.
//!
//! ## Descriptor set (binding layout)
//!
//! | Binding | Resource           | Type                                |
//! |---------|--------------------|-------------------------------------|
//! | 0       | depth              | sampler2D (gbuffer, shared)         |
//! | 1       | normal             | sampler2D (gbuffer, per-frame)      |
//! | 2       | mesh_id            | usampler2D (gbuffer, per-frame)     |
//! | 3       | LightBuffer        | SSBO (scene_buffers, per-frame)     |
//! | 4       | CameraUBO          | UBO (scene_buffers, per-frame)      |
//! | 5       | InstanceBuffer     | SSBO (scene_buffers, per-frame)     |
//! | 6       | TLAS               | acceleration_structure (per-frame)  |
//! | 7       | caustic accum      | uimage2D r32ui (this module, per-f) |
//! | 8       | CausticParams      | UBO (this module, per-frame)        |

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    image_barrier_undef_to_general, memory_barrier, write_combined_image_sampler,
    write_storage_buffer, write_storage_image, write_uniform_buffer, DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::CAUSTIC_FIXED_SCALE;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

const CAUSTIC_SPLAT_COMP_SPV: &[u8] = include_bytes!("../../shaders/caustic_splat.comp.spv");

/// Scalar caustic accumulator — luminance packed as 16.16 fixed-point per
/// `imageAtomicAdd`. Composite divides by `CAUSTIC_FIXED_SCALE` on read to
/// recover the accumulated luminance. Single channel keeps the memory cost
/// to 4 B/pixel; color tinting is encoded by the per-instance `avgAlbedo`
/// the shader uses to modulate the splatted value.
pub const CAUSTIC_FORMAT: vk::Format = vk::Format::R32_UINT;

/// UBO uploaded once per frame. Matches `CausticParams` in
/// `shaders/caustic_splat.comp` exactly.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CausticParams {
    /// xy = pixel size, zw = 1 / pixel size.
    pub screen: [f32; 4],
    /// x = fixed-point scale, y = IOR (glass = 1.5), z = max lights to
    /// iterate, w = caustic strength multiplier (all ≥ 0).
    pub tune: [f32; 4],
}

struct CausticSlot {
    image: vk::Image,
    /// `r32ui` storage view for atomic writes from the compute shader.
    storage_view: vk::ImageView,
    /// Separate view used by composite to sample as a `usampler2D`.
    sampled_view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
}

pub struct CausticPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    shader_module: vk::ShaderModule,

    /// Per-FIF accumulator images.
    slots: Vec<CausticSlot>,
    /// Point sampler for gbuffer reads (depth, normal, mesh_id).
    point_sampler: vk::Sampler,

    param_buffers: Vec<GpuBuffer>,

    pub width: u32,
    pub height: u32,

    /// Tuning knobs, mirrored to the params UBO each dispatch.
    pub ior: f32,
    pub strength: f32,
    pub max_lights: u32,
}

impl CausticPipeline {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        depth_view: vk::ImageView,
        normal_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        light_buffers: &[GpuBuffer],
        light_buffer_size: vk::DeviceSize,
        camera_buffers: &[GpuBuffer],
        camera_buffer_size: vk::DeviceSize,
        instance_buffers: &[GpuBuffer],
        instance_buffer_size: vk::DeviceSize,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let result = Self::new_inner(
            device,
            allocator,
            pipeline_cache,
            depth_view,
            normal_views,
            mesh_id_views,
            light_buffers,
            light_buffer_size,
            camera_buffers,
            camera_buffer_size,
            instance_buffers,
            instance_buffer_size,
            width,
            height,
        );
        if let Err(ref e) = result {
            log::debug!("Caustic pipeline creation failed at: {e}");
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        depth_view: vk::ImageView,
        normal_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        light_buffers: &[GpuBuffer],
        light_buffer_size: vk::DeviceSize,
        camera_buffers: &[GpuBuffer],
        camera_buffer_size: vk::DeviceSize,
        instance_buffers: &[GpuBuffer],
        instance_buffer_size: vk::DeviceSize,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        debug_assert_eq!(normal_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(mesh_id_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(light_buffers.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(camera_buffers.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(instance_buffers.len(), MAX_FRAMES_IN_FLIGHT);

        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            shader_module: vk::ShaderModule::null(),
            slots: Vec::new(),
            point_sampler: vk::Sampler::null(),
            param_buffers: Vec::new(),
            width,
            height,
            ior: 1.5,
            strength: 1.0,
            max_lights: 8,
        };

        // SAFETY (inside macro): `partial` is local to this fn and not
        // yet referenced by any command buffer / descriptor set;
        // cleanup-on-error closes the partial state before returning.
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

        // ── 1. Accumulator images ─────────────────────────────────────
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let slot = try_or_cleanup!(Self::create_slot(
                device,
                allocator,
                width,
                height,
                &format!("caustic_accum_{i}")
            ));
            partial.slots.push(slot);
        }

        // ── 2. Sampler ────────────────────────────────────────────────
        // SAFETY: SamplerCreateInfo fully populated above; handle owned
        // by `partial.point_sampler`, freed by destroy().
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
                .context("caustic point sampler")
        });

        // ── 3. Parameter UBOs ─────────────────────────────────────────
        let param_size = std::mem::size_of::<CausticParams>() as vk::DeviceSize;
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
            // 0 depth
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 1 normal
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 2 mesh_id
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 3 lights
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 4 camera UBO
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 5 instances
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 6 TLAS
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 7 output
            vk::DescriptorSetLayoutBinding::default()
                .binding(7)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 8 params
            vk::DescriptorSetLayoutBinding::default()
                .binding(8)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "caustic_splat.comp",
                spirv: CAUSTIC_SPLAT_COMP_SPV,
            }],
            "caustic",
            &[],
        )
        .expect("caustic descriptor layout drifted against caustic_splat.comp (see #427)");
        // SAFETY: `bindings` validated against caustic_splat.comp above.
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("caustic descriptor set layout")
        });

        // SAFETY: descriptor_set_layout just created above.
        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout)),
                    None,
                )
                .context("caustic pipeline layout")
        });

        // ── 5. Compute pipeline ───────────────────────────────────────
        partial.shader_module = try_or_cleanup!(super::pipeline::load_shader_module(
            device,
            CAUSTIC_SPLAT_COMP_SPV
        ));
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(partial.shader_module)
            .name(c"main");
        // SAFETY: `stage` references `partial.shader_module` (loaded
        // above) and `partial.pipeline_layout` (just created above).
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
                .context("caustic compute pipeline")
        } {
            Ok(p) => p[0],
            Err(e) => {
                // SAFETY: cleanup-on-error; shader_module on `partial`
                // is freed by destroy().
                unsafe { partial.destroy(device, allocator) };
                return Err(e);
            }
        };

        // ── 6. Descriptor pool + sets ─────────────────────────────────
        // Pool sizes derived from `bindings` (#1030 / REN-D10-NEW-09).
        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &bindings,
            MAX_FRAMES_IN_FLIGHT as u32,
        )
        .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
        .build(device, "caustic descriptor pool"));

        let set_layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: pool just sized for MAX_FRAMES_IN_FLIGHT sets with the
        // same descriptor_set_layout handle.
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("caustic descriptor sets")
        });

        // ── 7. Write non-TLAS descriptors (TLAS is written per-frame) ─
        partial.write_descriptor_sets(
            device,
            depth_view,
            normal_views,
            mesh_id_views,
            light_buffers,
            light_buffer_size,
            camera_buffers,
            camera_buffer_size,
            instance_buffers,
            instance_buffer_size,
        );

        log::info!("Caustic pipeline created: {}x{}", width, height);
        Ok(partial)
    }

    fn create_slot(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
        name: &str,
    ) -> Result<CausticSlot> {
        // Single-mip image. The downstream `base_mip_level: 0` /
        // `level_count: 1` literals (view subresource, clear range,
        // pre/post barriers — all paired with this image) are pinned
        // to that 1 here. Going wider (e.g. mipmapped for blur or
        // half-res accumulation) requires updating every subresource
        // range alongside the `mip_levels` bump. See REN-D13-NEW-06
        // (audit 2026-05-09).
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(CAUSTIC_FORMAT)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        // SAFETY: `info` fully populated above (TYPE_2D, CAUSTIC_FORMAT,
        // STORAGE | SAMPLED | TRANSFER_DST usage). On Err the `?` bubbles
        // up before any subsequent allocation.
        let image = unsafe { device.create_image(&info, None).context("caustic image")? };
        let alloc = match allocator
            .lock()
            .expect("allocator lock")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name,
                // SAFETY: `image` just created above.
                requirements: unsafe { device.get_image_memory_requirements(image) },
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .context("caustic image allocate")
        {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: alloc failed; image was created but never bound.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };
        // SAFETY: `image` matches the memory requirements that produced
        // `alloc`; bound once per image.
        if let Err(e) = unsafe {
            device
                .bind_image_memory(image, alloc.memory(), alloc.offset())
                .context("caustic bind image memory")
        } {
            allocator.lock().expect("allocator lock").free(alloc).ok();
            // SAFETY: bind failed; free alloc first, then destroy unbound image.
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }

        let make_view = |img: vk::Image| -> Result<vk::ImageView> {
            // SAFETY: callers below pass `image` (bound above) twice —
            // once for storage view, once for sampled view. Both views
            // are owned by the returned CausticSlot.
            Ok(unsafe {
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(CAUSTIC_FORMAT)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            }),
                        None,
                    )
                    .context("caustic image view")?
            })
        };
        let storage_view = match make_view(image) {
            Ok(v) => v,
            Err(e) => {
                allocator.lock().expect("allocator lock").free(alloc).ok();
                // SAFETY: storage view creation failed; free alloc first,
                // destroy bound image.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };
        let sampled_view = match make_view(image) {
            Ok(v) => v,
            Err(e) => {
                // SAFETY: sampled view creation failed; tear down
                // already-created storage view, free alloc, destroy image.
                unsafe { device.destroy_image_view(storage_view, None) };
                allocator.lock().expect("allocator lock").free(alloc).ok();
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        Ok(CausticSlot {
            image,
            storage_view,
            sampled_view,
            allocation: Some(alloc),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn write_descriptor_sets(
        &self,
        device: &ash::Device,
        depth_view: vk::ImageView,
        normal_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        light_buffers: &[GpuBuffer],
        light_buffer_size: vk::DeviceSize,
        camera_buffers: &[GpuBuffer],
        camera_buffer_size: vk::DeviceSize,
        instance_buffers: &[GpuBuffer],
        instance_buffer_size: vk::DeviceSize,
    ) {
        let param_size = std::mem::size_of::<CausticParams>() as vk::DeviceSize;
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let depth_info = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(depth_view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
            let normal_info = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(normal_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let mesh_id_info = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(mesh_id_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let light_info = [vk::DescriptorBufferInfo {
                buffer: light_buffers[f].buffer,
                offset: 0,
                range: light_buffer_size,
            }];
            let camera_info = [vk::DescriptorBufferInfo {
                buffer: camera_buffers[f].buffer,
                offset: 0,
                range: camera_buffer_size,
            }];
            let instance_info = [vk::DescriptorBufferInfo {
                buffer: instance_buffers[f].buffer,
                offset: 0,
                range: instance_buffer_size,
            }];
            let caustic_info = [vk::DescriptorImageInfo::default()
                .image_view(self.slots[f].storage_view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let params_info = [vk::DescriptorBufferInfo {
                buffer: self.param_buffers[f].buffer,
                offset: 0,
                range: param_size,
            }];

            let set = self.descriptor_sets[f];
            let writes = [
                write_combined_image_sampler(set, 0, &depth_info),
                write_combined_image_sampler(set, 1, &normal_info),
                write_combined_image_sampler(set, 2, &mesh_id_info),
                write_storage_buffer(set, 3, &light_info),
                write_uniform_buffer(set, 4, &camera_info),
                write_storage_buffer(set, 5, &instance_info),
                write_storage_image(set, 7, &caustic_info),
                write_uniform_buffer(set, 8, &params_info),
            ];
            // SAFETY: descriptor sets owned by `self`; writes reference
            // buffers / image views owned by `self` and caller-borrowed
            // G-buffer / scene resources (live for this call's duration).
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// Caustic accumulator view used by the composite pass as `usampler2D`.
    pub fn sampled_view(&self, frame: usize) -> vk::ImageView {
        self.slots[frame].sampled_view
    }

    /// Update the TLAS binding for a given frame (binding 6). Mirrors the
    /// scene descriptor set's `write_tlas` flow — TLAS is rebuilt per frame
    /// so this must be called every frame before `dispatch`.
    pub fn write_tlas(
        &self,
        device: &ash::Device,
        frame: usize,
        tlas: vk::AccelerationStructureKHR,
    ) {
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_sets[frame])
            .dst_binding(6)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .descriptor_count(1)
            .push_next(&mut accel_write);
        // SAFETY: `write` references `accel_write` (which carries the
        // caller-provided `tlas` handle, live for the call duration) and
        // `self.descriptor_sets[frame]` (live for `self`'s lifetime).
        unsafe { device.update_descriptor_sets(&[write], &[]) };
    }

    /// One-time transition UNDEFINED → GENERAL on every slot so the first
    /// dispatch + composite sample see a valid layout. Call once after
    /// `new()`.
    ///
    /// # Safety
    /// Device, queue and command pool must be valid; queue must support
    /// graphics/transfer for pipeline barriers.
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let mut barriers = Vec::with_capacity(self.slots.len());
            for slot in &self.slots {
                barriers.push(image_barrier_undef_to_general(slot.image));
            }
            // SAFETY: caller of `initialize_layouts` (unsafe fn) guarantees
            // device/queue/pool validity; `cmd` is the recording buffer
            // from `with_one_time_commands`. Each barrier targets a slot
            // image we own.
            // NONE as srcStageMask on UNDEFINED → GENERAL transitions: there
            // are no previous writes to make visible (the prior contents are
            // discarded), so TOP_OF_PIPE and NONE are semantically equivalent.
            // NONE is the Vulkan 1.3 replacement for the deprecated use of
            // TOP_OF_PIPE as a source stage in memory barriers (#949 / #1100).
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
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

    /// Clear + dispatch. Call after the main render pass ends (gbuffer is
    /// in SHADER_READ_ONLY_OPTIMAL) and the TLAS has been rebuilt+bound,
    /// but before the composite pass samples the result.
    ///
    /// # Safety
    /// `cmd` must be a valid recording command buffer. `frame` must be
    /// < MAX_FRAMES_IN_FLIGHT.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) -> Result<()> {
        // ── Upload params ─────────────────────────────────────────────
        let params = CausticParams {
            screen: [
                self.width as f32,
                self.height as f32,
                1.0 / self.width as f32,
                1.0 / self.height as f32,
            ],
            tune: [
                CAUSTIC_FIXED_SCALE,
                self.ior,
                self.max_lights as f32,
                self.strength,
            ],
        };
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&params))?;

        // HOST → COMPUTE_SHADER (UBO flush before dispatch).
        memory_barrier(
            device, cmd,
            vk::PipelineStageFlags::HOST,
            vk::AccessFlags::HOST_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::UNIFORM_READ,
        );

        // ── Clear accumulator ─────────────────────────────────────────
        // For steady-state frames the previous use of this FIF slot was
        // compute-write → fragment-read in composite, so we wait on
        // `COMPUTE_SHADER | FRAGMENT_SHADER` before re-clearing.
        //
        // For the FIRST use of each FIF slot the slot is in GENERAL
        // layout from `initialize_layouts` (one-shot transfer-pool
        // submit, fully signalled before any `record_dispatch` runs),
        // so the listed wait stages are over-specified at frame 0 but
        // not incorrect — the dependency just collapses to a no-op
        // when there's nothing in flight on those stages. The
        // pre-fix docstring claimed "previous use was compute
        // write" categorically and didn't acknowledge the
        // first-frame initialized-from-transfer path. See
        // REN-D13-NEW-03 (audit 2026-05-09).
        let slot_img = self.slots[frame].image;
        let pre_clear_barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot_img)
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
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[pre_clear_barrier],
        );

        let clear_value = vk::ClearColorValue {
            uint32: [0, 0, 0, 0],
        };
        let clear_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        device.cmd_clear_color_image(
            cmd,
            slot_img,
            vk::ImageLayout::GENERAL,
            &clear_value,
            &[clear_range],
        );

        // TRANSFER → COMPUTE barrier so the dispatch's atomic adds see zeros.
        let post_clear_barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot_img)
            .subresource_range(clear_range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[post_clear_barrier],
        );

        // ── Dispatch ──────────────────────────────────────────────────
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[frame]],
            &[],
        );
        let gx = self.width.div_ceil(8);
        let gy = self.height.div_ceil(8);
        device.cmd_dispatch(cmd, gx, gy, 1);

        // ── COMPUTE → FRAGMENT barrier for composite sample ───────────
        let out_barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot_img)
            .subresource_range(clear_range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[out_barrier],
        );

        Ok(())
    }

    /// Recreate accumulator images and rewrite descriptor sets on resize.
    ///
    /// Self-contained per #1031 / REN-D10-NEW-11: fresh slot images
    /// are created at `initial_layout: UNDEFINED` and walked to
    /// GENERAL via [`Self::initialize_layouts`] internally, so
    /// post-resize first dispatches see a valid storage layout.
    #[allow(clippy::too_many_arguments)]
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        depth_view: vk::ImageView,
        normal_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        light_buffers: &[GpuBuffer],
        light_buffer_size: vk::DeviceSize,
        camera_buffers: &[GpuBuffer],
        camera_buffer_size: vk::DeviceSize,
        instance_buffers: &[GpuBuffer],
        instance_buffer_size: vk::DeviceSize,
        width: u32,
        height: u32,
    ) -> Result<()> {
        for slot in self.slots.drain(..) {
            // SAFETY: `recreate_on_resize` runs from the fenced
            // swapchain-resize path (`VulkanContext::recreate_swapchain`
            // waits both frames-in-flight first). Slot view / image
            // handles are unreferenced by any in-flight command.
            unsafe {
                device.destroy_image_view(slot.storage_view, None);
                device.destroy_image_view(slot.sampled_view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
        self.width = width;
        self.height = height;

        let res = (|| -> Result<()> {
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                self.slots.push(Self::create_slot(
                    device,
                    allocator,
                    width,
                    height,
                    &format!("caustic_accum_{i}"),
                )?);
            }
            Ok(())
        })();
        if let Err(ref e) = res {
            log::error!("Caustic recreate partial failure: {e}");
            // SAFETY: fenced-resize path; partial state is unreferenced
            // by any in-flight command.
            unsafe { self.destroy(device, allocator) };
            return res;
        }

        self.write_descriptor_sets(
            device,
            depth_view,
            normal_views,
            mesh_id_views,
            light_buffers,
            light_buffer_size,
            camera_buffers,
            camera_buffer_size,
            instance_buffers,
            instance_buffer_size,
        );

        // #1031 — walk fresh slot images from UNDEFINED to GENERAL.
        // SAFETY: fenced-resize contract — no concurrent reader on
        // these images. Warn-log on failure matches the caller's
        // pre-#1031 behaviour.
        if let Err(e) = unsafe { self.initialize_layouts(device, queue, command_pool) } {
            log::warn!("Caustic layout re-init after resize failed: {e}");
        }
        Ok(())
    }

    /// # Safety
    /// Must be called before the device + allocator are dropped.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // SAFETY (whole function): caller of `destroy` (unsafe fn)
        // guarantees no in-flight command buffer references any object
        // owned by `self`. Per-handle `if != null()` guards make this
        // safe to call on partially-initialised state from
        // `try_or_cleanup`.
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
        for slot in self.slots.drain(..) {
            // SAFETY: caller's unsafe-fn contract — no in-flight cmd
            // buffer references slot resources.
            unsafe {
                device.destroy_image_view(slot.storage_view, None);
                device.destroy_image_view(slot.sampled_view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
    }
}

// CAUSTIC_FIXED_SCALE drift test moved to shader_constants::tests after #1038
// folded the constant into the build.rs codegen path. Canonical check:
//   shader_constants::tests::generated_header_contains_all_defines
//   shader_constants::tests::affected_shaders_include_constants_header
