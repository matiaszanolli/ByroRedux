//! Graphics pipeline creation and shader module loading.

use crate::vertex::{UiVertex, Vertex};
use anyhow::{Context, Result};
use ash::vk;

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

/// Pipeline set: opaque, alpha, opaque-two-sided, alpha-two-sided.
pub struct PipelineSet {
    pub opaque: vk::Pipeline,
    pub alpha: vk::Pipeline,
    pub opaque_two_sided: vk::Pipeline,
    pub alpha_two_sided: vk::Pipeline,
    pub layout: vk::PipelineLayout,
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
    create_triangle_pipeline_with_layout(
        device,
        render_pass,
        extent,
        pipeline_cache,
        None, // create new layout
        descriptor_set_layout,
        scene_set_layout,
    )
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
    create_triangle_pipeline_with_layout(
        device,
        render_pass,
        extent,
        pipeline_cache,
        Some(existing_layout),
        vk::DescriptorSetLayout::null(), // unused when layout is provided
        vk::DescriptorSetLayout::null(),
    )
}

fn create_triangle_pipeline_with_layout(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    pipeline_cache: vk::PipelineCache,
    existing_layout: Option<vk::PipelineLayout>,
    descriptor_set_layout: vk::DescriptorSetLayout,
    scene_set_layout: vk::DescriptorSetLayout,
) -> Result<PipelineSet> {
    let vert_spv = include_bytes!("../../shaders/triangle.vert.spv");
    let frag_spv = include_bytes!("../../shaders/triangle.frag.spv");

    let vert_module = load_shader_module(device, vert_spv)?;
    let frag_module = load_shader_module(device, frag_spv)?;

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

    let viewports = [vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }];

    let scissors = [vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent,
    }];

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewports(&viewports)
        .scissors(&scissors);

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
    let dynamic_states = [
        vk::DynamicState::VIEWPORT,
        vk::DynamicState::SCISSOR,
        vk::DynamicState::DEPTH_BIAS,
    ];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    // No push constants — per-draw data (model matrix, texture index, bone offset)
    // lives in the instance SSBO (set 1, binding 4). The vertex shader reads
    // instances[gl_InstanceIndex] for all per-instance data.
    let pipeline_layout = if let Some(layout) = existing_layout {
        layout
    } else {
        let set_layouts = [descriptor_set_layout, scene_set_layout];
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&set_layouts);
        unsafe {
            device
                .create_pipeline_layout(&layout_info, None)
                .context("Failed to create pipeline layout")?
        }
    };

    let depth_stencil_opaque = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    // Alpha blend pipeline: standard src-alpha, one-minus-src-alpha.
    // Depth test on but depth write OFF (transparent objects shouldn't occlude).
    // Attachment 0 (HDR) blends. Attachments 1/2/3 (normal/motion/mesh_id)
    // overwrite — the alpha surface's normal/motion/id replaces the opaque
    // one behind it because the alpha fragment IS the new visible surface.
    let color_blend_hdr_alpha = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD);
    let color_blend_alpha = [
        color_blend_hdr_alpha,
        color_blend_none, // normal: overwrite
        color_blend_none, // motion: overwrite
        color_blend_none, // mesh_id: overwrite
        color_blend_none, // raw_indirect: overwrite (alpha surface's own indirect)
        color_blend_none, // albedo: overwrite
    ];
    let color_blending_alpha = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_alpha);

    let depth_stencil_alpha = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(false) // transparent objects don't write depth
        .depth_compare_op(vk::CompareOp::LESS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let pipeline_infos = [
        // [0] Opaque pipeline.
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
        // [1] Alpha-blended pipeline.
        vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil_alpha)
            .color_blend_state(&color_blending_alpha)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0),
        // [2] Opaque two-sided (no backface culling).
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
        // [3] Alpha-blended two-sided (no backface culling).
        vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer_no_cull)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil_alpha)
            .color_blend_state(&color_blending_alpha)
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

    log::info!("Graphics pipelines created (opaque + alpha + two-sided variants)");

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
        alpha: pipelines[1],
        opaque_two_sided: pipelines[2],
        alpha_two_sided: pipelines[3],
        layout: pipeline_layout,
    })
}

/// Creates the UI overlay pipeline (no depth, no lighting, alpha blend).
///
/// Uses the same pipeline layout as the scene pipelines (push constants +
/// descriptor set for texture sampler). The UI vertex shader ignores push
/// constants — vertices are already in NDC clip space.
pub fn create_ui_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    pipeline_layout: vk::PipelineLayout,
    pipeline_cache: vk::PipelineCache,
) -> Result<vk::Pipeline> {
    let vert_spv = include_bytes!("../../shaders/ui.vert.spv");
    let frag_spv = include_bytes!("../../shaders/ui.frag.spv");

    let vert_module = load_shader_module(device, vert_spv)?;
    let frag_module = load_shader_module(device, frag_spv)?;

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

    let viewports = [vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }];
    let scissors = [vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent,
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

    // No depth test — UI renders on top of everything.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false)
        .stencil_test_enable(false);

    // Alpha blending for UI transparency.
    // Main render pass has 4 color attachments. UI writes to HDR (slot 0)
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
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

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
