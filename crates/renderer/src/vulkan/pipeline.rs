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

/// Creates the graphics pipeline for the hardcoded triangle (Phase 1).
///
/// No vertex input (positions are hardcoded in the shader).
/// Will be extended with vertex buffers in Phase 2.
pub fn create_triangle_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
) -> Result<(vk::Pipeline, vk::PipelineLayout, vk::ShaderModule, vk::ShaderModule)> {
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

    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::CLOCKWISE)
        .depth_bias_enable(false);

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
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    // Push constants: viewProj (mat4) + model (mat4) = 128 bytes.
    let push_constant_ranges = [vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::VERTEX,
        offset: 0,
        size: 128, // 2 * sizeof(mat4)
    }];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .push_constant_ranges(&push_constant_ranges);
    let pipeline_layout = unsafe {
        device
            .create_pipeline_layout(&layout_info, None)
            .context("Failed to create pipeline layout")?
    };

    let pipeline_info = [vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer)
        .multisample_state(&multisampling)
        .color_blend_state(&color_blending)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0)];

    let pipelines = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
            .map_err(|(_, err)| err)
            .context("Failed to create graphics pipeline")?
    };

    log::info!("Graphics pipeline created (triangle)");

    Ok((pipelines[0], pipeline_layout, vert_module, frag_module))
}
