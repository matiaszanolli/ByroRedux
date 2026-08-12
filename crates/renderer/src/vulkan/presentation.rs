//! Output-resolution presentation pass.
//!
//! Scene composition now produces render-resolution linear HDR. The frame
//! upscaler reconstructs it into an output-resolution HDR target, and this
//! final fullscreen pass applies IMAD lens/color curves, exposure, ACES, and
//! underwater treatment before writing the sRGB swapchain. Keeping this pass
//! after the upscale boundary is what makes the native-copy bridge and the
//! FSR dispatch interchangeable.

use super::descriptors::{
    write_combined_image_sampler, write_storage_buffer, DescriptorPoolBuilder,
};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;

const PRESENTATION_VERT_SPV: &[u8] = include_bytes!("../../shaders/composite.vert.spv");
const PRESENTATION_FRAG_SPV: &[u8] = include_bytes!("../../shaders/presentation.frag.spv");

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct PresentationPushConstants {
    underwater: [f32; 4],
    exposure: f32,
    padding: [f32; 3],
    lens: [f32; 4],
    radial_curve: [f32; 4],
    grade: [f32; 4],
    radial_center: [f32; 4],
    tint_color: [f32; 4],
    fade_color: [f32; 4],
}

/// Output-resolution image-space state supplied by the gameplay runtime.
/// Defaults are exact identity, so games and frames without active IMADs pay
/// only the shader's uniform branch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageSpaceModifierView {
    pub blur_radius_pixels: f32,
    pub double_vision_strength: f32,
    pub motion_blur_strength: f32,
    pub radial_blur_strength: f32,
    pub radial_blur_ramp_up: f32,
    pub radial_blur_start: f32,
    pub radial_blur_ramp_down: f32,
    pub radial_blur_down_start: f32,
    pub radial_blur_center: [f32; 2],
    pub saturation: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub tint_color: [f32; 4],
    pub fade_color: [f32; 4],
}

