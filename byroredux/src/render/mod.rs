//! Per-frame render data collection from ECS queries.

use byroredux_core::ecs::{ActiveCamera, EntityId, Transform, World};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::vulkan::water::WaterDrawCommand;
use byroredux_renderer::{MaterialTable, SkyParams};
use rayon::slice::ParallelSliceMut;
use std::collections::HashMap;

use crate::components::CellLightingRes;

static FRAME_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Convert an `f32` to a `u32` key whose unsigned ordering matches
/// IEEE 754 total ordering of the source values across the full real
/// domain (negatives, zero, subnormals, positives, infinities, and
/// canonical NaNs). The standard two-step:
///
/// - If the sign bit is clear (value ≥ +0.0): flip the sign bit only,
///   so +0.0 sorts just above the largest negative.
/// - If the sign bit is set (value < +0.0): invert all 32 bits, so
///   more-negative values end up smaller in the unsigned ordering.
///
/// Pre-#306 the sort keys in `build_render_data` stored `f32::to_bits()`
/// directly, which orders correctly for positive floats only; the
/// transparent back-to-front path used `!bits` which fails on negatives.
/// Frustum culling kept the pathological inputs out of the sort in
/// practice, but NaN/denormal/negative-w edge cases could silently
/// mis-order whenever they slipped past. See #306 (D3-03).
#[inline]
fn f32_sortable_u32(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 != 0 {
        !bits
    } else {
        bits ^ 0x8000_0000
    }
}

/// Pack per-draw depth state into a single u8 so consecutive same-state
/// draws cluster: bit 0 = z_test, bit 1 = z_write, bits 4-7 = z_function.
fn pack_depth_state(cmd: &DrawCommand) -> u8 {
    (cmd.z_test as u8) | ((cmd.z_write as u8) << 1) | ((cmd.z_function & 0x0F) << 4)
}

/// Daytime peak of `SkyParamsRes::sun_intensity` (per `systems.rs:1446`,
/// hours 7..=17). Used by [`compute_directional_upload`] to normalise
/// the 0..4 ramp into a 0..1 contribution multiplier.
///
/// Tracked here (not as a `pub const` next to `weather_system`) because
/// the consumer is the directional-light upload — a bump on the
/// systems-side ramp without a matching bump here would silently
/// re-introduce a daytime brightness regression. Pin it via the
/// `directional_upload_peak_matches_weather_system` test.
const SUN_INTENSITY_PEAK: f32 = 4.0;

/// Project the cell's authored directional colour into the value the
/// renderer pushes to the per-frame `GpuLight` SSBO.
///
/// Interior arm: 0.6× constant fill, `radius = -1` so the shader skips
/// shadow rays (sealed-wall leak protection). Independent of `sun_intensity`
/// because interior XCLL is a subtle aesthetic fill — not a physical sun.
/// The fragment shader applies an additional 0.4× isotropic factor on
/// top (`INTERIOR_FILL_AMBIENT_FACTOR` in `triangle.frag`), so the
/// surface receives `directional × 0.24 × albedo` — uniform low-key
/// fill, no Lambert wrap. The cumulative dim-down vs the legacy
/// half-Lambert path is intentional; see the shader-side comment for
/// the corrugated-metal regression context.
///
/// Exterior arm: ramp the contribution by `sun_intensity / SUN_INTENSITY_PEAK`
/// so surfaces fade in lockstep with the composite sun disc
/// (`composite.frag:217`). Normalised to keep daytime brightness at
/// pre-#798 magnitude — the `SUN_INTENSITY_PEAK = 4.0` value was tuned
/// for the disc's perceptual brightness (where it multiplies `sun_col`
/// alongside other compositing terms), not for surface BRDF input.
///
/// Pre-#798 the exterior arm uploaded `directional_color` raw regardless
/// of TOD; at midnight `sun_dir = (0, -1, 0)` per `systems.rs:1437-1442`
/// and ceilings/overhangs received the full TOD-NIGHT `SKY_SUNLIGHT`
/// colour. The WRS shadow ray subtracts when occluded, but at distances
/// > 4000 units `shadowFade` decays to zero, leaving the unshadowed
/// > contribution un-cancelled.
///
/// Returns `(color, radius)` where `radius == -1` flags the shader to
/// skip shadow rays (interior fill) and `radius == 0` is the standard
/// directional contract.
fn compute_directional_upload(
    directional_color: &[f32; 3],
    is_interior: bool,
    sun_intensity: f32,
) -> ([f32; 3], f32) {
    if is_interior {
        const INTERIOR_FILL_SCALE: f32 = 0.6;
        (
            [
                directional_color[0] * INTERIOR_FILL_SCALE,
                directional_color[1] * INTERIOR_FILL_SCALE,
                directional_color[2] * INTERIOR_FILL_SCALE,
            ],
            -1.0,
        )
    } else {
        let ramp = (sun_intensity / SUN_INTENSITY_PEAK).clamp(0.0, 1.0);
        (
            [
                directional_color[0] * ramp,
                directional_color[1] * ramp,
                directional_color[2] * ramp,
            ],
            0.0,
        )
    }
}

