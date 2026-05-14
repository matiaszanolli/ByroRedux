//! Water surface graphics pipeline.
//!
//! Owns the `vk::Pipeline` + `vk::PipelineLayout` for the water draw
//! variant of the main scene render pass. Structurally close to the
//! lazily-cached blend pipeline in [`pipeline::create_blend_pipeline`]:
//!
//! - shares the main render pass (subpass 0) so it draws into the
//!   same HDR + G-buffer attachments as the rest of the scene;
//! - reuses the bindless-texture descriptor set (set 0) and the
//!   scene descriptor set (set 1, with CameraUBO + TLAS + InstanceBuffer)
//!   for descriptor-set compatibility with bound state;
//! - reuses the engine [`Vertex`] attribute layout so water meshes
//!   travel through the same global vertex/index SSBOs.
//!
//! Unique to this pipeline:
//!
//! - `water.vert` + `water.frag` shaders;
//! - a 112-byte push-constant block ([`WaterPush`]) carrying
//!   time + flow + per-plane material params;
//! - SRC_ALPHA / ONE_MINUS_SRC_ALPHA blend on HDR attachment 0;
//!   attachments 1..5 (normal, motion, mesh_id, raw_indirect, albedo)
//!   are masked off (`color_write_mask = 0`) so water never pollutes
//!   the G-buffer feeding SVGF / motion-vector reprojection;
//! - depth test on, depth write **off** (transparent surface);
//! - cull NONE (water seen from both above + below).
//!
//! Per-frame flow expected by the caller (typically `draw.rs`):
//!
//! 1. After all opaque + alpha-blend draws have submitted to the main
//!    render pass but before `vkCmdEndRenderPass`, bind this pipeline.
//! 2. The bound set 0 + set 1 + dynamic state from prior triangle
//!    draws stays valid (layouts are compatible; dynamic state
//!    inheritance is documented at `pipeline.rs::UI_PIPELINE_DYNAMIC_STATES`).
//! 3. For each `WaterPlane` entity, push constants + bind its
//!    `MeshHandle` vertex/index buffers + `cmd_draw_indexed` with
//!    its instance index. The instance SSBO entry must already be
//!    populated (water planes are real instances — they reuse the
//!    same per-instance model matrix the rest of the scene uses).

use super::pipeline::load_shader_module;
use crate::vertex::Vertex;
use anyhow::{Context, Result};
use ash::vk;

const WATER_VERT_SPV: &[u8] = include_bytes!("../../shaders/water.vert.spv");
const WATER_FRAG_SPV: &[u8] = include_bytes!("../../shaders/water.frag.spv");

/// Push-constant block for one water draw. Layout matches the
/// `WaterPush` block in `shaders/water.frag` exactly — std430
/// scalar layout (push constants are always scalar, never std140).
///
/// 112 bytes total (7 × 16) — fits the Vulkan 1.1 spec minimum
/// `maxPushConstantsSize >= 128`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WaterPush {
    /// x = time (seconds since cell load), y = `WaterKind` as f32,
    /// z = foam_strength (0..1), w = ior.
    pub timing: [f32; 4],
    /// xyz = flow direction (unit vector), w = flow speed (wu/s).
    pub flow: [f32; 4],
    /// rgb = shallow_color (linear), a = fog_near.
    pub shallow: [f32; 4],
    /// rgb = deep_color (linear), a = fog_far.
    pub deep: [f32; 4],
    /// xy = scroll_a, zw = scroll_b (wu/s).
    pub scroll: [f32; 4],
    /// x = uv_scale_a, y = uv_scale_b, z = shoreline_width,
    /// w = reflectivity.
    pub tune: [f32; 4],
    /// x = fresnel_f0, y = reserved, z = normal_map_index bit-cast
    /// to f32 (shader does `floatBitsToUint`), w = reserved.
    pub misc: [f32; 4],
}

impl WaterPush {
    /// Pack `normal_map_index` as a float bit-pattern. The shader
    /// recovers the integer via `floatBitsToUint(push.misc.z)`.
    /// `u32::MAX` denotes "no normal map — use procedural noise."
    #[inline]
    pub fn pack_normal_index(idx: u32) -> f32 {
        f32::from_bits(idx)
    }
}

