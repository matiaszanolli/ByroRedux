//! Graphics pipeline creation and shader module loading.

use crate::vertex::{UiVertex, Vertex};
use anyhow::{Context, Result};
use ash::vk;

/// Main graphics pipeline SPIR-V bytes — exposed so other modules (scene_buffer,
/// texture_registry) can reflect them during descriptor layout validation (#427).
pub const TRIANGLE_VERT_SPV: &[u8] = include_bytes!("../../shaders/triangle.vert.spv");
pub const TRIANGLE_FRAG_SPV: &[u8] = include_bytes!("../../shaders/triangle.frag.spv");
pub const UI_VERT_SPV: &[u8] = include_bytes!("../../shaders/ui.vert.spv");
pub const UI_FRAG_SPV: &[u8] = include_bytes!("../../shaders/ui.frag.spv");

/// Load a SPIR-V shader module from raw bytes.
pub fn load_shader_module(device: &ash::Device, spv: &[u8]) -> Result<vk::ShaderModule> {
    // ash requires the data as &[u32]. SPIR-V is always 4-byte aligned.
    assert!(
        spv.len() % 4 == 0,
        "SPIR-V binary size must be a multiple of 4"
    );

    let code: Vec<u32> = spv
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    let create_info = vk::ShaderModuleCreateInfo::default().code(&code);

    let module = unsafe {
        device
            .create_shader_module(&create_info, None)
            .context("Failed to create shader module")?
    };

    Ok(module)
}

/// Pipeline selection key for a single draw.
///
/// The renderer keeps two pipelines that always exist (`Opaque` and
/// `Opaque { two_sided: true }`, depth-write on, no blend) plus a
/// lazily-populated cache of blended pipelines keyed by the exact
/// Gamebryo (src, dst) factor pair. This key is what batching logic
/// in `draw.rs` groups by. See #392.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineKey {
    /// Opaque: depth write on, no blend. Optionally two-sided (no
    /// backface culling for foliage / glass panes / cloth).
    Opaque { two_sided: bool },
    /// Blended: depth write off, blend on. `src`/`dst` are raw
    /// Gamebryo `AlphaFunction` enum values (0=ONE ... 10=SRC_ALPHA_SATURATE);
    /// see [`gamebryo_to_vk_blend_factor`].
    Blended { src: u8, dst: u8, two_sided: bool },
}

/// Convert a Gamebryo `TestFunction` enum value (from
/// `NiZBufferProperty.z_function` or `NiAlphaProperty` test bits) into
/// the matching [`vk::CompareOp`]. The TestFunction enum is shared
/// between depth + alpha test in Gamebryo. Default 3 (LESSEQUAL)
/// matches the Gamebryo runtime default and the renderer's pre-#398
/// hardcoded `vk::CompareOp::LESS_OR_EQUAL`. Out-of-range values fall
/// back to LESS_OR_EQUAL.
pub fn gamebryo_to_vk_compare_op(v: u8) -> vk::CompareOp {
    match v {
        0 => vk::CompareOp::ALWAYS,
        1 => vk::CompareOp::LESS,
        2 => vk::CompareOp::EQUAL,
        3 => vk::CompareOp::LESS_OR_EQUAL,
        4 => vk::CompareOp::GREATER,
        5 => vk::CompareOp::NOT_EQUAL,
        6 => vk::CompareOp::GREATER_OR_EQUAL,
        7 => vk::CompareOp::NEVER,
        _ => vk::CompareOp::LESS_OR_EQUAL,
    }
}