/// Sort key for `DrawCommand`s — the batch-merge pass in
/// `VulkanContext::draw_frame` relies on consecutive identical
/// (alpha_blend, render_layer, two_sided, depth_state, mesh, …) runs
/// to fold into single instanced draws. Owned here so the field order
/// can't silently drift from an assert in a downstream crate.
///
/// Both branches return the same 10-tuple shape so the compiler accepts
/// a single key closure. Per-branch semantics:
///   Slot 0       = `!in_raster` priority bit — `0` for in-frustum
///                 (rasterized) draws, `1` for off-frustum RT-only
///                 occluders. Out-of-frustum entities ride in the SSBO
///                 / TLAS so on-screen fragments' shadow / reflection /
///                 GI rays can hit them, but they don't render to the
///                 raster pipeline. Clustering them at the END of the
///                 sorted array means when `MAX_INSTANCES` cap fires
///                 it drops RT-only entries first — raster never gets
///                 dropped, and the dropped RT-only contributions
///                 degrade gracefully (those entries are off-screen
///                 by definition, so direct visual impact is bounded
///                 to shadow / reflection / GI from beyond the
///                 frustum). See `MAX_INSTANCES` doc + Option B of
///                 the transparent-draw-flicker root-cause writeup.
///   Slots 1–9    same as the pre-Option-B key:
///   Opaque      — slot 4/5 = 0 (blend factors unused); slot 6 = depth_state;
///                 slot 7 = mesh (cluster key); slot 8 = sort_depth
///                 (front-to-back); slot 9 = entity_id tiebreaker (#506).
///   Transparent — slot 4/5 = (src_blend, dst_blend); slot 6 = !sort_depth
///                 (back-to-front within a (blend, depth_state) cohort);
///                 slot 7 = depth_state; slot 8 = mesh; slot 9 = entity_id.
///                 Correctness: alpha compositing requires back-to-front
///                 order *within one pipeline state*, not across them.
///
/// Slot 2 widened from `is_decal as u8` to `render_layer as u8`
/// (#renderlayer): same shape, but consecutive same-layer draws now
/// cluster as one of `{0..3}` rather than `{0,1}`, matching the new
/// per-layer depth-bias state-change boundary in `DrawBatch`.
///
/// The entity_id final slot makes `par_sort_unstable_by_key` behave
/// deterministically across runs: without it, rayon's work-stealing
/// could reorder commands whose 9-tuple prefix tied, breaking
/// capture/replay and screenshot-diff workflows on scenes with many
/// identical-mesh / identical-depth entries (e.g. exterior rock
/// fields at a fixed camera distance).
pub(crate) fn draw_sort_key(cmd: &DrawCommand) -> (u8, u8, u8, u8, u32, u32, u32, u32, u32, u32) {
    // Off-frustum RT-only entries cluster at the END of the sorted
    // array. Cap-on-overflow at `upload_instances` drops them first,
    // never raster draws. See the doc comment above + the
    // `MAX_INSTANCES` writeup in `scene_buffer.rs`.
    let rt_only = (!cmd.in_raster) as u8;
    if cmd.alpha_blend {
        (
            rt_only,
            1u8, // after opaque
            cmd.render_layer as u8,
            cmd.two_sided as u8,
            cmd.src_blend as u32,
            cmd.dst_blend as u32,
            !cmd.sort_depth, // invert → larger depth first
            pack_depth_state(cmd) as u32,
            cmd.mesh_handle,
            cmd.entity_id,
        )
    } else {
        (
            rt_only,
            0u8,
            cmd.render_layer as u8,
            cmd.two_sided as u8,
            0,
            0,
            pack_depth_state(cmd) as u32,
            cmd.mesh_handle, // group identical meshes
            cmd.sort_depth,  // front-to-back within group
            cmd.entity_id,
        )
    }
}

