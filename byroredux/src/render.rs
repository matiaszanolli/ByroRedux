//! Per-frame render data collection from ECS queries.

use byroredux_core::ecs::components::water::{WaterFlow, WaterKind, WaterPlane};
use byroredux_core::ecs::{
    ActiveCamera, AnimatedUvTransform, AnimatedVisibility, Camera, EntityId, GlobalTransform,
    LightSource, Material, MeshHandle, ParticleEmitter, RenderLayer, SkinnedMesh, TextureHandle,
    TotalTime, Transform, World, WorldBound, MAX_BONES_PER_MESH,
};
use byroredux_core::math::{Mat4, Quat, Vec3, Vec4};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::vulkan::scene_buffer::MAX_TOTAL_BONES;
use byroredux_renderer::vulkan::water::{WaterDrawCommand, WaterPush};
use byroredux_renderer::{MaterialTable, SkyParams};
use rayon::slice::ParallelSliceMut;
use std::collections::HashMap;
use std::sync::Once;

use crate::components::{
    AlphaBlend, CellLightingRes, CloudSimState, DarkMapHandle, ExtraTextureMaps, NormalMapHandle,
    SkyParamsRes, TerrainTileSlot, TwoSided,
};

/// Once-per-session gate for the bone-palette overflow warn — see the
/// guard in `build_render_data`. A `Once` keeps the log out of the
/// per-frame hot path so the warn fires exactly the first time a
/// cell's skinned-mesh count exceeds `MAX_TOTAL_BONES`.
static BONE_PALETTE_OVERFLOW_WARNED: Once = Once::new();

/// M41.0 Phase 1b.x followup — frame-gated dump of any palette slot that
/// resolved to `Mat4::IDENTITY` after propagation. `compute_palette_into`
/// returns IDENTITY when (a) the bone entity was `None` at skin attach
/// time (bone-name not in the external skeleton map and not in the local
/// `node_by_name` either) or (b) the `world_transform_of` closure
/// returned `None` (entity has no `GlobalTransform`). Both cases produce
/// the long-thin-ribbon vertex artifact: vertices weighted to the
/// IDENTITY slot land at NIF skin-space coords, vertices weighted to
/// well-resolved slots land at world coords, and triangles span the gap.
static SKIN_DROPOUT_DUMPED: Once = Once::new();
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

/// Six frustum half-planes extracted from a view-projection matrix.
///
/// Uses the Gribb/Hartmann method: each plane is (a, b, c, d) where
/// `ax + by + cz + d >= 0` means the point is inside. Planes are
/// unnormalized — we normalize once at construction so the sphere
/// test can compare directly against radius.
struct FrustumPlanes {
    planes: [Vec4; 6],
}

impl FrustumPlanes {
    fn from_view_proj(m: Mat4) -> Self {
        let r0 = m.row(0);
        let r1 = m.row(1);
        let r2 = m.row(2);
        let r3 = m.row(3);

        let mut planes = [
            r3 + r0, // left
            r3 - r0, // right
            r3 + r1, // bottom
            r3 - r1, // top
            r3 + r2, // near
            r3 - r2, // far
        ];

        for p in &mut planes {
            let len = Vec3::new(p.x, p.y, p.z).length();
            if len > 1e-10 {
                *p /= len;
            }
        }

        Self { planes }
    }

