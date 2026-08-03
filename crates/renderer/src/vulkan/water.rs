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
//! - a 128-byte push-constant block ([`WaterPush`]) carrying
//!   time + flow + per-plane material params;
//! - SRC_ALPHA / ONE_MINUS_SRC_ALPHA blend on HDR attachment 0;
//!   attachments 1..6 (normal, motion, mesh_id, raw_indirect, albedo,
//!   reservoir) are masked off (`color_write_mask = 0`) so water never pollutes
//!   the G-buffer feeding SVGF / motion-vector reprojection;
//! - depth test on, depth write **off** (transparent surface);
//! - cull NONE (water seen from both above + below).
//!
//! Per-frame flow expected by the caller (`context::geometry_pass`):
//!
//! 1. After all opaque + alpha-blend draws have submitted to the main
//!    render pass but before `vkCmdEndRenderPass`, set the transparent
//!    dynamic state (depth test on, depth write off, cull NONE) and call
//!    [`WaterPipeline::bind_pass`] **once**. That binds the pipeline and
//!    all three descriptor sets for the whole pass (#2175).
//! 2. Descriptor state does *not* survive the pipeline bind: this
//!    pipeline's 128-byte push-constant range makes its layout
//!    incompatible with the triangle pipeline's for every set, so
//!    `bind_pass` rebinds sets 0 and 1 explicitly rather than inheriting
//!    them (#1258 — see its doc comment). Dynamic state does survive, per
//!    `pipeline.rs::UI_PIPELINE_DYNAMIC_STATES`.
//! 3. For each `WaterPlane` entity, bind its `MeshHandle` vertex/index
//!    buffers, then [`WaterPipeline::record_draw`] for the push constants
//!    + `cmd_draw_indexed` with its instance index. The instance SSBO
//!      entry must already be populated (water planes are real instances —
//!      they reuse the same per-instance model matrix the rest of the scene
//!      uses). Neither step disturbs the bindings from step 1.

use super::descriptors::{write_storage_image, DescriptorPoolBuilder};
use super::pipeline::load_shader_module;
use crate::vertex::Vertex;
use anyhow::{Context, Result};
use ash::vk;

// `pub(crate)` so the scene-descriptor validation in `scene_buffer::buffers`
// can pin the water shaders against the shared set=1 layout (#1561 STARTUP-
// VALIDATION — water reuses set 1 with TLAS at binding 2 as optional).
pub(crate) const WATER_VERT_SPV: &[u8] = include_bytes!("../../shaders/water.vert.spv");
pub(crate) const WATER_FRAG_SPV: &[u8] = include_bytes!("../../shaders/water.frag.spv");

/// Push-constant block for one water draw. Layout matches the
/// `WaterPush` block in `shaders/water.frag` exactly — std430
/// scalar layout (push constants are always scalar, never std140).
///
/// 128 bytes total (8 × 16) — exactly the Vulkan 1.1 spec minimum
/// `maxPushConstantsSize >= 128`. No further growth is possible
/// without raising a device capability check.
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
    /// w = reserved (was reflectivity pre-#1069; moved to `tint_reflect.w`).
    pub tune: [f32; 4],
    /// x = fresnel_f0, y = reserved, z = normal_map_index bit-cast
    /// to f32 (shader does `floatBitsToUint`), w = reserved.
    pub misc: [f32; 4],
    /// xyz = reflection_tint (WATR `reflection_color`; tints the
    /// geometry-hit colour in `traceWaterRay`). w = reflectivity
    /// (0..1 — fresnel multiplier; moved here from `tune.w` in #1069).
    pub tint_reflect: [f32; 4],
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
    std::mem::size_of::<WaterPush>() == 128,
    "WaterPush must be exactly 128 bytes (Vulkan 1.1 minimum maxPushConstantsSize; \
     no headroom remains — adding fields requires a device-capability check)"
);