/// Convert a Gamebryo `AlphaFunction` enum value (from `NiAlphaProperty`
/// flags, bits 1–4 = src, bits 5–8 = dst) into the matching
/// [`vk::BlendFactor`].
///
/// Gamebryo's 11-value enum:
/// ```text
/// 0  ONE                 5  INV_DEST_COLOR
/// 1  ZERO                6  SRC_ALPHA
/// 2  SRC_COLOR           7  INV_SRC_ALPHA
/// 3  INV_SRC_COLOR       8  DEST_ALPHA
/// 4  DEST_COLOR          9  INV_DEST_ALPHA
///                        10 SRC_ALPHA_SATURATE
/// ```
///
/// Any out-of-range value falls back to `SRC_ALPHA` (the Gamebryo
/// default). Defensive — NiAlphaProperty storage is a nibble, so the
/// maximum parsed value is 15.
pub fn gamebryo_to_vk_blend_factor(v: u8) -> vk::BlendFactor {
    match v {
        0 => vk::BlendFactor::ONE,
        1 => vk::BlendFactor::ZERO,
        2 => vk::BlendFactor::SRC_COLOR,
        3 => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
        4 => vk::BlendFactor::DST_COLOR,
        5 => vk::BlendFactor::ONE_MINUS_DST_COLOR,
        6 => vk::BlendFactor::SRC_ALPHA,
        7 => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        8 => vk::BlendFactor::DST_ALPHA,
        9 => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
        10 => vk::BlendFactor::SRC_ALPHA_SATURATE,
        _ => vk::BlendFactor::SRC_ALPHA,
    }
}

/// Pipeline set: the two pipelines that always exist (opaque + opaque
/// two-sided) plus the shared layout. All blended variants are created
/// lazily via [`create_blend_pipeline`] and cached on the VulkanContext
/// by (src, dst, two_sided).
pub struct PipelineSet {
    pub opaque: vk::Pipeline,
    pub opaque_two_sided: vk::Pipeline,
    pub layout: vk::PipelineLayout,
}

fn build_triangle_pipeline_layout(
    device: &ash::Device,
    descriptor_set_layout: vk::DescriptorSetLayout,
    scene_set_layout: vk::DescriptorSetLayout,
) -> Result<vk::PipelineLayout> {
    let set_layouts = [descriptor_set_layout, scene_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
    unsafe {
        device
            .create_pipeline_layout(&layout_info, None)
            .context("Failed to create pipeline layout")
    }
}

/// Creates the graphics pipelines with textured rendering.
pub fn create_triangle_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    descriptor_set_layout: vk::DescriptorSetLayout,
    scene_set_layout: vk::DescriptorSetLayout,
    pipeline_cache: vk::PipelineCache,
) -> Result<PipelineSet> {
    let layout = build_triangle_pipeline_layout(device, descriptor_set_layout, scene_set_layout)?;
    triangle_pipeline_inner(device, render_pass, extent, pipeline_cache, layout)
}

/// Recreate triangle pipelines reusing an existing pipeline layout.
/// Avoids redundant layout + shader module create/destroy on resize.
pub fn recreate_triangle_pipelines(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    pipeline_cache: vk::PipelineCache,
    existing_layout: vk::PipelineLayout,
) -> Result<PipelineSet> {
    triangle_pipeline_inner(device, render_pass, extent, pipeline_cache, existing_layout)
}

