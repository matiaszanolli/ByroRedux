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
    // #3578 — `u32`, not `f32`. These are a bitfield and a small enum; the
    // pass used to write them as `f32::from_bits(...)` and recover them with
    // `floatBitsToUint`, which made the struct's correctness depend on float-
    // representation preservation for data that is not a float. Most `DBG_*`
    // masks (every one whose highest set bit is <= 22) and every non-zero
    // debug mode are DENORMAL bit patterns, and masks spanning bits 23-30
    // reach the Inf/NaN exponent band. It works — denorm flush-to-zero is
    // specified for float *operations*, and neither a push-constant load nor
    // `OpBitcast` is one — but it inverts the idiom `GpuCamera.render_debug`
    // (`[u32; 4]` / `uvec4`) uses two files away, where a float riding in a
    // uint lane is cast OUT with `uintBitsToFloat`, the safe direction. The
    // fields sit in the same 16-byte block as `exposure` and `padding`, so
    // the struct stays 128 B and every offset pin is unchanged.
    render_debug_flags: u32,
    render_debug_mode: u32,
    padding: f32,
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
    pub render_debug_flags: u32,
    pub render_debug_mode: u32,
}

/// One Scaleform overlay draw, recorded inside this pass immediately after
/// the tone-map triangle (#3426).
///
/// The overlay used to be the tail of the *main geometry* render pass, which
/// blended it into the render-resolution HDR G-buffer — so every menu was
/// fogged, bloomed, exposure-scaled, ACES tone-mapped, TAA-accumulated with a
/// zero motion vector and no reactive mask, and FSR-upscaled. Drawing it here
/// instead puts it after all of that, at output resolution, straight onto the
/// swapchain: `aces()` has already been applied to the world behind it and is
/// not applied to the overlay at all.
///
/// The colour round-trip is exact and needs no shader work. Ruffle's capture
/// is sRGB-encoded bytes uploaded as `R8G8B8A8_SRGB`, so the sampler
/// linearises it; Vulkan blends in linear space against the sRGB swapchain
/// attachment and re-encodes on write. (Re-uploading the capture as `_UNORM`
/// — a natural-looking companion change — would double-encode it here.)
#[derive(Debug, Clone, Copy)]
pub(crate) struct UiOverlayDraw {
    /// Bindless texture set (set 0) and scene set (set 1) — the same two the
    /// geometry pass binds, since the overlay still runs `ui.vert`/`ui.frag`
    /// and reads its `textureIndex` out of the instance SSBO.
    pub texture_set: vk::DescriptorSet,
    pub scene_set: vk::DescriptorSet,
    pub vertex_buffer: vk::Buffer,
    pub index_buffer: vk::Buffer,
    pub index_count: u32,
    /// Index of the UI quad's `GpuInstance`, passed as `firstInstance`.
    pub instance_index: u32,
}

