//! Per-frame render data collection from ECS queries.

use byroredux_core::ecs::{resources::SkinSlotPool, ActiveCamera, EntityId, Transform, World};
use byroredux_core::math::Vec3;
use byroredux_physics::PhysicsWorld;
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::vulkan::water::WaterDrawCommand;
use byroredux_renderer::{MaterialTable, SkyParams};
use rayon::slice::ParallelSliceMut;
use rustc_hash::FxHashMap;

use crate::components::CellLightingRes;

static FRAME_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Downward-raycast distance (world units) for the height-fog ground
/// reference below — same order of magnitude as
/// `systems::locomotion::LOCOMOTION_GROUND_RAY_MAX_DISTANCE`, kept as its
/// own constant since that one is private to the locomotion module and the
/// two rays serve unrelated purposes (foot-placement vs. fog altitude).
const FOG_HEIGHT_REFERENCE_RAY_MAX_DISTANCE: f32 = 4096.0;

/// World-space Y used as the height-fog reference altitude (REN-D16-01 /
/// #2225). Both `proceduralDensityScale` (froxel injection) and
/// `heightFogOpticalDepth` (beyond-grid aerial-perspective continuation)
/// used the camera's own eye height before this fix, so fog density
/// always peaked at eye level and followed the player vertically instead
/// of thinning with real altitude — climbing a hill never cleared the
/// fog, and vertical-only camera motion (stairs, an elevator, the fly
/// camera) changed density at a fixed world point for reasons temporal
/// reprojection can't model, producing ghost bands.
///
/// A single downward ray-cast from the camera against static collision
/// (mirrors `systems::locomotion`'s ground-snap ray) anchors the reference
/// to the ground/floor near the camera instead: pure vertical motion at a
/// fixed XZ no longer moves the reference at all, and walking up a slope
/// tracks the slope's own height rather than the camera's eye height.
/// Falls back to the camera's own Y — the pre-fix behavior — when no
/// ground is found below (open sky, no `PhysicsWorld` resource yet).
fn fog_height_reference(world: &World, cam_pos: Vec3) -> f32 {
    // The player capsule must be excluded (#2859). In `PlayerMode::Character`
    // the camera is pinned to `body_pos + eye_height*Y` at the body's own XZ,
    // and `CharacterController::HUMAN` is deliberately sized so the eye sits
    // INSIDE the capsule (`eye_height 52 < half_height 46 + radius 18`). The
    // capsule is `KinematicPositionBased`, which `exclude_dynamic()` does not
    // filter, so with `solid = true` the ray returned a `toi = 0` self-hit —
    // i.e. exactly `cam_pos.y`, numerically identical to the fallback below.
    // That silently reverted this whole function to the pre-#2225 behaviour
    // it exists to remove, with no log and no test failure.
    let excluded_body = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)
        .and_then(|entity| {
            world
                .query::<byroredux_physics::RapierHandles>()
                .and_then(|handles| handles.get(entity).map(|h| h.body))
        });
    world
        .try_resource::<PhysicsWorld>()
        .and_then(|pw| {
            pw.cast_ray_down(
                cam_pos,
                FOG_HEIGHT_REFERENCE_RAY_MAX_DISTANCE,
                excluded_body,
            )
        })
        .unwrap_or(cam_pos.y)
}

#[cfg(test)]
mod fog_height_reference_tests {
    use super::*;
    use byroredux_core::ecs::components::collision::CollisionShape;
    use byroredux_core::ecs::components::{GlobalTransform, RigidBodyData};
    use byroredux_core::math::Quat;
    use byroredux_physics::{physics_sync_system, RapierHandles};