fn triangle_pipeline_inner(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    pipeline_cache: vk::PipelineCache,
    pipeline_layout: vk::PipelineLayout,
) -> Result<PipelineSet> {
    let vert_module = load_shader_module(device, TRIANGLE_VERT_SPV)?;
    let frag_module = load_shader_module(device, TRIANGLE_FRAG_SPV)?;

    let entry_point = c"main";

    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    // Vertex input from buffer — position + color per vertex.
    let binding_descriptions = [Vertex::binding_description()];
    let attribute_descriptions = Vertex::attribute_descriptions();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(&attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    // VIEWPORT + SCISSOR are declared dynamic below, so the arrays
    // on `PipelineViewportStateCreateInfo` are ignored per the
    // Vulkan spec — only the counts matter at pipeline-create time.
    // Dynamic values come from `cmd_set_viewport` / `cmd_set_scissor`
    // in `draw.rs`. See audit PIPE-1 / #578.
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let _ = extent;

    // NIF/D3D uses CW winding. The projection Y-flip in camera.rs reverses
    // apparent winding in clip space, so CW triangles appear CCW after
    // projection. front_face=CCW + cull_mode=BACK is therefore correct.
    // All pipelines enable depth bias (set dynamically per-draw to resolve
    // Z-fighting for coplanar geometry like decals).
    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(true);

    // Two-sided rasterizer for meshes flagged as double-sided (foliage, glass, etc.).
    let rasterizer_no_cull = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(true);

    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // Phase 2: main render pass has 6 color attachments (HDR + normal +
    // motion + mesh_id + raw_indirect + albedo). Each needs a blend state
    // entry. Opaque pipeline never blends any of them.
    let color_blend_none = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let color_blend_attachment = [
        color_blend_none,
        color_blend_none,
        color_blend_none,
        color_blend_none,
        color_blend_none,
        color_blend_none,
    ];

    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachment);

    // Use dynamic viewport/scissor so we don't need to recreate the pipeline on resize.
    // Depth test/write/compare-op are dynamic per #398 — every NIF mesh
    // can author its own NiZBufferProperty state, but baking each
    // (z_test, z_write, z_func) combo into a separate pipeline would
    // explode the cache. Vulkan 1.3 core extended dynamic state covers
    // exactly this case; the depth-stencil state below stays at the
    // pre-#398 hardcoded defaults and `draw.rs` issues per-batch
    // `cmd_set_depth_test_enable` / `_write_enable` / `_compare_op`
    // before each draw.
    //
    // CULL_MODE must be dynamic on EVERY pipeline in the draw loop:
    // per Vulkan spec, binding a pipeline that doesn't declare a
    // particular dynamic state invalidates any prior `cmd_set_*` for
    // it. If the opaque pipeline were static-cull, transitioning to
    // the blend pipeline (which does declare it dynamic) would leave
    // the next draw with undefined cull. Declaring it dynamic on both
    // pipelines keeps the value persistent across binds; opaque batches
    // just re-emit their baked BACK/NONE target. Phase 1 of Tier C glass.
    let dynamic_states = [
        vk::DynamicState::VIEWPORT,
        vk::DynamicState::SCISSOR,
        vk::DynamicState::DEPTH_BIAS,
        vk::DynamicState::DEPTH_TEST_ENABLE,
        vk::DynamicState::DEPTH_WRITE_ENABLE,
        vk::DynamicState::DEPTH_COMPARE_OP,
        vk::DynamicState::CULL_MODE,
    ];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    // LESS_OR_EQUAL matches draw.rs:cmd_set_depth_compare_op (the live source of truth).
    // depth_test/write/compare_op are all dynamic (#398); these static values are
    // ignored at runtime but must match the dynamic default to prevent silent breakage
    // if the dynamic-state declaration is ever dropped.
    let depth_stencil_opaque = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let pipeline_infos = [
        // [0] Opaque pipeline — depth write on, no blend.
        vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil_opaque)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0),
        // [1] Opaque two-sided — same as [0] but no backface culling
        //     (foliage, glass panes with no real back-face geometry, cloth).
        vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer_no_cull)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil_opaque)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0),
    ];

    let pipelines = unsafe {
        device
            .create_graphics_pipelines(pipeline_cache, &pipeline_infos, None)
            .map_err(|(_, err)| err)
            .context("Failed to create graphics pipelines")?
    };

    log::info!("Graphics pipelines created (opaque + opaque two-sided; blend variants lazy-cached by NiAlphaProperty (src, dst))");

    // SAFETY: Shader modules are compiled into the pipeline objects during
    // create_graphics_pipelines and are no longer needed. Destroy them
    // immediately to avoid holding GPU resources for the entire context
    // lifetime. See issue #98.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(PipelineSet {
        opaque: pipelines[0],
        opaque_two_sided: pipelines[1],
        layout: pipeline_layout,
    })
}

