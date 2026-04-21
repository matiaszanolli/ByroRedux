//! Per-frame render data collection from ECS queries.

use byroredux_core::ecs::{
    ActiveCamera, AnimatedVisibility, Camera, EntityId, GlobalTransform, LightSource, Material,
    MeshHandle, ParticleEmitter, SkinnedMesh, TextureHandle, Transform, World, WorldBound,
    MAX_BONES_PER_MESH,
};
use byroredux_core::math::{Mat4, Quat, Vec3, Vec4};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::SkyParams;
use rayon::slice::ParallelSliceMut;
use std::collections::HashMap;

use crate::components::{
    AlphaBlend, CellLightingRes, DarkMapHandle, Decal, ExtraTextureMaps, NormalMapHandle,
    SkyParamsRes, TerrainTileSlot, TwoSided,
};

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
/// Both branches return the same 7-tuple shape `(u8, u8, u8, u32, u32,
/// u32, u32)` so the compiler accepts a single key closure. Per-branch
/// semantics are encoded by which slot carries the depth value:
///   Opaque      — slot 3 = depth_state; slot 6 = sort_depth (front-to-back)
///   Transparent — slot 3 = !sort_depth (back-to-front); slot 4 = depth_state
pub(crate) fn draw_sort_key(cmd: &DrawCommand) -> (u8, u8, u8, u32, u32, u32, u32) {
    if cmd.alpha_blend {
        (
            1u8, // after opaque
            cmd.is_decal as u8,
            cmd.two_sided as u8,
            !cmd.sort_depth, // invert → larger depth first
            pack_depth_state(cmd) as u32,
            cmd.texture_handle,
            cmd.mesh_handle,
        )
    } else {
        (
            0u8,
            cmd.is_decal as u8,
            cmd.two_sided as u8,
            pack_depth_state(cmd) as u32,
            cmd.mesh_handle, // group identical meshes
            cmd.texture_handle,
            cmd.sort_depth, // front-to-back within group
        )
    }
}

/// Build the view-projection matrix and draw command list from ECS queries.
///
/// All scratch buffers — `draw_commands`, `gpu_lights`, `bone_palette`,
/// `skin_offsets` — are owned by the caller and cleared on entry so their
/// heap allocations persist across frames. See #253 for the `skin_offsets`
/// case specifically (was a fresh HashMap every frame).
pub(crate) fn build_render_data(
    world: &World,
    draw_commands: &mut Vec<DrawCommand>,
    gpu_lights: &mut Vec<byroredux_renderer::GpuLight>,
    bone_palette: &mut Vec<[[f32; 4]; 4]>,
    skin_offsets: &mut HashMap<EntityId, u32>,
    particle_quad_handle: Option<u32>,
) -> ([f32; 16], [f32; 3], [f32; 3], [f32; 3], f32, f32, SkyParams) {
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
        let mut palette_scratch = Vec::new();
        for (entity, skin) in skin_q.iter() {
            let offset = bone_palette.len() as u32;
            // World-lookup closure — reads GlobalTransform for each bone
            // entity through the same query guard. Missing bones fall
            // back to identity inside compute_palette_into.
            skin.compute_palette_into(&mut palette_scratch, |bone_entity| {
                gt_q.get(bone_entity).map(|gt| gt.to_matrix())
            });
            // Pad every skinned mesh to MAX_BONES_PER_MESH so per-mesh
            // bone offsets are trivially `offset + local_index` and the
            // shader doesn't need a per-mesh bone count.
            for mat in &palette_scratch {
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
    //      separately. That's 10 RwLock read acquisitions per frame; all
    //      reads can coexist (no deadlock risk), so no TypeId-sorted
    //      bundling is needed.
    //
    //   2. The bundle is held across the full `for (entity, mesh) in
    //      mq.iter()` loop. No system that writes these components
    //      runs concurrently (render runs outside the scheduler in
    //      `RedrawRequested`), so read contention is theoretical.
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

            // Frustum cull: skip entities whose WorldBound is entirely
            // outside the view frustum. Entities without a WorldBound
            // (or with radius 0, i.e. not yet computed) pass through
            // uncull to avoid disappearing objects. See #237.
            if let Some(ref wbq) = wb_q {
                if let Some(wb) = wbq.get(entity) {
                    if wb.radius > 0.0 && !frustum.contains_sphere(wb.center, wb.radius) {
                        continue;
                    }
                }
            }

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
                let terrain_tile_index =
                    terrain_tile_q.as_ref().and_then(|q| q.get(entity)).map(|s| s.0);

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
                        thresh,
                        func,
                    )
                } else {
                    (0.5, 0.0, 0.0, [0.0; 3], 1.0, [1.0; 3], 0.0, 0u32)
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
                    vertex_offset: v_off,
                    index_offset: i_off,
                    vertex_count: v_count,
                    sort_depth,
                    in_tlas: true,
                    // Average albedo for fast GI bounce approximation.
                    // Falls back to mid-gray (0.5) when no texture color
                    // data is available. A proper implementation would
                    // downsample the texture to 1×1 during asset load;
                    // for now we derive a heuristic from the material.
                    avg_albedo: [0.5, 0.5, 0.5],
                    z_test,
                    z_write,
                    z_function,
                    // BSLightingShaderProperty.shader_type → fragment
                    // shader variant dispatch (#344). 0 = Default lit
                    // when the entity has no Material component (e.g.
                    // the spinning cube demo).
                    material_kind: mat.map(|m| m.material_kind as u32).unwrap_or(0),
                    terrain_tile_index,
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

                    let model = Mat4::from_scale_rotation_translation(
                        Vec3::splat(size),
                        rot,
                        world_pos,
                    );
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
                        vertex_offset: 0,
                        index_offset: 0,
                        vertex_count: 0,
                        sort_depth,
                        in_tlas: false,
                        avg_albedo: [0.0, 0.0, 0.0],
                        material_kind: 0,
                        // Particles render with depth test on, depth
                        // write off (alpha-blended billboards). Default
                        // LESSEQUAL. See #398.
                        z_test: true,
                        z_write: false,
                        z_function: 3,
                        terrain_tile_index: None,
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
    let fog_near = cell_lit.as_ref().map(|l| l.fog_near).unwrap_or(0.0);
    let fog_far = cell_lit.as_ref().map(|l| l.fog_far).unwrap_or(0.0);
    drop(cell_lit);

    // Sky params from ECS resource (exterior cells) or default (interior/none).
    let sky = if let Some(sky_res) = world.try_resource::<SkyParamsRes>() {
        SkyParams {
            zenith_color: sky_res.zenith_color,
            horizon_color: sky_res.horizon_color,
            sun_direction: sky_res.sun_direction,
            sun_color: sky_res.sun_color,
            sun_size: sky_res.sun_size,
            sun_intensity: sky_res.sun_intensity,
            is_exterior: sky_res.is_exterior,
            cloud_scroll: sky_res.cloud_scroll,
            cloud_tile_scale: sky_res.cloud_tile_scale,
            cloud_texture_index: sky_res.cloud_texture_index,
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
            vertex_offset: 0,
            index_offset: 0,
            vertex_count: 0,
            sort_depth: 0,
            in_tlas: false,
            avg_albedo: [0.0; 3],
            material_kind: 0,
            z_test: true,
            z_write: true,
            z_function: 3,
            terrain_tile_index: None,
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