/// One water surface to draw in the current frame.
///
/// Built by the app's per-frame render code from `WaterPlane` ECS
/// entities and passed alongside [`DrawCommand`] to
/// [`VulkanContext::draw_frame`]. Kept separate from `DrawCommand` so
/// the 128-byte push-constant block doesn't bloat every regular draw.
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
///
/// Extent-independent: viewport and scissor are dynamic state set per
/// frame in the draw-recording path, and no descriptor binds reference
/// a fixed-extent resource (G-buffer, depth, swapchain). No
/// `recreate_on_resize` method exists — and intentionally so. Other
/// pipelines (SVGF, TAA, Bloom, Composite) DO carry one because they
/// own attachment-sized resources; if water ever picks up such a
/// resource (e.g. a dedicated caustic accumulator), wire the resize
/// hook at that time. See REN-D17-NEW-01 / #1130.
pub struct WaterPipeline {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    /// Set 2 descriptor layout — single STORAGE_IMAGE binding for the
    /// water-caustic accumulator (#1255 / Phase C of #1210). Bound at
    /// `record_draw` time from the per-FIF `descriptor_sets[frame]`.
    pub(super) water_caustic_set_layout: vk::DescriptorSetLayout,
    /// Pool that allocates the per-FIF descriptor sets above.
    pub(super) water_caustic_descriptor_pool: vk::DescriptorPool,
    /// Per-FIF descriptor sets pointing at the matching slot of
    /// `WaterCausticAccum`. Populated by `update_water_caustic_descriptors`
    /// at swapchain init / recreate time so the per-frame draw can bind
    /// them directly without re-writing.
    pub(super) water_caustic_descriptor_sets: Vec<vk::DescriptorSet>,
}