/// Create a single blended pipeline for a specific Gamebryo (src, dst)
/// factor pair, with depth-write disabled (correct for translucent
/// surfaces). Consumed by the lazy cache in `VulkanContext` when a draw
/// presents a factor pair that hasn't been seen yet (#392).
///
/// Shares the same render pass, pipeline layout, and shader pair as the
/// opaque pipelines. `src` / `dst` are raw Gamebryo AlphaFunction enum
/// values; [`gamebryo_to_vk_blend_factor`] maps them. Only the HDR
/// attachment (0) blends — G-buffer attachments (normal/motion/mesh_id/
/// raw_indirect/albedo) overwrite, matching the behaviour the old
/// `Alpha` / `Additive` static pipelines had.
pub fn create_blend_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    pipeline_cache: vk::PipelineCache,
    pipeline_layout: vk::PipelineLayout,
    src: u8,
    dst: u8,
    two_sided: bool,
) -> Result<vk::Pipeline> {
    let vert_module = load_shader_module(device, TRIANGLE_VERT_SPV)?;
    let frag_module = load_shader_module(device, TRIANGLE_FRAG_SPV)?;

    let entry_point = c"main";
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    let binding_descriptions = [Vertex::binding_description()];
    let attribute_descriptions = Vertex::attribute_descriptions();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(&attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    // VIEWPORT + SCISSOR are declared dynamic below — only the
    // counts matter at pipeline-create time. See audit PIPE-1 / #578.
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let _ = extent;

    let cull_mode = if two_sided {
        vk::CullModeFlags::NONE
    } else {
        vk::CullModeFlags::BACK
    };
    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(cull_mode)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(true);

    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let src_factor = gamebryo_to_vk_blend_factor(src);
    let dst_factor = gamebryo_to_vk_blend_factor(dst);
    let hdr_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(src_factor)
        .dst_color_blend_factor(dst_factor)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD);
    let overwrite = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let attachments = [
        hdr_blend, overwrite, overwrite, overwrite, overwrite, overwrite,
    ];
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&attachments);

    // Transparent surfaces never write depth — prevents z-fight with
    // other translucents at the same depth and keeps opaque geometry
    // visible behind glass / decals.
    // LESS_OR_EQUAL matches draw.rs:cmd_set_depth_compare_op (the live source of truth).
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    // Same #398 extended-dynamic-state additions as the opaque path —
    // blended draws can also author non-default z_test / z_write /
    // z_function (HUD overlays + ghost effects + fade halos).
    //
    // CULL_MODE is also dynamic on the blend pipeline so two-sided
    // alpha-blend draws can be split into a FRONT-cull pass (back
    // faces) followed by a BACK-cull pass (front faces) for correct
    // within-mesh back-to-front ordering. Gamebryo submits two-sided
    // glass as a single `D3DCULL_NONE` draw and relies on per-object
    // BTF sort to hide the face-to-face z-fight — at our sub-pixel
    // TAA jitter that interaction reads as cross-hatch moiré. The
    // split gives each face a deterministic depth winner per pixel.
    // Phase 1 of the Tier C glass plan.
    let dynamic_states = [
        vk::DynamicState::VIEWPORT,
        vk::DynamicState::SCISSOR,
        vk::DynamicState::DEPTH_BIAS,
        vk::DynamicState::DEPTH_TEST_ENABLE,
        vk::DynamicState::DEPTH_WRITE_ENABLE,
        vk::DynamicState::DEPTH_COMPARE_OP,
        vk::DynamicState::CULL_MODE,
    ];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let infos = [vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer)
        .multisample_state(&multisampling)
        .depth_stencil_state(&depth_stencil)
        .color_blend_state(&color_blending)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0)];

    let pipelines = unsafe {
        device
            .create_graphics_pipelines(pipeline_cache, &infos, None)
            .map_err(|(_, err)| err)
            .context("Failed to create blend pipeline variant")?
    };

    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    log::debug!(
        "Blend pipeline created: src={src} ({src_factor:?}), dst={dst} ({dst_factor:?}), two_sided={two_sided}"
    );

    Ok(pipelines[0])
}