    fn contains_sphere(&self, center: Vec3, radius: f32) -> bool {
        for p in &self.planes {
            let dist = p.x * center.x + p.y * center.y + p.z * center.z + p.w;
            if dist < -radius {
                return false;
            }
        }
        true
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
pub(crate) fn draw_sort_key(
    cmd: &DrawCommand,
) -> (u8, u8, u8, u8, u32, u32, u32, u32, u32, u32) {
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
/// All scratch buffers — `draw_commands`, `gpu_lights`, `bone_palette`,
/// `skin_offsets`, `palette_scratch` — are owned by the caller and
/// cleared on entry so their heap allocations persist across frames.
/// See #253 (`skin_offsets`), #243 (`draw_commands` / `gpu_lights` /
/// `bone_palette` scratch pattern), #509 (`palette_scratch`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_render_data(
    world: &World,
    draw_commands: &mut Vec<DrawCommand>,
    water_commands: &mut Vec<WaterDrawCommand>,
    gpu_lights: &mut Vec<byroredux_renderer::GpuLight>,
    bone_palette: &mut Vec<[[f32; 4]; 4]>,
    skin_offsets: &mut HashMap<EntityId, u32>,
    palette_scratch: &mut Vec<Mat4>,
    material_table: &mut MaterialTable,
    particle_quad_handle: Option<u32>,
) -> RenderFrameView {
    let frame_count = FRAME_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    draw_commands.clear();
    water_commands.clear();
    gpu_lights.clear();
    bone_palette.clear();
    skin_offsets.clear();
    // R1 Phase 2 — clear the material table so the per-frame dedup
    // starts from scratch. `intern` calls below populate it as the
    // mesh / particle paths emit DrawCommands.
    material_table.clear();
    // Slot 0 is always identity — rigid meshes tagged with bone_offset=0
    // that somehow hit the skinning path fall here harmlessly.
    bone_palette.push([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    // First pass: walk SkinnedMesh entities, compute each mesh's bone
    // palette slice, and record `entity → bone_offset` so the draw loop
    // below can stamp it onto the DrawCommand. Each skinned mesh reserves
    // exactly MAX_BONES_PER_MESH slots so per-mesh bone_offset arithmetic
    // stays trivial.
    //
    // Both queries are read-only (the palette closure dereferences
    // `GlobalTransform::to_matrix()` and the skin iter borrows each
    // `SkinnedMesh` immutably), so two separate read queries give the
    // correct lock pattern — the previous `query_2_mut::<GT, SkinnedMesh>`
    // took an unnecessary write lock on SkinnedMesh. See #246.
    let gt_q = world.query::<GlobalTransform>();
    let skin_q = world.query::<SkinnedMesh>();
    if let (Some(gt_q), Some(skin_q)) = (gt_q, skin_q) {
        // `palette_scratch` is owned by the caller; `compute_palette_into`
        // clears it internally before refilling, so any previous-frame
        // capacity is reused without a fresh allocation. See #509.
        for (entity, skin) in skin_q.iter() {
            // M29 — defensive guard against silent palette truncation.
            // `bone_buffers` are sized for `MAX_TOTAL_BONES` slots and the
            // renderer's `upload_bones` clamps writes (scene_buffer.rs:982);
            // every skinned mesh past the ceiling silently falls back to
            // bind pose with no error. Today this is unreachable (no NPC
            // spawning), but it'll fire the moment M41 lands a populated
            // cell. Log once per session and stop padding the palette so
            // the renderer's clamp is never reached.
            if bone_palette.len() + MAX_BONES_PER_MESH > MAX_TOTAL_BONES {
                BONE_PALETTE_OVERFLOW_WARNED.call_once(|| {
                    log::warn!(
                        "bone_palette: skinned-mesh count exceeds MAX_TOTAL_BONES={} \
                         slots ({} bones × {} meshes already pushed); remaining \
                         skinned meshes silently fall back to bind pose. Bump \
                         MAX_TOTAL_BONES or implement variable-stride packing (M29.5).",
                        MAX_TOTAL_BONES,
                        MAX_BONES_PER_MESH,
                        skin_offsets.len(),
                    );
                });
                break;
            }
            let offset = bone_palette.len() as u32;
            // World-lookup closure — reads GlobalTransform for each bone
            // entity through the same query guard. Missing bones fall
            // back to identity inside compute_palette_into.
            skin.compute_palette_into(palette_scratch, |bone_entity| {
                gt_q.get(bone_entity).map(|gt| gt.to_matrix())
            });
            // M41.0 Phase 1b.x followup — flag any palette slot that
            // resolved to identity post-propagation. These slots cause
            // the ribbon-vertex artifact described on
            // SKIN_DROPOUT_DUMPED.
            //
            // Gated on `debug_assertions` (#929 / PERF-CPU-01): the
            // outer `SKIN_DROPOUT_DUMPED.call_once` short-circuits the
            // log after the first hit, but the Vec allocation + per-
            // bone identity check still ran every frame for every
            // skinned mesh in release. The compiler folds
            // `cfg!(debug_assertions)` to a const and DCEs the entire
            // branch in release, restoring zero-cost. Debug builds
            // (developer + CI test profile) keep the diagnostic for
            // any future regression investigation.
            if cfg!(debug_assertions) && frame_count >= 60 {
                let mut dropout_slots: Vec<(usize, bool)> = Vec::new();
                for (i, ((bone_e, _bind), pal)) in skin
                    .bones
                    .iter()
                    .zip(skin.bind_inverses.iter())
                    .zip(palette_scratch.iter())
                    .enumerate()
                {
                    let m = *pal;
                    let is_identity = (m.x_axis - byroredux_core::math::Vec4::X).length_squared()
                        < 1e-6
                        && (m.y_axis - byroredux_core::math::Vec4::Y).length_squared() < 1e-6
                        && (m.z_axis - byroredux_core::math::Vec4::Z).length_squared() < 1e-6
                        && (m.w_axis - byroredux_core::math::Vec4::W).length_squared() < 1e-6;
                    if is_identity {
                        dropout_slots.push((i, bone_e.is_none()));
                    }
                }
                if !dropout_slots.is_empty() {
                    SKIN_DROPOUT_DUMPED.call_once(|| {
                        log::warn!(
                            "Phase 1b.x DROPOUT — skinned mesh entity {:?}: {} of {} palette \
                             slots are IDENTITY (frame {}). Sample (slot, bone_was_None): {:?}",
                            entity,
                            dropout_slots.len(),
                            skin.bones.len(),
                            frame_count,
                            &dropout_slots[..dropout_slots.len().min(8)],
                        );
                    });
                }
            }
            // Pad every skinned mesh to MAX_BONES_PER_MESH so per-mesh
            // bone offsets are trivially `offset + local_index` and the
            // shader doesn't need a per-mesh bone count.
            for mat in palette_scratch.iter() {
                bone_palette.push(mat.to_cols_array_2d());
            }
            for _ in palette_scratch.len()..MAX_BONES_PER_MESH {
                bone_palette.push([
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ]);
            }
            skin_offsets.insert(entity, offset);
            let _ = entity; // silence unused if debug_assertions off
        }
    }

    // Get camera view-projection + build frustum planes for culling.
    // `cam_pos` is also captured here for the particle billboard pass —
    // each live particle needs the active camera's world position to
    // compute a face-camera rotation matrix in `build_particle_draws`.
    let mut cam_pos = Vec3::ZERO;
    let (view_proj, frustum, vp_mat) = if let Some(active) = world.try_resource::<ActiveCamera>() {
        let cam_entity = active.0;
        drop(active);

        let cam_q = world.query::<Camera>();
        let transform_q = world.query::<Transform>();

        let vp = match (cam_q, transform_q) {
            (Some(cq), Some(tq)) => {
                let cam = cq.get(cam_entity);
                let t = tq.get(cam_entity);
                match (cam, t) {
                    (Some(c), Some(t)) => {
                        cam_pos = t.translation;
                        c.projection_matrix() * Camera::view_matrix(t)
                    }
                    _ => Mat4::IDENTITY,
                }
            }
            _ => Mat4::IDENTITY,
        };
        let frustum = FrustumPlanes::from_view_proj(vp);
        (vp.to_cols_array(), frustum, vp)
    } else {
        (
            Mat4::IDENTITY.to_cols_array(),
            FrustumPlanes::from_view_proj(Mat4::IDENTITY),
            Mat4::IDENTITY,
        )
    };

    // ── Render-data query bundle (#246) ──────────────────────────────
    //
    // Collect draw commands from entities with (GlobalTransform,
    // MeshHandle). Everything here is read-only, so each query is an
    // independent `QueryRead`. Two observations:
    //
    //   1. The ECS has no `query_n_mut!` macro for acquiring N optional
    //      components in one call, so we acquire each component
    //      separately. That's ~13 RwLock read acquisitions per frame; all
    //      reads can coexist (no deadlock risk), so no TypeId-sorted
    //      bundling is needed.
    //
    //   2. The bundle is held across the full `for (entity, mesh) in
    //      mq.iter()` loop. No system that writes these components
    //      runs concurrently (render runs outside the scheduler in
    //      `RedrawRequested`), so read contention is theoretical.
    //
    //   3. #501 / M40 — when the scheduler goes parallel (per CLAUDE.md
    //      architecture invariants), any concurrent writer to one of
    //      these ~13 storages will stall for the full build window
    //      (~1.5–2 ms). Fix at that point by introducing a
    //      `RenderExtract` stage that snapshots the per-entity data
    //      into a `Vec<RenderInstance>` resource in one pass and
    //      iterates it here with zero locks held (Bevy's extract-stage
    //      pattern). Deferred deliberately — implementing before M40
    //      lands would lock in a design without the constraints of the
    //      actual parallel scheduler to inform it, and would add
    //      ~0.5 ms/frame for zero benefit today.
    //
    // `GlobalTransform` and `MeshHandle` are required — if either is
    // absent there are no meshes to emit, so the whole collection path
    // is skipped. The other eight components are optional per-entity
    // modifiers (texture, alpha, two-sided, decal, visibility,
    // material, normal map, world bound) and stay as `Option<QueryRead>`
    // so entities without them fall through to the fallback path inside
    // the loop.
    let tq = world.query::<GlobalTransform>();
    let mq = world.query::<MeshHandle>();
    let tex_q = world.query::<TextureHandle>();
    let alpha_q = world.query::<AlphaBlend>();
    let two_sided_q = world.query::<TwoSided>();
    let vis_q = world.query::<AnimatedVisibility>();
    let mat_q = world.query::<Material>();
    // #525 — `AnimatedUvTransform` overrides the static
    // `Material::uv_offset` / `uv_scale` when an entity has an active
    // UV-scrolling controller (water, lava, conveyor belts, flickering
    // HUD backdrops). The component lands the per-axis values
    // independently so a single channel can drive offset.x while the
    // material's authored offset.y stays at 0 — the renderer reads the
    // full Vec2 transform here. Identity defaults (0, 0) / (1, 1)
    // mean the override is a no-op until the animation system writes
    // a non-identity slot.
    let anim_uv_q = world.query::<AnimatedUvTransform>();
    // #renderlayer — per-entity content-class for the depth-bias
    // ladder (Architecture / Clutter / Actor / Decal). Attached at
    // cell-load time from the REFR's base-record `RecordType` (see
    // `RecordType::render_layer`). Absent component falls back to
    // `Architecture` (zero bias) — identical to pre-fix behaviour.
    // The Decal escalation (`mesh.is_decal || alpha_test_func != 0`)
    // is applied at spawn time, not here, so this query reads the
    // final per-entity layer directly.
    let render_layer_q = world.query::<RenderLayer>();
    let nmap_q = world.query::<NormalMapHandle>();
    let dmap_q = world.query::<DarkMapHandle>();
    let extra_q = world.query::<ExtraTextureMaps>();
    let terrain_tile_q = world.query::<TerrainTileSlot>();
    let wb_q = world.query::<WorldBound>();
    if let (Some(tq), Some(mq)) = (tq, mq) {
        for (entity, mesh) in mq.iter() {
            // Skip entities hidden by animation.
            let visible = vis_q
                .as_ref()
                .and_then(|q| q.get(entity))
                .map(|v| v.0)
                .unwrap_or(true);
            if !visible {
                continue;
            }

            // Frustum cull: flag entities whose WorldBound is entirely
            // outside the view frustum with `in_raster = false`. The
            // draw loop skips rasterization for them but they still
            // reach the TLAS so on-screen fragments can hit their
            // occluder/reflector geometry via ray queries. Entities
            // without a WorldBound (or radius 0, i.e. not yet computed)
            // pass through as visible. See #237 (original cull) +
            // #516 (split raster / TLAS predicate).
            let in_raster = match wb_q.as_ref().and_then(|q| q.get(entity)) {
                Some(wb) if wb.radius > 0.0 => frustum.contains_sphere(wb.center, wb.radius),
                _ => true,
            };

            if let Some(transform) = tq.get(entity) {
                let tex_handle = tex_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|t| t.0)
                    .unwrap_or(0);
                let alpha_comp = alpha_q.as_ref().and_then(|q| q.get(entity));
                let alpha_blend = alpha_comp.is_some();
                let (src_blend, dst_blend) = alpha_comp
                    .map(|a| (a.src_blend, a.dst_blend))
                    .unwrap_or((6, 7)); // SRC_ALPHA / INV_SRC_ALPHA defaults
                let two_sided = two_sided_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
                // #renderlayer — `is_decal` is now derived from
                // `RenderLayer::Decal`, not a separate `Decal` marker.
                // The shader / GpuInstance flag paths still want a
                // bool, but the ECS source-of-truth is the layer enum.
                let render_layer_for_entity = render_layer_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .copied()
                    .unwrap_or_default();
                let is_decal = render_layer_for_entity == RenderLayer::Decal;
                let bone_offset = skin_offsets.get(&entity).copied().unwrap_or(0);
                let normal_map_index = nmap_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|n| n.0)
                    .unwrap_or(0);
                let dark_map_index = dmap_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|d| d.0)
                    .unwrap_or(0);
                // #399 — three NiTexturingProperty extra slots packed in
                // one component to keep the per-frame query count fixed.
                // Default to 0 (= no map; fragment shader falls through
                // to the inline material constants) for entities that
                // never had `ExtraTextureMaps` attached at cell load.
                let (
                    glow_map_index,
                    detail_map_index,
                    gloss_map_index,
                    parallax_map_index,
                    env_map_index,
                    env_mask_index,
                    parallax_height_scale,
                    parallax_max_passes,
                ) = extra_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|e| {
                        (
                            e.glow,
                            e.detail,
                            e.gloss,
                            e.parallax,
                            e.env,
                            e.env_mask,
                            e.parallax_height_scale,
                            e.parallax_max_passes,
                        )
                    })
                    .unwrap_or((0, 0, 0, 0, 0, 0, 0.04, 4.0));

                // Terrain splat tile index (#470). Only LAND terrain
                // entities carry the component; statics pass `None`.
                let terrain_tile_index = terrain_tile_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|s| s.0);

                // Material data + PBR classification.
                let mat = mat_q.as_ref().and_then(|q| q.get(entity));

                // Skip Gamebryo effect meshes (crossed glow quads, god rays).
                // These are sprite-billboard fakes for bloom halos — in a RT
                // renderer the actual point light already provides illumination
                // and these quads just render as blown-out white surfaces.
                if let Some(m) = mat {
                    if let Some(ref tp) = m.texture_path {
                        // Case-insensitive contains without allocation (#286).
                        fn contains_ci(haystack: &str, needle: &str) -> bool {
                            haystack
                                .as_bytes()
                                .windows(needle.len())
                                .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
                        }
                        if contains_ci(tp, "effects\\fx")
                            || contains_ci(tp, "effects/fx")
                            || contains_ci(tp, "fxsoftglow")
                            || contains_ci(tp, "fxpartglow")
                            || contains_ci(tp, "fxparttiny")
                            || contains_ci(tp, "fxlightrays")
                        {
                            continue;
                        }
                    }
                }

                let (
                    roughness,
                    metalness,
                    emissive_mult,
                    emissive_color,
                    specular_strength,
                    specular_color,
                    diffuse_color,
                    ambient_color,
                    alpha_threshold,
                    alpha_test_func,
                ) = if let Some(m) = mat {
                    let pbr = m.classify_pbr(m.texture_path.as_deref());
                    let thresh = if m.alpha_test { m.alpha_threshold } else { 0.0 };
                    let func = if m.alpha_test {
                        m.alpha_test_func as u32
                    } else {
                        0
                    };
                    (
                        pbr.roughness,
                        pbr.metalness,
                        m.emissive_mult,
                        m.emissive_color,
                        m.specular_strength,
                        m.specular_color,
                        m.diffuse_color,
                        m.ambient_color,
                        thresh,
                        func,
                    )
                } else {
                    // No Material → identity tint, identity ambient.
                    (
                        0.5, 0.0, 0.0, [0.0; 3], 1.0, [1.0; 3], [1.0; 3], [1.0; 3], 0.0, 0u32,
                    )
                };

                // #398 — depth state from NiZBufferProperty (Material).
                // Defaults match the Gamebryo runtime defaults the
                // pre-#398 hardcoded pipeline state used: depth test+
                // write on, LESSEQUAL.
                let (z_test, z_write, z_function) = mat
                    .map(|m| (m.z_test, m.z_write, m.z_function))
                    .unwrap_or((true, true, 3));

                // Geometry SSBO offsets for RT reflection UV lookups.
                let (v_off, i_off, v_count) = {
                    // SAFETY: mesh_registry is accessed immutably through the
                    // VulkanContext ref, not through the ECS.
                    // We can't access it here directly; pass zeros and let draw.rs fill from mesh_registry.
                    (0u32, 0u32, 0u32)
                };

                // Camera-space depth for draw order sorting. Transform
                // the model position through the VP matrix and use the
                // clip-space W (≈ linear depth) for sorting.
                let model_mat = transform.to_matrix();
                let pos = model_mat.col(3); // translation column
                let clip = vp_mat * pos;
                let sort_depth = f32_sortable_u32(clip.w);

                // Classify the effective material kind for shader
                // dispatch. BSLightingShaderProperty.shader_type is
                // forwarded verbatim for Skyrim+ variants (0..=19 —
                // SkinTint / HairTint / EyeEnvmap / MultiLayerParallax /
                // etc., see #344). Engine-synthesized kinds live in
                // the high range (100+):
                //   - `MATERIAL_KIND_GLASS` when the material is
                //     alpha-blend + low-metal + not a decal + low-
                //     roughness. The roughness gate was added after
                //     Tier C Phase 2 shipped with only the first three
                //     criteria: FNV wood tables and picture frames
                //     carry `NiAlphaProperty.flags=0x12ED` (blend=1) for
                //     edge smoothing, not because the surface is glass.
                //     Under the three-criterion rule they were tagged
                //     as glass and rendered through the RT refract /
                //     Fresnel path as near-white surfaces. Roughness
                //     classified from the texture path (glass=0.1,
                //     wood=0.7, fabric=0.95) cleanly separates actual
                //     transparent-refractive materials from
                //     alpha-blend-for-edges. See follow-up to #515.
                let base_material_kind = mat.map(|m| m.material_kind).unwrap_or(0);
                // Engine-synthesized kinds (>= 100) are pre-classified
                // upstream and must win over the heuristic Glass branch.
                // Today: BSEffectShaderProperty meshes arrive with
                // material_kind=101 (MATERIAL_KIND_EFFECT_SHADER) set at
                // import; the glass heuristic (alpha_blend + low metal +
                // low roughness) would otherwise misclassify a fire plane
                // as glass. See #706.
                let material_kind = if base_material_kind >= 100 {
                    base_material_kind
                } else if alpha_blend && !is_decal && metalness < 0.3 && roughness < 0.4 {
                    byroredux_renderer::MATERIAL_KIND_GLASS
                } else {
                    base_material_kind
                };

                // Glass single-sided override — Bethesda authors many
                // glass meshes (drinking glasses, pitchers, bottles)
                // with `TRIANGLE_FACING_CULL_DISABLE` so both inside
                // and outside walls render. With alpha blending and
                // no intra-mesh per-triangle depth sort, the back
                // walls composite over the front walls in arbitrary
                // mesh-vertex order, producing the visible "wireframe
                // through the glass" artifact on Prospector cups.
                //
                // The inter-mesh depth sort in `draw_sort_key` only
                // orders ENTIRE meshes back-to-front; per-triangle
                // ordering within one mesh would need OIT or per-
                // triangle CPU sort (impractical real-time). Effect-
                // shader (material_kind ≥ 100) and other two-sided
                // alpha — fire planes, foliage, banner cloth — keep
                // their authored two-sided behavior because they
                // typically aren't volumetric closed meshes.
                //
                // Trade-off: glass cups no longer render their
                // interior walls. For Bethesda content this is
                // fine — the alpha-blended exterior plus the IOR
                // refraction path in triangle.frag's glassIOR
                // branch already shows the scene through the cup.
                let two_sided = if material_kind == byroredux_renderer::MATERIAL_KIND_GLASS {
                    false
                } else {
                    two_sided
                };

                // #562 / #619 — Skyrim+ BSLightingShaderProperty variant
                // payload. Each field group is gated on the matching
                // `material_kind` so the pack runs only for materials
                // whose shader branch reads it. Pre-#619 every chain
                // ran on every draw — wasted work on the vast majority
                // of materials (every non-Skyrim mesh + every Skyrim
                // static, ~99% of a typical cell). `GpuInstance::default`
                // already zeroes the slots so non-active kinds emit
                // neutral output identical to pre-fix.
                //
                // Variant ↔ field mapping (must mirror the
                // `materialKind == N` ladder in triangle.frag:769-796):
                //   5  SkinTint            → `skin_tint_*`        (live)
                //   6  HairTint            → `hair_tint_*`        (live)
                //   11 MultiLayerParallax  → `multi_layer_*`      (stub)
                //   14 SparkleSnow         → `sparkle_*`          (live)
                //   16 EyeEnvmap           → `eye_*`              (stub)
                //
                // Variants 11 + 16 are shader stubs today (#619); the
                // pack still runs on those kinds so the data is already
                // plumbed when the shader branches land.
                let stf = mat.and_then(|m| m.shader_type_fields.as_deref());
                let skin_tint_rgba = if base_material_kind == 5 {
                    stf.and_then(|f| {
                        f.skin_tint_color
                            .map(|c| [c[0], c[1], c[2], f.skin_tint_alpha.unwrap_or(1.0)])
                    })
                    .unwrap_or([0.0; 4])
                } else {
                    [0.0; 4]
                };
                let hair_tint_rgb = if base_material_kind == 6 {
                    stf.and_then(|f| f.hair_tint_color).unwrap_or([0.0; 3])
                } else {
                    [0.0; 3]
                };
                let sparkle_rgba = if base_material_kind == 14 {
                    stf.and_then(|f| f.sparkle_parameters).unwrap_or([0.0; 4])
                } else {
                    [0.0; 4]
                };
                let (
                    multi_layer_envmap_strength,
                    multi_layer_inner_thickness,
                    multi_layer_refraction_scale,
                    multi_layer_inner_scale,
                ) = if base_material_kind == 11 {
                    (
                        stf.and_then(|f| f.multi_layer_envmap_strength)
                            .unwrap_or(0.0),
                        stf.and_then(|f| f.multi_layer_inner_thickness)
                            .unwrap_or(0.0),
                        stf.and_then(|f| f.multi_layer_refraction_scale)
                            .unwrap_or(0.0),
                        stf.and_then(|f| f.multi_layer_inner_layer_scale)
                            .unwrap_or([1.0, 1.0]),
                    )
                } else {
                    (0.0, 0.0, 0.0, [1.0, 1.0])
                };
                let (eye_left_center, eye_cubemap_scale, eye_right_center) =
                    if base_material_kind == 16 {
                        (
                            stf.and_then(|f| f.eye_left_reflection_center)
                                .unwrap_or([0.0; 3]),
                            stf.and_then(|f| f.eye_cubemap_scale).unwrap_or(0.0),
                            stf.and_then(|f| f.eye_right_reflection_center)
                                .unwrap_or([0.0; 3]),
                        )
                    } else {
                        ([0.0; 3], 0.0, [0.0; 3])
                    };
                // #620 / SK-D4-01 — BSEffectShaderProperty falloff cone
                // pulled from `MaterialInfo.effect_shader` (Skyrim+
                // BSEffectShaderProperty path) or `no_lighting_falloff`
                // (FO3/FNV BSShaderNoLightingProperty SIBLING path,
                // #451). Both populate the same `[start_angle,
                // stop_angle, start_opacity, stop_opacity, soft_depth]`
                // tuple; the FO3/FNV path leaves `soft_depth = 0.0` since
                // BSShaderNoLightingProperty has no soft-depth field. The
                // fragment shader gates the read on `material_kind == 101`,
                // so non-effect materials emit the identity-pass-through
                // tuple `[1.0, 1.0, 1.0, 1.0, 0.0]` (no view-angle fade,
                // no soft-depth fade).
                let effect_falloff =
                    if material_kind == byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER {
                        mat.and_then(|m| m.effect_falloff)
                            .map(|f| {
                                [
                                    f.start_angle,
                                    f.stop_angle,
                                    f.start_opacity,
                                    f.stop_opacity,
                                    f.soft_falloff_depth,
                                ]
                            })
                            .unwrap_or([1.0, 1.0, 1.0, 1.0, 0.0])
                    } else {
                        [1.0, 1.0, 1.0, 1.0, 0.0]
                    };

                let mut cmd = DrawCommand {
                    mesh_handle: mesh.0,
                    texture_handle: tex_handle,
                    model_matrix: model_mat.to_cols_array(),
                    alpha_blend,
                    src_blend,
                    dst_blend,
                    two_sided,
                    is_decal,
                    // #renderlayer — final per-entity layer (already
                    // computed above as `render_layer_for_entity`,
                    // includes the spawn-time `Decal` escalation for
                    // alpha-tested overlays).
                    render_layer: render_layer_for_entity,
                    bone_offset,
                    normal_map_index,
                    dark_map_index,
                    glow_map_index,
                    detail_map_index,
                    gloss_map_index,
                    parallax_map_index,
                    parallax_height_scale,
                    parallax_max_passes,
                    env_map_index,
                    env_mask_index,
                    alpha_threshold,
                    alpha_test_func,
                    roughness,
                    metalness,
                    emissive_mult,
                    emissive_color,
                    specular_strength,
                    specular_color,
                    diffuse_color,
                    ambient_color,
                    vertex_offset: v_off,
                    index_offset: i_off,
                    vertex_count: v_count,
                    sort_depth,
                    in_tlas: true,
                    in_raster,
                    entity_id: entity,
                    // #492 — UV transform + material alpha pulled from
                    // the `Material` component (already populated by
                    // the NIF importer and/or the FO4 BGSM resolver).
                    // Identity defaults when the mesh has no Material.
                    //
                    // #525 — `AnimatedUvTransform`, when present, REPLACES
                    // the static authored values entirely (rather than
                    // adds / multiplies). The component starts at
                    // identity (0, 0) / (1, 1) on insertion and the
                    // animation system writes per-channel slots over
                    // time; the static `Material` values are the
                    // baseline only for entities WITHOUT a controller.
                    // This matches `NiTextureTransformController`'s
                    // legacy semantic — the controller authored over
                    // the material's UV transform, not on top of it.
                    uv_offset: anim_uv_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|t| [t.offset.x, t.offset.y])
                        .or_else(|| mat.map(|m| m.uv_offset))
                        .unwrap_or([0.0, 0.0]),
                    uv_scale: anim_uv_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|t| [t.scale.x, t.scale.y])
                        .or_else(|| mat.map(|m| m.uv_scale))
                        .unwrap_or([1.0, 1.0]),
                    material_alpha: mat.map(|m| m.alpha).unwrap_or(1.0),
                    // Average albedo for fast GI bounce approximation.
                    // Falls back to mid-gray (0.5) when no texture color
                    // data is available. A proper implementation would
                    // downsample the texture to 1×1 during asset load;
                    // for now we derive a heuristic from the material.
                    avg_albedo: [0.5, 0.5, 0.5],
                    z_test,
                    z_write,
                    z_function,
                    material_kind,
                    terrain_tile_index,
                    skin_tint_rgba,
                    hair_tint_rgb,
                    multi_layer_envmap_strength,
                    eye_left_center,
                    eye_cubemap_scale,
                    eye_right_center,
                    multi_layer_inner_thickness,
                    multi_layer_refraction_scale,
                    multi_layer_inner_scale,
                    sparkle_rgba,
                    effect_falloff,
                    material_id: 0,
                    // O4-03 / #695 — `NiVertexColorProperty.vertex_mode
                    // == SOURCE_EMISSIVE` (encoded as `1` per
                    // `Material::vertex_color_mode`). Routes the
                    // per-vertex `fragColor` payload to the fragment
                    // shader's emissive accumulator instead of the
                    // albedo modulation. False on every mesh without a
                    // Material component (defaults to AmbientDiffuse) or
                    // when the property explicitly disables vertex
                    // colors (`Ignore`).
                    vertex_color_emissive: mat.is_some_and(|m| m.vertex_color_mode == 1),
                    // #890 Stage 2 — packed BSEffect flag bits captured
                    // at importer ingestion (see
                    // `cell_loader::pack_effect_shader_flags`). Layout
                    // matches `GpuMaterial::material_flags` so
                    // `to_gpu_material` ORs the word straight in.
                    effect_shader_flags: mat.map(|m| m.effect_shader_flags).unwrap_or(0),
                    is_water: false,
                };
                // #781 / PERF-N4 — `intern_by_hash` skips the
                // `to_gpu_material()` 260-byte construction on the
                // dedup-hit path (~97% of calls on Prospector).
                cmd.material_id =
                    material_table.intern_by_hash(cmd.material_hash(), || cmd.to_gpu_material());
                draw_commands.push(cmd);
            }
        }
    }
    // ── Particle billboards (#401) ──────────────────────────────────
    //
    // Each live particle becomes one DrawCommand referencing the unit
    // particle quad mesh. The model matrix is `translate(world_pos) ·
    // face_camera_rot · scale(size)`, so all per-particle dynamics live
    // in the model matrix and the existing instanced batching from #272
    // collapses every particle (consecutive in the sorted list,
    // sharing mesh+pipeline) into a single instanced cmd_draw_indexed.
    //
    // Color comes through `emissive_color` * `emissive_mult` — the
    // existing fragment shader already adds the emissive contribution
    // unconditionally, so an untextured quad lit only by emissive
    // produces the desired glowing-billboard look. Particles default to
    // additive blending (src=SRC_ALPHA, dst=ONE) per ParticleEmitter
    // defaults; per-emitter `(src_blend, dst_blend)` overrides ride
    // through the existing per-(src, dst, two_sided) pipeline cache
    // from #392.
    if let Some(particle_mesh) = particle_quad_handle {
        if let (Some(gtq), Some(eq)) = (
            world.query::<GlobalTransform>(),
            world.query::<ParticleEmitter>(),
        ) {
            for (entity, em) in eq.iter() {
                let _ = gtq.get(entity); // transform sampled by the system at spawn
                if em.particles.is_empty() {
                    continue;
                }
                let particle_count = em.particles.len();
                for i in 0..particle_count {
                    let p = em.particles.positions[i];
                    let world_pos = Vec3::new(p[0], p[1], p[2]);
                    // Face-camera rotation: align the quad's local +Z
                    // (its outward normal — see `quad_vertices` which
                    // sets normals to (0,0,1)) toward the camera.
                    let to_cam = cam_pos - world_pos;
                    let rot = if to_cam.length_squared() > 1.0e-6 {
                        Quat::from_rotation_arc(Vec3::Z, to_cam.normalize())
                    } else {
                        Quat::IDENTITY
                    };
                    // LERP color and size against age/life so particles
                    // fade out smoothly and grow/shrink as configured.
                    let t = (em.particles.ages[i] / em.particles.lifes[i]).clamp(0.0, 1.0);
                    let start_c = em.start_color;
                    let end_c = em.end_color;
                    let color = [
                        start_c[0] + (end_c[0] - start_c[0]) * t,
                        start_c[1] + (end_c[1] - start_c[1]) * t,
                        start_c[2] + (end_c[2] - start_c[2]) * t,
                        start_c[3] + (end_c[3] - start_c[3]) * t,
                    ];
                    let size = em.start_size + (em.end_size - em.start_size) * t;

                    let model =
                        Mat4::from_scale_rotation_translation(Vec3::splat(size), rot, world_pos);
                    let pos_clip = vp_mat * Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
                    let sort_depth = f32_sortable_u32(pos_clip.w);

                    let mut cmd = DrawCommand {
                        mesh_handle: particle_mesh,
                        texture_handle: 0,
                        model_matrix: model.to_cols_array(),
                        alpha_blend: true,
                        src_blend: em.src_blend,
                        dst_blend: em.dst_blend,
                        two_sided: true, // billboard quads are single-faced; cull-off avoids back-face flicker on extreme angles
                        is_decal: false,
                        // Particles ride emissive + alpha-blend with
                        // depth-write off — they never z-fight surfaces,
                        // so Architecture (zero bias) is correct. See
                        // `RenderLayer::depth_bias`.
                        render_layer: RenderLayer::Architecture,
                        bone_offset: 0,
                        normal_map_index: 0,
                        dark_map_index: 0,
                        glow_map_index: 0,
                        detail_map_index: 0,
                        gloss_map_index: 0,
                        parallax_map_index: 0,
                        parallax_height_scale: 0.04,
                        parallax_max_passes: 4.0,
                        env_map_index: 0,
                        env_mask_index: 0,
                        alpha_threshold: 0.0,
                        alpha_test_func: 0,
                        roughness: 1.0,
                        metalness: 0.0,
                        // Emissive carries the particle color * alpha so
                        // the existing fragment-shader emissive add lights
                        // the quad with no scene-light dependency. Alpha
                        // is folded into emissive_mult so the LERP-to-0
                        // end-color drives a true fade-out.
                        emissive_mult: color[3],
                        emissive_color: [color[0], color[1], color[2]],
                        specular_strength: 0.0,
                        specular_color: [0.0, 0.0, 0.0],
                        // Particles ride emissive; identity diffuse +
                        // ambient so the tint/ambient multipliers don't
                        // interact with the emissive add (#221).
                        diffuse_color: [1.0, 1.0, 1.0],
                        ambient_color: [1.0, 1.0, 1.0],
                        vertex_offset: 0,
                        index_offset: 0,
                        vertex_count: 0,
                        sort_depth,
                        in_tlas: false,
                        // Particles are drawn every frame they're alive;
                        // no frustum cull here (small, transient).
                        in_raster: true,
                        // Deterministic tiebreaker for same-emitter
                        // particles sharing depth bucket and color.
                        // XOR keeps the emitter grouping intact while
                        // giving each particle its own ordering slot.
                        entity_id: entity ^ (i as u32),
                        // Particles use identity UV + full alpha — the
                        // billboard quad is a unit square and the
                        // emitter's per-frame RGBA color already rides
                        // on `emissive_color` / `emissive_mult` above.
                        uv_offset: [0.0, 0.0],
                        uv_scale: [1.0, 1.0],
                        material_alpha: 1.0,
                        avg_albedo: [0.0, 0.0, 0.0],
                        material_kind: 0,
                        // Particles render with depth test on, depth
                        // write off (alpha-blended billboards). Default
                        // LESSEQUAL. See #398.
                        z_test: true,
                        z_write: false,
                        z_function: 3,
                        terrain_tile_index: None,
                        // Particles are never Skyrim+ variant shading.
                        skin_tint_rgba: [0.0; 4],
                        hair_tint_rgb: [0.0; 3],
                        multi_layer_envmap_strength: 0.0,
                        eye_left_center: [0.0; 3],
                        eye_cubemap_scale: 0.0,
                        eye_right_center: [0.0; 3],
                        multi_layer_inner_thickness: 0.0,
                        multi_layer_refraction_scale: 0.0,
                        multi_layer_inner_scale: [1.0, 1.0],
                        sparkle_rgba: [0.0; 4],
                        // #620 — particles never carry an effect-shader
                        // falloff cone; identity-pass-through.
                        effect_falloff: [1.0, 1.0, 1.0, 1.0, 0.0],
                        material_id: 0,
                        // Particles ride the emissive accumulator through
                        // `emissive_color` / `emissive_mult` already; no
                        // per-vertex emissive payload (#695).
                        vertex_color_emissive: false,
                        // #890 Stage 2 — particles never carry
                        // BSEffectShaderProperty flag bits.
                        effect_shader_flags: 0,
                        is_water: false,
                    };
                    // #781 / PERF-N4 — see SIBLING note above.
                    cmd.material_id = material_table
                        .intern_by_hash(cmd.material_hash(), || cmd.to_gpu_material());
                    draw_commands.push(cmd);
                }
            }
        }
    }

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

    // Collect lights from ECS.

    // Add cell directional light. For interior cells the XCLL directional
    // acts as a subtle fill light (not a physical sun), so we scale it down
    // to avoid hard shadow leakage through unsealed interior walls.
    if let Some(cell_lit) = world.try_resource::<CellLightingRes>() {
        let sun_intensity = world
            .try_resource::<SkyParamsRes>()
            .map(|sky| sky.sun_intensity)
            .unwrap_or(SUN_INTENSITY_PEAK);
        let (dir_color, dir_radius) = compute_directional_upload(
            &cell_lit.directional_color,
            cell_lit.is_interior,
            sun_intensity,
        );
        gpu_lights.push(byroredux_renderer::GpuLight {
            position_radius: [0.0, 0.0, 0.0, dir_radius],
            color_type: [dir_color[0], dir_color[1], dir_color[2], 2.0],
            direction_angle: [
                cell_lit.directional_dir[0],
                cell_lit.directional_dir[1],
                cell_lit.directional_dir[2],
                0.0,
            ],
        });
    }

    // Add placed point lights from LIGH records. Read-only — no write
    // needed on either component. Previously used query_2_mut (#290 P4-04).
    let light_gt_q = world.query::<GlobalTransform>();
    let light_q = world.query::<LightSource>();
    if let (Some(tq), Some(lq)) = (light_gt_q, light_q) {
        for (entity, light) in lq.iter() {
            if let Some(t) = tq.get(entity) {
                // #983 — `dimmer` and `intensity` are mutated by the
                // animation system when the source NIF carries
                // `NiLight{Dimmer,Intensity}Controller`. The product
                // is the per-frame multiplicative scalar on the
                // diffuse color; the renderer doesn't see the curves
                // directly, just the resolved factor here.
                // `radius` is similarly animated by
                // `NiLightRadiusController` and the value already
                // sits on `light.radius` from the same code path.
                let scale = light.dimmer * light.intensity;
                gpu_lights.push(byroredux_renderer::GpuLight {
                    position_radius: [
                        t.translation.x,
                        t.translation.y,
                        t.translation.z,
                        light.radius,
                    ],
                    color_type: [
                        light.color[0] * scale,
                        light.color[1] * scale,
                        light.color[2] * scale,
                        0.0,
                    ], // 0 = point
                    direction_angle: [0.0, 0.0, 0.0, 0.0],
                });
            }
        }
    }

    // Log light count once.
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static LOGGED: AtomicBool = AtomicBool::new(false);
        if !LOGGED.swap(true, Ordering::Relaxed) {
            log::info!(
                "Lights collected: {} (first 3: {:?})",
                gpu_lights.len(),
                gpu_lights
                    .iter()
                    .take(3)
                    .map(|l| (l.position_radius, l.color_type))
                    .collect::<Vec<_>>(),
            );
        }
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

    // Sky params from ECS resource (exterior cells) or default (interior/none).
    // #803 — cloud scroll lives on `CloudSimState` (survives cell
    // transitions); the rest comes from `SkyParamsRes` (rebuilt per
    // exterior load). When CloudSimState is absent (interior-only
    // session before any exterior load) the scroll defaults to zero.
    let sky = if let Some(sky_res) = world.try_resource::<SkyParamsRes>() {
        let clouds = world.try_resource::<CloudSimState>();
        let scroll = clouds
            .as_ref()
            .map(|c| {
                (
                    c.cloud_scroll,
                    c.cloud_scroll_1,
                    c.cloud_scroll_2,
                    c.cloud_scroll_3,
                )
            })
            .unwrap_or_default();
        SkyParams {
            zenith_color: sky_res.zenith_color,
            horizon_color: sky_res.horizon_color,
            lower_color: sky_res.lower_color,
            sun_direction: sky_res.sun_direction,
            sun_color: sky_res.sun_color,
            sun_size: sky_res.sun_size,
            sun_intensity: sky_res.sun_intensity,
            is_exterior: sky_res.is_exterior,
            cloud_scroll: scroll.0,
            cloud_tile_scale: sky_res.cloud_tile_scale,
            cloud_texture_index: sky_res.cloud_texture_index,
            sun_texture_index: sky_res.sun_texture_index,
            cloud_scroll_1: scroll.1,
            cloud_tile_scale_1: sky_res.cloud_tile_scale_1,
            cloud_texture_index_1: sky_res.cloud_texture_index_1,
            cloud_scroll_2: scroll.2,
            cloud_tile_scale_2: sky_res.cloud_tile_scale_2,
            cloud_texture_index_2: sky_res.cloud_texture_index_2,
            cloud_scroll_3: scroll.3,
            cloud_tile_scale_3: sky_res.cloud_tile_scale_3,
            cloud_texture_index_3: sky_res.cloud_texture_index_3,
        }
    } else {
        SkyParams::default()
    };

    // ── Water-plane re-emit ───────────────────────────────────────
    //
    // Walk every `WaterPlane` entity, locate its already-emitted
    // `DrawCommand` (the main mesh-iteration loop above produced it
    // because water entities carry `MeshHandle`), flip its
    // `is_water` flag so the regular triangle path skips it, and
    // emit a parallel `WaterDrawCommand` whose `instance_index`
    // matches the SSBO slot the renderer will assign to that draw.
    //
    // The slot-index ↔ Vec position map relies on the renderer's
    // 1:1 contract: `gpu_instances` is populated by iterating
    // `draw_commands` in order, and frustum-culled draws keep
    // their SSBO slot per #516. So the index into
    // `draw_commands` equals `gl_InstanceIndex` after upload.
    //
    // Linear scan over `draw_commands` per water entity is O(N×W);
    // typical N is ~thousands of draws and W is ≤ ~3 water planes
    // per cell, so this is well under a microsecond. A
    // `HashMap<EntityId, usize>` would be premature for the
    // expected scale.
    let time_secs = world
        .try_resource::<TotalTime>()
        .map(|t| t.0)
        .unwrap_or(0.0);
    if let Some(wq) = world.query::<WaterPlane>() {
        let fq = world.query::<WaterFlow>();
        for (entity, plane) in wq.iter() {
            let Some(idx) = draw_commands.iter().position(|c| c.entity_id == entity)
            else {
                // Entity has WaterPlane but no DrawCommand was emitted —
                // typically because the cell loader spawned the water
                // entity but the mesh wasn't yet uploaded, or the
                // entity is frustum-culled out of the regular emit
                // path. Skip silently.
                continue;
            };
            draw_commands[idx].is_water = true;

            let flow = fq.as_ref().and_then(|q| q.get(entity).copied());
            let (flow_dir, flow_speed) = match flow {
                Some(f) => (f.direction, f.speed),
                None => ([1.0, 0.0, 0.0], 0.0),
            };

            // ABI: matches `WaterPush` in `shaders/water.frag`.
            // Each vec4 maps to one std430-scalar slot — see
            // `crates/renderer/src/vulkan/water.rs::WaterPush` for
            // the layout contract.
            let mat = &plane.material;
            let push = WaterPush {
                timing: [
                    time_secs,
                    plane.kind as u8 as f32,
                    mat.foam_strength,
                    mat.ior,
                ],
                flow: [flow_dir[0], flow_dir[1], flow_dir[2], flow_speed],
                shallow: [
                    mat.shallow_color[0],
                    mat.shallow_color[1],
                    mat.shallow_color[2],
                    mat.fog_near,
                ],
                deep: [
                    mat.deep_color[0],
                    mat.deep_color[1],
                    mat.deep_color[2],
                    mat.fog_far,
                ],
                scroll: [
                    mat.scroll_a[0],
                    mat.scroll_a[1],
                    mat.scroll_b[0],
                    mat.scroll_b[1],
                ],
                tune: [
                    mat.uv_scale_a,
                    mat.uv_scale_b,
                    mat.shoreline_width,
                    mat.reflectivity,
                ],
                misc: [
                    mat.fresnel_f0,
                    0.0,
                    WaterPush::pack_normal_index(mat.normal_map_index),
                    0.0,
                ],
            };
            water_commands.push(WaterDrawCommand {
                mesh_handle: draw_commands[idx].mesh_handle,
                instance_index: idx as u32,
                push,
            });
            // Silence WaterKind-unused warning on builds where the
            // enum is only consumed by the f32 cast above.
            let _ = WaterKind::Calm;
        }
    }

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


#[cfg(test)]
mod frustum_tests;
#[cfg(test)]
mod draw_sort_key_tests;
#[cfg(test)]
mod sort_key_tests;
#[cfg(test)]
mod bone_palette_overflow_tests;
#[cfg(test)]
mod variant_pack_gating_tests;
#[cfg(test)]
mod directional_upload_tests;
#[cfg(test)]
mod fog_curve_propagation_tests;
