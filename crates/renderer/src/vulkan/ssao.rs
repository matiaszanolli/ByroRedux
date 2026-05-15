//! Screen-space ambient occlusion (SSAO) compute pipeline.
//!
//! Runs after the main render pass (but before composite) to produce an R8
//! occlusion texture from the depth buffer. The fragment shader reads this
//! texture the same frame to darken corners, crevices, and contact shadows.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    memory_barrier, write_combined_image_sampler, write_storage_image, write_uniform_buffer,
    DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use anyhow::{Context, Result};
use ash::vk;

const SSAO_COMP_SPV: &[u8] = include_bytes!("../../shaders/ssao.comp.spv");

/// SSAO parameters uploaded as a UBO.
#[repr(C)]
#[derive(Clone, Copy)]
struct SsaoParams {
    /// View-projection matrix (column-major).
    view_proj: [[f32; 4]; 4],
    /// Precomputed `inverse(viewProj)` for world-space reconstruction from depth.
    inv_view_proj: [[f32; 4]; 4],
    /// x = radius (pixels), y = depth bias, z = intensity, w = unused.
    params: [f32; 4],
    /// x = width, y = height, z = 1/width, w = 1/height.
    screen_size: [f32; 4],
    /// xyz = camera world position, w = unused.
    camera_pos: [f32; 4],
}

