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
        spv.len().is_multiple_of(4),
        "SPIR-V binary size must be a multiple of 4"
    );

    let code: Vec<u32> = spv
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    let create_info = vk::ShaderModuleCreateInfo::default().code(&code);

    let module = unsafe {
        // SAFETY: `device` is the live logical device; `create_info` borrows
        // `code`, which outlives this call, and the SPIR-V was 4-byte aligned
        // by the `chunks_exact(4)` decode above; the returned module is owned
        // and destroyed by the caller.
        device
            .create_shader_module(&create_info, None)
            .context("Failed to create shader module")?
    };

    Ok(module)
}

/// Build a compute pipeline from SPIR-V: load the shader module, create the
/// pipeline against `pipeline_cache` + `layout`, and destroy the module on
/// BOTH the success and failure paths. Returns the pipeline, or an error
/// tagged `"{name} compute pipeline"`.
///
/// Consolidates the load-module → create-pipeline → destroy-module dance
/// that was re-inlined across bloom / ssao / volumetrics / skin_compute
/// (#1751 / TD2-002). Each call site adds only its OWN resource rollback on
/// the returned `Err` — the shader module is already freed here, so the only
/// leak hazard (forgetting to destroy the module on the error path) lives in
/// one place.
pub(crate) fn create_compute_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    spv: &[u8],
    layout: vk::PipelineLayout,
    name: &str,
) -> Result<vk::Pipeline> {
    let shader_module = load_shader_module(device, spv)?;
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(shader_module)
        .name(c"main");
    // SAFETY: trivial ash create call; `device`, `pipeline_cache`, `layout`
    // and `shader_module` (created just above) are all live for the call.
    let result = unsafe {
        device
            .create_compute_pipelines(
                pipeline_cache,
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(stage)
                    .layout(layout)],
                None,
            )
            .map_err(|(_, e)| e)
            .with_context(|| format!("{name} compute pipeline"))
    };
    match result {
        Ok(pipelines) => {
            // SAFETY: `shader_module` was created by us above and not yet
            // destroyed; the pipeline is built so the module is no longer
            // needed (Vulkan copies it in at create time); device live.
            unsafe { device.destroy_shader_module(shader_module, None) };
            Ok(pipelines[0])
        }
        Err(e) => {
            // SAFETY: same module, same liveness — free it on the error path
            // too so no call site has to remember.
            unsafe { device.destroy_shader_module(shader_module, None) };
            Err(e)
        }
    }
}

/// Pipeline selection key for a single draw.
///
/// The renderer keeps a single opaque pipeline (depth-write on, no blend)
/// plus a lazily-populated cache of blended pipelines keyed by the exact
/// Gamebryo (src, dst) factor pair. This key is what batching logic
/// in `draw.rs` groups by. See #392 / #930.
///
/// **Two-sided is NOT in the cache key.** All pipelines declare CULL_MODE
/// as dynamic state (`vk::DynamicState::CULL_MODE`); per Vulkan spec, when
/// CULL_MODE is dynamic, the static value baked at pipeline-create is
/// ignored at draw time. `draw.rs` issues `cmd_set_cull_mode(NONE/BACK)`
/// based on the per-draw `two_sided` flag without needing distinct
/// pipelines. Pre-#930 the cache had separate entries per `(two_sided)`
/// value, doubling the blend-pipeline cache size and forcing a redundant
/// `cmd_bind_pipeline` on every Opaque{false}↔Opaque{true} flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineKey {
    /// Opaque: depth write on, no blend. Two-sided behavior comes from
    /// dynamic `cmd_set_cull_mode`, not a separate pipeline.
    /// `wireframe=true` selects the `vk::PolygonMode::LINE` variant
    /// for `NiWireframeProperty`-flagged content (#869); falls back
    /// to FILL when the device lacks `fillModeNonSolid`.
    Opaque { wireframe: bool },
    /// Blended: depth write off, blend on. `src`/`dst` are raw
    /// Gamebryo `AlphaFunction` enum values (0=ONE ... 10=SRC_ALPHA_SATURATE);
    /// see [`gamebryo_to_vk_blend_factor`]. Two-sided behavior comes from
    /// dynamic `cmd_set_cull_mode`, not a separate pipeline.
    /// `wireframe=true` selects the LINE variant (#869).
    /// `preserve_opaque_gbuffer=true` is used by refractive glass: glass
    /// blends into HDR color but must not replace the opaque surface's
    /// normal/motion/mesh-id/indirect/albedo while depth writes are off.
    /// Otherwise downstream passes would combine glass metadata with the
    /// opaque depth behind it and reconstruct a physically impossible
    /// surface (most visibly as denoiser smears and caustics through walls).
    Blended {
        src: u8,
        dst: u8,
        wireframe: bool,
        preserve_opaque_gbuffer: bool,
    },
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

