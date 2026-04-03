//! Graphics pipeline creation and shader module loading.

use crate::vertex::Vertex;
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
    pub vert_module: vk::ShaderModule,
    pub frag_module: vk::ShaderModule,
}

/// Creates the graphics pipelines with textured rendering.
pub fn create_triangle_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
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

    // Backface culling disabled until NIF winding convention is verified
    // empirically. The Z-up→Y-up conversion preserves winding (det=+1),
    // and the projection Y-flip swaps apparent winding in clip space.
    // Once we confirm NIF winding (CW or CCW), enable BACK culling
    // with the matching front_face setting.
    // All pipelines enable depth bias (set dynamically per-draw to resolve Z-fighting
    // for coplanar geometry: carpets on floors, labels on bottles, papers on desks).
    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(true);

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

    let color_blend_attachment = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false)];

    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachment);

    // Use dynamic viewport/scissor so we don't need to recreate the pipeline on resize.
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR, vk::DynamicState::DEPTH_BIAS];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    // Push constants: viewProj (mat4) + model (mat4) = 128 bytes.
    let push_constant_ranges = [vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::VERTEX,
        offset: 0,
        size: 128, // 2 * sizeof(mat4)
    }];
    let set_layouts = [descriptor_set_layout, scene_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(&push_constant_ranges);
    let pipeline_layout = unsafe {
        device
            .create_pipeline_layout(&layout_info, None)
            .context("Failed to create pipeline layout")?
    };

    let depth_stencil_opaque = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    // Alpha blend pipeline: standard src-alpha, one-minus-src-alpha.
    // Depth test on but depth write OFF (transparent objects shouldn't occlude).
    let color_blend_alpha = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD)];
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
            .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_infos, None)
            .map_err(|(_, err)| err)
            .context("Failed to create graphics pipelines")?
    };

    log::info!("Graphics pipelines created (opaque + alpha + two-sided variants)");

    Ok(PipelineSet {
        opaque: pipelines[0],
        alpha: pipelines[1],
        opaque_two_sided: pipelines[2],
        alpha_two_sided: pipelines[3],
        layout: pipeline_layout,
        vert_module,
        frag_module,
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
) -> Result<(vk::Pipeline, vk::ShaderModule, vk::ShaderModule)> {
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
    let color_blend_attachment = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD)];
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachment);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR, vk::DynamicState::DEPTH_BIAS];
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
            .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
            .map_err(|(_, err)| err)
            .context("Failed to create UI pipeline")?
    };

    log::info!("UI overlay pipeline created");

    Ok((pipelines[0], vert_module, frag_module))
}
