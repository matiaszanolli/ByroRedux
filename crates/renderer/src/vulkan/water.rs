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
//! - a growable per-frame [`GpuWaterParams`] SSBO carrying the full authored material
//!   payload, selected by a compact 16-byte [`WaterPush`] draw index;
//! - SRC_ALPHA / ONE_MINUS_SRC_ALPHA blend on HDR attachment 0;
//!   attachments 1..=5 (normal, motion, mesh_id, raw_indirect, albedo) are
//!   masked off (`color_write_mask = 0`) so water never pollutes the
//!   G-buffer feeding SVGF / motion-vector reprojection, while attachments
//!   6 and 7 — the FSR reactive and transparency-and-composition masks —
//!   ARE written, MAX-blended at full strength: water's surface colour
//!   comes from reflection and refraction that neither depth nor motion
//!   describes, so the upscaler has to be told. The blend table in
//!   `create_water_pipeline` is the authority here, and
//!   `attachment_doc_pin_tests` holds this list to it — before #3604 the
//!   two disagreed and this one claimed water wrote no FSR mask at all;
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
//!    pipeline's fragment push-constant range makes its layout
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

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{write_storage_buffer, write_storage_image, DescriptorPoolBuilder};
use super::pipeline::load_shader_module;
use crate::vertex::Vertex;
use anyhow::{Context, Result};
use ash::vk;

// `pub(crate)` so the scene-descriptor validation in `scene_buffer::buffers`
// can pin the water shaders against the shared set=1 layout (#1561 STARTUP-
// VALIDATION — water reuses set 1 with TLAS at binding 2 as optional).
pub(crate) const WATER_VERT_SPV: &[u8] = include_bytes!("../../shaders/water.vert.spv");
pub(crate) const WATER_FRAG_SPV: &[u8] = include_bytes!("../../shaders/water.frag.spv");

/// Canonical GPU material payload for one water draw. Layout matches
/// `WaterParams` in both water shaders exactly (23 std430 vec4 slots).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GpuWaterParams {
    /// x = time (seconds since cell load), y = `WaterKind` as f32,
    /// z = foam_strength (0..1), w = ior.
    pub timing: [f32; 4],
    /// xyz = flow direction (unit vector), w = flow speed (wu/s).
    pub flow: [f32; 4],
    /// rgb = shallow_color (linear), a = fog_near — the near plane of the
    /// absorption ramp. Consumed by `absorbWaterColumn` since #2785; it
    /// travelled the whole EXAL water arm unread before that, so every
    /// water body in every game shared one hard-coded curve. Wiring it up
    /// cost no payload space (the value was already uploaded here).
    /// `misc.w` is consumed by authored sun-specular power; new material
    /// fields should extend this SSBO record rather than the draw selector.
    pub shallow: [f32; 4],
    /// rgb = deep_color (linear), a = fog_far — the far plane of that same
    /// ramp.
    pub deep: [f32; 4],
    /// xy = scroll_a, zw = scroll_b (wu/s).
    pub scroll: [f32; 4],
    /// xy = scroll_c (wu/s), zw = underwater fog near/far.
    pub scroll_c: [f32; 4],
    /// x = uv_scale_a, y = uv_scale_b, z = shoreline_width,
    /// w = wave_amplitude (WATR `DATA` wave_amplitude, `#2240` — was
    /// reserved; reflectivity moved to `tint_reflect.w` in #1069).
    pub tune: [f32; 4],
    /// x = fresnel_f0, y = wave_frequency (WATR `DATA` wave_frequency,
    /// Hz, `#2240` — was reserved), z = normal_map_index bit-cast to
    /// f32 (shader does `floatBitsToUint`), w = WATR `Sun Specular Power`
    /// (Blinn-Phong exponent for the direct-sun glint).
    pub misc: [f32; 4],
    /// xyz = reflection_tint (WATR `reflection_color`, including legacy HDR
    /// multiplier; tints the geometry-hit colour in `traceWaterRay`). w = reflectivity
    /// (0..1 — fresnel multiplier; moved here from `tune.w` in #1069).
    pub tint_reflect: [f32; 4],
    /// xyz = bindless indices for authored NAM2/NAM3/NAM4 noise layers;
    /// w = WATR.ANAM opacity packed as f32 bits.
    pub noise_indices: [u32; 4],
    /// x = authored NAM4 UV scale; yzw = authored NAM2/3/4 amplitude
    /// scales, keeping this extension vec4-aligned.
    pub detail: [f32; 4],
    /// x = authored Skyrim noise-falloff distance; y = Blend Normals gate;
    /// z = Starfield surface roughness; w = Skyrim Specular Radius.
    pub noise_falloff: [f32; 4],
    /// x/y/z = shallow/deep/surface-effect normal falloff multipliers;
    /// w = packed legacy rain velocity/falloff/dampener controls.
    /// A zero triplet preserves the legacy always-on normal response.
    pub normal_falloff: [f32; 4],
    /// x = displacement starting size, y = radial falloff, z = dampener,
    /// w = legacy rain-simulator starting ripple size.
    pub displacement: [f32; 4],
    /// Authored depth-response weights: reflections, refraction, normals,
    /// and specular lighting.
    pub depth: [f32; 4],
    /// Authored effect controls: refraction magnitude, local specular power,
    /// reflection magnitude, and sun-specular magnitude.
    pub effects: [f32; 4],
    /// xyz = Starfield per-channel extinction coefficients;
    /// zero triplet is the legacy scalar-fog sentinel. w = precipitation ×
    /// authored rain-response (0..4), driving rain-surface response. Not a
    /// free slot — same trap as `VolumetricsParams.render_origin.w` (#1928)
    /// and `GpuCamera.render_origin.w` (#2164). The lane is live even though
    /// the growable SSBO removes the old fixed-capacity pressure.
    pub absorption: [f32; 4],
    /// Starfield water-column concentrations: phytoplankton, sediment,
    /// yellow matter, and oceanness.
    pub concentration: [f32; 4],
    /// xy = transient ripple center in world XZ, z = intensity, w = radius.
    /// A zero intensity disables the event-driven normal disturbance.
    pub ripple: [f32; 4],
    /// rgb = underwater post-process tint, a = authored underwater fog
    /// amount. The near/far ramp is packed in `scroll_c.zw`.
    pub underwater: [f32; 4],
    /// x/y = shallow/deep alpha, z/w = shallow/deep distance thresholds.
    /// All zero is the legacy constant-opacity sentinel.
    pub alpha: [f32; 4],
    /// xy = authored mesh-water UV offset; z carries the optional
    /// mesh-water flow-map bindless index as integer bits (`u32::MAX` =
    /// none); w carries its authored tile scale. Cell WATR surfaces upload
    /// the `u32::MAX` index and a neutral scale, not zero. Not a free slot —
    /// same trap as `VolumetricsParams.render_origin.w` (#1928) and
    /// `GpuCamera.render_origin.w` (#2164).
    pub uv_offset: [f32; 4],
    /// x = FO4+/Creation-2 WATR `Depth Amount`; yzw reserved. This remains
    /// separate from `shallow.a`/`deep.a`, which are actual fog distances.
    pub optical: [f32; 4],
}