/// Pipeline set: the single opaque pipeline plus the shared layout.
/// All blended variants are created lazily via [`create_blend_pipeline`]
/// and cached on the VulkanContext by (src, dst). Two-sided rendering
/// uses dynamic `cmd_set_cull_mode` rather than a distinct pipeline —
/// see [`PipelineKey`] for the rationale (#930).
pub struct PipelineSet {
    pub opaque: vk::Pipeline,
    /// Wireframe variant (`polygon_mode = LINE`). `None` when the device
    /// doesn't expose `fillModeNonSolid`; callers fall back to `opaque`
    /// in that case so `NiWireframeProperty` content still renders
    /// (filled). #869.
    pub opaque_wireframe: Option<vk::Pipeline>,
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
        // SAFETY: `device` is the live logical device; `layout_info` borrows
        // `set_layouts`, whose descriptor-set-layout handles are device-owned,
        // live, and outlive this call; the returned layout is caller-owned.
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
    wireframe_supported: bool,
) -> Result<PipelineSet> {
    let layout = build_triangle_pipeline_layout(device, descriptor_set_layout, scene_set_layout)?;
    triangle_pipeline_inner(
        device,
        render_pass,
        extent,
        pipeline_cache,
        layout,
        wireframe_supported,
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
    wireframe_supported: bool,
) -> Result<PipelineSet> {
    triangle_pipeline_inner(
        device,
        render_pass,
        extent,
        pipeline_cache,
        existing_layout,
        wireframe_supported,
    )
}

fn triangle_pipeline_inner(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    pipeline_cache: vk::PipelineCache,
    pipeline_layout: vk::PipelineLayout,
    wireframe_supported: bool,
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
    //
    // The base rasterizer below uses `polygon_mode(FILL)`. Wireframe is
    // LIVE (#869 / O4-D4-NEW-01): the importer captures a `wireframe: bool`
    // on `MaterialInfo` / `ImportedMesh` from `NiWireframeProperty`, it
    // rides the `PipelineKey::{Opaque,Blended}.wireframe` axis (LINE and
    // FILL are static state, so each needs its own pipeline), and a
    // `polygon_mode(LINE)` variant is built below (the wireframe-variant
    // clone) gated on the device exposing `features.fillModeNonSolid` —
    // when absent the variant is `None` and callers fall back to the FILL
    // pipeline. `context/draw.rs` selects the variant from
    // `draw_cmd.wireframe`. Oblivion vanilla ships zero wireframe meshes,
    // so this path is exercised only by authored content.
    // CULL_MODE is dynamic state (see `dynamic_states` below) so the
    // baked value here is ignored at draw time — it just needs to be
    // valid. The pre-#930 design built two pipelines (BACK + NONE) but
    // they compiled to identical machine code on every desktop driver
    // because of the dynamic-state override. Single rasterizer suffices.
    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(true);

    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // Main render pass has 8 color attachments (HDR + normal + motion +
    // mesh_id + raw_indirect + albedo + the two FSR masks). Each needs a
    // blend state entry. The opaque pipeline never blends any of them.
    let color_blend_none = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    // Opaque and alpha-tested geometry is fully described by depth and
    // motion, which is exactly the "leave the mask at its zero clear" case.
    // Masking the writes off (rather than writing zero) also keeps the
    // cleared attachment untouched where an opaque draw covers a
    // transparent one that already MAX-blended a value in.
    let fsr_mask_off = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::empty())
        .blend_enable(false);
    let color_blend_attachment = [
        color_blend_none, // 0 HDR color
        color_blend_none, // 1 normal
        color_blend_none, // 2 motion
        color_blend_none, // 3 mesh_id
        color_blend_none, // 4 raw_indirect
        color_blend_none, // 5 albedo
        fsr_mask_off,     // 6 fsr_reactive
        fsr_mask_off,     // 7 fsr_transparency
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
    // `stencil_test_enable(false)` is hardcoded — the stencil state
    // captured by the importer at `MaterialInfo.stencil_state` is
    // dormant until per-material stencil pipeline variants land.
    // Wiring those needs a depth-format flip too: `find_depth_format`
    // prefers `D32_SFLOAT` (no stencil bits) which is the better
    // precision pick when no consumer reads stencil. See #337.
    let depth_stencil_opaque = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    // Wireframe variant: clone the rasterizer with `polygon_mode(LINE)`
    // when the device exposed `fillModeNonSolid`. When not, the variant
    // is skipped and `NiWireframeProperty` content silently renders
    // filled. #869.
    let rasterizer_wireframe = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::LINE)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(true);

    let mut pipeline_infos: Vec<vk::GraphicsPipelineCreateInfo> = Vec::with_capacity(2);
    // Opaque pipeline — depth write on, no blend. Two-sided behavior
    // comes from dynamic `cmd_set_cull_mode` per draw, not a separate
    // pipeline (#930).
    pipeline_infos.push(
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
    );
    if wireframe_supported {
        pipeline_infos.push(
            vk::GraphicsPipelineCreateInfo::default()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterizer_wireframe)
                .multisample_state(&multisampling)
                .depth_stencil_state(&depth_stencil_opaque)
                .color_blend_state(&color_blending)
                .dynamic_state(&dynamic_state)
                .layout(pipeline_layout)
                .render_pass(render_pass)
                .subpass(0),
        );
    }

    let pipelines = unsafe {
        // SAFETY: `device` + `pipeline_cache` are live; every `pipeline_infos`
        // entry borrows state (stages, vertex input, dynamic state) that
        // outlives this call and references live device-owned handles —
        // `pipeline_layout`, `render_pass`, and the `vert_module`/`frag_module`
        // shader modules (destroyed just below); the returned pipelines are
        // caller-owned.
        device
            .create_graphics_pipelines(pipeline_cache, &pipeline_infos, None)
            .map_err(|(_, err)| err)
            .context("Failed to create graphics pipelines")?
    };

    log::info!(
        "Graphics pipeline created (opaque + {} wireframe; blend variants lazy-cached by (src, dst, wireframe))",
        if wireframe_supported { "1" } else { "0 (fillModeNonSolid unavailable)" }
    );

    // SAFETY: Shader modules are compiled into the pipeline objects during
    // create_graphics_pipelines and are no longer needed. Destroy them
    // immediately to avoid holding GPU resources for the entire context
    // lifetime. See issue #98.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    let opaque_wireframe = if wireframe_supported {
        Some(pipelines[1])
    } else {
        None
    };
    Ok(PipelineSet {
        opaque: pipelines[0],
        opaque_wireframe,
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
/// values; [`gamebryo_to_vk_blend_factor`] maps them. HDR uses the authored
/// Gamebryo blend factors. Raw indirect and albedo
/// use conventional source-alpha blending so a transmissive surface cannot
/// replace the opaque receiver's indirect-light state with an opaque glass
/// G-buffer record. Normal/motion/mesh_id still overwrite: they carry the
/// frontmost transparent surface state used by diagnostics, TAA rejection,
/// and glass caustics rather than composited radiance.
/// The shared Vulkan state a blend-pipeline build reuses from the opaque
/// pipelines: device, render pass, extent, pipeline cache, and layout.
/// Groups the handles that travel together into [`create_blend_pipeline`].
#[derive(Clone, Copy)]
pub struct BlendPipelineCtx<'a> {
    /// Logical device the pipeline is created on.
    pub device: &'a ash::Device,
    /// Render pass the pipeline is compatible with.
    pub render_pass: vk::RenderPass,
    /// Viewport/scissor extent baked into the pipeline state.
    pub extent: vk::Extent2D,
    /// Pipeline cache the build draws from / writes to.
    pub pipeline_cache: vk::PipelineCache,
    /// Pipeline layout shared with the opaque pipelines.
    pub pipeline_layout: vk::PipelineLayout,
}

/// The 8 G-buffer color-attachment blend states for a blend pipeline,
/// keyed on `preserve_opaque_gbuffer`. Pure (no `ash::Device` needed) so
/// the write-mask contract is unit-testable directly — split out of
/// [`create_blend_pipeline`] for exactly that reason (#2745 /
/// REN-D11-2026-08-12-01: pin `is_caustic_source(cmd) ⇒ mesh-ID
/// writable`, which a device-requiring pipeline-creation test couldn't
/// have caught at `cargo test` time).
///
/// `hdr_blend` is the one attachment state that depends on the caller's
/// authored (src, dst) Gamebryo blend factors, so it's built by the
/// caller and passed in; every other state here is fixed.
fn blend_gbuffer_attachments(
    hdr_blend: vk::PipelineColorBlendAttachmentState,
    preserve_opaque_gbuffer: bool,
) -> [vk::PipelineColorBlendAttachmentState; 8] {
    let overwrite = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let no_write = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::empty())
        .blend_enable(false);
    let auxiliary_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD);
    // MAX over ONE/ONE: a stack of overlapping translucent surfaces reports
    // the most reactive coverage in the stack rather than whichever happened
    // to draw last. AMD's integration contract asks for exactly this
    // accumulation rule for both masks.
    let fsr_mask_max = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::R)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE)
        .color_blend_op(vk::BlendOp::MAX)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE)
        .alpha_blend_op(vk::BlendOp::MAX);
    if preserve_opaque_gbuffer {
        [
            hdr_blend, // 0 HDR color (glass blends over opaque color)
            no_write,  // 1 preserve opaque normal paired with depth
            no_write,  // 2 preserve opaque motion
            // 3 mesh_id: #2745 (REN-D11-2026-08-12-01) — WRITABLE, not
            // preserved. `preserve_opaque_gbuffer` is set exclusively for
            // refractive-glass draws (`is_refractive_glass`), the same
            // predicate that gates `INSTANCE_FLAG_CAUSTIC_SOURCE` on
            // these draws' `GpuInstance.flags`. `caustic_splat.comp`
            // finds its sources ONLY through this attachment's bit 31
            // (`triangle.frag`'s `outMeshID` already sets it correctly
            // for every alpha-blended fragment); masking it to no_write
            // here made the producing (glass) and consuming
            // (caustic-source-flagged) pixel sets disjoint, so the
            // glass-side caustic accumulator received zero splats every
            // frame. Unlike normal/motion/raw_indirect/albedo, letting
            // this one through is SAFE for the original "denoiser
            // smears" concern the `no_write` mask was added for: bit 31
            // is exactly the signal `svgf_temporal.comp`'s history gate
            // (`(currID & 0x80000000u) != 0u`) already uses to bypass
            // temporal reprojection for alpha-blended pixels, so a
            // glass fragment's mesh_id is never mistaken for a stable
            // opaque surface identity.
            overwrite,
            no_write,     // 4 preserve opaque raw_indirect
            no_write,     // 5 preserve opaque albedo
            fsr_mask_max, // 6 fsr_reactive
            fsr_mask_max, // 7 fsr_transparency
        ]
    } else {
        [
            hdr_blend,       // 0 HDR color (blends)
            overwrite,       // 1 normal
            overwrite,       // 2 motion
            overwrite,       // 3 mesh_id
            auxiliary_blend, // 4 raw_indirect (coverage blend)
            auxiliary_blend, // 5 albedo (coverage blend)
            fsr_mask_max,    // 6 fsr_reactive
            fsr_mask_max,    // 7 fsr_transparency
        ]
    }
}