const _: () = assert!(
    std::mem::size_of::<WaterPush>() == 112,
    "WaterPush must stay 112 bytes (fits the Vulkan 1.1 minimum push constant range of 128)"
);

/// One water surface to draw in the current frame.
///
/// Built by the app's per-frame render code from `WaterPlane` ECS
/// entities and passed alongside [`DrawCommand`] to
/// [`VulkanContext::draw_frame`]. Kept separate from `DrawCommand` so
/// the 112-byte push-constant block doesn't bloat every regular draw.
///
/// The water entity must also appear as a regular `DrawCommand` so its
/// `GpuInstance` slot (`instance_index`) is uploaded with the correct
/// model matrix — the regular path is what fills the instance SSBO.
/// The duplicate emit is gated on `is_water_skip` (a bit on
/// `DrawCommand`, added when water plumbing lands) so the regular
/// triangle pipeline doesn't double-draw the surface.
///
/// [`DrawCommand`]: crate::vulkan::context::DrawCommand
/// [`VulkanContext::draw_frame`]: crate::vulkan::context::VulkanContext::draw_frame
pub struct WaterDrawCommand {
    /// Mesh registry handle for the flat water quad (or per-cell
    /// shoreline-fit mesh — both work).
    pub mesh_handle: u32,
    /// Instance buffer slot — must match the `gl_InstanceIndex`
    /// emitted for this water entity's regular draw command.
    pub instance_index: u32,
    /// Push-constant block uploaded for this draw. Built from the
    /// entity's `WaterPlane` + `WaterFlow` components + the frame's
    /// `TotalTime`.
    pub push: WaterPush,
}

/// Contract check (#1026 / F-WAT-05): every entry in `water_commands`
/// must point at a `DrawCommand` slot whose `is_water` flag is set
/// and whose `mesh_handle` matches. This pins the no-resort contract
/// the app's per-frame render code relies on — `WaterDrawCommand`
/// records `instance_index` as the position in `draw_commands` at
/// emit time, so any code path that re-sorts `draw_commands` after
/// the water emit would silently desync the recorded
/// `instance_index` from the actual SSBO slot the renderer assigns.
/// (The SSBO is built by iterating `draw_commands` in order with
/// frustum-culled entries reserving their slot per #516, so the
/// vec position is the slot.)
///
/// Pure function so the regression test can pin the contract
/// without a live Vulkan device. The hot path calls it inside a
/// `debug_assert!` in `draw_frame` immediately before the
/// water-pipeline loop consumes `wc.instance_index`.
pub fn water_commands_match_draw_slots(
    water_commands: &[WaterDrawCommand],
    draw_commands: &[crate::vulkan::context::DrawCommand],
) -> bool {
    water_commands
        .iter()
        .all(|wc| match draw_commands.get(wc.instance_index as usize) {
            Some(dc) => dc.is_water && dc.mesh_handle == wc.mesh_handle,
            None => false,
        })
}

/// Owns the water graphics pipeline + its layout.
pub struct WaterPipeline {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
}