/// Per-frame view + lighting + sky payload returned by
/// [`build_render_data`]. The draw command list / GPU light list /
/// bone palette / skin offsets / material table are written into the
/// scratch buffers passed by the caller; everything that's a fresh
/// per-frame value lives here.
pub(crate) struct RenderFrameView {
    pub view_proj: [f32; 16],
    pub camera_pos: [f32; 3],
    pub ambient: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    /// XCLL cubic-fog clip distance (FNV+). `0.0` = no curve authored,
    /// composite falls through to the linear `fog_near..fog_far` ramp.
    /// When `> 0` and paired with `fog_power > 0`, composite uses
    /// `pow(distance / fog_clip, fog_power)` instead of the linear
    /// blend. See #865 / FNV-D3-NEW-06.
    pub fog_clip: f32,
    /// XCLL cubic-fog falloff exponent (FNV+). `0.0` = no curve. See
    /// `fog_clip` for the activation contract.
    pub fog_power: f32,
    pub sky: SkyParams,
}

/// Build the view-projection matrix and draw command list from ECS queries.
///
/// All scratch buffers — `draw_commands`, `gpu_lights`, `bone_world`,
/// `bind_inverses`, `skin_offsets` — are owned by the caller and
/// cleared on entry so their heap allocations persist across frames.
/// See #253 (`skin_offsets`), #243 (`draw_commands` / `gpu_lights` /
/// `bone_world` scratch pattern), #M29.5 (`bone_world` + `bind_inverses`
/// split — replaces the single pre-multiplied palette buffer with the
/// two GPU-compute inputs).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_render_data(
    world: &World,
    draw_commands: &mut Vec<DrawCommand>,
    water_commands: &mut Vec<WaterDrawCommand>,
    gpu_lights: &mut Vec<byroredux_renderer::GpuLight>,
    bone_world: &mut Vec<[[f32; 4]; 4]>,
    bind_inverses: &mut Vec<[[f32; 4]; 4]>,
    skin_offsets: &mut HashMap<EntityId, u32>,
    material_table: &mut MaterialTable,
    particle_quad_handle: Option<u32>,
) -> RenderFrameView {
    let frame_count = FRAME_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    draw_commands.clear();
    water_commands.clear();
    gpu_lights.clear();
    bone_world.clear();
    bind_inverses.clear();
    skin_offsets.clear();
    // R1 Phase 2 — clear the material table so the per-frame dedup
    // starts from scratch. `intern` calls below populate it as the
    // mesh / particle paths emit DrawCommands.
    material_table.clear();
    // M29.5 — slot 0 of BOTH inputs is identity so the GPU compute
    // produces `palette[0] = identity × identity = identity`. Rigid
    // meshes tagged with `bone_offset = 0` that somehow hit the
    // skinning path fall here harmlessly. Keeping both arrays
    // parallel from the first push enforces the
    // `bone_world.len() == bind_inverses.len()` invariant that
    // `skinned::build_skinned_palettes` asserts on entry.
    const IDENTITY_4X4: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    bone_world.push(IDENTITY_4X4);
    bind_inverses.push(IDENTITY_4X4);

    // First pass: skinned-mesh palette assembly — see
    // `render::skinned::build_skinned_palettes`. Pushes per-bone
    // raw world matrix + bind-inverse into the two parallel Vecs;
    // the GPU `skin_palette.comp` does the per-slot multiply.
    skinned::build_skinned_palettes(
        world,
        frame_count,
        bone_world,
        bind_inverses,
        skin_offsets,
    );

    // Camera view-projection + frustum + cam_pos — see
    // `render::camera::assemble_camera`.
    let camera::CameraView {
        view_proj,
        frustum,
        vp_mat,
        cam_pos,
    } = camera::assemble_camera(world);

    // Static mesh main loop — see `render::static_meshes::collect_static_mesh_draws`.
    static_meshes::collect_static_mesh_draws(
        world,
        &frustum,
        vp_mat,
        skin_offsets,
        draw_commands,
        material_table,
    );
    // Particle billboards — see `render::particles::emit_particles`.
    particles::emit_particles(
        world,
        particle_quad_handle,
        cam_pos,
        vp_mat,
        draw_commands,
        material_table,
    );

    // Sort: opaque → decal → alpha.
    //
    // Opaque: group by (pipeline_key, depth_state, mesh, texture) to
    // maximize instanced draw batching — consecutive draws sharing
    // mesh+pipeline+depth_state merge into a single cmd_draw_indexed
    // and pay zero state-change cost across the batch boundary. Depth
    // is a tie-breaker within each group (front-to-back for early-Z).
    // This trades some early-Z benefit for dramatically fewer draw
    // calls on scenes with many identical meshes (e.g. 400 rocks → 1
    // instanced draw instead of 400). #272 + #398.
    //
    // Alpha-blend: must remain back-to-front (depth-primary) for correct
    // transparency ordering — instancing is irrelevant here.
    //
    // #934 / PERF-DC-01 — rayon's fork-join overhead loses to serial
    // `sort_unstable_by_key` below ~2K elements on the closure-extracted
    // 9-tuple key. Measured on a 7950X (see
    // `bench_draw_sort_serial_vs_parallel` in
    // `byroredux/src/render/draw_sort_key_tests.rs`):
    //
    //     N= 400: serial 21µs vs parallel 27µs  (serial 28% faster)
    //     N= 800: serial 46µs vs parallel 60µs  (serial 31% faster)
    //     N=1500: serial 97µs vs parallel 131µs (serial 35% faster)
    //     N=2000: 161µs ≈ 165µs                  (tied)
    //     N=3000: serial 269µs vs parallel 235µs (parallel 14% faster)
    //     N=10K : serial 1122µs vs parallel 673µs(parallel 67% faster)
    //
    // Typical Bethesda cell counts sit in 400–1500 (Prospector ~811,
    // GSDocMitchell ~263, exterior radius-3 grid ~1200), so serial is
    // the default. The fallback to `par_sort_unstable_by_key` at ≥2K
    // covers exterior radius-5+ grids and Skyrim+ city interiors.
    if draw_commands.len() >= 2000 {
        draw_commands.par_sort_unstable_by_key(draw_sort_key);
    } else {
        draw_commands.sort_unstable_by_key(draw_sort_key);
    }
    // ⚠ No-resort contract (#1026 / F-WAT-05).
    //
    // The water-plane re-emit below records each WaterDrawCommand's
    // `instance_index` as the current position into `draw_commands`.
    // The renderer relies on that index pointing at the same draw
    // slot at GPU upload time (the SSBO is built by iterating
    // `draw_commands` in order — frustum-culled draws keep their
    // slot per #516, so the vec position is the slot). Any code
    // path that re-sorts `draw_commands` AFTER the water emit below
    // breaks this contract silently — the recorded `instance_index`
    // would now point at a different draw whose model matrix /
    // material is wrong for the water shader.
    //
    // The defensive gate is a `debug_assert!` in
    // `VulkanContext::draw_frame` (immediately before the
    // water-pipeline loop) using
    // `byroredux_renderer::vulkan::water::water_commands_match_draw_slots`
    // — see the function's doc-comment for the predicate.

    // Collect lights from ECS — directional fill + placed point lights.
    // See `render::lights::collect_lights`.
    lights::collect_lights(world, gpu_lights);

    // Camera position.
    let camera_pos = if let Some(active) = world.try_resource::<ActiveCamera>() {
        let cam_entity = active.0;
        drop(active);
        let tq = world.query::<Transform>();
        tq.and_then(|q| {
            q.get(cam_entity)
                .map(|t| [t.translation.x, t.translation.y, t.translation.z])
        })
        .unwrap_or([0.0; 3])
    } else {
        [0.0; 3]
    };

    // Cell ambient color (or default).
    let cell_lit = world.try_resource::<CellLightingRes>();
    // XCLL ambient passed through as-is — the per-light ambient fill in
    // the shader (0.5 × lightColor × atten × albedo per light) provides
    // the additional fill that Gamebryo's D3D9 equation contributes.
    let ambient = cell_lit
        .as_ref()
        .map(|l| l.ambient)
        .unwrap_or([0.08, 0.08, 0.08]);
    // Fog is passed through as authored. Cross-checking FalloutNV.esm:
    // 89% of interior cells author both fog_near and fog_far (median
    // 64/4000); only ~10% leave them zero — for those, the author's
    // intent is "no distance fog, rely on XCLL ambient fill." The
    // composite pass gates the fog mix on `fog_params.y > fog_params.x`,
    // so leaving both at zero disables it cleanly. Exterior cells set
    // fog via WTHR/CLMT in weather_system, which writes into
    // CellLightingRes before render.
    let fog_color = cell_lit.as_ref().map(|l| l.fog_color).unwrap_or([0.0; 3]);
    // CLMT-authored fog_near can be negative (artistic intent: "fog
    // starts before the camera"). The composite shader's gate
    // `fog_far > fog_near` would still pass with a negative near, but
    // `smoothstep(neg, pos, dist)` then returns nonzero at dist=0 and
    // every fragment — including the camera origin — gets fog mixed in.
    // Clamping at the render-side boundary keeps both the camera UBO
    // upload (draw.rs:356) and the composite UBO upload (draw.rs:1566)
    // in sync without a per-fragment branch. #666.
    let fog_near = cell_lit
        .as_ref()
        .map(|l| l.fog_near.max(0.0))
        .unwrap_or(0.0);
    let fog_far = cell_lit.as_ref().map(|l| l.fog_far).unwrap_or(0.0);
    // #865 / FNV-D3-NEW-06 — XCLL cubic-fog curve (FNV+). Both fields
    // default to 0.0 (no curve), in which case composite falls through
    // to the linear `fog_near..fog_far` ramp. Authored values pack into
    // `fog_params.z` / `.w` for the composite shader (see draw.rs).
    let fog_clip = cell_lit
        .as_ref()
        .and_then(|l| l.fog_clip)
        .unwrap_or(0.0)
        .max(0.0);
    let fog_power = cell_lit
        .as_ref()
        .and_then(|l| l.fog_power)
        .unwrap_or(0.0)
        .max(0.0);
    drop(cell_lit);

    // Sky params (TOD palette + cloud scroll + sun + DALC cube) — see
    // `render::sky::build_sky_params`.
    let sky = sky::build_sky_params(world);

    // Water-plane re-emit — see `render::water::reemit_water_planes`.
    // MUST run after the sort above and before the renderer consumes
    // draw_commands (no-resort contract, #1026 / F-WAT-05).
    water::reemit_water_planes(world, draw_commands, water_commands);

    RenderFrameView {
        view_proj,
        camera_pos,
        ambient,
        fog_color,
        fog_near,
        fog_far,
        fog_clip,
        fog_power,
        sky,
    }
}

// Per-section sub-modules (TD9-001 sweep, #1115). Each sibling owns
// one of the 8 query families in `build_render_data`; the parent
// orchestrator above acquires the World queries once and threads
// references through.
mod camera;
mod lights;
mod particles;
mod skinned;
mod sky;
mod static_meshes;
mod water;

#[cfg(test)]
mod bone_palette_overflow_tests;
#[cfg(test)]
mod directional_upload_tests;
#[cfg(test)]
mod draw_sort_key_tests;
#[cfg(test)]
mod fog_curve_propagation_tests;
#[cfg(test)]
mod frustum_tests;
#[cfg(test)]
mod sort_key_tests;
#[cfg(test)]
mod variant_pack_gating_tests;