/// Alpha-lane blend factors for the HDR attachment, turning its alpha
/// channel into an **accumulated coverage lane** (#2466 / REN-D8-N01).
///
/// `composite.frag`'s sky branch needs to know how much of a `depth ==
/// 1.0` pixel is already owned by transparent geometry: blend pipelines
/// run with `depth_write_enable(false)`, so an alpha-blended fragment
/// with nothing opaque behind it leaves depth at the cleared 1.0 and
/// composite classifies the pixel as sky. Pre-fix the branch *replaced*
/// the pixel with `compute_sky(dir)` and the fragment's HDR contribution
/// was discarded outright — smoke, steam, magic billboards, banners and
/// glass panes silhouetted against open sky simply vanished.
///
/// The rule the lane implements is `a' = 1 - (1 - a) · D`, where `D` is
/// how much of the destination this draw's colour equation preserves —
/// i.e. exactly the authored Gamebryo destination factor:
///
/// * `dst = ONE` (additive fire / glow / magic FX): `D = 1`, the draw
///   occludes nothing, so coverage must pass through untouched —
///   `(ZERO, ONE)`. Getting this wrong would darken the sky behind every
///   additive effect card by `1 - alpha`.
/// * `dst = ONE_MINUS_SRC_ALPHA` (the classic alpha blend): `D = 1 - a`,
///   giving `a' = a + a_dst·(1 - a)` — proper over-operator coverage
///   accumulation across a stack of translucent layers, which the old
///   `(ONE, ZERO)` "last writer wins" pair could not express.
/// * anything else: same structural form, `a' = a_src + a_dst · D`.
///   Vulkan evaluates a colour-flavoured factor's alpha component in the
///   alpha equation, so exotic authored factors stay well-defined.
///
/// The lane has no other consumer: `taa.comp` forwards HDR alpha
/// untouched and `composite.frag` forwards it to the swapchain, which
/// ignores it (#676 / DEN-6, DEN-11).
fn coverage_alpha_factors(dst_factor: vk::BlendFactor) -> (vk::BlendFactor, vk::BlendFactor) {
    if dst_factor == vk::BlendFactor::ONE {
        (vk::BlendFactor::ZERO, vk::BlendFactor::ONE)
    } else {
        (vk::BlendFactor::ONE, dst_factor)
    }
}