impl WaterPipeline {
    /// Create the water pipeline.
    ///
    /// `descriptor_set_layout` is the bindless-texture set 0 layout
    /// (same one the triangle pipeline uses). `scene_set_layout` is
    /// the scene set 1 layout (CameraUBO + TLAS + InstanceBuffer +
    /// the rest of the per-frame bindings — water only reads camera,
    /// TLAS, and instance buffer, but compatibility requires the
    /// full layout).
    pub fn new(
        device: &ash::Device,
        render_pass: vk::RenderPass,
        pipeline_cache: vk::PipelineCache,
        descriptor_set_layout: vk::DescriptorSetLayout,
        scene_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        // ── Pipeline layout: set 0 + set 1 (compat) + push constants ──
        let set_layouts = [descriptor_set_layout, scene_set_layout];
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<WaterPush>() as u32);
        let push_ranges = [push_range];
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&set_layouts)
            .push_constant_ranges(&push_ranges);

        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(&layout_info, None)
                .context("water pipeline layout")?
        };

        // ── Pipeline ──
        let pipeline = match build_pipeline(device, render_pass, pipeline_cache, pipeline_layout) {
            Ok(p) => p,
            Err(e) => {
                // Clean up the layout on failure so we don't leak.
                unsafe { device.destroy_pipeline_layout(pipeline_layout, None) };
                return Err(e);
            }
        };

        log::info!("Water pipeline created (water.vert + water.frag, SRC_ALPHA blend on HDR, cull NONE, depth-write off, 112B push constants)");

        Ok(Self {
            pipeline,
            pipeline_layout,
        })
    }

    /// Record a single water draw into a command buffer that is
    /// already inside the main render pass with set 0 + set 1 bound
    /// and dynamic viewport / scissor / depth state set to valid
    /// transparent-draw values.
    ///
    /// Caller is responsible for:
    ///
    /// - binding the per-mesh vertex + index buffers before this
    ///   call (water meshes are not in the global SSBO — they're
    ///   freshly registered per cell, same as terrain tile meshes);
    /// - issuing `cmd_set_depth_write_enable(false)`,
    ///   `cmd_set_depth_test_enable(true)`,
    ///   `cmd_set_cull_mode(vk::CullModeFlags::NONE)` before the
    ///   first water draw of the frame;
    /// - having uploaded the `GpuInstance` entry at `instance_index`
    ///   with the water plane's model matrix.
    ///
    /// # Safety
    ///
    /// `cmd` must be a valid command buffer in the recording state.
    /// All Vulkan handles passed in must outlive the GPU's
    /// consumption of this command buffer. This is the same
    /// contract every other `record_*` helper in the renderer
    /// honours.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn record_draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        push: &WaterPush,
        index_count: u32,
        instance_index: u32,
    ) {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

        let bytes = std::slice::from_raw_parts(
            (push as *const WaterPush) as *const u8,
            std::mem::size_of::<WaterPush>(),
        );
        device.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            bytes,
        );

        // gl_InstanceIndex resolves to instance_index in the bound
        // SSBO — same pattern the triangle path uses, just with a
        // single draw per water plane rather than batched.
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, instance_index);
    }

    /// Tear down — call only after `vkDeviceWaitIdle` to make sure
    /// no in-flight command buffer still references the pipeline.
    ///
    /// # Safety
    ///
    /// Caller must ensure the device is idle. The pipeline + layout
    /// must not have been destroyed previously (destructor is not
    /// idempotent — same policy as every other `destroy` in the
    /// renderer).
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        if self.pipeline != vk::Pipeline::null() {
            device.destroy_pipeline(self.pipeline, None);
            self.pipeline = vk::Pipeline::null();
        }
        if self.pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.pipeline_layout = vk::PipelineLayout::null();
        }
    }
}