impl Default for ImageSpaceModifierView {
    fn default() -> Self {
        Self {
            blur_radius_pixels: 0.0,
            double_vision_strength: 0.0,
            motion_blur_strength: 0.0,
            radial_blur_strength: 0.0,
            radial_blur_ramp_up: 0.0,
            radial_blur_start: 0.0,
            radial_blur_ramp_down: 0.0,
            radial_blur_down_start: 1.0,
            radial_blur_center: [0.5, 0.5],
            saturation: 1.0,
            brightness: 1.0,
            contrast: 1.0,
            tint_color: [1.0, 1.0, 1.0, 0.0],
            fade_color: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PresentationFrame {
    pub exposure: f32,
    pub underwater: [f32; 4],
    pub image_space: ImageSpaceModifierView,
}

pub struct PresentationPipeline {
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    sampler: vk::Sampler,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,
    extent: vk::Extent2D,
    /// Per-frame image-health counter buffers (EX-05 / #2736).
    ///
    /// Handles only — the allocations are owned by `VulkanContext` because
    /// they must survive a swapchain recreate, which rebuilds this pipeline.
    health_buffers: Vec<vk::Buffer>,
}

impl PresentationPipeline {
    pub fn new(
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        swapchain_format: vk::Format,
        swapchain_views: &[vk::ImageView],
        upscaled_views: &[vk::ImageView],
        health_buffers: &[vk::Buffer],
        extent: vk::Extent2D,
    ) -> Result<Self> {
        debug_assert_eq!(upscaled_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(health_buffers.len(), MAX_FRAMES_IN_FLIGHT);
        let mut pipeline = Self {
            render_pass: vk::RenderPass::null(),
            framebuffers: Vec::new(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            sampler: vk::Sampler::null(),
            health_buffers: health_buffers.to_vec(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            vert_module: vk::ShaderModule::null(),
            frag_module: vk::ShaderModule::null(),
            extent,
        };

        let result = pipeline.create(
            device,
            pipeline_cache,
            swapchain_format,
            swapchain_views,
            upscaled_views,
        );
        if let Err(error) = result {
            unsafe {
                // SAFETY: construction failed before the partial pipeline
                // escaped; no submitted command buffer can reference it.
                pipeline.destroy(device);
            }
            return Err(error);
        }
        Ok(pipeline)
    }

    fn create(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        swapchain_format: vk::Format,
        swapchain_views: &[vk::ImageView],
        upscaled_views: &[vk::ImageView],
    ) -> Result<()> {
        self.sampler = unsafe {
            // SAFETY: device is live; the returned sampler is stored for
            // explicit teardown on every path.
            device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )
        }
        .context("create presentation sampler")?;

        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            // EX-05 / #2736 — per-frame image-health counters, incremented
            // atomically by the fragment shader on a non-finite scene texel.
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        self.descriptor_set_layout = unsafe {
            // SAFETY: `bindings` outlives the call and the returned layout is
            // owned by this pipeline.
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
        }
        .context("create presentation descriptor layout")?;
        self.descriptor_pool =
            DescriptorPoolBuilder::from_layout_bindings(&bindings, MAX_FRAMES_IN_FLIGHT as u32)
                .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
                .build(device, "presentation descriptor pool")?;
        let layouts = vec![self.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        self.descriptor_sets = unsafe {
            // SAFETY: the pool/layout are live and owned by this pipeline.
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.descriptor_pool)
                    .set_layouts(&layouts),
            )
        }
        .context("allocate presentation descriptor sets")?;
        self.write_inputs(device, upscaled_views);

        let color = vk::AttachmentDescription::default()
            .format(swapchain_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
        let color_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };
        let color_refs = [color_ref];
        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs);
        let incoming = vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(
                vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::TRANSFER
                    | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            )
            .src_access_mask(
                vk::AccessFlags::SHADER_WRITE
                    | vk::AccessFlags::TRANSFER_WRITE
                    | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .dst_stage_mask(
                vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            )
            .dst_access_mask(
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            );
        // #2143 / CONC-D1-2026-07-25-01 — declaring this dependency at all
        // *replaces* Vulkan's implicit end-of-pass dependency, so a
        // `dst_stage_mask` of NONE left the pass's COLOR_ATTACHMENT_WRITE with
        // an empty second sync scope: unordered against everything after it.
        // No live hazard, because both current consumers of the swapchain
        // image carry their own incoming barrier — the egui overlay
        // (COLOR_ATTACHMENT_OUTPUT) and the screenshot copy (TRANSFER) — and
        // the present itself is covered by `render_finished`. The exposure was
        // a future pass added here without its own barrier, which nothing in
        // `cargo test` can catch.
        //
        // The dst scope below names those two consumers rather than being
        // maximally wide, so it stays a description of the frame graph. A
        // third consumer means extending it. Mirrors `composite.rs`'s outgoing
        // dependency; `egui_pass.rs` makes the same point with BOTTOM_OF_PIPE.
        let outgoing = vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::TRANSFER,
            )
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags::TRANSFER_READ,
            );
        let attachments = [color];
        let subpasses = [subpass];
        let dependencies = [incoming, outgoing];
        self.render_pass = unsafe {
            // SAFETY: all referenced arrays outlive this call.
            device.create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments)
                    .subpasses(&subpasses)
                    .dependencies(&dependencies),
                None,
            )
        }
        .context("create presentation render pass")?;

        for &view in swapchain_views {
            let attachments = [view];
            let framebuffer = unsafe {
                // SAFETY: render pass and swapchain view are live and the
                // framebuffer extent matches the swapchain extent.
                device.create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(self.render_pass)
                        .attachments(&attachments)
                        .width(self.extent.width)
                        .height(self.extent.height)
                        .layers(1),
                    None,
                )
            }
            .context("create presentation framebuffer")?;
            self.framebuffers.push(framebuffer);
        }

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<PresentationPushConstants>() as u32);
        let set_layouts = [self.descriptor_set_layout];
        let push_ranges = [push_range];
        self.pipeline_layout = unsafe {
            // SAFETY: both local slices outlive the call.
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(&push_ranges),
                None,
            )
        }
        .context("create presentation pipeline layout")?;
        self.vert_module = super::pipeline::load_shader_module(device, PRESENTATION_VERT_SPV)?;
        self.frag_module = super::pipeline::load_shader_module(device, PRESENTATION_FRAG_SPV)?;

        let entry = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(self.vert_module)
                .name(entry),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(self.frag_module)
                .name(entry),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA);
        let blend_attachments = [blend_attachment];
        let blend =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
        let dynamic = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic);
        let create_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic_state)
            .layout(self.pipeline_layout)
            .render_pass(self.render_pass)
            .subpass(0);
        self.pipeline = match unsafe {
            // SAFETY: every create-info dependency is live and owned by this
            // pipeline; cache is the renderer's live shared cache.
            device.create_graphics_pipelines(pipeline_cache, &[create_info], None)
        } {
            Ok(pipelines) => pipelines[0],
            Err((_, error)) => {
                return Err(anyhow::anyhow!(
                    "create presentation graphics pipeline: {error}"
                ));
            }
        };
        Ok(())
    }

    fn write_inputs(&self, device: &ash::Device, upscaled_views: &[vk::ImageView]) {
        debug_assert_eq!(upscaled_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(self.health_buffers.len(), MAX_FRAMES_IN_FLIGHT);
        for (frame, &view) in upscaled_views.iter().enumerate() {
            let info = [vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(self.health_buffers[frame])
                .offset(0)
                .range(vk::WHOLE_SIZE)];
            let writes = [
                write_combined_image_sampler(self.descriptor_sets[frame], 0, &info),
                write_storage_buffer(self.descriptor_sets[frame], 1, &buffer_info),
            ];
            unsafe {
                // SAFETY: descriptor set and sampler are owned by `self`; the
                // caller guarantees each output view and health buffer
                // outlives this pipeline.
                device.update_descriptor_sets(&writes, &[]);
            }
        }
    }

    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass and `image_index` must
    /// name the currently-acquired swapchain image.
    pub(crate) unsafe fn dispatch(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        image_index: usize,
        input: PresentationFrame,
    ) {
        let clear = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        }];
        let begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: self.extent,
            })
            .clear_values(&clear);
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: self.extent.width as f32,
            height: self.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let constants = PresentationPushConstants {
            underwater: input.underwater,
            exposure: input.exposure,
            padding: [0.0; 3],
            lens: [
                input.image_space.blur_radius_pixels,
                input.image_space.double_vision_strength,
                input.image_space.motion_blur_strength,
                input.image_space.radial_blur_strength,
            ],
            radial_curve: [
                input.image_space.radial_blur_ramp_up,
                input.image_space.radial_blur_start,
                input.image_space.radial_blur_ramp_down,
                input.image_space.radial_blur_down_start,
            ],
            grade: [
                input.image_space.saturation,
                input.image_space.brightness,
                input.image_space.contrast,
                0.0,
            ],
            radial_center: [
                input.image_space.radial_blur_center[0],
                input.image_space.radial_blur_center[1],
                0.0,
                0.0,
            ],
            tint_color: input.image_space.tint_color,
            fade_color: input.image_space.fade_color,
        };
        let constant_bytes = unsafe {
            // SAFETY: `PresentationPushConstants` is repr(C), contains only
            // plain floats, and this byte slice is consumed synchronously.
            std::slice::from_raw_parts(
                (&constants as *const PresentationPushConstants).cast::<u8>(),
                std::mem::size_of::<PresentationPushConstants>(),
            )
        };
        unsafe {
            // SAFETY: caller contract guarantees recording state and valid
            // indices; all pipeline resources are live.
            device.cmd_begin_render_pass(cmd, &begin, vk::SubpassContents::INLINE);
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_set_viewport(cmd, 0, &[viewport]);
            device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent: self.extent,
                }],
            );
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_sets[frame]],
                &[],
            );
            device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                constant_bytes,
            );
            device.cmd_draw(cmd, 3, 1, 0, 0);
            device.cmd_end_render_pass(cmd);
        }
    }

    pub fn recreate(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        swapchain_format: vk::Format,
        swapchain_views: &[vk::ImageView],
        upscaled_views: &[vk::ImageView],
        extent: vk::Extent2D,
    ) -> Result<()> {
        // Capture before `destroy`/overwrite: the health buffers are owned by
        // `VulkanContext` and deliberately survive a swapchain recreate, so
        // the rebuilt pipeline must rebind the same ones.
        let health_buffers = std::mem::take(&mut self.health_buffers);
        unsafe {
            // SAFETY: swapchain recreation waits for device idle before this
            // method is called.
            self.destroy(device);
        }
        *self = Self::new(
            device,
            pipeline_cache,
            swapchain_format,
            swapchain_views,
            upscaled_views,
            &health_buffers,
            extent,
        )?;
        Ok(())
    }

    /// # Safety
    ///
    /// No in-flight command buffer may reference this pipeline.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        for framebuffer in self.framebuffers.drain(..) {
            unsafe {
                // SAFETY: `framebuffer` was created by this device in `new()`
                // and, per this fn's `# Safety` contract, no in-flight command
                // buffer still references it.
                device.destroy_framebuffer(framebuffer, None)
            };
        }
        if self.pipeline != vk::Pipeline::null() {
            unsafe {
                // SAFETY: `self.pipeline` was created by this device and is
                // not bound by any in-flight command buffer (fn contract).
                device.destroy_pipeline(self.pipeline, None)
            };
            self.pipeline = vk::Pipeline::null();
        }
        if self.vert_module != vk::ShaderModule::null() {
            unsafe {
                // SAFETY: `self.vert_module` was created by this device; shader
                // modules may be destroyed once the pipeline that references
                // them has been created (Vulkan spec), which already happened.
                device.destroy_shader_module(self.vert_module, None)
            };
            self.vert_module = vk::ShaderModule::null();
        }
        if self.frag_module != vk::ShaderModule::null() {
            unsafe {
                // SAFETY: same as `vert_module` above.
                device.destroy_shader_module(self.frag_module, None)
            };
            self.frag_module = vk::ShaderModule::null();
        }
        if self.pipeline_layout != vk::PipelineLayout::null() {
            unsafe {
                // SAFETY: `self.pipeline_layout` was created by this device;
                // the pipeline that referenced it is destroyed above.
                device.destroy_pipeline_layout(self.pipeline_layout, None)
            };
            self.pipeline_layout = vk::PipelineLayout::null();
        }
        if self.render_pass != vk::RenderPass::null() {
            unsafe {
                // SAFETY: `self.render_pass` was created by this device; all
                // framebuffers built against it are destroyed above, and per
                // this fn's contract no in-flight command buffer uses it.
                device.destroy_render_pass(self.render_pass, None)
            };
            self.render_pass = vk::RenderPass::null();
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            unsafe {
                // SAFETY: `self.descriptor_pool` was created by this device;
                // destroying the pool implicitly frees every set allocated
                // from it (Vulkan spec), which is why `descriptor_sets` is
                // cleared without individually freeing them below.
                device.destroy_descriptor_pool(self.descriptor_pool, None)
            };
            self.descriptor_pool = vk::DescriptorPool::null();
        }
        if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
            unsafe {
                // SAFETY: `self.descriptor_set_layout` was created by this
                // device; all sets allocated from it are freed by the pool
                // destroy above.
                device.destroy_descriptor_set_layout(self.descriptor_set_layout, None)
            };
            self.descriptor_set_layout = vk::DescriptorSetLayout::null();
        }
        if self.sampler != vk::Sampler::null() {
            unsafe {
                // SAFETY: `self.sampler` was created by this device and is
                // not referenced by any in-flight command buffer (fn contract).
                device.destroy_sampler(self.sampler, None)
            };
            self.sampler = vk::Sampler::null();
        }
        self.descriptor_sets.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_push_constants_match_shader_alignment() {
        assert_eq!(std::mem::size_of::<PresentationPushConstants>(), 128);
        assert_eq!(
            std::mem::offset_of!(PresentationPushConstants, exposure),
            16
        );
        assert_eq!(std::mem::offset_of!(PresentationPushConstants, lens), 32);
        assert_eq!(
            std::mem::offset_of!(PresentationPushConstants, fade_color),
            112
        );
    }

    #[test]
    fn image_space_defaults_are_identity() {
        let view = ImageSpaceModifierView::default();
        assert_eq!(view.saturation, 1.0);
        assert_eq!(view.brightness, 1.0);
        assert_eq!(view.contrast, 1.0);
        assert_eq!(view.blur_radius_pixels, 0.0);
        assert_eq!(view.tint_color[3], 0.0);
    }
}
