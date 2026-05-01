//! Per-frame render data collection from ECS queries.

use byroredux_core::ecs::{
    ActiveCamera, AnimatedVisibility, Camera, EntityId, GlobalTransform, LightSource, Material,
    MeshHandle, ParticleEmitter, SkinnedMesh, TextureHandle, Transform, World, WorldBound,
    MAX_BONES_PER_MESH,
};
use byroredux_core::math::{Mat4, Quat, Vec3, Vec4};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::vulkan::scene_buffer::MAX_TOTAL_BONES;
use byroredux_renderer::SkyParams;
use rayon::slice::ParallelSliceMut;
use std::collections::HashMap;
use std::sync::Once;

use crate::components::{
    AlphaBlend, CellLightingRes, DarkMapHandle, Decal, ExtraTextureMaps, NormalMapHandle,
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

/// Sort key for `DrawCommand`s — the batch-merge pass in
/// `VulkanContext::draw_frame` relies on consecutive identical
/// (alpha_blend, is_decal, two_sided, depth_state, mesh, …) runs to fold
/// into single instanced draws. Owned here so the field order can't
/// silently drift from an assert in a downstream crate.
///
/// Both branches return the same 9-tuple shape so the compiler accepts a
/// single key closure. Per-branch semantics:
///   Opaque      — slot 3/4 = 0 (blend factors unused); slot 5 = depth_state;
///                 slot 6 = mesh (cluster key); slot 7 = sort_depth (front-to-back);
///                 slot 8 = entity_id tiebreaker (#506).
///   Transparent — slot 3/4 = (src_blend, dst_blend) so additive vs alpha vs
///                 modulate draws cluster together and don't thrash the
///                 blend-pipeline cache; slot 5 = !sort_depth (back-to-front
///                 within a (blend, depth_state) cohort); slot 6 = depth_state;
///                 slot 7 = mesh; slot 8 = entity_id tiebreaker (#506).
///                 Correctness: alpha compositing requires back-to-front order
///                 *within one pipeline state*, not across them. Draws sharing
///                 (src, dst) still sort back-to-front; different-blend draws
///                 are already visually separable (#499).
///
/// The entity_id final slot makes `par_sort_unstable_by_key` behave
/// deterministically across runs: without it, rayon's work-stealing
/// could reorder commands whose 8-tuple prefix tied, breaking
/// capture/replay and screenshot-diff workflows on scenes with many
/// identical-mesh / identical-depth entries (e.g. exterior rock
/// fields at a fixed camera distance).
pub(crate) fn draw_sort_key(cmd: &DrawCommand) -> (u8, u8, u8, u32, u32, u32, u32, u32, u32) {
    if cmd.alpha_blend {
        (
            1u8, // after opaque
            cmd.is_decal as u8,
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
            0u8,
            cmd.is_decal as u8,
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

/// Build the view-projection matrix and draw command list from ECS queries.
///
/// All scratch buffers — `draw_commands`, `gpu_lights`, `bone_palette`,
/// `skin_offsets`, `palette_scratch` — are owned by the caller and
/// cleared on entry so their heap allocations persist across frames.
/// See #253 (`skin_offsets`), #243 (`draw_commands` / `gpu_lights` /
/// `bone_palette` scratch pattern), #509 (`palette_scratch`).
pub(crate) fn build_render_data(
    world: &World,
    draw_commands: &mut Vec<DrawCommand>,
    gpu_lights: &mut Vec<byroredux_renderer::GpuLight>,
    bone_palette: &mut Vec<[[f32; 4]; 4]>,
    skin_offsets: &mut HashMap<EntityId, u32>,
    palette_scratch: &mut Vec<Mat4>,
    particle_quad_handle: Option<u32>,
) -> ([f32; 16], [f32; 3], [f32; 3], [f32; 3], f32, f32, SkyParams) {
    let frame_count = FRAME_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    draw_commands.clear();
    gpu_lights.clear();
    bone_palette.clear();
    skin_offsets.clear();
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
            if frame_count >= 60 {
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
    let decal_q = world.query::<Decal>();
    let vis_q = world.query::<AnimatedVisibility>();
    let mat_q = world.query::<Material>();
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
                let is_decal = decal_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
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
                let base_material_kind = mat.map(|m| m.material_kind as u32).unwrap_or(0);
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
                let effect_falloff = if material_kind
                    == byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER
                {
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

                draw_commands.push(DrawCommand {
                    mesh_handle: mesh.0,
                    texture_handle: tex_handle,
                    model_matrix: model_mat.to_cols_array(),
                    alpha_blend,
                    src_blend,
                    dst_blend,
                    two_sided,
                    is_decal,
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
                    uv_offset: mat.map(|m| m.uv_offset).unwrap_or([0.0, 0.0]),
                    uv_scale: mat.map(|m| m.uv_scale).unwrap_or([1.0, 1.0]),
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
                });
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

                    draw_commands.push(DrawCommand {
                        mesh_handle: particle_mesh,
                        texture_handle: 0,
                        model_matrix: model.to_cols_array(),
                        alpha_blend: true,
                        src_blend: em.src_blend,
                        dst_blend: em.dst_blend,
                        two_sided: true, // billboard quads are single-faced; cull-off avoids back-face flicker on extreme angles
                        is_decal: false,
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
                    });
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
    draw_commands.par_sort_unstable_by_key(draw_sort_key);

    // Collect lights from ECS.

    // Add cell directional light. For interior cells the XCLL directional
    // acts as a subtle fill light (not a physical sun), so we scale it down
    // to avoid hard shadow leakage through unsealed interior walls.
    if let Some(cell_lit) = world.try_resource::<CellLightingRes>() {
        let (dir_color, dir_radius) = if cell_lit.is_interior {
            // Interior fill: scale down and flag unshadowed (radius = -1)
            // so the shader skips shadow rays that would hit sealed walls.
            let s = 0.6;
            (
                [
                    cell_lit.directional_color[0] * s,
                    cell_lit.directional_color[1] * s,
                    cell_lit.directional_color[2] * s,
                ],
                -1.0_f32,
            )
        } else {
            (cell_lit.directional_color, 0.0)
        };
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
                gpu_lights.push(byroredux_renderer::GpuLight {
                    position_radius: [
                        t.translation.x,
                        t.translation.y,
                        t.translation.z,
                        light.radius,
                    ],
                    color_type: [light.color[0], light.color[1], light.color[2], 0.0], // 0 = point
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
    drop(cell_lit);

    // Sky params from ECS resource (exterior cells) or default (interior/none).
    let sky = if let Some(sky_res) = world.try_resource::<SkyParamsRes>() {
        SkyParams {
            zenith_color: sky_res.zenith_color,
            horizon_color: sky_res.horizon_color,
            lower_color: sky_res.lower_color,
            sun_direction: sky_res.sun_direction,
            sun_color: sky_res.sun_color,
            sun_size: sky_res.sun_size,
            sun_intensity: sky_res.sun_intensity,
            is_exterior: sky_res.is_exterior,
            cloud_scroll: sky_res.cloud_scroll,
            cloud_tile_scale: sky_res.cloud_tile_scale,
            cloud_texture_index: sky_res.cloud_texture_index,
            sun_texture_index: sky_res.sun_texture_index,
            cloud_scroll_1: sky_res.cloud_scroll_1,
            cloud_tile_scale_1: sky_res.cloud_tile_scale_1,
            cloud_texture_index_1: sky_res.cloud_texture_index_1,
            cloud_scroll_2: sky_res.cloud_scroll_2,
            cloud_tile_scale_2: sky_res.cloud_tile_scale_2,
            cloud_texture_index_2: sky_res.cloud_texture_index_2,
            cloud_scroll_3: sky_res.cloud_scroll_3,
            cloud_tile_scale_3: sky_res.cloud_tile_scale_3,
            cloud_texture_index_3: sky_res.cloud_texture_index_3,
        }
    } else {
        SkyParams::default()
    };

    (
        view_proj, camera_pos, ambient, fog_color, fog_near, fog_far, sky,
    )
}

#[cfg(test)]
mod frustum_tests {
    use super::*;
    use byroredux_core::math::{Mat4, Vec3};

    fn perspective_vp() -> Mat4 {
        let proj = Mat4::perspective_rh(
            std::f32::consts::FRAC_PI_2, // 90° FOV
            1.0,
            0.1,
            1000.0,
        );
        let view = Mat4::look_at_rh(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        proj * view
    }

    #[test]
    fn sphere_in_front_is_inside() {
        let f = FrustumPlanes::from_view_proj(perspective_vp());
        assert!(f.contains_sphere(Vec3::new(0.0, 0.0, -50.0), 5.0));
    }

    #[test]
    fn sphere_behind_camera_is_outside() {
        let f = FrustumPlanes::from_view_proj(perspective_vp());
        assert!(!f.contains_sphere(Vec3::new(0.0, 0.0, 50.0), 5.0));
    }

    #[test]
    fn sphere_far_left_is_outside() {
        let f = FrustumPlanes::from_view_proj(perspective_vp());
        assert!(!f.contains_sphere(Vec3::new(-500.0, 0.0, -10.0), 1.0));
    }

    #[test]
    fn sphere_straddling_near_plane_is_inside() {
        let f = FrustumPlanes::from_view_proj(perspective_vp());
        assert!(f.contains_sphere(Vec3::new(0.0, 0.0, -0.05), 0.2));
    }

    #[test]
    fn identity_vp_contains_origin() {
        let f = FrustumPlanes::from_view_proj(Mat4::IDENTITY);
        assert!(f.contains_sphere(Vec3::ZERO, 0.5));
    }

    #[test]
    fn sphere_beyond_far_plane_is_outside() {
        let f = FrustumPlanes::from_view_proj(perspective_vp());
        assert!(!f.contains_sphere(Vec3::new(0.0, 0.0, -1100.0), 5.0));
    }
}

/// Regression tests for #306 (D3-03) — the draw-order sort key on
/// `DrawCommand.sort_depth` must give a unsigned u32 ordering that
/// matches IEEE 754 total ordering over the full f32 domain, not just
/// the positive half-line. Pre-fix `f32::to_bits()` was stored
/// directly, so `!bits` for the transparent back-to-front key only
/// produced correct order on positive floats; negatives, denormals,
/// and special values could silently reorder whenever frustum culling
/// let them through.
#[cfg(test)]
mod draw_sort_key_tests {
    use super::{draw_sort_key, DrawCommand};

    /// Minimal DrawCommand builder — only the fields that affect the
    /// sort key are interesting. Everything else is zeroed.
    fn cmd(alpha_blend: bool, is_decal: bool, two_sided: bool) -> DrawCommand {
        DrawCommand {
            mesh_handle: 0,
            texture_handle: 0,
            model_matrix: [0.0; 16],
            alpha_blend,
            src_blend: 6,
            dst_blend: 7,
            two_sided,
            is_decal,
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
            in_tlas: false,
            in_raster: true,
            entity_id: 0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            material_alpha: 1.0,
            avg_albedo: [0.0; 3],
            material_kind: 0,
            z_test: true,
            z_write: true,
            z_function: 3,
            terrain_tile_index: None,
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
            effect_falloff: [1.0, 1.0, 1.0, 1.0, 0.0],
        }
    }

    /// Regression for #500 (PERF D3-M2): a stale debug_assert! in
    /// `draw_frame` had the sort-key tuple fields in the wrong order.
    /// This test owns the sort contract in the same crate as the sort
    /// itself, so drift can't happen silently.
    ///
    /// Cluster order must be:
    ///   1. alpha_blend   (opaque before transparent)
    ///   2. is_decal
    ///   3. two_sided
    #[test]
    fn sort_key_clusters_by_alpha_decal_twosided() {
        // Construct every 2³ combination in scrambled order.
        let mut cmds = vec![
            cmd(true, true, true),
            cmd(false, false, false),
            cmd(true, false, true),
            cmd(false, true, false),
            cmd(true, true, false),
            cmd(false, false, true),
            cmd(true, false, false),
            cmd(false, true, true),
        ];
        cmds.sort_by_key(draw_sort_key);

        let observed: Vec<(bool, bool, bool)> = cmds
            .iter()
            .map(|c| (c.alpha_blend, c.is_decal, c.two_sided))
            .collect();
        let expected = [
            (false, false, false),
            (false, false, true),
            (false, true, false),
            (false, true, true),
            (true, false, false),
            (true, false, true),
            (true, true, false),
            (true, true, true),
        ];
        assert_eq!(observed, expected.to_vec());
    }

    /// Opaque draws sort front-to-back within the same
    /// (is_decal, two_sided, depth_state) cluster — the last key slot
    /// carries `sort_depth` ascending so early-Z benefits most draws.
    #[test]
    fn opaque_within_cluster_sorts_front_to_back() {
        let mut near = cmd(false, false, false);
        near.sort_depth = 100;
        let mut far = cmd(false, false, false);
        far.sort_depth = 900;
        let mut cmds = vec![far, near];
        cmds.sort_by_key(draw_sort_key);
        assert_eq!(cmds[0].sort_depth, 100);
        assert_eq!(cmds[1].sort_depth, 900);
    }

    /// Transparent draws sort back-to-front for correct blending —
    /// the key uses `!sort_depth` so larger depth sorts first.
    #[test]
    fn transparent_within_cluster_sorts_back_to_front() {
        let mut near = cmd(true, false, false);
        near.sort_depth = 100;
        let mut far = cmd(true, false, false);
        far.sort_depth = 900;
        let mut cmds = vec![near, far];
        cmds.sort_by_key(draw_sort_key);
        assert_eq!(cmds[0].sort_depth, 900);
        assert_eq!(cmds[1].sort_depth, 100);
    }

    /// Regression for #499: interleaved additive and alpha-blend draws
    /// sort into separate `(src_blend, dst_blend)` cohorts so the
    /// blend-pipeline cache doesn't thrash on every depth alternation.
    #[test]
    fn transparent_clusters_by_blend_factors() {
        let mut alpha_near = cmd(true, false, false);
        alpha_near.src_blend = 6;
        alpha_near.dst_blend = 7;
        alpha_near.sort_depth = 100;
        let mut additive_far = cmd(true, false, false);
        additive_far.src_blend = 6;
        additive_far.dst_blend = 1;
        additive_far.sort_depth = 900;
        let mut alpha_far = cmd(true, false, false);
        alpha_far.src_blend = 6;
        alpha_far.dst_blend = 7;
        alpha_far.sort_depth = 500;
        let mut cmds = vec![alpha_near, additive_far, alpha_far];
        cmds.sort_by_key(draw_sort_key);
        // Additive (dst=1) sorts before alpha (dst=7) by u32 order.
        // Both alpha draws land together, sorted back-to-front within.
        assert_eq!(cmds[0].dst_blend, 1);
        assert_eq!(cmds[1].dst_blend, 7);
        assert_eq!(cmds[1].sort_depth, 500);
        assert_eq!(cmds[2].dst_blend, 7);
        assert_eq!(cmds[2].sort_depth, 100);
    }

    /// Regression for #506: with ties in the 8-tuple prefix (same
    /// mesh, same pipeline state, same depth bucket) the `entity_id`
    /// final slot must break them deterministically so two sorts of
    /// the same input produce byte-identical output. Pre-#506 the
    /// key ended on `mesh_handle` and rayon's work-stealing in
    /// `par_sort_unstable_by_key` could reorder tied entries across
    /// runs.
    #[test]
    fn sort_key_is_deterministic_for_full_tuple_ties() {
        // Ten draws that collide on every slot except entity_id —
        // identical mesh, texture, depth bucket, blend factors.
        // `DrawCommand` isn't Clone, so build two independent Vecs
        // from the same factory and feed them opposite starting orders.
        fn make_tied_batch() -> Vec<DrawCommand> {
            (0..10u32)
                .map(|id| {
                    let mut c = cmd(false, false, false);
                    c.mesh_handle = 42;
                    c.texture_handle = 7;
                    c.sort_depth = 500;
                    c.entity_id = id;
                    c
                })
                .collect()
        }

        let mut a = make_tied_batch();
        // Shuffle `a` so a stable sort starting from insertion order
        // wouldn't accidentally produce ordered output.
        a.swap(0, 7);
        a.swap(3, 9);
        a.swap(1, 5);

        let mut b = make_tied_batch();
        b.reverse(); // fully different starting order from `a`

        a.sort_by_key(draw_sort_key);
        b.sort_by_key(draw_sort_key);

        let a_ids: Vec<u32> = a.iter().map(|c| c.entity_id).collect();
        let b_ids: Vec<u32> = b.iter().map(|c| c.entity_id).collect();
        assert_eq!(
            a_ids, b_ids,
            "same input → same output regardless of starting order"
        );
        assert_eq!(
            a_ids,
            (0..10u32).collect::<Vec<_>>(),
            "entity_id breaks ties ascending"
        );
    }
}

#[cfg(test)]
mod sort_key_tests {
    use super::f32_sortable_u32;

    /// Helper — assert `a < b` implies `key(a) < key(b)` for a sorted
    /// reference slice of f32 values, covering negatives, zero,
    /// positives, and infinities.
    #[test]
    fn sortable_u32_preserves_finite_ordering() {
        let sorted = [
            f32::NEG_INFINITY,
            -1.0e38,
            -1.0,
            -f32::MIN_POSITIVE, // smallest-magnitude negative normal
            -0.0,
            0.0,
            f32::MIN_POSITIVE,
            1.0,
            1.0e38,
            f32::INFINITY,
        ];
        for window in sorted.windows(2) {
            let ka = f32_sortable_u32(window[0]);
            let kb = f32_sortable_u32(window[1]);
            assert!(
                ka <= kb,
                "f32 order {} < {} must map to u32 order {} <= {}",
                window[0],
                window[1],
                ka,
                kb
            );
        }
    }

    /// The negative sign branch was the whole point of #306. A naive
    /// `bits` sort would invert the order on negatives — positive
    /// `-1.0.to_bits()` is larger than `-1000.0.to_bits()` because
    /// IEEE 754 stores magnitude in the low bits regardless of sign.
    #[test]
    fn sortable_u32_orders_negatives_correctly() {
        // -1000 < -1 < 0 must produce key(-1000) < key(-1) < key(0)
        let k_minus_1000 = f32_sortable_u32(-1000.0);
        let k_minus_1 = f32_sortable_u32(-1.0);
        let k_zero = f32_sortable_u32(0.0);
        assert!(
            k_minus_1000 < k_minus_1,
            "-1000 should sort below -1 (got {k_minus_1000} vs {k_minus_1})"
        );
        assert!(
            k_minus_1 < k_zero,
            "-1 should sort below 0 (got {k_minus_1} vs {k_zero})"
        );
        // Raw `to_bits` would reverse this — -1000 has smaller
        // magnitude bits than -1, so `(-1000.0).to_bits() > (-1.0).to_bits()`.
        assert!(
            (-1000f32).to_bits() > (-1f32).to_bits(),
            "sanity: raw to_bits DOES reverse negatives, so the fix is load-bearing"
        );
    }

    /// +0.0 and -0.0 differ only in the sign bit but must hit the
    /// same ordering bucket (they compare equal in IEEE 754).
    #[test]
    fn sortable_u32_handles_signed_zero() {
        let k_pos = f32_sortable_u32(0.0);
        let k_neg = f32_sortable_u32(-0.0);
        // -0.0 sorts strictly below +0.0 under our total order
        // (that's what `max_by(normalized_weight)`-style code expects
        // when both appear — deterministic placement without special-casing).
        // Specifically: -0.0 has sign bit set, so key = !bits =
        // !0x80000000 = 0x7FFFFFFF. +0.0 has sign bit clear, so key =
        // bits ^ 0x80000000 = 0x80000000. Hence k_neg < k_pos.
        assert_eq!(k_neg, 0x7FFF_FFFF);
        assert_eq!(k_pos, 0x8000_0000);
        assert!(k_neg < k_pos);
    }

    /// Infinities land at the extreme ends of the u32 range — no
    /// wraparound, no overlap with any finite value.
    #[test]
    fn sortable_u32_places_infinities_at_extremes() {
        let k_neg_inf = f32_sortable_u32(f32::NEG_INFINITY);
        let k_pos_inf = f32_sortable_u32(f32::INFINITY);
        let k_huge_neg = f32_sortable_u32(-1.0e38);
        let k_huge_pos = f32_sortable_u32(1.0e38);
        assert!(k_neg_inf < k_huge_neg);
        assert!(k_huge_pos < k_pos_inf);
    }

    /// Transparent back-to-front path inverts the key via `!` — that
    /// inversion must still produce a legal total ordering (i.e., the
    /// opposite of the forward ordering). Tests the actual usage
    /// pattern the sort in `build_render_data` relies on.
    #[test]
    fn sortable_u32_invertible_for_back_to_front() {
        let near = f32_sortable_u32(1.0); // close to camera
        let far = f32_sortable_u32(100.0); // far
                                           // Forward: near < far (front-to-back opaque path)
        assert!(near < far);
        // Back-to-front (transparent path): !near > !far so `far`
        // draws first, `near` last — exactly what alpha-compositing
        // needs.
        assert!(!near > !far);
    }

    /// Denormal (subnormal) values must still sort strictly between
    /// zero and the smallest normal positive. A prior implementation
    /// that treated subnormals specially or clamped to zero would
    /// collapse a band of distinct depths into one sort bucket.
    #[test]
    fn sortable_u32_orders_denormals() {
        // f32 smallest subnormal = 1e-45 (approximately).
        let denorm = f32::from_bits(1); // positive denormal
        let k_zero = f32_sortable_u32(0.0);
        let k_denorm = f32_sortable_u32(denorm);
        let k_min_normal = f32_sortable_u32(f32::MIN_POSITIVE);
        assert!(k_zero < k_denorm);
        assert!(k_denorm < k_min_normal);
    }

    /// NaN has a well-defined position in the total ordering — it
    /// doesn't interfere with the finite range. Any NaN produced by
    /// an out-of-frustum clip-space projection won't silently drop
    /// into a random sort bucket; it ends up at the positive end
    /// alongside +infinity.
    #[test]
    fn sortable_u32_places_canonical_nan_past_positive_infinity() {
        let k_nan = f32_sortable_u32(f32::NAN);
        let k_pos_inf = f32_sortable_u32(f32::INFINITY);
        // Canonical f32::NAN has the sign bit clear, so it falls in
        // the `bits ^ 0x80000000` branch and sorts above +infinity.
        assert!(k_nan > k_pos_inf);
    }
}

/// M29 — bone-palette overflow guard regression tests.
///
/// `MAX_TOTAL_BONES = 4096` slots ÷ `MAX_BONES_PER_MESH = 128` = 32
/// concurrently-skinned-mesh ceiling. The 33rd skinned mesh in a
/// frame must fall through the `break` at the top of the
/// `build_render_data` skinning loop and stay out of `skin_offsets`
/// (and out of the bone palette upload — `upload_bones` clamps
/// anyway, but we want the truncation visible at the ECS layer so
/// the warn fires once and the data shape is consistent).
#[cfg(test)]
mod bone_palette_overflow_tests {
    use super::*;
    use byroredux_core::ecs::{GlobalTransform, SkinnedMesh, World};
    use byroredux_core::math::Mat4;

    fn make_skinned_world(num_meshes: usize) -> World {
        let mut world = World::new();
        for _ in 0..num_meshes {
            // Each mesh declares MAX_BONES_PER_MESH bones with self-
            // EntityId pointers. The palette closure looks up
            // GlobalTransform on each bone — we don't insert any, so
            // every bone falls back to identity (the test only cares
            // about overflow accounting, not the matrix values).
            let mesh_entity = world.spawn();
            world.insert(mesh_entity, GlobalTransform::IDENTITY);
            let bones = vec![Some(mesh_entity); MAX_BONES_PER_MESH];
            let binds = vec![Mat4::IDENTITY; MAX_BONES_PER_MESH];
            world.insert(mesh_entity, SkinnedMesh::new(None, bones, binds));
        }
        world
    }

    fn run_build(world: &World) -> (Vec<[[f32; 4]; 4]>, HashMap<EntityId, u32>) {
        let mut draw_commands = Vec::new();
        let mut gpu_lights = Vec::new();
        let mut bone_palette = Vec::new();
        let mut skin_offsets = HashMap::new();
        let mut palette_scratch = Vec::new();
        let _ = build_render_data(
            world,
            &mut draw_commands,
            &mut gpu_lights,
            &mut bone_palette,
            &mut skin_offsets,
            &mut palette_scratch,
            None,
        );
        (bone_palette, skin_offsets)
    }

    #[test]
    fn at_capacity_fills_palette_completely() {
        // 32 meshes × 128 bones + 1 identity slot = 4097 slots, which
        // overshoots by 1. The guard fires only when adding the NEXT
        // mesh would overflow; 32 meshes still fit at the boundary
        // because slot 0 is the rigid-fallback identity. Document the
        // exact off-by-one.
        let max_skinned = MAX_TOTAL_BONES / MAX_BONES_PER_MESH; // 32
        let world = make_skinned_world(max_skinned - 1);
        let (palette, offsets) = run_build(&world);
        assert_eq!(
            offsets.len(),
            max_skinned - 1,
            "all {} meshes must register a bone offset",
            max_skinned - 1
        );
        // 1 identity slot + (max_skinned - 1) × MAX_BONES_PER_MESH
        let expected_slots = 1 + (max_skinned - 1) * MAX_BONES_PER_MESH;
        assert_eq!(palette.len(), expected_slots);
    }

    #[test]
    fn over_capacity_breaks_loop_and_truncates_offsets() {
        // 33 meshes × 128 bones = 4224 slots requested; 4096 ceiling.
        // The guard at the top of the loop trips before mesh 32 (the
        // one that would push the palette past MAX_TOTAL_BONES) gets
        // its offset registered. `skin_offsets` should hold strictly
        // fewer than 33 entries.
        let max_skinned = MAX_TOTAL_BONES / MAX_BONES_PER_MESH; // 32
        let world = make_skinned_world(max_skinned + 1);
        let (palette, offsets) = run_build(&world);
        assert!(
            offsets.len() < max_skinned + 1,
            "overflow guard must drop at least one mesh; got {} offsets for {} meshes",
            offsets.len(),
            max_skinned + 1
        );
        assert!(
            palette.len() <= MAX_TOTAL_BONES,
            "palette must never exceed MAX_TOTAL_BONES={}; got {}",
            MAX_TOTAL_BONES,
            palette.len()
        );
    }
}

/// Regression: #619 / SK-D3-05 — `BSLightingShaderProperty` variant
/// payload pack must run only for materials whose shader branch reads
/// it. A material with `material_kind == 0` (default lit, ~99% of a
/// typical cell) but a populated `shader_type_fields` box must emit a
/// DrawCommand with zero/identity variant slots — the fragment shader
/// won't read them and packing the values is dead CPU work.
#[cfg(test)]
mod variant_pack_gating_tests {
    use super::*;
    use byroredux_core::ecs::components::material::ShaderTypeFields;
    use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, World};

    fn run_build(world: &World) -> Vec<DrawCommand> {
        let mut draw_commands = Vec::new();
        let mut gpu_lights = Vec::new();
        let mut bone_palette = Vec::new();
        let mut skin_offsets = HashMap::new();
        let mut palette_scratch = Vec::new();
        let _ = build_render_data(
            world,
            &mut draw_commands,
            &mut gpu_lights,
            &mut bone_palette,
            &mut skin_offsets,
            &mut palette_scratch,
            None,
        );
        draw_commands
    }

    /// Build a world with a single renderable mesh whose Material
    /// supplies `shader_type_fields` for every variant slot. The
    /// `material_kind` argument controls which variant the renderer
    /// dispatches; the test caller picks 0 (default) to prove the
    /// gate skips the pack, or 5/6/14 to prove the gate lets the
    /// matching variant through.
    fn world_with_variant_material(material_kind: u8) -> World {
        let mut world = World::new();

        // Camera entity — ActiveCamera is required by build_render_data.
        let cam = world.spawn();
        world.insert(cam, Transform::IDENTITY);
        world.insert(cam, GlobalTransform::IDENTITY);
        world.insert(cam, Camera::default());
        world.insert_resource(ActiveCamera(cam));

        // Renderable mesh — Material populates every variant field
        // with a non-default value so a passing pack would surface
        // those values on the DrawCommand.
        let mesh_e = world.spawn();
        world.insert(mesh_e, Transform::IDENTITY);
        world.insert(mesh_e, GlobalTransform::IDENTITY);
        world.insert(mesh_e, MeshHandle(1));
        world.insert(mesh_e, TextureHandle(1));
        world.insert(
            mesh_e,
            Material {
                material_kind,
                shader_type_fields: Some(Box::new(ShaderTypeFields {
                    skin_tint_color: Some([0.9, 0.8, 0.7]),
                    skin_tint_alpha: Some(0.5),
                    hair_tint_color: Some([0.1, 0.2, 0.3]),
                    eye_cubemap_scale: Some(0.42),
                    eye_left_reflection_center: Some([1.0, 2.0, 3.0]),
                    eye_right_reflection_center: Some([4.0, 5.0, 6.0]),
                    parallax_max_passes: None,
                    parallax_height_scale: None,
                    multi_layer_inner_thickness: Some(0.7),
                    multi_layer_refraction_scale: Some(0.3),
                    multi_layer_inner_layer_scale: Some([2.0, 3.0]),
                    multi_layer_envmap_strength: Some(0.55),
                    sparkle_parameters: Some([0.1, 0.2, 0.3, 0.4]),
                })),
                ..Material::default()
            },
        );

        world
    }

    #[test]
    fn default_kind_zero_skips_all_variant_packs() {
        let world = world_with_variant_material(0);
        let cmds = run_build(&world);
        assert_eq!(cmds.len(), 1, "exactly one DrawCommand expected");
        let c = &cmds[0];
        assert_eq!(
            c.skin_tint_rgba, [0.0; 4],
            "kind=0 must skip skin tint pack"
        );
        assert_eq!(c.hair_tint_rgb, [0.0; 3], "kind=0 must skip hair tint pack");
        assert_eq!(c.sparkle_rgba, [0.0; 4], "kind=0 must skip sparkle pack");
        assert_eq!(c.multi_layer_envmap_strength, 0.0);
        assert_eq!(c.multi_layer_inner_thickness, 0.0);
        assert_eq!(c.multi_layer_refraction_scale, 0.0);
        assert_eq!(c.multi_layer_inner_scale, [1.0, 1.0]);
        assert_eq!(c.eye_left_center, [0.0; 3]);
        assert_eq!(c.eye_right_center, [0.0; 3]);
        assert_eq!(c.eye_cubemap_scale, 0.0);
    }

    #[test]
    fn kind_5_skin_tint_packs_only_skin_fields() {
        let world = world_with_variant_material(5);
        let cmds = run_build(&world);
        let c = &cmds[0];
        // SkinTint group lands.
        assert_eq!(c.skin_tint_rgba, [0.9, 0.8, 0.7, 0.5]);
        // Other groups stay default-zero — gate worked.
        assert_eq!(c.hair_tint_rgb, [0.0; 3]);
        assert_eq!(c.sparkle_rgba, [0.0; 4]);
        assert_eq!(c.multi_layer_envmap_strength, 0.0);
        assert_eq!(c.eye_left_center, [0.0; 3]);
    }

    #[test]
    fn kind_11_multilayer_parallax_packs_only_multilayer_fields() {
        // Stub variant — shader doesn't consume yet, but the pack
        // still runs so when the shader branch lands the data is
        // already there. See triangle.frag:797-813.
        let world = world_with_variant_material(11);
        let cmds = run_build(&world);
        let c = &cmds[0];
        assert_eq!(c.multi_layer_inner_thickness, 0.7);
        assert_eq!(c.multi_layer_refraction_scale, 0.3);
        assert_eq!(c.multi_layer_inner_scale, [2.0, 3.0]);
        assert_eq!(c.multi_layer_envmap_strength, 0.55);
        // Other groups stay default-zero.
        assert_eq!(c.skin_tint_rgba, [0.0; 4]);
        assert_eq!(c.hair_tint_rgb, [0.0; 3]);
        assert_eq!(c.sparkle_rgba, [0.0; 4]);
        assert_eq!(c.eye_left_center, [0.0; 3]);
    }

    #[test]
    fn kind_16_eye_envmap_packs_only_eye_fields() {
        let world = world_with_variant_material(16);
        let cmds = run_build(&world);
        let c = &cmds[0];
        assert_eq!(c.eye_left_center, [1.0, 2.0, 3.0]);
        assert_eq!(c.eye_right_center, [4.0, 5.0, 6.0]);
        assert_eq!(c.eye_cubemap_scale, 0.42);
        // Other groups stay default-zero.
        assert_eq!(c.skin_tint_rgba, [0.0; 4]);
        assert_eq!(c.hair_tint_rgb, [0.0; 3]);
        assert_eq!(c.multi_layer_envmap_strength, 0.0);
    }

    /// Regression for #620 / SK-D4-01. Material with
    /// `effect_falloff = Some(...)` and `material_kind = 101`
    /// (`MATERIAL_KIND_EFFECT_SHADER`) must surface the falloff cone
    /// on the resulting `DrawCommand.effect_falloff`. Identity defaults
    /// stay in place when `material_kind != 101` even if the Material
    /// carries `effect_falloff` (the gate is on `material_kind`, not
    /// on the option).
    #[test]
    fn effect_shader_kind_packs_falloff_cone() {
        use byroredux_core::ecs::components::material::EffectFalloff;
        let mut world = World::new();
        let cam = world.spawn();
        world.insert(cam, Transform::IDENTITY);
        world.insert(cam, GlobalTransform::IDENTITY);
        world.insert(cam, Camera::default());
        world.insert_resource(ActiveCamera(cam));

        let mesh_e = world.spawn();
        world.insert(mesh_e, Transform::IDENTITY);
        world.insert(mesh_e, GlobalTransform::IDENTITY);
        world.insert(mesh_e, MeshHandle(1));
        world.insert(mesh_e, TextureHandle(1));
        world.insert(
            mesh_e,
            Material {
                // `MATERIAL_KIND_EFFECT_SHADER` (101) — engine-synthesized
                // upstream of the renderer, see render.rs:603.
                material_kind: 101,
                effect_falloff: Some(EffectFalloff {
                    start_angle: 0.95,
                    stop_angle: 0.30,
                    start_opacity: 1.0,
                    stop_opacity: 0.0,
                    soft_falloff_depth: 8.0,
                }),
                ..Material::default()
            },
        );

        let cmds = run_build(&world);
        assert_eq!(cmds.len(), 1);
        let c = &cmds[0];
        assert_eq!(
            c.effect_falloff,
            [0.95, 0.30, 1.0, 0.0, 8.0],
            "effect-shader DrawCommand must carry the captured cone"
        );
    }

    /// Companion: when `material_kind != 101` the Material's
    /// `effect_falloff` is ignored and the DrawCommand emits the
    /// identity-pass-through tuple. Pre-fix the gate was missing — a
    /// non-effect mesh authored with stale `effect_falloff` would have
    /// faded incorrectly.
    #[test]
    fn non_effect_kind_emits_identity_falloff_even_when_material_has_it() {
        use byroredux_core::ecs::components::material::EffectFalloff;
        let mut world = World::new();
        let cam = world.spawn();
        world.insert(cam, Transform::IDENTITY);
        world.insert(cam, GlobalTransform::IDENTITY);
        world.insert(cam, Camera::default());
        world.insert_resource(ActiveCamera(cam));

        let mesh_e = world.spawn();
        world.insert(mesh_e, Transform::IDENTITY);
        world.insert(mesh_e, GlobalTransform::IDENTITY);
        world.insert(mesh_e, MeshHandle(1));
        world.insert(mesh_e, TextureHandle(1));
        world.insert(
            mesh_e,
            Material {
                material_kind: 0, // default lit, NOT EffectShader
                effect_falloff: Some(EffectFalloff {
                    start_angle: 0.5,
                    stop_angle: 0.1,
                    start_opacity: 1.0,
                    stop_opacity: 0.0,
                    soft_falloff_depth: 4.0,
                }),
                ..Material::default()
            },
        );

        let cmds = run_build(&world);
        assert_eq!(cmds.len(), 1);
        assert_eq!(
            cmds[0].effect_falloff,
            [1.0, 1.0, 1.0, 1.0, 0.0],
            "non-effect kind must emit identity-pass-through falloff \
             regardless of Material.effect_falloff content"
        );
    }
}