fn build_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    pipeline_cache: vk::PipelineCache,
    pipeline_layout: vk::PipelineLayout,
) -> Result<vk::Pipeline> {
    let vert = load_shader_module(device, WATER_VERT_SPV)?;
    let frag = load_shader_module(device, WATER_FRAG_SPV)?;

    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(entry),
    ];

    // Same vertex layout as the triangle pipeline — water meshes
    // travel through the engine Vertex format so they can share
    // the global vertex / index SSBOs.
    let bindings = [Vertex::binding_description()];
    let attrs = Vertex::attribute_descriptions();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attrs);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    // Cull NONE — water is rendered from above and from below
    // (player swims under and looks up at the underside; rapids
    // boats see the surface from above). Front/back are both
    // valid view sides for a water plane.
    //
    // Static cull state is OK here because every other pipeline in
    // the main render pass declares CULL_MODE dynamic (#930). When
    // a triangle / blend pipeline binds after a water draw, it
    // re-emits its own dynamic cull immediately, and when this
    // pipeline binds it sets the cull mode dynamically (see
    // `record_draw`'s caller contract).
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

    // SRC_ALPHA / ONE_MINUS_SRC_ALPHA blend on HDR (attachment 0).
    // Attachments 1..5 are write-masked off: water never updates
    // the G-buffer (normal / motion / mesh_id / raw_indirect /
    // albedo) so SVGF and motion-vector reprojection see only
    // the opaque pass behind the water.
    let hdr_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD);
    let masked_off = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::empty())
        .blend_enable(false);
    let attachments = [
        hdr_blend, masked_off, masked_off, masked_off, masked_off, masked_off,
    ];
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&attachments);

    // Depth test on, depth write off — same as the blend pipeline
    // (transparent surfaces never write depth). `LESS_OR_EQUAL`
    // matches the engine-wide convention; both compare-op and
    // depth-test/write are declared dynamic for inheritance from
    // the surrounding triangle / blend draws.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [
        vk::DynamicState::VIEWPORT,
        vk::DynamicState::SCISSOR,
        vk::DynamicState::DEPTH_TEST_ENABLE,
        vk::DynamicState::DEPTH_WRITE_ENABLE,
        vk::DynamicState::DEPTH_COMPARE_OP,
    ];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let infos = [vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
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
            .context("water graphics pipeline")?
    };

    unsafe {
        device.destroy_shader_module(vert, None);
        device.destroy_shader_module(frag, None);
    }

    Ok(pipelines[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulkan::context::DrawCommand;

    #[test]
    fn water_push_layout_is_112_bytes() {
        assert_eq!(std::mem::size_of::<WaterPush>(), 112);
        assert_eq!(std::mem::align_of::<WaterPush>(), 4);
    }

    #[test]
    fn pack_normal_index_roundtrips() {
        for v in [0u32, 1, 42, 0xDEADBEEF, u32::MAX] {
            let packed = WaterPush::pack_normal_index(v);
            assert_eq!(packed.to_bits(), v);
        }
    }

    // ── #1026 / F-WAT-05 — water-instance-index contract ──────────

    fn make_draw_command(mesh_handle: u32, is_water: bool) -> DrawCommand {
        DrawCommand {
            mesh_handle,
            texture_handle: 0,
            model_matrix: [0.0; 16],
            alpha_blend: false,
            src_blend: 6,
            dst_blend: 7,
            two_sided: false,
            is_decal: false,
            render_layer: byroredux_core::ecs::components::RenderLayer::Architecture,
            bone_offset: 0,
            normal_map_index: 0,
            dark_map_index: 0,
            glow_map_index: 0,
            detail_map_index: 0,
            gloss_map_index: 0,
            parallax_map_index: 0,
            parallax_height_scale: 0.0,
            parallax_max_passes: 0.0,
            env_map_index: 0,
            env_mask_index: 0,
            alpha_threshold: 0.0,
            alpha_test_func: 0,
            roughness: 0.5,
            metalness: 0.0,
            emissive_mult: 0.0,
            emissive_color: [0.0; 3],
            specular_strength: 0.0,
            specular_color: [0.0; 3],
            diffuse_color: [1.0; 3],
            ambient_color: [1.0; 3],
            vertex_offset: 0,
            index_offset: 0,
            vertex_count: 0,
            sort_depth: 0,
            in_tlas: true,
            in_raster: true,
            avg_albedo: [0.0; 3],
            material_kind: 0,
            z_test: true,
            z_write: true,
            z_function: 3,
            terrain_tile_index: None,
            entity_id: 0,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
            material_alpha: 1.0,
            skin_tint_rgba: [0.0; 4],
            hair_tint_rgb: [0.0; 3],
            multi_layer_envmap_strength: 0.0,
            eye_left_center: [0.0; 3],
            eye_cubemap_scale: 0.0,
            eye_right_center: [0.0; 3],
            multi_layer_inner_thickness: 0.0,
            multi_layer_refraction_scale: 0.0,
            multi_layer_inner_scale: [0.0; 2],
            sparkle_rgba: [0.0; 4],
            effect_falloff: [0.0; 5],
            material_id: 0,
            vertex_color_emissive: false,
            effect_shader_flags: 0,
            is_water,
        }
    }

    fn water_cmd(mesh_handle: u32, instance_index: u32) -> WaterDrawCommand {
        WaterDrawCommand {
            mesh_handle,
            instance_index,
            push: WaterPush {
                timing: [0.0; 4],
                flow: [0.0; 4],
                shallow: [0.0; 4],
                deep: [0.0; 4],
                scroll: [0.0; 4],
                tune: [0.0; 4],
                misc: [0.0; 4],
            },
        }
    }

    /// Happy path: water command's `instance_index` points at the
    /// draw whose `mesh_handle` matches and whose `is_water` is set.
    /// Mirrors the post-emit, pre-upload state when no re-sort has
    /// run.
    #[test]
    fn matching_slot_passes_contract() {
        let draws = vec![
            make_draw_command(7, false),  // opaque rock
            make_draw_command(42, true),  // the water entity at idx 1
            make_draw_command(99, false), // another opaque draw
        ];
        let water = vec![water_cmd(42, 1)];
        assert!(water_commands_match_draw_slots(&water, &draws));
    }

    /// The trap: a re-sort moved the water draw to a different slot
    /// AFTER the WaterDrawCommand was emitted, so the recorded
    /// `instance_index` now points at an opaque draw with the wrong
    /// `mesh_handle` AND `is_water=false`. The predicate must reject
    /// both signals.
    #[test]
    fn resort_after_emit_breaks_contract() {
        // Original layout — water authored at idx 1.
        let original = vec![
            make_draw_command(7, false),
            make_draw_command(42, true),
            make_draw_command(99, false),
        ];
        let water = vec![water_cmd(42, 1)];
        assert!(
            water_commands_match_draw_slots(&water, &original),
            "sanity: pre-resort layout must satisfy the contract"
        );

        // Synthetic re-sort: rotate so the water draw lands at idx 0
        // and the opaque entries shift. `DrawCommand` isn't `Clone`
        // (it owns vk handle fields in production), so we rebuild
        // the rotated layout from scratch.
        let resorted = vec![
            make_draw_command(42, true),  // was at idx 1, now at idx 0
            make_draw_command(99, false), // was at idx 2, now at idx 1
            make_draw_command(7, false),  // was at idx 0, now at idx 2
        ];
        // Index 1 in `resorted` is now mesh 99, is_water=false — the
        // predicate catches it on BOTH signals (mesh mismatch AND
        // is_water=false).
        assert!(
            !water_commands_match_draw_slots(&water, &resorted),
            "predicate must reject the post-resort layout"
        );
    }

    /// `is_water == false` on the indexed slot is the strongest
    /// failure signal — even when the mesh_handle still matches
    /// (mesh-sharing case), the predicate must reject because
    /// `WaterDrawCommand` only exists for slots the water-emit
    /// flipped to `is_water=true`.
    #[test]
    fn mesh_match_but_is_water_false_fails_contract() {
        let draws = vec![
            make_draw_command(42, false), // same mesh_handle but is_water cleared
        ];
        let water = vec![water_cmd(42, 0)];
        assert!(!water_commands_match_draw_slots(&water, &draws));
    }

    /// `mesh_handle` mismatch fails even when `is_water=true` —
    /// covers the case where someone replaces the WaterPlane's
    /// MeshHandle between emit and upload (re-upload during cell
    /// transition, hypothetical mod hot-reload).
    #[test]
    fn mesh_handle_mismatch_fails_contract() {
        let draws = vec![make_draw_command(7, true)];
        let water = vec![water_cmd(42, 0)];
        assert!(!water_commands_match_draw_slots(&water, &draws));
    }

    /// Out-of-bounds `instance_index` (the vec was truncated after
    /// the emit — extreme case) is rejected rather than panicking.
    #[test]
    fn out_of_bounds_index_fails_contract() {
        let draws = vec![make_draw_command(42, true)];
        let water = vec![water_cmd(42, 5)]; // index past the end
        assert!(!water_commands_match_draw_slots(&water, &draws));
    }

    /// Empty water_commands trivially passes — no draws to validate.
    #[test]
    fn empty_water_commands_passes() {
        let draws = vec![make_draw_command(7, false)];
        assert!(water_commands_match_draw_slots(&[], &draws));
    }
}