/// Dynamic states declared by the UI pipeline. Cross-referenced by
/// the overlay call site in `vulkan/context/draw.rs` so a future
/// addition to this list trips a `const_assert` and forces the
/// author to add the matching `cmd_set_*` defensive call. Depth /
/// cull / depth-bias state on `pipeline_ui` is intentionally STATIC
/// (off / off / NONE / off) — pipeline bind applies those values
/// automatically, no per-bind cmd_set needed. See #663.
pub const UI_PIPELINE_DYNAMIC_STATES: &[vk::DynamicState] =
    &[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

/// Creates the UI overlay pipeline (no depth, no lighting, alpha blend).
///
/// Uses the same pipeline layout as the scene pipelines (set 0 = bindless
/// textures, set 1 = scene UBO/SSBOs including the instance buffer at
/// binding 4). No push constants exist on any pipeline — per-instance
/// data lives in the instance SSBO. The UI vertex shader reads only the
/// `textureIndex` field; vertices are already in NDC clip space so the
/// `model` matrix is ignored.
pub fn create_ui_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    pipeline_layout: vk::PipelineLayout,
    pipeline_cache: vk::PipelineCache,
) -> Result<vk::Pipeline> {
    let vert_module = load_shader_module(device, UI_VERT_SPV)?;
    let frag_module = load_shader_module(device, UI_FRAG_SPV)?;

    let entry_point = c"main";

    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    // UI pipeline uses the lightweight UiVertex (position + UV only, 20 bytes)
    // instead of the full 76-byte Vertex with unused bone/normal/color fields.
    let binding_descriptions = [UiVertex::binding_description()];
    let attribute_descriptions = UiVertex::attribute_descriptions();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(&attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    // VIEWPORT + SCISSOR are declared dynamic below — only the
    // counts matter at pipeline-create time. See audit PIPE-1 / #578.
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let _ = extent;

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

    // No depth test — UI renders on top of everything.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false)
        .stencil_test_enable(false);

    // Alpha blending for UI transparency.
    // Main render pass has 6 color attachments. UI writes to HDR (slot 0)
    // with alpha blending and masks out writes to normal/motion/mesh_id
    // via color_write_mask(empty) so UI doesn't pollute the G-buffer.
    let ui_hdr_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD);
    let ui_noop_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::empty())
        .blend_enable(false);
    let color_blend_attachment = [
        ui_hdr_blend,
        ui_noop_blend,
        ui_noop_blend,
        ui_noop_blend,
        ui_noop_blend,
        ui_noop_blend,
    ];
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachment);

    // No DEPTH_BIAS — UI pipeline has depth_bias_enable(false).
    //
    // **Contract** (#663). The UI overlay path in
    // `vulkan/context/draw.rs` (post-`cmd_bind_pipeline(pipeline_ui)`)
    // defensively re-sets every state in this list, then relies on
    // pipeline_ui's STATIC depth/cull state to take effect on bind.
    // If you add a state here, you MUST also extend the overlay path
    // to `cmd_set_*` it — otherwise the new dynamic state will inherit
    // whatever the last main-batch pipeline left set, which is a hard-
    // to-reproduce visual bug. The `_UI_PIPELINE_DYNAMIC_STATES_LEN`
    // const_assert at the call site fires at compile time when this
    // list grows, forcing you to come read the contract.
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(UI_PIPELINE_DYNAMIC_STATES);

    let pipeline_info = [vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer)
        .multisample_state(&multisampling)
        .depth_stencil_state(&depth_stencil)
        .color_blend_state(&color_blending)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0)];

    let pipelines = unsafe {
        device
            .create_graphics_pipelines(pipeline_cache, &pipeline_info, None)
            .map_err(|(_, err)| err)
            .context("Failed to create UI pipeline")?
    };

    log::info!("UI overlay pipeline created");

    // SAFETY: Shader modules are compiled into the pipeline object during
    // create_graphics_pipelines and are no longer needed. See issue #98.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(pipelines[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: #392 — every Gamebryo `AlphaFunction` value (0..10)
    /// must map to a distinct, correct `vk::BlendFactor`. The previous
    /// `BlendType::from_nif_blend` collapsed 11×11 = 121 possible
    /// (src, dst) pairs into 3 buckets, dropping enough information to
    /// render glass / additive premultiplied / DST_COLOR-modulated
    /// content incorrectly across every supported game.
    #[test]
    fn gamebryo_to_vk_blend_factor_covers_all_11_values() {
        let cases = [
            (0u8, vk::BlendFactor::ONE),
            (1, vk::BlendFactor::ZERO),
            (2, vk::BlendFactor::SRC_COLOR),
            (3, vk::BlendFactor::ONE_MINUS_SRC_COLOR),
            (4, vk::BlendFactor::DST_COLOR),
            (5, vk::BlendFactor::ONE_MINUS_DST_COLOR),
            (6, vk::BlendFactor::SRC_ALPHA),
            (7, vk::BlendFactor::ONE_MINUS_SRC_ALPHA),
            (8, vk::BlendFactor::DST_ALPHA),
            (9, vk::BlendFactor::ONE_MINUS_DST_ALPHA),
            (10, vk::BlendFactor::SRC_ALPHA_SATURATE),
        ];
        for (gb, vk_expected) in cases {
            assert_eq!(
                gamebryo_to_vk_blend_factor(gb),
                vk_expected,
                "Gamebryo factor {gb} must map to {vk_expected:?}"
            );
        }

        // Out-of-range falls back to SRC_ALPHA (the Gamebryo default).
        // NiAlphaProperty stores src/dst as nibbles so 11..=15 is the
        // realistic out-of-range space.
        for v in 11u8..=15 {
            assert_eq!(
                gamebryo_to_vk_blend_factor(v),
                vk::BlendFactor::SRC_ALPHA,
                "out-of-range factor {v} must default to SRC_ALPHA"
            );
        }
    }

    /// Regression: the cache key must distinguish all the combos that
    /// previously collapsed to the same static pipeline. `(6, 7)` (alpha)
    /// and `(6, 0)` (additive) are the two combos the old code handled;
    /// `(4, 0)` (DEST_COLOR/ONE — Oblivion glass modulation) and
    /// `(0, 0)` (ONE/ONE — premultiplied additive) are the two it
    /// silently aliased to the wrong bucket.
    #[test]
    fn pipeline_key_distinguishes_combos_old_code_collapsed() {
        let alpha = PipelineKey::Blended {
            src: 6,
            dst: 7,
            two_sided: false,
        };
        let additive = PipelineKey::Blended {
            src: 6,
            dst: 0,
            two_sided: false,
        };
        let glass_modulate = PipelineKey::Blended {
            src: 4,
            dst: 0,
            two_sided: false,
        };
        let premul_additive = PipelineKey::Blended {
            src: 0,
            dst: 0,
            two_sided: false,
        };
        let opaque = PipelineKey::Opaque { two_sided: false };

        let all = [alpha, additive, glass_modulate, premul_additive, opaque];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b, "{a:?} vs {b:?}");
                }
            }
        }

        let alpha_2s = PipelineKey::Blended {
            src: 6,
            dst: 7,
            two_sided: true,
        };
        assert_ne!(alpha, alpha_2s);
    }

    /// Regression: #398 (OBL-D4-H1) — every Gamebryo `TestFunction`
    /// enum value must round-trip to the right `vk::CompareOp`. Out-
    /// of-range falls back to `LESS_OR_EQUAL` (the renderer's pre-#398
    /// hardcoded default and Gamebryo's runtime default).
    #[test]
    fn gamebryo_to_vk_compare_op_covers_all_8_values() {
        let cases: &[(u8, vk::CompareOp)] = &[
            (0, vk::CompareOp::ALWAYS),
            (1, vk::CompareOp::LESS),
            (2, vk::CompareOp::EQUAL),
            (3, vk::CompareOp::LESS_OR_EQUAL),
            (4, vk::CompareOp::GREATER),
            (5, vk::CompareOp::NOT_EQUAL),
            (6, vk::CompareOp::GREATER_OR_EQUAL),
            (7, vk::CompareOp::NEVER),
        ];
        for (gb, vk_expected) in cases {
            assert_eq!(
                gamebryo_to_vk_compare_op(*gb),
                *vk_expected,
                "Gamebryo TestFunction {gb} must map to {vk_expected:?}"
            );
        }
        // Out-of-range falls back to LESS_OR_EQUAL.
        for v in 8u8..=15 {
            assert_eq!(
                gamebryo_to_vk_compare_op(v),
                vk::CompareOp::LESS_OR_EQUAL,
                "out-of-range TestFunction {v} must default to LESS_OR_EQUAL"
            );
        }
    }
}
