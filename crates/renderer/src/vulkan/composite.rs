//! HDR composite + tone-mapping pass.
//!
//! Owns the HDR color intermediate images that the main render pass writes
//! to, and the fullscreen composite pipeline that samples those images,
//! applies ACES tone mapping, and writes the result to the sRGB swapchain.
//!
//! This is the last pass of the frame. Runs as a dedicated render pass
//! after the main render pass ends:
//!
//!   main render pass → (HDR image in SHADER_READ_ONLY layout)
//!   composite render pass → (swapchain image in PRESENT_SRC_KHR layout)
//!   SSAO dispatch (reads depth, unchanged)
//!   submit + present
//!
//! ## Per-frame HDR images
//!
//! With MAX_FRAMES_IN_FLIGHT in flight simultaneously, a single HDR image
//! would create a read-after-write hazard: frame N's composite reads HDR
//! while frame N+1's main render pass writes it. We use one HDR image
//! per frame-in-flight slot. Memory cost: ~16 MB at 1080p (2 × RGBA16F).
//!
//! ## Per-swapchain-image composite framebuffers
//!
//! The composite render pass writes to the swapchain, which has its own
//! image per swapchain slot (typically 3). We create one composite
//! framebuffer per swapchain image, binding just the swapchain view (no
//! depth needed — fullscreen triangle, depth test disabled).

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

/// Composite parameter UBO. Currently minimal — fog state is the only
/// per-frame input. Grows in later phases as SVGF settings are added.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CompositeParams {
    /// xyz = RGB, w = fog enabled (1.0 = yes, 0.0 = no)
    pub fog_color: [f32; 4],
    /// x = fog near, y = fog far, z/w = unused
    pub fog_params: [f32; 4],
    /// Reserved for future use (camera near/far, debug flags, etc.)
    pub depth_params: [f32; 4],
}

/// HDR color format. RGBA16F = 8 bytes/pixel, sufficient dynamic range
/// for all real-world scene brightness, supports alpha for glass blending.
pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// Owns the HDR intermediates + composite pipeline + composite render pass.
pub struct CompositePipeline {
    /// HDR color images (one per frame-in-flight slot).
    pub hdr_images: Vec<vk::Image>,
    /// HDR color image views (parallel to hdr_images).
    pub hdr_image_views: Vec<vk::ImageView>,
    /// GPU-local allocations backing hdr_images.
    hdr_allocations: Vec<Option<vk_alloc::Allocation>>,

    /// Dedicated render pass for the composite step. Single color attachment
    /// = swapchain format, no depth.
    pub composite_render_pass: vk::RenderPass,
    /// Per-swapchain-image composite framebuffer (binds just swapchain view).
    composite_framebuffers: Vec<vk::Framebuffer>,

    /// Graphics pipeline: fullscreen triangle + ACES tone map fragment shader.
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    /// One descriptor set per frame-in-flight. Each references that frame's
    /// HDR + raw_indirect + albedo views + the per-frame params UBO.
    descriptor_sets: Vec<vk::DescriptorSet>,
    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,
    /// Sampler for reading all the input textures (HDR, indirect, albedo).
    hdr_sampler: vk::Sampler,
    /// Per-frame parameter UBOs.
    param_buffers: Vec<GpuBuffer>,

    pub width: u32,
    pub height: u32,
}