impl GpuWaterParams {
    /// Pack `normal_map_index` as a float bit-pattern. The shader
    /// recovers the integer via `floatBitsToUint(push.misc.z)`.
    /// `u32::MAX` denotes "no normal map — use procedural noise."
    #[inline]
    pub fn pack_normal_index(idx: u32) -> f32 {
        f32::from_bits(idx)
    }
}

const _: () = assert!(
    std::mem::size_of::<GpuWaterParams>() == 368,
    "GpuWaterParams must remain 23 std430 vec4 slots"
);

/// Per-draw selector for the material array uploaded once per frame.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WaterPush {
    pub water_index: u32,
    pub _reserved: [u32; 3],
}

const _: () = assert!(
    std::mem::size_of::<WaterPush>() == 16,
    "WaterPush must match the shader's 16-byte push block"
);

/// Small initial allocation; each frame slot grows geometrically when a cell
/// exposes more water surfaces. This is a capacity hint, never a draw limit.
const INITIAL_WATER_DRAW_CAPACITY: usize = 8;

/// One water surface to draw in the current frame.
///
/// Built by the app's per-frame render code from `WaterPlane` ECS
/// entities and passed alongside [`DrawCommand`] to
/// [`VulkanContext::draw_frame`]. Kept separate from `DrawCommand` so the
/// water-only material payload doesn't bloat every regular draw.
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
    /// Material payload uploaded into this frame's compact water SSBO.
    pub params: GpuWaterParams,
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
/// The graphics pipeline is recreated with the swapchain render pass, while
/// its screen-sized per-FIF `WaterCausticAccum` is recreated separately and
/// set 2 rebound to the new views (or its storage fallback) by the resize
/// path. `water_caustic_rebind_is_not_gated_on_accumulator_presence` and
/// `init_path_water_set_2_falls_back_and_drops_the_stale_comment` pin that
/// split lifecycle (#3126).
pub struct WaterPipeline {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    /// Set 2 descriptor layout — water-caustic STORAGE_IMAGE plus the
    /// per-frame water-parameter SSBO. Bound at
    /// `record_draw` time from the per-FIF `descriptor_sets[frame]`.
    pub(super) water_caustic_set_layout: vk::DescriptorSetLayout,
    /// Pool that allocates the per-FIF descriptor sets above.
    pub(super) water_caustic_descriptor_pool: vk::DescriptorPool,
    /// Per-FIF descriptor sets pointing at the matching slot of
    /// `WaterCausticAccum`. Populated by `update_water_caustic_descriptors`
    /// at swapchain init / recreate time so the per-frame draw can bind
    /// them directly without re-writing.
    pub(super) water_caustic_descriptor_sets: Vec<vk::DescriptorSet>,
    /// One host-visible, geometrically growable SSBO per frame-in-flight.
    param_buffers: Vec<GpuBuffer>,
    /// Element capacity of each per-FIF buffer.
    param_capacities: Vec<usize>,
    /// Tracks whether the current slot received a successful upload.
    params_ready: Vec<bool>,
    /// Reused packing storage so the per-frame upload does not allocate.
    param_scratch: Vec<GpuWaterParams>,
    /// Retained so the allocator-independent context teardown can still
    /// release these buffers before the allocator is unwrapped.
    allocator: Option<SharedAllocator>,
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
        allocator: &SharedAllocator,
        render_pass: vk::RenderPass,
        pipeline_cache: vk::PipelineCache,
        descriptor_set_layout: vk::DescriptorSetLayout,
        scene_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        // ── Set 2 layout: water-caustic STORAGE_IMAGE + water params SSBO
        // accumulator (#1255 / Phase C of #1210). water.frag binding
        // matches: `layout(set = 2, binding = 0, r32ui)
        //          uniform uimage2D waterCausticAccum;`. water.frag
        // writes it and composite.frag reads it (Phase D/E, live).
        let water_caustic_binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let water_params_binding = vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT);
        let water_caustic_bindings = [water_caustic_binding, water_params_binding];
        let water_caustic_set_layout = unsafe {
            // SAFETY: `device` is live; both bindings are a stack slice valid
            // for this call; the returned layout is owned by `WaterPipeline`.
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&water_caustic_bindings),
                    None,
                )
                .context("water-caustic set layout")?
        };

        // Descriptor pool — one STORAGE_IMAGE + one SSBO per FIF slot. Built through
        // the shared DescriptorPoolBuilder for consistency with every other
        // render pass (#1318 / TD3-NEW-C); equivalent to the prior raw
        // create_descriptor_pool (same sizes / max_sets / empty flags).
        let water_caustic_descriptor_pool = match DescriptorPoolBuilder::new()
            .pool(
                vk::DescriptorType::STORAGE_IMAGE,
                super::sync::MAX_FRAMES_IN_FLIGHT as u32,
            )
            .pool(
                vk::DescriptorType::STORAGE_BUFFER,
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

        let param_buffer_size =
            (INITIAL_WATER_DRAW_CAPACITY * std::mem::size_of::<GpuWaterParams>()) as vk::DeviceSize;
        let mut param_buffers = Vec::with_capacity(super::sync::MAX_FRAMES_IN_FLIGHT);
        for _ in 0..super::sync::MAX_FRAMES_IN_FLIGHT {
            match GpuBuffer::create_host_visible(
                device,
                allocator,
                param_buffer_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            ) {
                Ok(buffer) => param_buffers.push(buffer),
                Err(error) => {
                    for buffer in &mut param_buffers {
                        buffer.destroy(device, allocator);
                    }
                    // SAFETY: all handles were created by `device`; pipeline
                    // construction has not exposed them to a command buffer.
                    unsafe {
                        device.destroy_pipeline(pipeline, None);
                        device.destroy_pipeline_layout(pipeline_layout, None);
                        device.destroy_descriptor_pool(water_caustic_descriptor_pool, None);
                        device.destroy_descriptor_set_layout(water_caustic_set_layout, None);
                    }
                    return Err(error.context("water parameter SSBOs"));
                }
            }
        }

        for (frame, buffer) in param_buffers.iter().enumerate() {
            let info = [vk::DescriptorBufferInfo {
                buffer: buffer.buffer,
                offset: 0,
                range: param_buffer_size,
            }];
            let write = write_storage_buffer(water_caustic_descriptor_sets[frame], 1, &info);
            // SAFETY: binding 1 is a single STORAGE_BUFFER descriptor in the
            // matching live set; `info` names the just-created live buffer.
            unsafe { device.update_descriptor_sets(&[write], &[]) };
        }

        log::info!("Water pipeline created (water.vert + water.frag, SRC_ALPHA blend on HDR, cull NONE, depth-write off, 16B push selector, set 2 = caustic image + growable water params SSBO)");

        Ok(Self {
            pipeline,
            pipeline_layout,
            water_caustic_set_layout,
            water_caustic_descriptor_pool,
            water_caustic_descriptor_sets,
            param_buffers,
            param_capacities: vec![INITIAL_WATER_DRAW_CAPACITY; super::sync::MAX_FRAMES_IN_FLIGHT],
            params_ready: vec![false; super::sync::MAX_FRAMES_IN_FLIGHT],
            param_scratch: Vec::with_capacity(8),
            allocator: Some(allocator.clone()),
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

    /// Upload all per-water material records before the frame's bulk
    /// HOST→shader barrier. The caller has already waited for this frame
    /// slot's fence, so growing its buffer and rewriting its descriptor cannot
    /// race an in-flight command buffer.
    pub fn upload_params(
        &mut self,
        device: &ash::Device,
        frame: usize,
        commands: &[WaterDrawCommand],
    ) -> Result<()> {
        self.params_ready[frame] = false;
        if commands.is_empty() {
            return Ok(());
        }
        if commands.len() > self.param_capacities[frame] {
            let new_capacity = commands.len().next_power_of_two();
            let new_size = (new_capacity * std::mem::size_of::<GpuWaterParams>()) as vk::DeviceSize;
            let allocator = self
                .allocator
                .as_ref()
                .expect("water allocator missing before teardown")
                .clone();
            let new_buffer = GpuBuffer::create_host_visible(
                device,
                &allocator,
                new_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )
            .context("grow water parameter SSBO")?;
            let info = [vk::DescriptorBufferInfo {
                buffer: new_buffer.buffer,
                offset: 0,
                range: new_size,
            }];
            let write = write_storage_buffer(self.water_caustic_descriptor_sets[frame], 1, &info);
            // SAFETY: the frame-slot fence was waited before upload; binding 1
            // is a live STORAGE_BUFFER descriptor and `new_buffer` outlives it.
            unsafe { device.update_descriptor_sets(&[write], &[]) };
            let mut old_buffer = std::mem::replace(&mut self.param_buffers[frame], new_buffer);
            old_buffer.destroy(device, &allocator);
            self.param_capacities[frame] = new_capacity;
        }
        self.param_scratch.clear();
        self.param_scratch
            .extend(commands.iter().map(|command| command.params));
        self.param_buffers[frame].write_mapped(device, &self.param_scratch)?;
        self.params_ready[frame] = true;
        Ok(())
    }

    #[inline]
    pub fn params_ready(&self, frame: usize) -> bool {
        self.params_ready[frame]
    }

    /// Bind the water pipeline and all three descriptor sets — **once per
    /// pass**, before the first [`record_draw`](Self::record_draw).
    ///
    /// #2175 / D2-04 — this used to live inside `record_draw`, so a scene
    /// with N water planes issued N pipeline binds and 3N descriptor-set
    /// binds for state that is identical across every plane. All three
    /// sets are per-frame, not per-plane: sets 0 and 1 are the caller's
    /// bindless-texture and scene sets, and set 2 is indexed by `frame`
    /// alone. Only the push selector, index count, and instance index
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
    /// handles. Triangle has no push constants; water has a 16 B `WaterPush`
    /// at FRAGMENT. The push-range mismatch un-binds *all*
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
        water_index: u32,
        index_count: u32,
        instance_index: u32,
    ) {
        let push = WaterPush {
            water_index,
            _reserved: [0; 3],
        };
        // SAFETY: WaterPush is #[repr(C)] + Copy and contains only u32 values.
        let bytes = std::slice::from_raw_parts(
            (&push as *const WaterPush) as *const u8,
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
        if let Some(allocator) = self.allocator.as_ref() {
            for buffer in &mut self.param_buffers {
                buffer.destroy(device, allocator);
            }
        }
        self.param_buffers.clear();
        self.param_capacities.clear();
        self.params_ready.clear();
        self.param_scratch.clear();
        self.allocator = None;
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
    // Attachments 1..=5 are write-masked off: water never updates
    // the G-buffer (normal / motion / mesh_id / raw_indirect /
    // albedo) so SVGF and motion-vector reprojection see only the
    // opaque pass behind the water. Attachments 6 and 7 (the FSR
    // masks) are written — see below.
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
    fn water_gpu_contract_layouts_are_stable() {
        assert_eq!(std::mem::size_of::<GpuWaterParams>(), 368);
        assert_eq!(std::mem::align_of::<GpuWaterParams>(), 4);
        assert_eq!(std::mem::size_of::<WaterPush>(), 16);
        assert_eq!(std::mem::align_of::<WaterPush>(), 4);
        assert_eq!(INITIAL_WATER_DRAW_CAPACITY, 8);
    }

    #[test]
    fn water_vertex_shader_keeps_the_full_material_array_stride() {
        let src = include_str!("../../shaders/water.vert");
        assert!(
            src.contains("vec4 absorption;")
                && src.contains("vec4 concentration;")
                && src.contains("vec4 noise_falloff;")
                && src.contains("vec4 normal_falloff;")
                && src.contains("vec4 displacement;")
                && src.contains("vec4 ripple;")
                && src.contains("vec4 underwater;")
                && src.contains("vec4 alpha;")
                && src.contains("vec4 optical;"),
            "water.vert must declare the trailing material slots so indexed\n\
             WaterParams elements retain the 368-byte std430 stride used by\n\
             Rust and water.frag"
        );
        assert!(
            src.contains("float dHeightDx")
                && src.contains("float dHeightDz")
                && src.contains("localNormal = normalize(vec3(-dHeightDx"),
            "water.vert must derive normals from the authored vertex-wave slope"
        );
        assert!(
            src.contains("float waveRateA = clamp(length(water.scroll.xy)")
                && src.contains("float waveRateB = clamp(length(water.scroll.zw)")
                && src.contains("frequency * waveRateA")
                && src.contains("frequency * waveRateB"),
            "water.vert must let the shared weather/water scroll velocity drive
             the geometric wave rate as well as fragment normal motion"
        );
    }

    #[test]
    fn water_fragment_shader_uses_authored_underwater_response() {
        let src = include_str!("../../shaders/water.frag");
        assert!(src.contains("vec4 underwater;"));
        assert!(src.contains("cameraUnderwater"));
        assert!(src.contains("dot(cameraPos.xyz - vWorldPos, Nsurface) < 0.0"));
        assert!(src.contains("push.scroll_c.z"));
        assert!(src.contains("push.underwater.rgb"));
    }

    #[test]
    fn water_fragment_shader_honors_authored_noise_falloff() {
        let src = include_str!("../../shaders/water.frag");
        assert!(src.contains("float noiseFalloff = push.noise_falloff.x"));
        assert!(src.contains("distance(vWorldPos, cameraPos.xyz) / noiseFalloff"));
        assert!(src.contains("mix(vec3(0.0, 0.0, 1.0), nMix, normalFade)"));
        assert!(src.contains("push.normal_falloff.x"));
        assert!(src.contains("shallowNormalScale"));
        assert!(src.contains("surfaceNormalScale"));
        assert!(src.contains("float authoredStart = max(push.displacement.x, 0.0)"));
        assert!(src.contains("exp(-distanceToCenter / authoredDampener)"));
        assert!(src.contains("float rainScale = 1.7"));
        assert!(src.contains("1.7 / max(push.displacement.w, 0.25)"));
        assert!(src.contains("uint rainPacked = floatBitsToUint(push.normal_falloff.w)"));
        assert!(src.contains("rainControls.z / 4.0"));
    }

    #[test]
    fn water_fragment_shader_uses_skyrim_blend_bit_only_for_third_layer() {
        let src = include_str!("../../shaders/water.frag");
        assert!(src.contains("push.noise_falloff.y > 0.5"));
        assert!(src.contains(
            "if (blendAuthoredNormals && (kind == WATER_RAPIDS || hasAuthoredThirdLayer))"
        ));
        assert!(!src.contains("if (!blendAuthoredNormals)"));
    }

    #[test]
    fn water_fragment_shader_skips_authored_disabled_mesh_refraction() {
        let src = include_str!("../../shaders/water.frag");
        assert!(
            src.contains("kind != WATER_WATERFALL && kind != WATER_LAVA && push.effects.x >= 0.0")
        );
        assert!(src.contains("float refractionNormalWeight = push.effects.x > 0.0"));
    }

    #[test]
    fn water_fragment_shader_keeps_two_layers_when_skyrim_blend_bit_is_clear() {
        let src = include_str!("../../shaders/water.frag");
        assert!(src.contains("nMix = normalize(nA + nB);"));
        assert!(!src.contains("if (!blendAuthoredNormals)"));
    }

    #[test]
    fn authored_lava_bypasses_water_refraction_underwater_and_foam_paths() {
        let src = include_str!("../../shaders/water.frag");
        assert!(src.contains("kind != WATER_WATERFALL && kind != WATER_LAVA"));
        assert!(
            src.contains("bool cameraUnderwater = kind != WATER_WATERFALL && kind != WATER_LAVA")
        );
    }

    #[test]
    fn water_fragment_shader_preserves_zero_authored_opacity() {
        let src = include_str!("../../shaders/water.frag");
        assert!(
            src.contains(
                "float authoredOpacity = clamp(uintBitsToFloat(push.noise_indices.w), 0.0, 1.0)"
            ),
            "ANAM=0 must remain a valid transparent-water value"
        );
        assert!(
            src.contains("float alpha = baseAlpha <= 0.0") && src.contains("? 0.0"),
            "opacity zero must bypass grazing and foam alpha boosts"
        );
    }

    #[test]
    fn water_fragment_shader_uses_authored_depth_alpha_ramp() {
        let src = include_str!("../../shaders/water.frag");
        assert!(src.contains("push.alpha.x > 0.0 || push.alpha.y > 0.0"));
        assert!(src.contains("float alphaT = clamp((refrDist - push.alpha.z)"));
        assert!(src.contains("mix(clamp(push.alpha.x, 0.0, 1.0)"));
        assert!(src.contains("baseAlpha = mix(clamp(push.alpha.x, 0.0, 1.0)"));
        assert!(!src.contains("baseAlpha *= mix(clamp(push.alpha.x, 0.0, 1.0)"));
    }

    #[test]
    fn water_rapids_third_layer_uses_world_xz_flow() {
        let src = include_str!("../../shaders/water.frag");
        assert!(
            src.contains("vec2(push.flow.x, push.flow.z) * push.flow.w * 2.0"),
            "rapids' third normal layer must project the Y-up flow vector onto XZ"
        );
        assert!(
            !src.contains("push.flow.xy * push.flow.w * 2.0"),
            "rapids must not use world XY, whose Y component is zero for horizontal water"
        );
    }

    #[test]
    fn water_foam_flow_has_a_degenerate_tangent_fallback() {
        let src = include_str!("../../shaders/water.frag");
        assert!(
            src.contains("vec3 perpRaw = cross(vWorldNormal, flowDir)")
                && src.contains("length(perpRaw) > 1.0e-5"),
            "parallel flow and surface vectors must not normalize a zero tangent"
        );
    }

    #[test]
    fn water_shader_builds_a_finite_basis_for_degenerate_mesh_tangents() {
        let src = include_str!("../../shaders/water.frag");
        assert!(
            src.contains("vec3 NsurfaceRaw = vWorldNormal")
                && src.contains(
                    "vec3 tangentProjected = tangentRaw - Nsurface * dot(tangentRaw, Nsurface)"
                )
                && src.contains("vec3 fallbackAxis = abs(Nsurface.y) < 0.9"),
            "legacy water meshes need a deterministic tangent-frame fallback"
        );
    }

    #[test]
    fn water_caustics_keep_the_top_side_wave_normal_below_the_surface() {
        let src = include_str!("../../shaders/water.frag");
        let normal_decl = src
            .find("vec3 causticNormal = Nperturbed;")
            .expect("water.frag must preserve the pre-view-flip wave normal");
        let orientation = src
            .find("if (!viewFromPositiveSide)")
            .expect("water.frag must retain the camera-side orientation branch");
        let caustic_refract = src
            .find("refract(-sunDir, causticNormal")
            .expect("water caustic transport must use the top-side normal");
        assert!(
            normal_decl < orientation && orientation < caustic_refract,
            "camera-side normal flipping must not feed water caustic Snell transport"
        );
        assert!(
            src.contains("dot(causticNormal, Nsurface)"),
            "water caustic normal needs a top-side stability clamp"
        );
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
            let packed = GpuWaterParams::pack_normal_index(v);
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
            glass_fresnel_color: [1.0; 3],
            glass_refraction_scale: 0.05,
            glass_blur_scale: 0.4,
            glass_blur_scale_factor: 1.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            fresnel_power: 5.0,
            grayscale_to_palette_scale: 1.0,
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
            supplemental_texture_indices: [0; 16],
            translucency_subsurface_color: [0.0; 3],
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
            shader_color: [0.0; 3],
            shader_float: 0.0,
            is_water,
        }
    }

    fn water_cmd(mesh_handle: u32, instance_index: u32) -> WaterDrawCommand {
        WaterDrawCommand {
            mesh_handle,
            instance_index,
            params: GpuWaterParams {
                timing: [0.0; 4],
                flow: [0.0; 4],
                shallow: [0.0; 4],
                deep: [0.0; 4],
                scroll: [0.0; 4],
                scroll_c: [0.0; 4],
                tune: [0.0; 4],
                misc: [0.0; 4],
                tint_reflect: [0.0; 4],
                noise_indices: [u32::MAX; 4],
                detail: [0.0; 4],
                noise_falloff: [0.0; 4],
                normal_falloff: [0.0; 4],
                displacement: [0.0; 4],
                depth: [1.0; 4],
                effects: [0.0, 0.0, 1.0, 1.0],
                absorption: [0.0; 4],
                concentration: [0.0; 4],
                ripple: [0.0; 4],
                underwater: [0.0; 4],
                alpha: [0.0; 4],
                uv_offset: [0.0; 4],
                optical: [0.0; 4],
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
            .find("water_commands.iter().enumerate()")
            .expect("the per-plane water loop changed shape");
        assert!(
            bind < loop_head,
            "water.bind_pass (byte {bind}) must be hoisted above the \
             per-plane loop (byte {loop_head}) — binding inside it is the \
             regression #2175 removed",
        );
    }
}

/// #2785 (REN-D15-04) — the underwater absorption ramp, mirrored from
/// `water.frag`'s `absorbWaterColumn` so its shape can be pinned without a
/// GPU. GLSL is the source of truth; this exists to make "wiring `fog_near`
/// in does not change vanilla water" a checked claim rather than a hope.
#[cfg(test)]
mod absorption_ramp_tests {
    /// `t` as `absorbWaterColumn` computes it. Kept to the same three
    /// statements as the shader so a divergence is visible by inspection.
    fn ramp_t(hit_dist: f32, fog_near: f32, fog_far: f32) -> f32 {
        let span = (fog_far - fog_near).max(1.0);
        ((hit_dist - fog_near) / span).clamp(0.0, 1.0)
    }

    /// The pre-#2785 curve: `fog_near` unread, `t = hitDist / fog_far`.
    fn legacy_t(hit_dist: f32, fog_far: f32) -> f32 {
        (hit_dist / fog_far.max(1.0)).clamp(0.0, 1.0)
    }

    /// The compatibility claim that makes this change safe to ship without a
    /// capture: with `fog_near == 0` — which is what Skyrim, FNV and
    /// Oblivion author for nearly every water body — the new ramp is
    /// *identical* to the curve it replaces. Vanilla water cannot shift.
    #[test]
    fn a_zero_near_plane_reproduces_the_legacy_curve_exactly() {
        // fog_far values taken from real records: MarkarthWater 110,
        // BlackreachWater 290, NVMurkyWater2 94, Vault101eWaterType 1.
        for fog_far in [1.0f32, 94.0, 110.0, 290.0, 4710.0] {
            for hit in [0.0f32, 0.5, 7.0, 50.0, 110.0, 1000.0, 1.0e5] {
                assert_eq!(
                    ramp_t(hit, 0.0, fog_far),
                    legacy_t(hit, fog_far),
                    "fog_near=0 must be a no-op (fog_far={fog_far}, hit={hit})"
                );
            }
        }
    }

    /// And it does something for the records that DO author a near plane:
    /// clear water out to `fog_near`, then the same ramp compressed into
    /// the remaining span. `HorseTroughWater01` (220/4710) is the strongest
    /// vanilla case.
    #[test]
    fn an_authored_near_plane_keeps_the_column_clear_until_it() {
        let (near, far) = (220.0f32, 4710.0);
        assert_eq!(ramp_t(0.0, near, far), 0.0);
        assert_eq!(ramp_t(near, near, far), 0.0, "clear right up to fog_near");
        assert!(
            ramp_t(near + 1.0, near, far) > 0.0,
            "and absorbing immediately past it"
        );
        assert_eq!(ramp_t(far, near, far), 1.0, "fully deep by fog_far");
        // The legacy curve was already 4.7% absorbed at the near plane and
        // reached only 1.0 at fog_far by coincidence of the same clamp.
        assert!(
            legacy_t(near, far) > 0.0,
            "premise: the old curve did not honour the near plane"
        );
    }

    /// The Rust mirrors above are only worth anything if they still
    /// describe the GLSL. Pin the two expressions they model, and #2784's
    /// sibling guard in the same file: the caustic splat must reject on the
    /// INTEGER pixel against the image size — the way `caustic_splat.comp`
    /// does — rather than on `lessThanEqual(uv01, vec2(1.0))`, which admits
    /// `uv01 == 1.0` and writes one texel past the edge.
    #[test]
    fn the_shader_still_computes_what_these_tests_model() {
        let src = include_str!("../../shaders/water.frag");

        // #2782 — storage-image deposits must happen only after the depth
        // test accepts the water fragment.
        assert!(
            src.contains("layout(early_fragment_tests) in;"),
            "water.frag must request early fragment tests before its storage-image writes"
        );

        // Keep the checked-in SPIR-V in lockstep with the source declaration.
        // A source-only guard can pass while an old artifact is still loaded
        // by `include_bytes!`; that would re-open the overdraw/caustic race on
        // devices that execute the stale module.  OpExecutionMode is opcode
        // 16 and EarlyFragmentTests is execution-mode literal 9.
        let spirv = include_bytes!("../../shaders/water.frag.spv");
        assert_eq!(spirv.len() % 4, 0, "water.frag.spv must be word aligned");
        let words: Vec<u32> = spirv
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
            .collect();
        assert_eq!(
            words.first().copied(),
            Some(0x0723_0203),
            "invalid SPIR-V magic"
        );
        let mut has_early_tests = false;
        let mut cursor = 5; // SPIR-V header is five words.
        while cursor < words.len() {
            let instruction = words[cursor];
            let word_count = (instruction >> 16) as usize;
            let opcode = instruction & 0xffff;
            if word_count == 3 && opcode == 16 && words.get(cursor + 2).copied() == Some(9) {
                has_early_tests = true;
                break;
            }
            if word_count == 0 {
                break;
            }
            cursor = cursor.saturating_add(word_count);
        }
        assert!(
            has_early_tests,
            "water.frag.spv must encode EarlyFragmentTests (regenerate it with glslangValidator)"
        );

        // #2789 — both post-TAA caustic writers use one normalized spatial
        // footprint; water must not regress to a single-pixel atomic.
        assert!(
            src.contains("#include \"include/caustic_kernel.glsl\"")
                && src.contains("causticGauss5Weight(kx, ky)"),
            "water caustics must use the shared 5x5 Gaussian footprint"
        );

        // #2785 — the ramp, not `hitDist / fog_far`.
        assert!(
            src.contains("float span = max(fogFar - fogNear, 1.0);")
                && src.contains("float t = clamp((hitDist - fogNear) / span, 0.0, 1.0);"),
            "absorbWaterColumn no longer computes the fog_near..fog_far ramp \
             these tests mirror"
        );
        assert!(
            src.contains(": push.shallow.a;")
                && src.contains("cameraUnderwater && hasUnderwaterRamp"),
            "fog_near must feed the above-water ramp while the underwater \
             variant may select its own authored near plane"
        );
        assert!(
            src.contains("vec3 authoredCoefficients = max(push.absorption.rgb, vec3(0.0));")
                && src.contains("float concentrationDensity = clamp(")
                && src.contains("/ STARFIELD_WATER_CONCENTRATION_REFERENCE")
                && src.contains("clamp(push.concentration.a, 0.0, 1.0) * 0.25")
                && src.contains("float oceanScatter = 1.0 + clamp(push.concentration.a")
                && src.contains("-hitDist * authoredCoefficients * (1.0 + concentrationDensity)"),
            "Starfield extinction coefficients and concentrations must feed the Beer-Lambert and scattering paths"
        );
        assert!(
            src.contains("float surfaceRoughness = clamp(push.noise_falloff.z, 0.0, 1.0)")
                && src.contains("push.noise_falloff.w / (push.noise_falloff.w + 100.0)")
                && src.contains("reflColor = mix(reflColor, reflectionMiss, surfaceRoughness * surfaceRoughness)"),
            "authored water reflection-width controls must soften geometry-hit reflections"
        );
        assert!(
            src.contains("float h4 = valueNoise")
                && src.contains("float h5 = valueNoise")
                && src.contains("h5 * 0.04"),
            "procedural legacy water must retain the full six-octave chop fallback"
        );
        assert!(
            src.contains("if (push.ripple.z > 0.0)")
                && src.contains("distanceToCenter - radius")
                && src.contains("radial * ring * 0.28"),
            "water.frag must consume transient RippleEvent data as a bounded normal ring"
        );

        // WATAL Phase 1 — the last push-constant slot carries Bethesda's
        // authored sun-glint exponent and visibly contributes to the HDR
        // surface. The shadow result is shared with the caustic path so the
        // new highlight does not double water's per-fragment shadow query.
        assert!(
            src.contains("push.effects.y > 0.0 ? push.effects.y : push.misc.w")
                && src.contains("push.effects.w")
                && src.contains("surfaceColor += vec3(sunSpecular);"),
            "water.frag must consume authored local and sun specular controls"
        );
        assert!(
            src.contains("push.effects.x / 10.0") && src.contains("refractionNormalWeight"),
            "water.frag must apply the authored refraction magnitude to the ray normal"
        );
        assert_eq!(
            src.matches("vec3 sunTransmission = traceShadowTransmittance(")
                .count(),
            1,
            "surface glint and caustics must share one sun-visibility query"
        );

        // #2784 — the caustic splat bound.
        assert!(
            src.contains("&& all(lessThan(pixel, causticSize))"),
            "the caustic splat must reject the upper edge on the integer \
             pixel (mirrors caustic_splat.comp's `greaterThanEqual(pixel, size)`)"
        );
        // The active-code form, not the bare call: the shader comment
        // quotes the retired guard while explaining what replaced it, and a
        // looser needle would match that prose and fail a correct file.
        assert!(
            !src.contains("&& all(lessThanEqual(uv01, vec2(1.0)))"),
            "the inclusive float guard is the #2784 off-by-one: uv01 == 1.0 \
             maps to pixel == screen, one past the last texel"
        );
    }

    /// Degenerate spans cannot divide by zero or invert. The ESM parser
    /// clamps `fog_far >= fog_near + 1`, but a hand-built `GpuWaterParams` (the
    /// Cornell harness, a test fixture) is not bound by that.
    #[test]
    fn a_degenerate_span_stays_finite_and_clamped() {
        for (near, far) in [(100.0f32, 100.0f32), (100.0, 0.0), (0.0, 0.0)] {
            for hit in [0.0f32, 1.0, 1000.0] {
                let t = ramp_t(hit, near, far);
                assert!(t.is_finite(), "t must stay finite for {near}/{far}");
                assert!((0.0..=1.0).contains(&t), "t must stay in 0..=1");
            }
        }
    }
}

/// #3604 — the module doc and `create_water_pipeline`'s blend table must
/// agree about which color attachments water writes.
///
/// The doc said "attachments 1..6 (… reservoir) are masked off", i.e. that
/// water writes no FSR mask — the opposite of the transparency-and-
/// composition contract the builder implements, and it named a slot removed
/// under #1583. Extending the file's existing `include_str!` scan convention
/// so the pair cannot drift apart again silently.
#[cfg(test)]
mod attachment_doc_pin_tests {
    const WATER_RS: &str = include_str!("water.rs");

    /// The 8-entry array literal `create_water_pipeline` hands to
    /// `PipelineColorBlendStateCreateInfo`, with whitespace collapsed.
    fn blend_table() -> String {
        let start = WATER_RS
            .find("let attachments = [")
            .expect("create_water_pipeline's blend table");
        let rest = &WATER_RS[start..];
        let end = rest.find("];").expect("blend table terminator") + 2;
        rest[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The `//!` block at the top of the file.
    fn module_doc() -> String {
        WATER_RS
            .lines()
            .take_while(|line| line.starts_with("//!") || line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn module_doc_matches_the_blend_table() {
        let table = blend_table();
        assert_eq!(
            table,
            "let attachments = [ hdr_blend, masked_off, masked_off, masked_off, \
             masked_off, masked_off, fsr_mask_max, fsr_mask_max, ];",
            "the water blend table changed — update the module doc's \
             attachment list in the same edit (#3604)",
        );

        let doc = module_doc();
        assert!(
            doc.contains("1..=5"),
            "the module doc must name the write-masked range as 1..=5, \
             matching the five `masked_off` entries in the blend table \
             (#3604)",
        );
        assert!(
            !doc.contains("1..6"),
            "the module doc still claims attachments 1..6 are masked off — \
             the table masks 1..=5 and WRITES 6 and 7 (#3604)",
        );
        assert!(
            !doc.contains("reservoir"),
            "the module doc still names the reservoir attachment, removed \
             under #1583 (#3604)",
        );
        assert!(
            doc.contains("FSR"),
            "the module doc must state that attachments 6 and 7 (the FSR \
             masks) are written, not masked off (#3604)",
        );
    }
}