    /// REN-D16-01 / #2225 — no `PhysicsWorld` resource at all (e.g. the
    /// bench demo scene, or before the first cell load) must fall back to
    /// the camera's own Y, the pre-fix behavior, rather than panicking or
    /// silently returning some other value.
    #[test]
    fn falls_back_to_camera_y_without_a_physics_world() {
        let world = World::new();
        let cam_pos = Vec3::new(100.0, 250.0, -40.0);
        assert_eq!(fog_height_reference(&world, cam_pos), cam_pos.y);
    }

    /// An empty `PhysicsWorld` (registered, but no colliders synced in
    /// yet) must also fall back to the camera's own Y — `cast_ray_down`
    /// returns `None` on an empty query pipeline, not a false hit at Y=0.
    #[test]
    fn falls_back_to_camera_y_with_an_empty_physics_world() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        let cam_pos = Vec3::new(0.0, 500.0, 0.0);
        assert_eq!(fog_height_reference(&world, cam_pos), cam_pos.y);
    }

    /// With a real static floor collider below the camera, the reference
    /// must be the floor's height, not the camera's — the entire point of
    /// this fix. Spawns a collider entity through the same ECS path the
    /// cell loader uses (`CollisionShape` + `RigidBodyData` +
    /// `GlobalTransform`, synced via `physics_sync_system`), matching
    /// `rapier_release_tests.rs`'s established test pattern.
    #[test]
    fn returns_floor_height_when_a_static_collider_is_below_the_camera() {
        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.register::<RapierHandles>();

        let floor = world.spawn();
        world.insert(floor, CollisionShape::Ball { radius: 16.0 });
        world.insert(floor, RigidBodyData::STATIC);
        world.insert(
            floor,
            GlobalTransform::new(Vec3::new(0.0, 100.0, 0.0), Quat::IDENTITY, 1.0),
        );
        physics_sync_system(&world, 0.0);
        world.resource_mut::<PhysicsWorld>().update_query_pipeline();

        // Camera is far above the floor's centre (Y=100 + radius 16 = 116
        // top surface); same XZ column so the downward ray hits it.
        let cam_pos = Vec3::new(0.0, 800.0, 0.0);
        let reference = fog_height_reference(&world, cam_pos);
        assert!(
            (reference - 116.0).abs() < 0.1,
            "expected the floor's top surface (~116.0), got {reference}"
        );
        assert_ne!(
            reference, cam_pos.y,
            "must not fall back to camera Y when a floor exists"
        );
    }

    /// #2859 — in `PlayerMode::Character` the camera sits INSIDE the player
    /// capsule by design (`eye_height 52 < half_height 46 + radius 18`), and
    /// the capsule is `KinematicPositionBased`, which `exclude_dynamic()`
    /// does not filter. With `solid = true` the ray returned a `toi = 0`
    /// self-hit — i.e. exactly `cam_pos.y`, numerically identical to the
    /// fallback — so the whole #2225 height-fog fix was a silent no-op in
    /// every gameplay cell. The three tests above cannot see it because none
    /// of them registers a player body.
    #[test]
    fn excludes_the_player_capsule_the_camera_sits_inside() {
        use byroredux_core::ecs::components::MotionType;

        let mut world = World::new();
        world.insert_resource(PhysicsWorld::new());
        world.register::<RapierHandles>();

        let floor = world.spawn();
        world.insert(floor, CollisionShape::Ball { radius: 16.0 });
        world.insert(floor, RigidBodyData::STATIC);
        world.insert(
            floor,
            GlobalTransform::new(Vec3::new(0.0, 100.0, 0.0), Quat::IDENTITY, 1.0),
        );

        // Player capsule centred at 300 spans [236, 364]; the eye sits at
        // 352, inside it — exactly the production geometry.
        let player = world.spawn();
        world.insert(
            player,
            CollisionShape::Capsule {
                half_height: 46.0,
                radius: 18.0,
            },
        );
        world.insert(
            player,
            RigidBodyData {
                motion_type: MotionType::CharacterKinematic,
                ..RigidBodyData::STATIC
            },
        );
        world.insert(
            player,
            GlobalTransform::new(Vec3::new(0.0, 300.0, 0.0), Quat::IDENTITY, 1.0),
        );

        physics_sync_system(&world, 0.0);
        world.resource_mut::<PhysicsWorld>().update_query_pipeline();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));

        let cam_pos = Vec3::new(0.0, 352.0, 0.0);
        let reference = fog_height_reference(&world, cam_pos);
        assert_ne!(
            reference, cam_pos.y,
            "self-hit on the player capsule — the #2859 regression"
        );
        assert!(
            (reference - 116.0).abs() < 0.1,
            "expected the floor's top surface (~116.0), got {reference}"
        );
    }
}

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
fn apply_fog_overrides(near: f32, far: f32) -> (f32, f32, bool) {
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
    (
        n_ovr.unwrap_or(near),
        f_ovr.unwrap_or(far),
        n_ovr.is_some() || f_ovr.is_some(),
    )
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
/// Interior arm: authored `directional_fade` is the source-strength
/// multiplier, independent of `sun_intensity` because interior XCLL is cell
/// lighting rather than the exterior weather sun. Formats without that field
/// retain the established 0.6 fallback.
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
/// Beyond the renderer's shared shadow fade start, `shadowFade` decays
/// to zero, leaving the unshadowed contribution un-cancelled.
///
/// Both arms remain physical directional sources. Interior XCLL keeps its
/// authored/fallback strength calibration, but its ambient irradiance is the
/// separate flat/cube term on `CellLightingRes`/`GpuDalcCube`; reclassifying
/// the directional colour as ambient would bypass N.L and visibility.
fn compute_directional_upload(
    directional_color: &[f32; 3],
    is_interior: bool,
    sun_intensity: f32,
    directional_fade: Option<f32>,
) -> [f32; 3] {
    let source_scale = if is_interior {
        const LEGACY_INTERIOR_DIRECTIONAL_SOURCE_SCALE: f32 = 0.6;
        directional_fade
            .filter(|fade| fade.is_finite())
            .unwrap_or(LEGACY_INTERIOR_DIRECTIONAL_SOURCE_SCALE)
            .max(0.0)
    } else {
        (sun_intensity / SUN_INTENSITY_PEAK).clamp(0.0, 1.0)
    };
    [
        directional_color[0] * source_scale,
        directional_color[1] * source_scale,
        directional_color[2] * source_scale,
    ]
}

/// Sort key for `DrawCommand`s — the batch-merge pass in
/// `VulkanContext::draw_frame` relies on consecutive identical
/// (alpha_blend, render_layer, two_sided, depth_state, mesh, …) runs
/// to fold into single instanced draws. Owned here so the field order
/// can't silently drift from an assert in a downstream crate.
///
/// #2459 — `DrawCommand` has no per-draw "keep file order" hint. NIF's
/// `NiAVObject::DISABLE_SORTING` bit (parsed into
/// `byroredux_core::ecs::components::SceneFlags::DISABLE_SORTING`, see
/// that constant's doc for the verified Gamebryo semantics) never
/// reaches this key — every alpha-blended draw goes through the global
/// back-to-front sort below regardless. Deliberately left unwired; see
/// the `SceneFlags` doc for why.
///
/// All branches return the same 11-tuple shape so the compiler accepts
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
///   Slot 1       = composition phase: opaque (`0`), fire-refraction
///                 proxies (`1`), then ordinary transparent/effect draws
///                 (`2`). The middle phase lets heat haze replace the
///                 opaque scene before its associated flame cards composite
///                 over it.
///   Slot 2       = transparent blend class: additive (`0`) before true
///                 alpha-over (`1`). Opaque also uses `0`.
///   Opaque      — slots 3/4 = render layer/two-sided; slots 5/6 = 0
///                 (blend factors unused); slot 7 = depth_state; slot 8 =
///                 mesh (cluster key); slot 9 = sort_depth
///                 (front-to-back); slot 10 = entity_id tiebreaker (#506).
///   Additive    — order-independent, so slots 3–9 retain state/mesh
///                 clustering: render layer, two-sided, blend factors,
///                 depth_state, mesh, then sort_depth. This keeps particles
///                 instance-batchable (#1649/#1994).
///   Alpha-over  — slot 3 = !sort_depth (global back-to-front order), then
///                 render layer, two-sided, blend factors, depth_state and
///                 mesh as tie-breakers in slots 4–9. Depth must precede
///                 every raster-state boundary: grouping by render layer
///                 first lets a distant puddle/decal draw after nearer
///                 glass and appear as a rectangular patch on the glass.
///
/// D2-NEW-05 (#1806): every branch's `depth_state` slot (7 for Opaque and
/// additive Transparent, 8 for alpha-over Transparent) also carries the
/// `wireframe` pipeline-bind boundary, packed into `pack_depth_state`'s
/// spare bit 2 — see that function's doc comment. Without it a wireframe
/// draw could land mid-run among fill draws of the same mesh/depth state
/// and split the batch.
///
/// Opaque/additive slot 3 widened from `is_decal as u8` to
/// `render_layer as u32` (#renderlayer), matching the per-layer depth-bias
/// state-change boundary in `DrawBatch`. Alpha-over deliberately places
/// this boundary after depth for correct compositing.
///
/// The entity_id final slot makes `par_sort_unstable_by_key` behave
/// deterministically across runs: without it, rayon's work-stealing
/// could reorder commands whose 11-tuple prefix tied, breaking
/// capture/replay and screenshot-diff workflows on scenes with many
/// identical-mesh / identical-depth entries (e.g. exterior rock
/// fields at a fixed camera distance).
pub(crate) fn draw_sort_key(
    cmd: &DrawCommand,
) -> (u8, u8, u8, u32, u32, u32, u32, u32, u32, u32, u32) {
    // Off-frustum RT-only entries cluster at the END of the sorted
    // array. Cap-on-overflow at `upload_instances` drops them first,
    // never raster draws. See the doc comment above + the
    // `MAX_INSTANCES` writeup in `scene_buffer.rs`.
    let rt_only = (!cmd.in_raster) as u8;
    if cmd.alpha_blend {
        // Fire-refraction proxies must compose after the opaque scene but
        // before their associated flame cards (`MATERIAL_KIND_EFFECT_SHADER`).
        // Sorting them in the normal alpha-over phase puts a nearer proxy
        // after its flame cards (global back-to-front order), so the proxy
        // covers the flames it is meant only to distort around.
        //
        // Scoped to `EFFECT_SHADER` specifically (REN-D12-02, #2237):
        // forcing *every* non-fire alpha-blend draw (glass, generic
        // transparents) into this later phase made a fire-refraction proxy
        // globally invert back-to-front order against unrelated
        // transparents sharing its screen space, e.g. a window pane behind
        // a torch. Draw commands don't carry a link from a flame card back
        // to the specific proxy it belongs to, so a fully general per-pair
        // fix isn't possible with this key-based sort (there is a provable
        // cycle: fire-before-flame, fire-vs-glass-by-depth, and
        // glass-vs-flame-by-depth cannot all hold at once for arbitrary
        // depths). Folding glass/generic transparents into the same phase
        // as fire-refraction fixes the common case (depth-correct against
        // fire) at the cost of also sorting them before flame cards
        // unconditionally — narrower than the previous blanket rule, and
        // a smaller-impact residual than the bug it replaces.
        let composition_phase =
            if cmd.material_kind == byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER {
                2u8
            } else {
                1u8
            };
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
        if cmd.dst_blend == GAMEBRYO_BLEND_ONE {
            // State dominates → contiguous fill/wireframe and mesh runs;
            // sort_depth is only a deterministic within-mesh tiebreaker.
            (
                rt_only,
                composition_phase,
                0u8, // additive before alpha-over
                cmd.render_layer as u32,
                cmd.two_sided as u32,
                cmd.src_blend as u32,
                cmd.dst_blend as u32,
                pack_depth_state(cmd) as u32,
                cmd.mesh_handle,
                cmd.sort_depth,
                cmd.entity_id,
            )
        } else {
            // Global depth dominates → back-to-front across render layers,
            // cull modes, blend factors and depth states. Those state axes
            // only break ties within one quantized depth bucket.
            //
            // RT-1 / #2215 — THIS is the real mechanism behind the
            // fnv/oblivion/fo4 `bench_draws_gpu_calls`/`bench_draws_batches`
            // rise that #2165/8e55a714 misattributed to
            // `needs_two_sided_blend_split`. Depth-primary order means
            // draws that would otherwise share `group_state` (same
            // pipeline/layer/cull/depth) are no longer contiguous in the
            // sorted array whenever a different-state alpha-over draw sits
            // at an intervening depth — each interruption forces a new
            // `DrawBatch` / indirect-merge boundary. Confirmed by direct
            // experiment: forcing this branch back to state-primary order
            // (matching the additive branch's shape) drops FNV
            // `FreesideAtomicWrangler` from 25 to 8 GPU calls, matching the
            // pre-883f57cd baseline almost exactly.
            //
            // This is NOT a bug to revert — it's the deliberate, correct
            // cost of the #1804/#2237 fix for a real compositing artifact
            // (a farther decal/puddle drawing after a nearer glass pane and
            // showing through as a rectangular patch). Global back-to-front
            // order for true alpha-over is required for correctness; a
            // state-primary sort can only be correct when the interleaved
            // draws never visually overlap on screen, which this key alone
            // can't determine. Treat the current `bench_draws_*` baselines
            // as the accepted cost, not a regression to chase again — see
            // `.claude/audit-baselines/runtime/`.
            (
                rt_only,
                composition_phase,
                1u8, // after additive transparent draws
                !cmd.sort_depth,
                cmd.render_layer as u32,
                cmd.two_sided as u32,
                cmd.src_blend as u32,
                cmd.dst_blend as u32,
                pack_depth_state(cmd) as u32,
                cmd.mesh_handle,
                cmd.entity_id,
            )
        }
    } else {
        (
            rt_only,
            0u8,
            0u8,
            cmd.render_layer as u32,
            cmd.two_sided as u32,
            0,
            0,
            pack_depth_state(cmd) as u32,
            cmd.mesh_handle, // group identical meshes
            cmd.sort_depth,  // front-to-back within group
            cmd.entity_id,
        )
    }
}

/// Partition RT-only occluders behind raster-visible draws, then sort only
/// the raster prefix that the draw-batch builder consumes.
///
/// RT-only entries still need an instance slot and a TLAS entry so rays from
/// visible fragments can hit them, but `draw_frame` deliberately excludes
/// them from raster batches. Sorting that tail by pipeline, mesh, and depth
/// therefore burns CPU without changing either raster output or ray-query
/// identity. The in-place partition retains the cap-on-overflow contract:
/// `MAX_INSTANCES` always drops off-frustum entries before raster draws.
///
/// Returns the length of the sorted raster prefix.
pub(crate) fn sort_draw_commands(draw_commands: &mut [DrawCommand]) -> usize {
    let mut raster_len = 0;
    for index in 0..draw_commands.len() {
        if draw_commands[index].in_raster {
            // #2682 (PERF-D2-05) — `<[T]>::swap` lowers to `ptr::swap`, which
            // performs the full three-way ~480-byte copy regardless of index
            // equality. `raster_len == index` for the whole initial run of
            // consecutive `in_raster` commands (i.e. every frame where
            // nothing gets culled, or any run under `BYRO_NO_CULL=1`), so an
            // unconditional swap wastes up to `N × 2 × sizeof(DrawCommand)`
            // of self-copy traffic in the fully-visible case.
            if raster_len != index {
                draw_commands.swap(raster_len, index);
            }
            raster_len += 1;
        }
    }

    const DRAW_SORT_PARALLEL_THRESHOLD: usize = 3000;
    let raster_draws = &mut draw_commands[..raster_len];
    if raster_draws.len() >= DRAW_SORT_PARALLEL_THRESHOLD {
        raster_draws.par_sort_unstable_by_key(draw_sort_key);
    } else {
        raster_draws.sort_unstable_by_key(draw_sort_key);
    }

    raster_len
}

/// Per-frame view + lighting + sky payload returned by
/// [`build_render_data`]. The draw command list / GPU light list /
/// bone palette / skin offsets / material table are written into the
/// scratch buffers passed by the caller; everything that's a fresh
/// per-frame value lives here.
pub(crate) struct RenderFrameView {
    pub view_proj: [f32; 16],
    pub camera_pos: [f32; 3],
    /// Cell-grid-snapped render origin — computed once in
    /// `camera::assemble_camera`, threaded to `FrameInputs` so
    /// `context::draw::draw_frame` doesn't independently recompute it
    /// from a separately-passed `camera_pos` (#2043 / PERF-D9-04).
    pub render_origin: [f32; 3],
    pub ambient: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    /// Engine-native participating medium fitted at the cell/weather
    /// translation boundary. The renderer consumes this instead of deriving
    /// extinction from the legacy distance ramp.
    pub fog_medium: crate::fog::FogMedium,
    /// XCLL cubic-fog clip distance (FNV+). Retained for diagnostics and
    /// explicit legacy compatibility; the active volumetric path consumes
    /// `fog_medium`.
    pub fog_clip: f32,
    /// XCLL cubic-fog falloff exponent (FNV+). `0.0` = no curve. See
    /// `fog_clip` for the activation contract.
    pub fog_power: f32,
    /// World-space Y anchor for height-fog density (REN-D16-01 / #2225).
    /// See [`fog_height_reference`] for how this is derived.
    pub fog_height_reference: f32,
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
    /// Exact authored perspective parameters for temporal reconstruction.
    pub camera_near: f32,
    pub camera_far: f32,
    pub camera_fov_y: f32,
    /// Lens aperture half-radius (world units). `0.0` = pinhole / no DOF.
    pub aperture: f32,
    /// Focal distance (world units). Surfaces at this depth are sharp.
    pub focus_dist: f32,
}

/// Build the view-projection matrix and draw command list from ECS queries.
///
/// All scratch buffers — `draw_commands`, `gpu_lights`, `gpu_fog_volumes`,
/// `light_sort_scratch`, `bone_world`, `skin_offsets` — are owned by the
/// caller and cleared on entry (or, for `light_sort_scratch`, at its point
/// of use in `collect_lights`) so
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
    gpu_fog_volumes: &mut Vec<byroredux_renderer::GpuFogVolume>,
    // Decorate-sort scratch for `collect_lights`' GI-priority ordering.
    // Caller-owned purely so its capacity survives the frame (#2172).
    light_sort_scratch: &mut Vec<(f32, byroredux_renderer::GpuLight)>,
    bone_world: &mut Vec<[[f32; 4]; 4]>,
    skin_offsets: &mut FxHashMap<EntityId, u32>,
    skin_slot_pool: &mut SkinSlotPool,
    material_table: &mut MaterialTable,
    particle_quad_handle: Option<u32>,
) -> RenderFrameView {
    let frame_count = FRAME_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    draw_commands.clear();
    water_commands.clear();
    gpu_lights.clear();
    gpu_fog_volumes.clear();
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
        render_origin,
        cam_right,
        cam_up,
        cam_forward,
        proj_mat,
        camera_near,
        camera_far,
        camera_fov_y,
        aperture,
        focus_dist,
    } = camera::assemble_camera(world);

    fog_volumes::collect_fog_volumes(world, &frustum, cam_pos, gpu_fog_volumes);

    // Height-fog reference altitude — see `fog_height_reference` doc.
    let fog_height_ref = fog_height_reference(world, cam_pos);

    // Static mesh main loop — see `render::static_meshes::collect_static_mesh_draws`.
    let t_static = mark(profile);
    static_meshes::collect_static_mesh_draws(
        world,
        &frustum,
        vp_mat,
        cam_pos,
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
        &frustum,
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
    // #934 / PERF-DC-01, re-measured for #2173 — rayon's fork-join
    // overhead loses to serial `sort_unstable_by_key` below ~3K elements
    // on the closure-extracted key.
    //
    // Re-measured 2026-07-25 on a 7950X after `883f57cd` widened the sort
    // key from 10 to 11 tuples (the stable surface ID), which raised
    // per-comparison cost and moved the crossover UP. Run
    // `manual_bench_draw_sort_serial_vs_parallel` in
    // `byroredux/src/render/draw_sort_key_tests.rs` to reproduce:
    //
    //     N=  400: serial  24µs vs parallel  30µs (serial 19% faster)
    //     N=  800: serial  54µs vs parallel  71µs (serial 24% faster)
    //     N= 1500: serial 115µs vs parallel 144µs (serial 20% faster)
    //     N= 2000: serial 161µs vs parallel 198µs (serial 19% faster)
    //     N= 2250: serial 204µs vs parallel 222µs (serial  8% faster)
    //     N= 2750: serial 293µs vs parallel 300µs (≈ tied)
    //     N= 3000: serial 348µs vs parallel 362µs (≈ tied, run-to-run
    //              ratios straddled 1.0 across three runs)
    //     N= 5000: serial 596µs vs parallel 446µs (parallel 34% faster)
    //     N=  10K: serial 1429µs vs parallel 873µs (parallel 64% faster)
    //
    // The old table put N=2000 at "tied" and set the threshold there. With
    // the wider key serial is still ~19% ahead at 2000 and holds through
    // ~2750, so the old constant dispatched to rayon across a whole band
    // where serial wins. 3000 is the first size where the two are reliably
    // interchangeable and above which parallel pulls away.
    //
    // PERF-D2-03 / #2691 — the old prose here claimed "typical Bethesda cell
    // counts sit in 400–1500 … so serial remains the common path either way".
    // The repo's own checked-in runtime baselines contradict that: see the
    // `bench_draws_cmds` column of `.claude/audit-baselines/runtime/*.tsv`,
    // where exactly one of five cells falls in that band, three sit in the
    // 1800–2600 range, and the FO4 baseline is *above* this gate and takes the
    // parallel path. Cited rather than transcribed, per the audit's
    // cite-don't-copy rule — a number copied here is a number that rots.
    //
    // The constant is unaffected: the crossover table above is what places it,
    // and it still holds. This note exists so the next person tuning it does
    // not reason from the stale band and lower the gate back into the
    // 2000–2750 range where that same table measures serial winning. The

    // threshold is applied to the raster-visible prefix, not the complete
    // instance/TLAS list: RT-only occluders never enter a raster batch and
    // do not benefit from pipeline/mesh/depth ordering. Large visible sets
    // still use `par_sort_unstable_by_key`; scenes whose total draw count is
    // large only because of off-frustum ray-query occluders avoid sorting
    // that tail entirely.
    let ms_particles = took(t_particles);
    let t_sort = mark(profile);
    let raster_draws = sort_draw_commands(draw_commands);
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

    // Collect lights from ECS — cell directional + placed point lights.
    // See `render::lights::collect_lights`.
    let t_lights = mark(profile);
    lights::collect_lights(world, gpu_lights, light_sort_scratch);
    let ms_lights = took(t_lights);
    if profile {
        log::info!(
            "build_render_data: skinned={ms_skin:.2}ms static_loop={ms_static:.2}ms ({n_draws} draws, {raster_draws} raster) particles={ms_particles:.2}ms sort={ms_sort:.2}ms lights={ms_lights:.2}ms"
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
    // XCLL ambient passed through as-is. This is the only flat/cube cell-fill
    // input; XCLL directional colour follows the ordinary shadowable-light
    // path so it cannot bypass N.L, visibility, or AO.
    let ambient = cell_lit
        .as_ref()
        .map(|l| l.ambient)
        .unwrap_or([0.08, 0.08, 0.08]);
    // Retain the authored fog color and legacy ramp for diagnostics and
    // compatibility UBOs. The active path consumes `fog_medium`; exterior
    // weather updates all of these values together in `weather_system`.
    let fog_color = cell_lit.as_ref().map(|l| l.fog_color).unwrap_or([0.0; 3]);
    // CLMT-authored fog_near can be negative. Clamp only the retained legacy
    // value; the canonical medium was already fit from the original ramp at
    // translation time. This keeps the camera and composite UBOs aligned.
    let fog_near = cell_lit
        .as_ref()
        .map(|l| l.fog_near.max(0.0))
        .unwrap_or(0.0);
    let fog_far = cell_lit.as_ref().map(|l| l.fog_far).unwrap_or(0.0);
    let authored_fog_medium = cell_lit
        .as_ref()
        .map(|lighting| lighting.fog_medium)
        .unwrap_or_default();
    // `BYRO_FOG_NEAR` / `BYRO_FOG_FAR` overrides (Bethesda units) for
    // offline / diagnostic renders. The authored weather/XCLL ramp matches
    // the original game's ~tens-of-K-BU view distance; when inspecting the
    // distant-terrain LOD ring (`cell_loader::terrain_lod`, ~197K BU) those
    // values wash the far terrain to the fog colour, so this lever lets a
    // capture push fog out to match. Applied at this single consumption
    // point (downstream of `weather_system`'s per-frame fog write) so it is
    // authoritative every frame. Cached so the hot path doesn't `getenv`
    // per frame. No-op when unset. Mirrors the `BYRO_HOUR` convention.
    let (fog_near, fog_far, fog_overridden) = apply_fog_overrides(fog_near, fog_far);
    // Diagnostic overrides deliberately bypass the load-time table. Keep the
    // normal hot path conversion-free; only an explicitly enabled override
    // pays the fit here.
    let fog_medium = if fog_overridden {
        crate::fog::FogMedium::from_legacy_ramp(fog_near, fog_far, None)
    } else {
        authored_fog_medium
    };
    // Retain the authored XCLL cubic-fog curve for diagnostics and a future
    // explicit compatibility toggle. The physical path does not evaluate it.
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
        render_origin: render_origin.to_array(),
        ambient,
        fog_color,
        fog_near,
        fog_far,
        fog_medium,
        fog_clip,
        fog_power,
        fog_height_reference: fog_height_ref,
        sky,
        cam_right: cam_right.to_array(),
        cam_up: cam_up.to_array(),
        cam_forward: cam_forward.to_array(),
        proj_mat: proj_mat.to_cols_array(),
        camera_near,
        camera_far,
        camera_fov_y,
        aperture,
        focus_dist,
    }
}

// Per-section sub-modules (TD9-001 sweep, #1115). Each sibling owns
// one of the 8 query families in `build_render_data`; the parent
// orchestrator above acquires the World queries once and threads
// references through.
mod camera;
mod fog_volumes;
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
mod render_origin_shared_tests;
#[cfg(test)]
mod sort_key_tests;
#[cfg(test)]
mod static_mesh_fx_skip_tests;
#[cfg(test)]
mod variant_pack_gating_tests;
#[cfg(test)]
mod water_wave_params_tests;