impl CompositePipeline {
    /// Create all HDR intermediate images, the composite render pass +
    /// pipeline, and the per-swapchain-image composite framebuffers.
    ///
    /// `raw_indirect_views` and `albedo_views` are owned by the G-buffer
    /// module (one view per frame-in-flight); the composite pipeline just
    /// references them via descriptor sets.
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        swapchain_format: vk::Format,
        swapchain_views: &[vk::ImageView],
        raw_indirect_views: &[vk::ImageView],
        albedo_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let result = Self::new_inner(
            device,
            allocator,
            swapchain_format,
            swapchain_views,
            raw_indirect_views,
            albedo_views,
            width,
            height,
        );
        if let Err(ref e) = result {
            log::debug!("Composite pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        swapchain_format: vk::Format,
        swapchain_views: &[vk::ImageView],
        raw_indirect_views: &[vk::ImageView],
        albedo_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        debug_assert_eq!(raw_indirect_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(albedo_views.len(), MAX_FRAMES_IN_FLIGHT);
        // Build a partially-valid Self so we can use destroy() for cleanup
        // on any error. Fields that haven't been created yet use null
        // handles — destroy() calls vkDestroy* on null (always a no-op).
        let mut partial = Self {
            hdr_images: Vec::new(),
            hdr_image_views: Vec::new(),
            hdr_allocations: Vec::new(),
            composite_render_pass: vk::RenderPass::null(),
            composite_framebuffers: Vec::new(),
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            vert_module: vk::ShaderModule::null(),
            frag_module: vk::ShaderModule::null(),
            hdr_sampler: vk::Sampler::null(),
            param_buffers: Vec::new(),
            width,
            height,
        };

        // Macro to clean up partial state on any fallible call.
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

        // ── 1. Create HDR images (one per frame-in-flight) ───────────
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let img_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(HDR_FORMAT)
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
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::SAMPLED,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let img = try_or_cleanup!(unsafe {
                device
                    .create_image(&img_info, None)
                    .context("Failed to create HDR color image")
            });
            partial.hdr_images.push(img);
            partial.hdr_allocations.push(None);

            let alloc = try_or_cleanup!(allocator
                .lock()
                .expect("allocator lock")
                .allocate(&vk_alloc::AllocationCreateDesc {
                    name: &format!("hdr_color_{}", i),
                    requirements: unsafe { device.get_image_memory_requirements(img) },
                    location: gpu_allocator::MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                })
                .context("Failed to allocate HDR image memory"));
            try_or_cleanup!(unsafe {
                device
                    .bind_image_memory(img, alloc.memory(), alloc.offset())
                    .context("bind HDR image memory")
            });
            partial.hdr_allocations[i] = Some(alloc);

            let view = try_or_cleanup!(unsafe {
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(HDR_FORMAT)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            }),
                        None,
                    )
                    .context("HDR image view")
            });
            partial.hdr_image_views.push(view);
        }

        // ── 2. HDR sampler (linear filter for slight bilinear smoothing) ──
        partial.hdr_sampler = try_or_cleanup!(unsafe {
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
                .context("HDR sampler")
        });

        // ── 3. Composite render pass ─────────────────────────────────
        // Single color attachment = swapchain. Load DONT_CARE (fullscreen
        // triangle covers every pixel). Final layout PRESENT_SRC_KHR.
        let composite_color = vk::AttachmentDescription::default()
            .format(swapchain_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let composite_color_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };
        let composite_color_refs = [composite_color_ref];

        let composite_subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&composite_color_refs);

        // Incoming dependency: wait for the main render pass to finish
        // writing the HDR color attachment (FRAGMENT_SHADER stage reads it).
        let composite_dep_in = vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags::SHADER_READ);

        // Outgoing dependency: ensure swapchain write finishes before present.
        let composite_dep_out = vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags::empty());

        let attachments = [composite_color];
        let subpasses = [composite_subpass];
        let dependencies = [composite_dep_in, composite_dep_out];
        let rp_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);
        partial.composite_render_pass = try_or_cleanup!(unsafe {
            device
                .create_render_pass(&rp_info, None)
                .context("composite render pass")
        });

        // ── 4. Composite framebuffers (one per swapchain image) ─────
        for &view in swapchain_views {
            let attachments = [view];
            let fb_info = vk::FramebufferCreateInfo::default()
                .render_pass(partial.composite_render_pass)
                .attachments(&attachments)
                .width(width)
                .height(height)
                .layers(1);
            let fb = try_or_cleanup!(unsafe {
                device
                    .create_framebuffer(&fb_info, None)
                    .context("composite framebuffer")
            });
            partial.composite_framebuffers.push(fb);
        }

        // ── 5. Per-frame parameter UBOs ──────────────────────────────
        let param_size = std::mem::size_of::<CompositeParams>() as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buf);
        }

        // ── 6. Descriptor set layout + pipeline layout ───────────────
        // Phase 2: 4 bindings — HDR, indirect, albedo, params UBO.
        let ds_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&ds_bindings),
                    None,
                )
                .context("composite descriptor set layout")
        });

        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout)),
                    None,
                )
                .context("composite pipeline layout")
        });

        // ── 7. Descriptor pool + per-frame descriptor sets ───────────
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 3) as u32,
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
                .context("composite descriptor pool")
        });

        let set_layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("composite descriptor sets")
        });

        // Write each descriptor set to sample its own frame's HDR + indirect +
        // albedo views and bind its own param UBO.
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let hdr_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.hdr_sampler)
                .image_view(partial.hdr_image_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let indirect_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.hdr_sampler)
                .image_view(raw_indirect_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let albedo_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.hdr_sampler)
                .image_view(albedo_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let params_info = [vk::DescriptorBufferInfo {
                buffer: partial.param_buffers[i].buffer,
                offset: 0,
                range: param_size,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&hdr_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&indirect_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&albedo_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&params_info),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        // ── 8. Shader modules ────────────────────────────────────────
        let vert_spv = include_bytes!("../../shaders/composite.vert.spv");
        let frag_spv = include_bytes!("../../shaders/composite.frag.spv");
        partial.vert_module =
            try_or_cleanup!(super::pipeline::load_shader_module(device, vert_spv));
        partial.frag_module =
            try_or_cleanup!(super::pipeline::load_shader_module(device, frag_spv));

        // ── 9. Graphics pipeline ─────────────────────────────────────
        let entry_point = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(partial.vert_module)
                .name(entry_point),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(partial.frag_module)
                .name(entry_point),
        ];

        // No vertex input — the fullscreen triangle is generated in the
        // vertex shader from gl_VertexIndex.
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewports = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let scissors = [vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D { width, height },
        }];
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissors);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        // No depth test/write — fullscreen triangle covers everything.
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(false)
            .depth_write_enable(false);

        let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(false)];
        let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state)
            .layout(partial.pipeline_layout)
            .render_pass(partial.composite_render_pass)
            .subpass(0);

        partial.pipeline = match unsafe {
            device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[pipeline_info],
                None,
            )
        } {
            Ok(pipelines) => pipelines[0],
            Err((_, e)) => {
                unsafe { partial.destroy(device, allocator) };
                return Err(anyhow::anyhow!("composite graphics pipeline: {e}"));
            }
        };

        log::info!("Composite pipeline created: {}x{} HDR", width, height);

        Ok(partial)
    }

    /// Begin composite render pass + draw fullscreen triangle + end.
    /// Call after the main render pass ends and before submit.
    ///
    /// Safety: `cmd` must be a valid recording command buffer. Frame index
    /// must be < MAX_FRAMES_IN_FLIGHT. Swapchain image index must be valid.
    pub unsafe fn dispatch(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        swapchain_image_index: usize,
    ) {
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        }];
        let rp_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.composite_render_pass)
            .framebuffer(self.composite_framebuffers[swapchain_image_index])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: self.width,
                    height: self.height,
                },
            })
            .clear_values(&clear_values);
        unsafe {
            device.cmd_begin_render_pass(cmd, &rp_begin, vk::SubpassContents::INLINE);
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.width as f32,
                height: self.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };
            device.cmd_set_viewport(cmd, 0, &[viewport]);
            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: self.width,
                    height: self.height,
                },
            };
            device.cmd_set_scissor(cmd, 0, &[scissor]);

            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_sets[frame]],
                &[],
            );

            // Fullscreen triangle: 3 vertices, no bound vertex buffer.
            device.cmd_draw(cmd, 3, 1, 0, 0);

            device.cmd_end_render_pass(cmd);
        }
    }

    /// Recreate framebuffers and pipeline viewport-dependent state on
    /// swapchain resize. The HDR images themselves are recreated because
    /// their size matches the swapchain. Caller must also pass the
    /// G-buffer's new raw_indirect + albedo views (which they just
    /// recreated via `GBuffer::recreate_on_resize`).
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        swapchain_views: &[vk::ImageView],
        raw_indirect_views: &[vk::ImageView],
        albedo_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<()> {
        // Destroy old framebuffers
        for &fb in &self.composite_framebuffers {
            unsafe { device.destroy_framebuffer(fb, None) };
        }
        self.composite_framebuffers.clear();

        // Destroy old HDR images
        for &view in &self.hdr_image_views {
            unsafe { device.destroy_image_view(view, None) };
        }
        self.hdr_image_views.clear();
        for &img in &self.hdr_images {
            unsafe { device.destroy_image(img, None) };
        }
        self.hdr_images.clear();
        for alloc in self.hdr_allocations.drain(..) {
            if let Some(a) = alloc {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }

        self.width = width;
        self.height = height;

        // Recreate HDR images
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let img_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(HDR_FORMAT)
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
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let img = unsafe { device.create_image(&img_info, None)? };
            self.hdr_images.push(img);

            let alloc = allocator
                .lock()
                .expect("allocator lock")
                .allocate(&vk_alloc::AllocationCreateDesc {
                    name: &format!("hdr_color_{}", i),
                    requirements: unsafe { device.get_image_memory_requirements(img) },
                    location: gpu_allocator::MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                })?;
            unsafe { device.bind_image_memory(img, alloc.memory(), alloc.offset())? };
            self.hdr_allocations.push(Some(alloc));

            let view = unsafe {
                device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(img)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(HDR_FORMAT)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )?
            };
            self.hdr_image_views.push(view);
        }

        // Rewrite descriptor sets to point at the new HDR, raw_indirect,
        // and albedo image views. Params UBO buffers are unchanged.
        let param_size = std::mem::size_of::<CompositeParams>() as vk::DeviceSize;
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let hdr_info = [vk::DescriptorImageInfo::default()
                .sampler(self.hdr_sampler)
                .image_view(self.hdr_image_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let indirect_info = [vk::DescriptorImageInfo::default()
                .sampler(self.hdr_sampler)
                .image_view(raw_indirect_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let albedo_info = [vk::DescriptorImageInfo::default()
                .sampler(self.hdr_sampler)
                .image_view(albedo_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let params_info = [vk::DescriptorBufferInfo {
                buffer: self.param_buffers[i].buffer,
                offset: 0,
                range: param_size,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&hdr_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[i])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&indirect_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[i])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&albedo_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_sets[i])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&params_info),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        // Recreate composite framebuffers (bound to swapchain views).
        for &view in swapchain_views {
            let attachments = [view];
            let fb_info = vk::FramebufferCreateInfo::default()
                .render_pass(self.composite_render_pass)
                .attachments(&attachments)
                .width(width)
                .height(height)
                .layers(1);
            let fb = unsafe { device.create_framebuffer(&fb_info, None)? };
            self.composite_framebuffers.push(fb);
        }

        Ok(())
    }

    /// Upload per-frame composite parameters (fog state, etc.) to the
    /// frame's UBO. Call once per frame before `dispatch`.
    pub fn upload_params(
        &mut self,
        device: &ash::Device,
        frame: usize,
        params: &CompositeParams,
    ) -> Result<()> {
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(params))
    }

    /// Destroy all Vulkan objects. Must be called before the device/allocator
    /// are dropped. Safe to call on partially-initialized state.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        self.param_buffers.clear();
        if self.pipeline != vk::Pipeline::null() {
            unsafe { device.destroy_pipeline(self.pipeline, None) };
        }
        if self.vert_module != vk::ShaderModule::null() {
            unsafe { device.destroy_shader_module(self.vert_module, None) };
        }
        if self.frag_module != vk::ShaderModule::null() {
            unsafe { device.destroy_shader_module(self.frag_module, None) };
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
        for &fb in &self.composite_framebuffers {
            unsafe { device.destroy_framebuffer(fb, None) };
        }
        self.composite_framebuffers.clear();
        if self.composite_render_pass != vk::RenderPass::null() {
            unsafe { device.destroy_render_pass(self.composite_render_pass, None) };
        }
        if self.hdr_sampler != vk::Sampler::null() {
            unsafe { device.destroy_sampler(self.hdr_sampler, None) };
        }
        for &view in &self.hdr_image_views {
            unsafe { device.destroy_image_view(view, None) };
        }
        self.hdr_image_views.clear();
        for &img in &self.hdr_images {
            unsafe { device.destroy_image(img, None) };
        }
        self.hdr_images.clear();
        for alloc in self.hdr_allocations.drain(..) {
            if let Some(a) = alloc {
                allocator
                    .lock()
                    .expect("allocator lock")
                    .free(a)
                    .ok();
            }
        }
    }
}