pub fn create_blend_pipeline(
    ctx: BlendPipelineCtx,
    src: u8,
    dst: u8,
    wireframe: bool,
    preserve_opaque_gbuffer: bool,
) -> Result<vk::Pipeline> {
    let BlendPipelineCtx {
        device,
        render_pass,
        extent,
        pipeline_cache,
        pipeline_layout,
    } = ctx;
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

    // CULL_MODE is dynamic state (see `dynamic_states` below); the
    // baked value is ignored at draw time. Two-sided alpha-blend
    // ordering split (FRONT-cull pass for back faces + BACK-cull
    // pass for front faces) is implemented via per-draw
    // `cmd_set_cull_mode`, not separate pipelines (#930). #869:
    // `polygon_mode` is now LINE for the wireframe variant; the
    // factory caller (`get_or_create_blend_pipeline`) gates this on
    // `caps.fill_mode_non_solid_supported`, so `wireframe=true` is
    // only reachable when the device supports LINE rasterization.
    let polygon_mode = if wireframe {
        vk::PolygonMode::LINE
    } else {
        vk::PolygonMode::FILL
    };
    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(polygon_mode)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(true);

    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let src_factor = gamebryo_to_vk_blend_factor(src);
    let dst_factor = gamebryo_to_vk_blend_factor(dst);
    let (src_alpha_factor, dst_alpha_factor) = coverage_alpha_factors(dst_factor);
    let hdr_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(src_factor)
        .dst_color_blend_factor(dst_factor)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(src_alpha_factor)
        .dst_alpha_blend_factor(dst_alpha_factor)
        .alpha_blend_op(vk::BlendOp::ADD);
    let attachments = blend_gbuffer_attachments(hdr_blend, preserve_opaque_gbuffer);
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&attachments);

    // Transparent surfaces never write depth — prevents z-fight with
    // other translucents at the same depth and keeps opaque geometry
    // visible behind glass / decals.
    // LESS_OR_EQUAL matches draw.rs:cmd_set_depth_compare_op (the live source of truth).
    // `stencil_test_enable(false)` matches the opaque path — see the
    // stencil-deferral comment there. The blend pipeline would also
    // need stencil variants wired before #337's portal / shadow-volume
    // / mask-decal future work could land.
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
        // SAFETY: `device` + `pipeline_cache` are live; the single `infos` entry
        // borrows state that outlives this call and references live device-owned
        // handles — `pipeline_layout`, `render_pass`, and the `vert_module`/
        // `frag_module` shader modules (destroyed just below); the returned
        // pipeline is caller-owned.
        device
            .create_graphics_pipelines(pipeline_cache, &infos, None)
            .map_err(|(_, err)| err)
            .context("Failed to create blend pipeline variant")?
    };

    unsafe {
        // SAFETY: both modules were created by us for this build and not yet
        // destroyed; the pipeline is built so the modules are no longer needed
        // (Vulkan copies them in at create time); `device` is live.
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    log::debug!(
        "Blend pipeline created: src={src} ({src_factor:?}), dst={dst} ({dst_factor:?}), \
         wireframe={wireframe}, preserve_opaque_gbuffer={preserve_opaque_gbuffer}"
    );

    Ok(pipelines[0])
}