/// Dynamic states declared on the water graphics pipeline. Extracted
/// to a module-level `const` (#1129) so the contents are inspectable
/// from unit tests without spinning up a Vulkan device.
///
/// Every value the static rasterizer / depth-stencil sets and that
/// the code comments label a "no-op baseline" must also appear here —
/// otherwise the static value silently wins at runtime.
///
/// Current "no-op baseline" → dynamic-state pairings:
///   * `rasterizer.cull_mode(NONE)`          → `CULL_MODE`
///   * `depth_stencil.depth_test_enable`     → `DEPTH_TEST_ENABLE`
///   * `depth_stencil.depth_write_enable`    → `DEPTH_WRITE_ENABLE`
///   * `depth_stencil.depth_compare_op`      → `DEPTH_COMPARE_OP`
pub(super) const WATER_PIPELINE_DYNAMIC_STATES: [vk::DynamicState; 6] = [
    vk::DynamicState::VIEWPORT,
    vk::DynamicState::SCISSOR,
    vk::DynamicState::DEPTH_TEST_ENABLE,
    vk::DynamicState::DEPTH_WRITE_ENABLE,
    vk::DynamicState::DEPTH_COMPARE_OP,
    // #1071 / F-WAT-11 — caller emits cmd_set_cull_mode(NONE) before
    // the water draw.
    vk::DynamicState::CULL_MODE,
];

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
        // ── Set 2 layout: single STORAGE_IMAGE for the water-caustic
        // accumulator (#1255 / Phase C of #1210). water.frag binding
        // matches: `layout(set = 2, binding = 0, r32ui)
        //          uniform uimage2D waterCausticAccum;`. water.frag
        // writes it and composite.frag reads it (Phase D/E, live).
        let water_caustic_binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let water_caustic_bindings = [water_caustic_binding];
        let water_caustic_set_layout = unsafe {
            // SAFETY: `device` is the live logical device; `water_caustic_bindings` (one FRAGMENT STORAGE_IMAGE) is a stack slice valid for the call; the returned layout is owned by `WaterPipeline` and freed in `destroy()`.
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&water_caustic_bindings),
                    None,
                )
                .context("water-caustic set layout")?
        };

        // Descriptor pool — one STORAGE_IMAGE per FIF slot. Built through
        // the shared DescriptorPoolBuilder for consistency with every other
        // render pass (#1318 / TD3-NEW-C); equivalent to the prior raw
        // create_descriptor_pool (same sizes / max_sets / empty flags).
        let water_caustic_descriptor_pool = match DescriptorPoolBuilder::new()
            .pool(
                vk::DescriptorType::STORAGE_IMAGE,
                super::sync::MAX_FRAMES_IN_FLIGHT as u32,
            )
            .max_sets(super::sync::MAX_FRAMES_IN_FLIGHT as u32)
            .build(device, "water-caustic descriptor pool")
        {
            Ok(p) => p,
            Err(e) => {
                // SAFETY: layout was just created; no descriptor sets
                // allocated from it yet; safe to destroy.
                unsafe { device.destroy_descriptor_set_layout(water_caustic_set_layout, None) };
                return Err(e);
            }
        };

        // Allocate per-FIF descriptor sets up-front. Caller wires the
        // image views via `update_water_caustic_descriptors` once the
        // WaterCausticAccum is created (typically at the same scope as
        // WaterPipeline::new — see context::new). Until then the sets
        // are uninitialised; record_draw should NOT bind them until
        // after the first descriptor write.
        let layouts = vec![water_caustic_set_layout; super::sync::MAX_FRAMES_IN_FLIGHT];
        let water_caustic_descriptor_sets = match unsafe {
            // SAFETY: `device` is live; `water_caustic_descriptor_pool` was just created and sized for MAX_FRAMES_IN_FLIGHT sets of `water_caustic_set_layout`; the AllocateInfo borrows both live handles (via `layouts`) only for the call.
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(water_caustic_descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("water-caustic descriptor sets")
        } {
            Ok(s) => s,
            Err(e) => {
                // SAFETY: pool just created, no sets in flight; safe to destroy.
                unsafe {
                    device.destroy_descriptor_pool(water_caustic_descriptor_pool, None);
                    device.destroy_descriptor_set_layout(water_caustic_set_layout, None);
                }
                return Err(e);
            }
        };

        // ── Pipeline layout: set 0 + set 1 (compat) + set 2 (water-caustic) + push constants ──
        let set_layouts = [
            descriptor_set_layout,
            scene_set_layout,
            water_caustic_set_layout,
        ];
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<WaterPush>() as u32);
        let push_ranges = [push_range];
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&set_layouts)
            .push_constant_ranges(&push_ranges);

        let pipeline_layout = match unsafe {
            // SAFETY: `device` is live; `layout_info` borrows `set_layouts` (three live descriptor-set-layout handles) and `push_ranges`, all stack temporaries valid for the call; the returned layout is owned by `self` and freed in `destroy()`.
            device
                .create_pipeline_layout(&layout_info, None)
                .context("water pipeline layout")
        } {
            Ok(l) => l,
            Err(e) => {
                // SAFETY: sets / pool / layout above all just created;
                // destroy in reverse order to avoid leaking on partial init.
                unsafe {
                    device.destroy_descriptor_pool(water_caustic_descriptor_pool, None);
                    device.destroy_descriptor_set_layout(water_caustic_set_layout, None);
                }
                return Err(e);
            }
        };

        // ── Pipeline ──
        let pipeline = match build_pipeline(device, render_pass, pipeline_cache, pipeline_layout) {
            Ok(p) => p,
            Err(e) => {
                // Clean up the layout on failure so we don't leak.
                unsafe {
                    // SAFETY: `pipeline_layout`, `water_caustic_descriptor_pool`, and `water_caustic_set_layout` were all just created above by this `device` and are unreferenced by any command buffer; destroyed in reverse dependency order on this partial-init failure.
                    device.destroy_pipeline_layout(pipeline_layout, None);
                    device.destroy_descriptor_pool(water_caustic_descriptor_pool, None);
                    device.destroy_descriptor_set_layout(water_caustic_set_layout, None);
                }
                return Err(e);
            }
        };

        log::info!("Water pipeline created (water.vert + water.frag, SRC_ALPHA blend on HDR, cull NONE, depth-write off, 128B push constants, set 2 = water-caustic STORAGE_IMAGE)");

        Ok(Self {
            pipeline,
            pipeline_layout,
            water_caustic_set_layout,
            water_caustic_descriptor_pool,
            water_caustic_descriptor_sets,
        })
    }

    /// Point the per-FIF descriptor sets at the given accumulator's
    /// storage views. Call once after construction AND after every
    /// `WaterCausticAccum::recreate_on_resize` (the view handles
    /// change on resize). Safe to call multiple times — descriptor
    /// updates are idempotent for the same image view.
    ///
    /// `accum_views` must have length `MAX_FRAMES_IN_FLIGHT`, indexed
    /// by frame.
    pub fn update_water_caustic_descriptors(
        &self,
        device: &ash::Device,
        accum_views: &[vk::ImageView],
    ) {
        debug_assert_eq!(
            accum_views.len(),
            super::sync::MAX_FRAMES_IN_FLIGHT,
            "accum_views must be per-FIF",
        );
        for (frame, &view) in accum_views.iter().enumerate() {
            let img_info = [vk::DescriptorImageInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let write =
                write_storage_image(self.water_caustic_descriptor_sets[frame], 0, &img_info);
            // SAFETY: descriptor set / binding / type all match the
            // layout declared above; image_info length == 1 ==
            // descriptor_count.
            unsafe { device.update_descriptor_sets(&[write], &[]) };
        }
    }

    /// Bind the water pipeline and all three descriptor sets — **once per
    /// pass**, before the first [`record_draw`](Self::record_draw).
    ///
    /// #2175 / D2-04 — this used to live inside `record_draw`, so a scene
    /// with N water planes issued N pipeline binds and 3N descriptor-set
    /// binds for state that is identical across every plane. All three
    /// sets are per-frame, not per-plane: sets 0 and 1 are the caller's
    /// bindless-texture and scene sets, and set 2 is indexed by `frame`
    /// alone. Only the push constants, index count, and instance index
    /// vary per plane, and the per-mesh vertex/index binds the caller
    /// interleaves between draws do not disturb pipeline or descriptor
    /// state.
    ///
    /// Why sets 0 and 1 are rebound here at all, rather than inherited
    /// from the triangle path: pre-#1258, `WaterPipeline` assumed the
    /// prior triangle-pipeline's set 0 + set 1 bindings stayed live across
    /// `cmd_bind_pipeline`. That holds only when both pipelines' layouts
    /// are "compatible for set N" per the Vulkan spec, which requires
    /// identical push-constant ranges as well as identical set-layout
    /// handles. Triangle has no push constants; water has 128 B (`WaterPush`
    /// at VERTEX + FRAGMENT). The push-range mismatch un-binds *all*
    /// descriptor sets when `cmd_bind_pipeline(WaterPipeline)` runs, so
    /// every water draw fired without sets 0 + 1 — spammy
    /// `VUID-vkCmdDrawIndexed-None-08600` on Riverwood, the first cell with
    /// real water draws to surface the latent bug. The descriptor handles
    /// are the same ones the triangle path uses; only the pipeline layout
    /// differs. Hoisting the binds out of the per-plane loop does not
    /// weaken that fix — the invalidation happens on the pipeline bind,
    /// which is now also hoisted, so the rebinds still follow it.
    ///
    /// # Safety
    ///
    /// `cmd` must be a valid command buffer in the recording state, inside
    /// the main render pass. All Vulkan handles passed in must outlive the
    /// GPU's consumption of this command buffer.
    pub unsafe fn bind_pass(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        bindless_texture_set: vk::DescriptorSet, // set 0 — passed in by caller
        scene_set: vk::DescriptorSet,            // set 1 — passed in by caller
    ) {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline_layout,
            0,
            &[bindless_texture_set],
            &[],
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline_layout,
            1,
            &[scene_set],
            &[],
        );

        // #1255 / Phase C of #1210 — bind set 2 (water-caustic
        // accumulator storage image). The descriptor view itself is
        // wired by `update_water_caustic_descriptors` at swapchain
        // init / recreate time.
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline_layout,
            2,
            &[self.water_caustic_descriptor_sets[frame]],
            &[],
        );
    }

    /// Record a single water draw into a command buffer that is
    /// already inside the main render pass with dynamic viewport /
    /// scissor / depth state set to valid transparent-draw values.
    ///
    /// Caller is responsible for:
    ///
    /// - calling [`bind_pass`](Self::bind_pass) once before the first
    ///   draw of the pass (#2175 — this function no longer binds the
    ///   pipeline or any descriptor set);
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
    pub unsafe fn record_draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        push: &WaterPush,
        index_count: u32,
        instance_index: u32,
    ) {
        // SAFETY: WaterPush is #[repr(C)] + Copy with only [f32; 4] fields —
        // no padding bytes, no invalid byte patterns, trivially initialised.
        // `push` is a valid shared reference, so the pointer is aligned and
        // non-null; the byte slice is bounded exactly by size_of::<WaterPush>().
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
        // #1255 / Phase C of #1210 — destroy the set 2 descriptor
        // pool + layout. Pool first so the per-FIF descriptor sets
        // (allocated from it) free implicitly. Idempotent via
        // null-handle guards, same policy as the pipeline pair above.
        if self.water_caustic_descriptor_pool != vk::DescriptorPool::null() {
            device.destroy_descriptor_pool(self.water_caustic_descriptor_pool, None);
            self.water_caustic_descriptor_pool = vk::DescriptorPool::null();
            self.water_caustic_descriptor_sets.clear();
        }
        if self.water_caustic_set_layout != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.water_caustic_set_layout, None);
            self.water_caustic_set_layout = vk::DescriptorSetLayout::null();
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
    // #1071 / F-WAT-11 — declare CULL_MODE dynamic to match every other
    // pipeline in the main render pass (#930). The `cull_mode` value
    // here is a no-op at runtime because the dynamic-state override
    // takes precedence; we keep `NONE` as the "intended baseline" for
    // pipeline introspection / debug-layer reads. The caller in
    // `draw_frame` is now required to emit `cmd_set_cull_mode(NONE)`
    // before the water draw (mirrors the triangle/blend pipelines'
    // contract — the per-batch coalescing helper handles this when
    // `last_cull_mode != Some(NONE)`).
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
    // Attachments 1..6 are write-masked off: water never updates
    // the G-buffer (normal / motion / mesh_id / raw_indirect /
    // albedo / reservoir) so SVGF and motion-vector reprojection see
    // only the opaque pass behind the water.
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
    // Water is the canonical transparency-and-composition case: its surface
    // colour comes from reflection and refraction that neither depth nor
    // motion describes, so it writes both FSR masks at full strength with
    // the same MAX accumulation the alpha-blend pipeline uses.
    let fsr_mask_max = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::R)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE)
        .color_blend_op(vk::BlendOp::MAX)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE)
        .alpha_blend_op(vk::BlendOp::MAX);
    // Eight entries to match the main render pass's eight color attachments
    // (0 HDR + 1..5 G-buffer + 6/7 FSR masks). The array length sets
    // `attachmentCount`; it must equal the subpass color-attachment count
    // exactly, or the driver reads missing entries out of bounds / the
    // pipeline fails creation with a count mismatch
    // (VUID-VkGraphicsPipelineCreateInfo-renderPass-07609). See #647 / RP-1;
    // the reservoir attachment was removed under #1583.
    let attachments = [
        hdr_blend,
        masked_off,
        masked_off,
        masked_off,
        masked_off,
        masked_off,
        fsr_mask_max,
        fsr_mask_max,
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

    // #1129 — extracted to a module-level `const` so the contents are
    // inspectable from unit tests without needing a Vulkan device.
    // Forward-compat trap: every static rasterizer / depth-stencil
    // value documented as a "no-op baseline" MUST also appear in this
    // list (or `water_pipeline_dynamic_states_cover_documented_no_ops`
    // below fires).
    let dynamic_states = WATER_PIPELINE_DYNAMIC_STATES;
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
        // SAFETY: `device` is live; `infos` is a stack slice whose GraphicsPipelineCreateInfo references live `stages` (the `vert`/`frag` modules), `pipeline_layout`, and `render_pass`, all valid for the call; the returned pipeline is owned by the caller.
        device
            .create_graphics_pipelines(pipeline_cache, &infos, None)
            .map_err(|(_, err)| err)
            .context("water graphics pipeline")?
    };

    unsafe {
        // SAFETY: `vert`/`frag` were created by this `device` earlier in this fn; their code is now baked into the built pipeline, so neither is referenced by any live pipeline or command buffer.
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
    fn water_push_layout_is_128_bytes() {
        assert_eq!(std::mem::size_of::<WaterPush>(), 128);
        assert_eq!(std::mem::align_of::<WaterPush>(), 4);
    }

    /// #1129 — forward-compat trap. Every "no-op baseline" the water
    /// rasterizer / depth-stencil sets statically MUST also appear in
    /// `WATER_PIPELINE_DYNAMIC_STATES` — otherwise the static value
    /// silently wins at runtime and the comment lies.
    #[test]
    fn water_pipeline_dynamic_states_cover_documented_no_ops() {
        let states: &[vk::DynamicState] = &WATER_PIPELINE_DYNAMIC_STATES;
        // The documented no-op pairings:
        for required in [
            vk::DynamicState::CULL_MODE,
            vk::DynamicState::DEPTH_TEST_ENABLE,
            vk::DynamicState::DEPTH_WRITE_ENABLE,
            vk::DynamicState::DEPTH_COMPARE_OP,
        ] {
            assert!(
                states.contains(&required),
                "WATER_PIPELINE_DYNAMIC_STATES missing {required:?} — \
                 a static value documented as a no-op baseline is no longer \
                 overridden by the dynamic state. Either drop the static \
                 value or add the dynamic-state declaration."
            );
        }
        // VIEWPORT / SCISSOR are also required by every pipeline in the
        // main render pass.
        assert!(states.contains(&vk::DynamicState::VIEWPORT));
        assert!(states.contains(&vk::DynamicState::SCISSOR));
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
            wireframe: false,
            flat_shading: false,
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
            ior: 1.5,        // #1248 — test fixture; default dielectric.
            subsurface: 0.0, // #1249 — Disney diffuse off in test fixture.
            sheen: 0.0,
            sheen_tint: 0.0,
            anisotropic: 0.0, // #1250
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
            greyscale_lut_index: 0,
            supplemental_texture_indices: [0; 12],
            translucency_subsurface_color: [0.0; 3],
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
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
                tint_reflect: [0.0; 4],
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

/// #2175 / D2-04 — the pipeline + descriptor binds are hoisted out of the
/// per-plane draw loop.
///
/// These are static source assertions because the failure mode is invisible
/// to `cargo test`: it needs a real device, a cell with water, and the
/// validation layer to observe. What *is* checkable without a device is the
/// structural property the fix rests on — `record_draw` binds nothing, and
/// the one caller binds once ahead of its loop. A future edit that pushes a
/// bind back inside the loop (or drops `bind_pass` entirely, which would
/// resurrect the #1258 unbound-descriptor-set crash) fails here.
#[cfg(test)]
mod pass_bind_hoist_tests {
    const WATER_RS: &str = include_str!("water.rs");
    const GEOMETRY_PASS_RS: &str = include_str!("context/geometry_pass.rs");

    /// Everything before the test modules — needed so the needles below
    /// cannot match this file's own assertion strings.
    fn water_production_src() -> &'static str {
        WATER_RS
            .split_once("\n#[cfg(test)]")
            .expect("water.rs lost its test modules")
            .0
    }

    /// Body of `record_draw`, from its signature to the closing brace at
    /// method indentation.
    fn record_draw_body() -> &'static str {
        let src = water_production_src();
        let start = src
            .find("pub unsafe fn record_draw(")
            .expect("record_draw disappeared from water.rs");
        let rest = &src[start..];
        let end = rest
            .find("\n    }")
            .expect("record_draw has no closing brace at method indentation");
        &rest[..end]
    }

    #[test]
    fn record_draw_binds_neither_pipeline_nor_descriptor_sets() {
        let body = record_draw_body();
        for needle in ["cmd_bind_pipeline", "cmd_bind_descriptor_sets"] {
            assert!(
                !body.contains(needle),
                "record_draw calls `{needle}` again — that is per-plane state \
                 the pass-level `bind_pass` already covers (#2175). If a new \
                 per-plane descriptor really is needed, bind only that set, \
                 not the pipeline and all three.",
            );
        }
        assert!(
            body.contains("cmd_draw_indexed"),
            "record_draw no longer issues a draw — the extraction above is \
             matching the wrong function",
        );
    }

    #[test]
    fn bind_pass_binds_the_pipeline_and_all_three_sets() {
        let src = water_production_src();
        let start = src.find("pub unsafe fn bind_pass(").expect(
            "bind_pass disappeared — record_draw would draw with no pipeline bound (#1258)",
        );
        let body = &src[start..];
        let end = body
            .find("\n    }")
            .expect("bind_pass has no closing brace at method indentation");
        let body = &body[..end];
        assert!(body.contains("cmd_bind_pipeline"));
        assert_eq!(
            body.matches("cmd_bind_descriptor_sets").count(),
            3,
            "bind_pass must bind all three sets (0 bindless, 1 scene, \
             2 water-caustic) — a missing rebind is the #1258 crash, since \
             water's push-constant range makes its layout incompatible with \
             the triangle pipeline's for every set",
        );
    }

    /// The bind must precede the loop, not sit inside it.
    #[test]
    fn geometry_pass_binds_once_before_the_per_plane_loop() {
        let bind = GEOMETRY_PASS_RS
            .find("water.bind_pass(")
            .expect("geometry_pass no longer calls bind_pass");
        let loop_head = GEOMETRY_PASS_RS
            .find("for wc in water_commands {")
            .expect("the per-plane water loop changed shape");
        assert!(
            bind < loop_head,
            "water.bind_pass (byte {bind}) must be hoisted above the \
             per-plane loop (byte {loop_head}) — binding inside it is the \
             regression #2175 removed",
        );
    }
}