/// The per-swapchain inputs [`PresentationPipeline::new`] binds: the
/// swapchain it presents to, the upscaler output it samples, the
/// image-health counters it writes, and the extent all three share.
///
/// #3525 — bundled because passing them separately put `new` at 8
/// parameters, one past `clippy::too_many_arguments`, and the workspace CI
/// gate is `cargo clippy --workspace -- -D warnings`. The grouping is not
/// arbitrary: every field here is invalidated together by a swapchain
/// recreate, which is exactly when the two call sites (`context::init` and
/// `context::resize`) rebuild this pipeline. Mirrors the `Dx10TexInfo`
/// precedent in `crates/bsa/src/ba2.rs`.
#[derive(Debug, Clone, Copy)]
pub struct PresentationTargets<'a> {
    pub swapchain_format: vk::Format,
    pub swapchain_views: &'a [vk::ImageView],
    /// Per-frame-in-flight upscaler outputs — the pipeline's sampled source.
    pub upscaled_views: &'a [vk::ImageView],
    /// Per-frame-in-flight image-health counter buffers (EX-05 / #2736).
    /// Handles only; the allocations are owned by `VulkanContext` because
    /// they must survive the swapchain recreate that rebuilds this pipeline.
    pub health_buffers: &'a [vk::Buffer],
    pub extent: vk::Extent2D,
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
    /// Scaleform overlay pipeline, built against *this* pass's render pass so
    /// the menu composites after tone-mapping and upscale (#3426). Owned here
    /// because it is render-pass-bound and this struct is what a swapchain
    /// recreate rebuilds; `overlay_pipeline_layout` is NOT owned — it is the
    /// shared scene layout (set 0 bindless textures, set 1 scene SSBOs) that
    /// `VulkanContext` creates and destroys.
    overlay_pipeline: vk::Pipeline,
    overlay_pipeline_layout: vk::PipelineLayout,
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
        targets: PresentationTargets<'_>,
        overlay_pipeline_layout: vk::PipelineLayout,
    ) -> Result<Self> {
        let PresentationTargets {
            swapchain_format,
            swapchain_views,
            upscaled_views,
            health_buffers,
            extent,
        } = targets;
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
            overlay_pipeline: vk::Pipeline::null(),
            overlay_pipeline_layout,
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
        // #2465 (REN-D4-2026-08-07-01) — MEASURED, deliberately unchanged.
        //
        // The concern: this pass is the swapchain writer, its attachment is
        // `UNDEFINED → PRESENT_SRC_KHR`, so it performs an automatic
        // `UNDEFINED → COLOR_ATTACHMENT_OPTIMAL` transition. Per spec that
        // transition is ordered between the two synchronization scopes of
        // this `SUBPASS_EXTERNAL` dependency — whose dst scope is
        // `FRAGMENT_SHADER | COLOR_ATTACHMENT_OUTPUT`. `draw_frame` waits
        // `image_available` at `COLOR_ATTACHMENT_OUTPUT` only, and
        // `FRAGMENT_SHADER` is logically EARLIER than that, so the
        // transition is not provably ordered after the acquire. The
        // `FRAGMENT_SHADER` limb exists for the upscaled-image sampler read
        // (a different resource), but subpass-dependency scopes are
        // pass-wide, so the swapchain attachment inherits the looser
        // ordering. The `AUDIT_RENDERER_2026-04-25.md` invariant note
        // recorded `(UNDEFINED, wait at COLOR_ATTACHMENT_OUTPUT)` as
        // load-bearing back when *composite* was the swapchain writer; the
        // writer moved here with a wider dependency and it was not re-checked.
        //
        // Verified 2026-08-14, release build, `BYRO_VALIDATION=1` (which
        // enables `SYNCHRONIZATION_VALIDATION` unconditionally — see
        // `instance.rs`), 300 frames on a live FNV exterior (1214 draws,
        // 3757 entities): **zero SYNC-HAZARD reports**. The only validation
        // output was a pre-existing, unrelated `shaderInt64` capability
        // complaint.
        //
        // So this stays as-is. The two candidate hardenings — dropping the
        // `FRAGMENT_SHADER` limb (needs proof the upscaler already barriers
        // its own output, else it trades a theoretical hazard for a real
        // read-before-write one) and widening `wait_stages` (serializes the
        // whole frame behind acquire, since the wait applies to the entire
        // submit and the swapchain is only touched by this pass at the tail)
        // both cost more than the measurement justifies. Revisit only with
        // an actual sync-val hazard or a driver-observed artifact; a repeat
        // of the static reading alone is not new evidence.
        //
        // **Scope note (2026-08-31, REN-2026-08-30-D23-05, observation
        // only — no barrier change proposed):** this measurement's date
        // (2026-08-14) predates #3426 (2026-08-29), which added a second
        // draw into this same subpass — `record_overlay`'s Scaleform/UI
        // quad, reading a vertex buffer, an index buffer, and the scene
        // instance SSBO in the vertex stage. Neither `VERTEX_INPUT` nor
        // `VERTEX_SHADER` is in this dependency's dst scope. No live hazard
        // is claimed (the UI quad's buffers are written by a separate
        // fenced one-time submit, not this command buffer; the instance
        // SSBO is a host-mapped write made visible by the submit's
        // implicit host-write dependency) — but the 300-frame measurement
        // above no longer covers this pass's current contents. Re-run it
        // with the overlay actually drawing (`--menu` route,
        // `docs/smoke-tests/m48-menu-load.sh`) before treating this
        // dependency's scope as re-verified.
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

        // #3426 — the Scaleform overlay composites in THIS pass, so its
        // pipeline is built against this render pass (one swapchain colour
        // attachment) rather than the geometry pass's eight-attachment
        // G-buffer. Failure here is fatal for the same reason the pipeline
        // above is: `create` is unwound by `new`/`recreate`, which destroy
        // every handle produced so far.
        self.overlay_pipeline = super::pipeline::create_ui_pipeline(
            device,
            self.render_pass,
            self.extent,
            self.overlay_pipeline_layout,
            pipeline_cache,
        )?;
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
        overlay: Option<UiOverlayDraw>,
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
            render_debug_flags: input.render_debug_flags,
            render_debug_mode: input.render_debug_mode,
            padding: 0.0,
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
            // #3426 — the Scaleform overlay, composited on top of the
            // tone-mapped output rather than blended into the HDR G-buffer.
            // Same subpass as the draw above, so rasterization order gives
            // the blend its ordering for free; the viewport/scissor set for
            // the fullscreen triangle are the output extent, which is exactly
            // what the overlay wants, but they are re-set below anyway for
            // the reason `UI_PIPELINE_DYNAMIC_STATES` documents.
            if let Some(overlay) = overlay {
                // SAFETY: the render pass is open (begun above) and the caller
                // supplies live per-frame descriptor sets and the UI quad's
                // live vertex/index buffers.
                self.record_overlay(device, cmd, overlay);
            }
            device.cmd_end_render_pass(cmd);
        }
    }

    /// Record the Scaleform overlay quad inside the already-open presentation
    /// render pass.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording inside this pipeline's render pass, and every
    /// handle in `overlay` must be live for the frame being recorded.
    unsafe fn record_overlay(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        overlay: UiOverlayDraw,
    ) {
        use super::pipeline::UI_PIPELINE_DYNAMIC_STATES;
        // CONTRACT (#663), inherited from the geometry pass this draw moved
        // out of: the explicit `cmd_set_*` calls below must cover every state
        // in `UI_PIPELINE_DYNAMIC_STATES`. Depth / cull / depth-bias state on
        // the overlay pipeline is STATIC and applied by the bind itself.
        const _UI_OVERLAY_DEFENSIVE_STATE_INVARIANT: () = {
            assert!(
                UI_PIPELINE_DYNAMIC_STATES.len() == 2,
                "UI overlay path covers VIEWPORT + SCISSOR only —                  extend it before growing UI_PIPELINE_DYNAMIC_STATES",
            );
        };
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: self.extent.width as f32,
            height: self.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: self.extent,
        };
        unsafe {
            // SAFETY: fn contract — `cmd` is recording inside this render
            // pass and every supplied handle is live.
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.overlay_pipeline);
            device.cmd_set_viewport(cmd, 0, &[viewport]);
            device.cmd_set_scissor(cmd, 0, &[scissor]);
            // The presentation draw above bound this pass's own set 0, which
            // is layout-incompatible with the scene layout, so both overlay
            // sets are (re)bound here rather than inherited.
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.overlay_pipeline_layout,
                0,
                &[overlay.texture_set, overlay.scene_set],
                &[],
            );
            device.cmd_bind_vertex_buffers(cmd, 0, &[overlay.vertex_buffer], &[0]);
            device.cmd_bind_index_buffer(cmd, overlay.index_buffer, 0, vk::IndexType::UINT32);
            device.cmd_draw_indexed(cmd, overlay.index_count, 1, 0, 0, overlay.instance_index);
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
        // Borrowed handle, not owned — `destroy` leaves it set, but read it
        // out explicitly so the rebuild does not depend on that.
        let overlay_pipeline_layout = self.overlay_pipeline_layout;
        unsafe {
            // SAFETY: swapchain recreation waits for device idle before this
            // method is called.
            self.destroy(device);
        }
        *self = Self::new(
            device,
            pipeline_cache,
            PresentationTargets {
                swapchain_format,
                swapchain_views,
                upscaled_views,
                health_buffers: &health_buffers,
                extent,
            },
            overlay_pipeline_layout,
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
        // Destroyed before `render_pass` below, which it was built against
        // (#3426). `overlay_pipeline_layout` is deliberately NOT destroyed —
        // it is `VulkanContext::pipeline_layout`, shared with the scene
        // pipelines and outliving every swapchain recreate.
        if self.overlay_pipeline != vk::Pipeline::null() {
            unsafe {
                // SAFETY: created by this device in `create`; no in-flight
                // command buffer binds it (fn contract).
                device.destroy_pipeline(self.overlay_pipeline, None)
            };
            self.overlay_pipeline = vk::Pipeline::null();
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

    /// #3578 / REN-2026-08-30-D3-03 — the two debug integers must stay
    /// `u32` on both sides of the push-constant boundary. They used to be
    /// declared `f32` and round-tripped through the float representation
    /// (`f32::from_bits` on the host, `floatBitsToUint` in the shader), which
    /// made a GPU struct's correctness depend on float-representation
    /// preservation for data that has no reason to be typed as float: every
    /// `DBG_*` mask whose highest set bit is <= 22 is a DENORMAL `f32`, every
    /// non-zero `RENDER_DEBUG_*` mode is a denormal, and masks spanning bits
    /// 23-30 land in the Inf/NaN exponent band. It also inverted the
    /// `GpuCamera.render_debug` idiom (`[u32; 4]` / `uvec4`), where a float
    /// riding in a uint lane is cast OUT with `uintBitsToFloat` — a uint lane
    /// never subjects its payload to float interpretation.
    ///
    /// The size/offset pin above covers the layout; this pins the TYPES, on
    /// both sides, so the fragile cast cannot be reintroduced.
    #[test]
    fn render_debug_push_constants_ride_in_uint_lanes() {
        // Scan the production half only: the negative assertions below spell
        // the forbidden casts literally, so scanning the whole file (this test
        // included) would always self-match.
        let src = include_str!("presentation.rs")
            .split_once("#[cfg(test)]")
            .expect("presentation.rs must have a test module")
            .0;
        let decl = src
            .split_once("struct PresentationPushConstants {")
            .expect("PresentationPushConstants must exist")
            .1
            .split_once('}')
            .expect("struct must close")
            .0;
        for field in ["render_debug_flags: u32,", "render_debug_mode: u32,"] {
            assert!(
                decl.contains(field),
                "PresentationPushConstants must declare `{field}` — a bitfield and a \
                 small enum must not ride in f32 lanes as denormal bit patterns (#3578)"
            );
        }
        assert!(
            !src.contains("f32::from_bits(input.render_debug"),
            "the host must assign the debug integers directly, not bitcast them into \
             float lanes (#3578)"
        );

        let frag = include_str!("../../shaders/presentation.frag");
        for field in ["uint renderDebugFlags;", "uint renderDebugMode;"] {
            assert!(
                frag.contains(field),
                "presentation.frag's PresentationParams must declare `{field}` to mirror \
                 the host struct (#3578)"
            );
        }
        assert!(
            !frag.contains("floatBitsToUint(params.renderDebug"),
            "presentation.frag must read the debug integers straight out of their uint \
             lanes; recovering them with floatBitsToUint means they were shipped as \
             floats (#3578)"
        );
    }

    /// #3426 — the Scaleform overlay must composite in this pass, after the
    /// tone-map draw, and must no longer be blended into the geometry pass's
    /// render-resolution HDR G-buffer (where it was fogged, bloomed,
    /// exposure-scaled, ACES tone-mapped, TAA-accumulated with a zero motion
    /// vector and FSR-upscaled).
    #[test]
    fn ui_overlay_composites_after_the_tone_map_draw() {
        let src = include_str!("presentation.rs");
        let dispatch = src
            .split_once("pub(crate) unsafe fn dispatch(")
            .expect("dispatch must exist")
            .1;
        let body = dispatch
            .split_once("unsafe fn record_overlay(")
            .expect("record_overlay must follow dispatch")
            .0;
        let tone_map = body
            .find("cmd_draw(cmd, 3, 1, 0, 0)")
            .expect("the fullscreen tone-map triangle must still be drawn");
        let overlay = body
            .find("self.record_overlay(")
            .expect("the overlay must be recorded in the presentation pass");
        let end = body
            .find("cmd_end_render_pass(cmd)")
            .expect("the pass must end");
        assert!(
            tone_map < overlay && overlay < end,
            "the overlay must be recorded after the tone-map draw and before \
             the pass ends, so the menu is composited onto tone-mapped output \
             rather than being tone-mapped itself (#3426)"
        );

        // The other half: the geometry pass must not draw it any more.
        let geometry = include_str!("context/geometry_pass.rs");
        assert!(
            !geometry.contains("pipeline_ui"),
            "the geometry pass must not bind a UI pipeline — the overlay \
             belongs to the presentation pass since #3426"
        );

        // And the pipeline it binds must be built for a single swapchain
        // colour attachment, not the eight-attachment G-buffer.
        let pipeline = include_str!("pipeline.rs");
        let ui_pipeline = pipeline
            .split_once("pub fn create_ui_pipeline(")
            .expect("create_ui_pipeline must exist")
            .1;
        assert!(
            ui_pipeline.contains("let color_blend_attachment = [ui_blend];"),
            "the UI pipeline must declare exactly one colour-blend attachment \
             — the presentation pass's swapchain target (#3426)"
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