pub struct SsaoPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per-frame parameter UBOs.
    param_buffers: Vec<GpuBuffer>,
    /// Per-frame AO output images (full-resolution R8). Double-buffered to
    /// prevent cross-frame RAW hazards — each frame-in-flight slot writes
    /// its own image, so frame N's compute dispatch doesn't race with frame
    /// N-1's fragment shader read. See #267.
    pub ao_images: Vec<vk::Image>,
    pub ao_image_views: Vec<vk::ImageView>,
    ao_allocations: Vec<Option<gpu_allocator::vulkan::Allocation>>,
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
        pipeline_cache: vk::PipelineCache,
        depth_image_view: vk::ImageView,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        // Inner function does the actual work. On error, the caller
        // cleans up any resources that were partially created.
        let result = Self::new_inner(
            device,
            allocator,
            pipeline_cache,
            depth_image_view,
            width,
            height,
        );
        if let Err(ref e) = result {
            log::debug!("SSAO pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        depth_image_view: vk::ImageView,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let max_frames = 2;

        // Build a partially-valid Self so we can use destroy() for cleanup.
        // Fields that haven't been created yet use null handles — destroy()
        // calls vkDestroy* on null which is always a no-op per Vulkan spec.
        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            param_buffers: Vec::new(),
            ao_images: Vec::new(),
            ao_image_views: Vec::new(),
            ao_allocations: Vec::new(),
            ao_sampler: vk::Sampler::null(),
            depth_sampler: vk::Sampler::null(),
            width,
            height,
        };

        // Create per-frame AO output images (R8, full resolution).
        // Double-buffered to prevent cross-frame RAW hazards (#267).
        for fi in 0..max_frames {
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
                .usage(
                    vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_DST,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);

            // SAFETY: `ao_image_info` is fully populated above with valid
            // extent / format / usage. Handle is owned by `partial.ao_images`
            // on Ok, freed by `partial.destroy()` on Err.
            let ao_image = match unsafe { device.create_image(&ao_image_info, None) } {
                Ok(img) => img,
                Err(e) => {
                    // SAFETY: `partial` is local; no GPU work has begun
                    // referencing any of its handles yet. Cleanup-on-error.
                    unsafe { partial.destroy(device, allocator) };
                    return Err(anyhow::anyhow!("Failed to create AO image {fi}: {e}"));
                }
            };
            partial.ao_images.push(ao_image);

            let ao_allocation = match allocator.lock().expect("allocator lock").allocate(
                &gpu_allocator::vulkan::AllocationCreateDesc {
                    name: &format!("ssao_output_{fi}"),
                    // SAFETY: `ao_image` was just created above and pushed
                    // into `partial.ao_images`; handle is live.
                    requirements: unsafe { device.get_image_memory_requirements(ao_image) },
                    location: gpu_allocator::MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
                },
            ) {
                Ok(a) => a,
                Err(e) => {
                    // SAFETY: see `partial.destroy` above — cleanup-on-error.
                    unsafe { partial.destroy(device, allocator) };
                    return Err(anyhow::anyhow!("Failed to allocate AO memory {fi}: {e}"));
                }
            };

            // SAFETY: `ao_image` matches the memory requirements that
            // produced `ao_allocation`; bound once per image.
            if let Err(e) = unsafe {
                device.bind_image_memory(ao_image, ao_allocation.memory(), ao_allocation.offset())
            } {
                allocator
                    .lock()
                    .expect("allocator lock")
                    .free(ao_allocation)
                    .ok();
                // SAFETY: cleanup-on-error after explicitly freeing the
                // unboundallocation above.
                unsafe { partial.destroy(device, allocator) };
                return Err(anyhow::anyhow!("Failed to bind AO image memory {fi}: {e}"));
            }
            partial.ao_allocations.push(Some(ao_allocation));

            // SAFETY: `ao_image` is bound to backing memory (line above).
            // View ownership transfers to `partial.ao_image_views` on Ok.
            let view = match unsafe {
                device.create_image_view(
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
            } {
                Ok(v) => v,
                Err(e) => {
                    // SAFETY: cleanup-on-error.
                    unsafe { partial.destroy(device, allocator) };
                    return Err(anyhow::anyhow!("Failed to create AO view {fi}: {e}"));
                }
            };
            partial.ao_image_views.push(view);
        }

        // Macro to clean up partial state on error and return.
        // SAFETY (inside macro): `partial` is local to the enclosing
        // function; cleanup-on-error before any GPU work references it.
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

        // Samplers.
        // SAFETY: SamplerCreateInfo is fully populated above; handle
        // ownership transfers to `partial.depth_sampler`, freed by destroy().
        partial.depth_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::NEAREST)
                        .min_filter(vk::Filter::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("depth sampler")
        });

        // SAFETY: same contract as `depth_sampler` above.
        partial.ao_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("ao sampler")
        });

        // Parameter UBOs.
        let param_size = std::mem::size_of::<SsaoParams>() as vk::DeviceSize;
        for _ in 0..max_frames {
            let buf = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buf);
        }

        // Descriptor set layout.
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
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "ssao.comp",
                spirv: SSAO_COMP_SPV,
            }],
            "ssao",
            &[],
        )
        .expect("ssao descriptor layout drifted against ssao.comp (see #427)");
        // SAFETY: bindings array (above) is validated against ssao.comp
        // by `validate_set_layout`. Layout handle owned by `partial`.
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("SSAO descriptor set layout")
        });

        // SAFETY: `partial.descriptor_set_layout` just created above; the
        // slice-from-ref borrow lives only for this call.
        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout)),
                    None,
                )
                .context("SSAO pipeline layout")
        });

        // Compute pipeline.
        let shader_module =
            try_or_cleanup!(super::pipeline::load_shader_module(device, SSAO_COMP_SPV));
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader_module)
            .name(c"main");
        // SAFETY: `stage` references `shader_module` (just loaded above),
        // `partial.pipeline_layout` (just created above). Pipeline cache
        // is `vk::PipelineCache::null()` from VulkanContext on first call
        // — valid.
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
                .context("SSAO compute pipeline")
        } {
            Ok(pipelines) => {
                // SAFETY: shader module is no longer needed once the
                // compute pipeline has been created (Vulkan spec: the
                // shader is copied into the pipeline at create time).
                unsafe { device.destroy_shader_module(shader_module, None) };
                pipelines[0]
            }
            Err(e) => {
                // SAFETY: same as Ok branch above — shader module no
                // longer referenced. Then cleanup-on-error for partial.
                unsafe { device.destroy_shader_module(shader_module, None) };
                unsafe { partial.destroy(device, allocator) };
                return Err(e);
            }
        };

        // Descriptor pool + sets — sizes derived from `bindings`
        // (#1030 / REN-D10-NEW-09).
        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &bindings,
            max_frames as u32,
        )
        .max_sets(max_frames as u32)
        .build(device, "SSAO descriptor pool"));

        let layouts = vec![partial.descriptor_set_layout; max_frames];
        // SAFETY: `partial.descriptor_pool` was just created with enough
        // capacity for `max_frames` sets; `layouts` is a length-`max_frames`
        // vec of the same descriptor_set_layout handle.
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("SSAO descriptor sets")
        });

        // Write descriptor sets.
        for i in 0..max_frames {
            let depth_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.depth_sampler)
                .image_view(depth_image_view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
            let ao_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.ao_image_views[i])
                .image_layout(vk::ImageLayout::GENERAL)];
            let param_info = [vk::DescriptorBufferInfo {
                buffer: partial.param_buffers[i].buffer,
                offset: 0,
                range: param_size,
            }];
            let set = partial.descriptor_sets[i];
            let writes = [
                write_combined_image_sampler(set, 0, &depth_info),
                write_storage_image(set, 1, &ao_info),
                write_uniform_buffer(set, 2, &param_info),
            ];
            // SAFETY: descriptor sets just allocated above; `writes`
            // references images/buffers all owned by `partial` (depth view,
            // ao image view, param buffer) — all live for the duration of
            // this descriptor set.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        log::info!("SSAO pipeline created: {}x{}", width, height);

        Ok(partial)
    }

    /// Transition all per-frame AO images from UNDEFINED to
    /// SHADER_READ_ONLY_OPTIMAL and clear them to white (1.0 = no occlusion).
    /// Must be called once after creation so the fragment shader sees a valid
    /// image on the first frame (before the first SSAO dispatch has run).
    pub unsafe fn initialize_ao_images(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            for &img in &self.ao_images {
                // UNDEFINED → TRANSFER_DST for the clear.
                let barrier = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .image(img)
                    .subresource_range(range);
                // SAFETY: caller of `initialize_ao_images` (unsafe fn,
                // line 404) guarantees device/queue/pool validity. `cmd`
                // is the recording buffer from `with_one_time_commands`.
                // The barrier transitions UNDEFINED → TRANSFER_DST_OPTIMAL
                // and the subsequent calls (cmd_clear_color_image,
                // post-clear barrier) run in the same recording scope.
                unsafe {
                    device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TOP_OF_PIPE,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier],
                    );
                    device.cmd_clear_color_image(
                        cmd,
                        img,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &vk::ClearColorValue {
                            float32: [1.0, 0.0, 0.0, 0.0],
                        },
                        &[range],
                    );
                    let barrier2 = vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .image(img)
                        .subresource_range(range);
                    device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier2],
                    );
                }
            }
            Ok(())
        })
    }

    /// Upload SSAO parameters and dispatch the compute shader.
    ///
    /// Call AFTER the main render pass but BEFORE composite (depth buffer
    /// must be written and transitioned to READ_ONLY). The AO output image
    /// is written in GENERAL layout, transitioned to SHADER_READ_ONLY, and
    /// sampled by this frame's fragment shader.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        view_proj: &[[f32; 4]; 4],
        inv_view_proj: &[[f32; 4]; 4],
        camera_pos: [f32; 3],
    ) -> Result<()> {
        let params = SsaoParams {
            view_proj: *view_proj,
            inv_view_proj: *inv_view_proj,
            params: [16.0, 0.0002, 2.0, 0.0], // radius=16px, bias=0.0002, intensity=2.0
            screen_size: [
                self.width as f32,
                self.height as f32,
                1.0 / self.width as f32,
                1.0 / self.height as f32,
            ],
            camera_pos: [camera_pos[0], camera_pos[1], camera_pos[2], 0.0],
        };
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&params))?;

        // HOST → COMPUTE_SHADER (UBO flush before dispatch; required even for
        // HOST_COHERENT memory — the execution dependency ensures ordering).
        memory_barrier(
            device, cmd,
            vk::PipelineStageFlags::HOST,
            vk::AccessFlags::HOST_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::UNIFORM_READ,
        );

        // Transition this frame's AO image to GENERAL for compute write.
        // The actual layout coming in is always SHADER_READ_ONLY_OPTIMAL:
        // `initialize_ao_images` leaves it that way after the clear-to-1.0,
        // and the post-dispatch barrier below restores it at end of every
        // frame. The pre-#673 form used `UNDEFINED` which the spec defines
        // as "discard contents" — the cleared 1.0 (no-occlusion) value
        // initialize_ao_images sets up was being formally discarded on
        // frame 1 before the dispatch ever wrote it. Invisible in the
        // common case (compute writes every pixel) but UB on a partial
        // dispatch (early-out bounds check, lost device). Match the
        // steady-state pattern svgf.rs:746 / taa.rs:617 already use.
        let ao_image = self.ao_images[frame];
        let ao_barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(ao_image)
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
            .image(ao_image)
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
        for &view in &self.ao_image_views {
            device.destroy_image_view(view, None);
        }
        self.ao_image_views.clear();
        for &img in &self.ao_images {
            device.destroy_image(img, None);
        }
        self.ao_images.clear();
        for alloc in self.ao_allocations.drain(..) {
            if let Some(a) = alloc {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        // #732 LIFE-N1 — drop the `GpuBuffer` structs after their GPU
        // allocations are freed so each one's `Arc<Mutex<Allocator>>`
        // clone releases now, not when `SsaoPipeline` itself naturally
        // drops at the tail of `VulkanContext::Drop` (after
        // `Arc::try_unwrap` has already given up).
        self.param_buffers.clear();
        device.destroy_sampler(self.ao_sampler, None);
        device.destroy_sampler(self.depth_sampler, None);
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}
