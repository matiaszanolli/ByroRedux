//! Per-frame render data collection from ECS queries.

use byroredux_core::ecs::{resources::SkinSlotPool, ActiveCamera, EntityId, Transform, World};
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

/// Pack per-draw depth state (plus the wireframe pipeline-bind boundary)
/// into a single u8 so consecutive same-state draws cluster: bit 0 =
/// z_test, bit 1 = z_write, bit 2 = wireframe, bits 4-7 = z_function.
///
/// D2-NEW-05 (#1806): `wireframe` folded into this slot's spare bit 2 —
/// #869 made it a `PipelineKey` axis (`Opaque { wireframe }` /
/// `Blended { .., wireframe }`), a pipeline-bind boundary the sort key
/// must respect the same way it already respects blend factors and
/// depth state, or a wireframe draw interleaved among fill draws would
/// split an otherwise-contiguous batch and force extra binds.
fn pack_depth_state(cmd: &DrawCommand) -> u8 {
    (cmd.z_test as u8)
        | ((cmd.z_write as u8) << 1)
        | ((cmd.wireframe as u8) << 2)
        | ((cmd.z_function & 0x0F) << 4)
}

/// Apply the optional `BYRO_FOG_NEAR` / `BYRO_FOG_FAR` distance overrides
/// (Bethesda units) to the authored fog ramp. See the call site in
/// `build_render_data` for rationale. Each override is parsed once and
/// cached (the values can't change within a process), so the per-frame
/// cost is two atomic loads. `None` (unset / unparseable / negative)
/// leaves the corresponding authored value untouched.
fn apply_fog_overrides(near: f32, far: f32) -> (f32, f32) {
    use std::sync::OnceLock;
    static OVERRIDES: OnceLock<(Option<f32>, Option<f32>)> = OnceLock::new();
    let (n_ovr, f_ovr) = OVERRIDES.get_or_init(|| {
        let env_f32 = |key: &str| {
            std::env::var(key)
                .ok()
                .and_then(|s| s.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
        };
        let o = (env_f32("BYRO_FOG_NEAR"), env_f32("BYRO_FOG_FAR"));
        if o.0.is_some() || o.1.is_some() {
            log::info!(
                "BYRO_FOG override active: fog_near={:?} fog_far={:?} (authored values bypassed)",
                o.0,
                o.1
            );
        }
        o
    });
    (n_ovr.unwrap_or(near), f_ovr.unwrap_or(far))
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
///   Transparent — slot 4/5 = (src_blend, dst_blend); slot 9 = entity_id.
///                 Slots 6/7/8 vary by blend mode:
///                 * alpha-over (dst != ONE) → slot 6 = !sort_depth
///                   (back-to-front — compositing correctness must win
///                   even over the pipeline-bind boundary), slot 7 =
///                   depth_state (tie-break only), slot 8 = mesh.
///                 * additive (Gamebryo dst_blend == ONE == 0) is
///                   order-independent → slot 6 = depth_state (the
///                   pipeline-bind boundary dominates, mirroring the
///                   Opaque branch — #1994/DIM2-01), slot 7 = mesh
///                   (cluster key), slot 8 = sort_depth, so same-mesh
///                   particles of one pipeline state stay contiguous and
///                   instance-batch (#1649).
///
/// D2-NEW-05 (#1806): every branch's `depth_state` slot (6 for Opaque and
/// additive Transparent, 7 for alpha-over Transparent) also carries the
/// `wireframe` pipeline-bind boundary, packed into `pack_depth_state`'s
/// spare bit 2 — see that function's doc comment. Without it a wireframe
/// draw could land mid-run among fill draws of the same mesh/depth state
/// and split the batch.
///
/// Slot 2 widened from `is_decal as u8` to `render_layer as u8`
/// (#renderlayer): same shape, but consecutive same-layer draws now
/// cluster as one of `{0..3}` rather than `{0,1}`, matching the new
/// per-layer depth-bias state-change boundary in `DrawBatch`.
///
/// The entity_id final slot makes `par_sort_unstable_by_key` behave
/// deterministically across runs: without it, rayon's work-stealing
/// could reorder commands whose 10-tuple prefix tied, breaking
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
        // Additive blending (Gamebryo `dst_blend == ONE`, value 0 — see
        // `gamebryo_to_vk_blend_factor`) is order-independent: the HDR
        // target accumulates `src*srcF + dst*1` commutatively, so
        // same-mesh particles need no back-to-front sort. For that subset
        // the pipeline-bind boundary (depth_state, which folds in
        // `wireframe` — D2-NEW-05 / #1806) dominates mesh, mirroring the
        // Opaque branch below — otherwise a wireframe draw interleaved
        // among fill draws of the same mesh forces extra pipeline binds
        // instead of two contiguous fill/wireframe runs (#1994 / DIM2-01).
        // Mesh still dominates *within* one depth_state, so same-mesh
        // billboards stay contiguous and the CPU batch-merge collapses
        // them into a single instanced indirect draw (#1649). True
        // alpha-over (e.g. ONE_MINUS_SRC_ALPHA = 7) keeps depth dominant
        // over even the pipeline-bind boundary — its compositing order is
        // visible, so correctness wins over bind-count efficiency there.
        const GAMEBRYO_BLEND_ONE: u8 = 0;
        let (slot6, slot7, slot8) = if cmd.dst_blend == GAMEBRYO_BLEND_ONE {
            // depth_state dominates → contiguous fill/wireframe runs;
            // mesh clusters within each; sort_depth is front-to-back.
            (pack_depth_state(cmd) as u32, cmd.mesh_handle, cmd.sort_depth)
        } else {
            // depth dominates → back-to-front; depth_state only breaks
            // ties within a depth bucket; mesh is the final tiebreaker.
            (!cmd.sort_depth, pack_depth_state(cmd) as u32, cmd.mesh_handle)
        };
        (
            rt_only,
            1u8, // after opaque
            cmd.render_layer as u8,
            cmd.two_sided as u8,
            cmd.src_blend as u32,
            cmd.dst_blend as u32,
            slot6,
            slot7,
            slot8,
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
    /// Camera right vector (world space, unit length). Used by the renderer
    /// to compute the per-frame aperture disk offset for depth of field.
    pub cam_right: [f32; 3],
    /// Camera up vector (world space, unit length).
    pub cam_up: [f32; 3],
    /// Camera forward vector (world space, unit length, into the scene).
    pub cam_forward: [f32; 3],
    /// Perspective projection matrix (column-major, Vulkan clip space).
    /// Stored separately so the renderer can apply a DOF-jittered view
    /// matrix and recompute view_proj without reassembling the camera.
    pub proj_mat: [f32; 16],
    /// Lens aperture half-radius (world units). `0.0` = pinhole / no DOF.
    pub aperture: f32,
    /// Focal distance (world units). Surfaces at this depth are sharp.
    pub focus_dist: f32,
}

/// Build the view-projection matrix and draw command list from ECS queries.
///
/// All scratch buffers — `draw_commands`, `gpu_lights`, `bone_world`,
/// `skin_offsets` — are owned by the caller and cleared on entry so
/// their heap allocations persist across frames. See #253
/// (`skin_offsets`), #243 (`draw_commands` / `gpu_lights` /
/// `bone_world` scratch pattern), M29.5 (the bone_world / bind_inverses
/// GPU-compute split), M29.6 (`bind_inverses` promoted to a persistent
/// SSBO indexed by [`SkinSlotPool`] slot — written once per
/// skinned-mesh first-sight rather than per frame).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_render_data(
    world: &World,
    draw_commands: &mut Vec<DrawCommand>,
    water_commands: &mut Vec<WaterDrawCommand>,
    gpu_lights: &mut Vec<byroredux_renderer::GpuLight>,
    bone_world: &mut Vec<[[f32; 4]; 4]>,
    skin_offsets: &mut HashMap<EntityId, u32>,
    skin_slot_pool: &mut SkinSlotPool,
    material_table: &mut MaterialTable,
    particle_quad_handle: Option<u32>,
) -> RenderFrameView {
    let frame_count = FRAME_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    draw_commands.clear();
    water_commands.clear();
    gpu_lights.clear();
    skin_offsets.clear();
    // R1 Phase 2 — clear the material table so the per-frame dedup
    // starts from scratch. `intern` calls below populate it as the
    // mesh / particle paths emit DrawCommands.
    material_table.clear();
    // M29.6 — slot 0 of bone_world is identity. The persistent
    // `bind_inverses` SSBO holds whatever identity was written at
    // first-sight (the pool reserves slot 0; the renderer either
    // pre-seeds it at startup or leaves it zero — palette dispatch
    // overwrites `palette[0] = bone_world[0] × bind_inverses[0]`
    // every frame, so a non-identity bind_inverses[0] would only
    // matter if some draw with `bone_offset = 0` AND a non-trivial
    // bone weight existed, which doesn't happen by construction).
    const IDENTITY_4X4: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    // #1794 / PERF-D4-NEW-01 — `bone_world` deliberately does NOT
    // `.clear()` like the other scratch buffers above. It used to, then
    // got unconditionally re-grown from empty every frame in
    // `build_skinned_palettes`'s Pass 2, which wrote a full
    // MAX_BONES_PER_MESH-stride identity fill for every allocated slot
    // — including the portion each entity's Pass 3 write was about to
    // overwrite anyway, and the per-mesh padding tail beyond
    // `skin.bones.len()`, which nothing ever reads once the slot has
    // been filled once (a vertex's bone-weight index is bounded by its
    // OWN mesh's bone count at import time, so it structurally can't
    // reach a stale or reused slot's padding tail — same invariant
    // `upload_bone_worlds`'s doc comment already relies on for
    // never-referenced whole slots). Keeping the array's content
    // across frames and letting Pass 2's `resize` grow-or-shrink
    // in place means steady-state frames (same entities, same slots)
    // pay zero identity-fill cost — `resize` only touches genuinely
    // NEW tail elements when the pool's high-water mark grows, and
    // TRUNCATES (no fill at all) when it shrinks.
    if bone_world.is_empty() {
        bone_world.push(IDENTITY_4X4);
    } else {
        bone_world[0] = IDENTITY_4X4;
    }

    // `BYRO_PROFILE=1` breaks the per-frame render-data build into its
    // phases (skinned palettes / static-mesh main loop / particles / draw
    // sort / lights) so the dominant cost is localized without guessing.
    // PERF-D1-NEW-02 / #1802 — cached via `OnceLock` so the hot path
    // doesn't `getenv` per frame, mirroring `apply_fog_overrides`. Env
    // vars can't change mid-process, so caching is semantics-preserving.
    let profile = {
        use std::sync::OnceLock;
        static PROFILE: OnceLock<bool> = OnceLock::new();
        *PROFILE.get_or_init(|| std::env::var_os("BYRO_PROFILE").is_some())
    };
    let mark = |on: bool| on.then(std::time::Instant::now);
    let took =
        |s: Option<std::time::Instant>| s.map_or(0.0, |i| i.elapsed().as_secs_f32() * 1000.0);

    // First pass: skinned-mesh palette assembly — see
    // `render::skinned::build_skinned_palettes`. Allocates pool slots,
    // writes per-entity bone_world matrices into sparse slots, and
    // queues first-sight `bind_inverses` uploads on the pool.
    let t_skin = mark(profile);
    skinned::build_skinned_palettes(world, frame_count, bone_world, skin_offsets, skin_slot_pool);
    let ms_skin = took(t_skin);

    // Camera view-projection + frustum + cam_pos — see
    // `render::camera::assemble_camera`.
    let camera::CameraView {
        view_proj,
        frustum,
        vp_mat,
        cam_pos,
        cam_right,
        cam_up,
        cam_forward,
        proj_mat,
        aperture,
        focus_dist,
    } = camera::assemble_camera(world);

    // Static mesh main loop — see `render::static_meshes::collect_static_mesh_draws`.
    let t_static = mark(profile);
    static_meshes::collect_static_mesh_draws(
        world,
        &frustum,
        vp_mat,
        skin_offsets,
        draw_commands,
        material_table,
    );
    let ms_static = took(t_static);
    let n_draws = draw_commands.len();
    // Particle billboards — see `render::particles::emit_particles`.
    let t_particles = mark(profile);
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
    // 10-tuple key. Measured on a 7950X (see
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
    // the default. The fallback to `par_sort_unstable_by_key` at
    // `DRAW_SORT_PARALLEL_THRESHOLD` covers exterior radius-5+ grids and
    // Skyrim+ city interiors.
    const DRAW_SORT_PARALLEL_THRESHOLD: usize = 2000;
    let ms_particles = took(t_particles);
    let t_sort = mark(profile);
    if draw_commands.len() >= DRAW_SORT_PARALLEL_THRESHOLD {
        draw_commands.par_sort_unstable_by_key(draw_sort_key);
    } else {
        draw_commands.sort_unstable_by_key(draw_sort_key);
    }
    let ms_sort = took(t_sort);
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
    let t_lights = mark(profile);
    lights::collect_lights(world, gpu_lights);
    let ms_lights = took(t_lights);
    if profile {
        log::info!(
            "build_render_data: skinned={ms_skin:.2}ms static_loop={ms_static:.2}ms ({n_draws} draws) particles={ms_particles:.2}ms sort={ms_sort:.2}ms lights={ms_lights:.2}ms"
        );
    }

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
    // `BYRO_FOG_NEAR` / `BYRO_FOG_FAR` overrides (Bethesda units) for
    // offline / diagnostic renders. The authored weather/XCLL ramp matches
    // the original game's ~tens-of-K-BU view distance; when inspecting the
    // distant-terrain LOD ring (`cell_loader::terrain_lod`, ~197K BU) those
    // values wash the far terrain to the fog colour, so this lever lets a
    // capture push fog out to match. Applied at this single consumption
    // point (downstream of `weather_system`'s per-frame fog write) so it is
    // authoritative every frame. Cached so the hot path doesn't `getenv`
    // per frame. No-op when unset. Mirrors the `BYRO_HOUR` convention.
    let (fog_near, fog_far) = apply_fog_overrides(fog_near, fog_far);
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
        cam_right: cam_right.to_array(),
        cam_up: cam_up.to_array(),
        cam_forward: cam_forward.to_array(),
        proj_mat: proj_mat.to_cols_array(),
        aperture,
        focus_dist,
    }
}

// Per-section sub-modules (TD9-001 sweep, #1115). Each sibling owns
// one of the 8 query families in `build_render_data`; the parent
// orchestrator above acquires the World queries once and threads
// references through.
mod camera;
// `pub(crate)` so the `light.atten` console command (REND-#1451) can
// read `LIGHT_RANGE_EXTENSION` to report the effective brightness at
// the authored radius.
pub(crate) mod lights;
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
mod env_var_cache_tests;
#[cfg(test)]
mod fog_curve_propagation_tests;
#[cfg(test)]
mod frustum_tests;
#[cfg(test)]
mod sort_key_tests;
#[cfg(test)]
mod static_mesh_fx_skip_tests;
#[cfg(test)]
mod variant_pack_gating_tests;