/// Dynamic states declared by the UI pipeline. Cross-referenced by
/// the overlay call site in `presentation.rs::record_overlay` (moved
/// there from `vulkan/context/draw.rs`'s `pipeline_ui` by #3426 — that
/// symbol no longer exists anywhere in the tree) so a future addition
/// to this list trips the `_UI_OVERLAY_DEFENSIVE_STATE_INVARIANT`
/// const-assert in `record_overlay` and forces the author to add the
/// matching `cmd_set_*` defensive call. Depth / cull / depth-bias state
/// on the UI overlay pipeline is intentionally STATIC (off / off / NONE /
/// off) — pipeline bind applies those values automatically, no per-bind
/// cmd_set needed. See #663.
pub const UI_PIPELINE_DYNAMIC_STATES: &[vk::DynamicState] =
    &[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

/// Creates the UI overlay pipeline (no depth, no lighting, alpha blend).
///
/// Uses the same pipeline layout as the scene pipelines (set 0 = bindless
/// textures, set 1 = scene UBO/SSBOs including the instance buffer at
/// binding 4). No push constants on this shared scene/UI layout —
/// per-instance data lives in the instance SSBO (water uses its own
/// 128-byte push-constant layout on a separate pipeline layout). The UI
/// vertex shader reads only the `textureIndex` field; vertices are
/// already in NDC clip space so the `model` matrix is ignored.
///
/// `render_pass` is the **presentation** pass, not the geometry pass (#3426):
/// the overlay composites onto the tone-mapped, upscaled swapchain image, so
/// the pipeline is built for that pass's single colour attachment and is
/// owned by `PresentationPipeline` (which is what a swapchain recreate
/// rebuilds). `extent` is unused (`let _ = extent;` below) — viewport/scissor
/// have been dynamic since #578, so this pipeline is not extent-bound and a
/// resize can rebuild it safely; the parameter is retained only for
/// signature symmetry with the other `create_*_pipeline` functions.
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
    // instead of the full 104-byte Vertex (color widened vec3→vec4, `cd2b5fe4`;
    // pinned by `vertex_size_matches_attribute_stride`, #2761) with unused
    // bone / normal / color / tangent fields.
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

    // Hard-coded `polygon_mode(FILL)`; wireframe routing deferred per #869.
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
    // `stencil_test_enable(false)` is correct for UI (no stencil
    // semantics on Scaleform / SWF overlays); world-geometry stencil
    // dispatch lives in the opaque + blend pipelines above. See #337.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false)
        .stencil_test_enable(false);

    // Alpha blending for UI transparency, over the presentation pass's single
    // swapchain colour attachment.
    //
    // #3426 — this used to be an eight-entry table because the overlay was
    // the tail of the *geometry* pass: it blended into the render-resolution
    // HDR G-buffer (slot 0) and masked itself out of normal/motion/mesh_id so
    // it would not pollute them, and it wrote no FSR reactive/transparency
    // mask at all. That put every menu through fog, bloom, exposure, ACES,
    // TAA (with a zero motion vector and no reactive mask) and FSR upscale.
    // The overlay now composites in `presentation.rs`, after all of it, at
    // output resolution — so there is exactly one attachment to blend into
    // and nothing left to mask off. The swapchain attachment is sRGB, so this
    // blend happens in linear space and is re-encoded on write, which is the
    // exact inverse of the `R8G8B8A8_SRGB` sampler read of Ruffle's capture.
    let ui_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD);
    let color_blend_attachment = [ui_blend];
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachment);

    // No DEPTH_BIAS — UI pipeline has depth_bias_enable(false).
    //
    // **Contract** (#663). The UI overlay call site,
    // `presentation.rs::record_overlay` (moved there from
    // `vulkan/context/draw.rs`'s retired `pipeline_ui` by #3426),
    // defensively re-sets every state in this list, then relies on the
    // UI pipeline's STATIC depth/cull state to take effect on bind.
    // If you add a state here, you MUST also extend the overlay path
    // to `cmd_set_*` it — otherwise the new dynamic state will inherit
    // whatever the last main-batch pipeline left set, which is a hard-
    // to-reproduce visual bug. The `_UI_OVERLAY_DEFENSIVE_STATE_INVARIANT`
    // const_assert in `record_overlay` fires at compile time when this
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
        // SAFETY: `device` + `pipeline_cache` are live; the single `pipeline_info`
        // entry borrows state that outlives this call and references live
        // device-owned handles — `pipeline_layout`, `render_pass`, and the
        // `vert_module`/`frag_module` shader modules (destroyed just below); the
        // returned pipeline is caller-owned.
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

    /// #2466 / REN-D8-N01 — the HDR attachment's alpha lane carries the
    /// accumulated transparent coverage `composite.frag`'s sky branch
    /// weighs `compute_sky()` against. The two cases that matter:
    ///
    /// * additive FX (`dst = ONE`) preserve the destination entirely, so
    ///   they must contribute **zero** occlusion — otherwise every fire /
    ///   glow / magic card would darken the sky behind it by `1 - alpha`.
    /// * a classic alpha blend (`dst = ONE_MINUS_SRC_ALPHA`) must
    ///   accumulate as the over operator, `a' = a + a_dst·(1 - a)`, so a
    ///   stack of translucent layers reports combined coverage instead of
    ///   whichever layer happened to draw last.
    #[test]
    fn coverage_alpha_lane_tracks_the_authored_destination_factor() {
        assert_eq!(
            coverage_alpha_factors(vk::BlendFactor::ONE),
            (vk::BlendFactor::ZERO, vk::BlendFactor::ONE),
            "additive draws must pass coverage through untouched"
        );
        assert_eq!(
            coverage_alpha_factors(vk::BlendFactor::ONE_MINUS_SRC_ALPHA),
            (vk::BlendFactor::ONE, vk::BlendFactor::ONE_MINUS_SRC_ALPHA),
            "alpha blends must accumulate coverage as the over operator"
        );
        // Every other authored destination factor keeps the same
        // structural form (`a' = a_src + a_dst · D`) with D taken from
        // the colour equation, so no Gamebryo factor is left on the old
        // "last writer wins" pair.
        for gb in (0u8..=10).filter(|gb| *gb != 0) {
            let dst = gamebryo_to_vk_blend_factor(gb);
            assert_eq!(
                coverage_alpha_factors(dst),
                (vk::BlendFactor::ONE, dst),
                "factor {gb} ({dst:?}) must mirror its colour destination factor"
            );
        }
    }

    /// Regression: the cache key must distinguish all the combos that
    /// previously collapsed to the same static pipeline. `(6, 7)` (alpha)
    /// and `(6, 0)` (additive) are the two combos the old code handled;
    /// `(4, 0)` (DEST_COLOR/ONE — Oblivion glass modulation) and
    /// `(0, 0)` (ONE/ONE — premultiplied additive) are the two it
    /// silently aliased to the wrong bucket.
    ///
    /// Post-#930: `two_sided` is no longer a key axis (collapsed into
    /// dynamic CULL_MODE state). The test no longer asserts that
    /// `Blended { src, dst, two_sided=true }` is distinct from
    /// `Blended { src, dst, two_sided=false }` because both go through
    /// the same pipeline now.
    #[test]
    fn pipeline_key_distinguishes_combos_old_code_collapsed() {
        let alpha = PipelineKey::Blended {
            src: 6,
            dst: 7,
            wireframe: false,
            preserve_opaque_gbuffer: false,
        };
        let additive = PipelineKey::Blended {
            src: 6,
            dst: 0,
            wireframe: false,
            preserve_opaque_gbuffer: false,
        };
        let glass_modulate = PipelineKey::Blended {
            src: 4,
            dst: 0,
            wireframe: false,
            preserve_opaque_gbuffer: false,
        };
        let premul_additive = PipelineKey::Blended {
            src: 0,
            dst: 0,
            wireframe: false,
            preserve_opaque_gbuffer: false,
        };
        let opaque = PipelineKey::Opaque { wireframe: false };

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
    }

    /// #869 — wireframe is a key axis; FILL and LINE must not collide.
    #[test]
    fn pipeline_key_wireframe_is_distinct_axis() {
        let opaque_fill = PipelineKey::Opaque { wireframe: false };
        let opaque_line = PipelineKey::Opaque { wireframe: true };
        assert_ne!(opaque_fill, opaque_line);
        let blend_fill = PipelineKey::Blended {
            src: 6,
            dst: 7,
            wireframe: false,
            preserve_opaque_gbuffer: false,
        };
        let blend_line = PipelineKey::Blended {
            src: 6,
            dst: 7,
            wireframe: true,
            preserve_opaque_gbuffer: false,
        };
        assert_ne!(blend_fill, blend_line);
    }

    /// Refractive glass needs a distinct pipeline because it blends only
    /// HDR + FSR masks and preserves the opaque G-buffer behind it.
    #[test]
    fn pipeline_key_gbuffer_preservation_is_distinct_axis() {
        let ordinary_blend = PipelineKey::Blended {
            src: 6,
            dst: 7,
            wireframe: false,
            preserve_opaque_gbuffer: false,
        };
        let refractive_glass = PipelineKey::Blended {
            src: 6,
            dst: 7,
            wireframe: false,
            preserve_opaque_gbuffer: true,
        };
        assert_ne!(ordinary_blend, refractive_glass);
    }

    /// Regression for #2745 (REN-D11-2026-08-12-01). `preserve_opaque_
    /// gbuffer` is set exclusively for refractive-glass draws
    /// (`context::draw::is_refractive_glass`), the same predicate that
    /// gates `INSTANCE_FLAG_CAUSTIC_SOURCE`, and `caustic_splat.comp`
    /// finds its sources ONLY through the mesh-ID attachment's bit 31.
    /// Masking mesh_id to `no_write` alongside normal/motion/raw_indirect
    /// /albedo made the producing (glass) and consuming (caustic-source-
    /// flagged) pixel sets disjoint — the glass-side caustic accumulator
    /// received zero splats in every cell. Pin the contract this issue
    /// asked for: `is_caustic_source(cmd) ⇒ mesh-ID (attachment 3) is
    /// writable for that cmd's pipeline key`. Every OTHER attachment this
    /// axis preserves (1/2/4/5) must still be masked off — this is not a
    /// blanket revert of the `preserve_opaque_gbuffer` feature.
    #[test]
    fn glass_pipeline_keeps_mesh_id_writable_for_its_own_caustic_source_gate() {
        let hdr_blend = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true);

        let glass = blend_gbuffer_attachments(hdr_blend, true);
        assert_eq!(
            glass[3].color_write_mask,
            vk::ColorComponentFlags::RGBA,
            "mesh_id (attachment 3) must stay writable for the \
             preserve_opaque_gbuffer (refractive-glass) pipeline, or its \
             own INSTANCE_FLAG_CAUSTIC_SOURCE draws are invisible to \
             caustic_splat.comp's bit-31 gate (#2745)"
        );
        // The rest of the "preserve opaque" set must still be masked off —
        // this fix is scoped to mesh_id only.
        for (idx, name) in [
            (1, "normal"),
            (2, "motion"),
            (4, "raw_indirect"),
            (5, "albedo"),
        ] {
            assert_eq!(
                glass[idx].color_write_mask,
                vk::ColorComponentFlags::empty(),
                "{name} (attachment {idx}) must stay preserved (no_write) \
                 for the refractive-glass pipeline — only mesh_id (#2745) \
                 was the fix, not a blanket revert of preserve_opaque_gbuffer"
            );
        }

        // Sibling case: the ordinary blend pipeline (non-glass) already
        // wrote mesh_id before this fix — must still write it after.
        let ordinary = blend_gbuffer_attachments(hdr_blend, false);
        assert_eq!(ordinary[3].color_write_mask, vk::ColorComponentFlags::RGBA);
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
