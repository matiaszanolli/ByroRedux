//! Per-frame render data collection from ECS queries.

use byroredux_core::ecs::{
    ActiveCamera, AnimatedVisibility, Camera, EntityId, GlobalTransform, LightSource, Material,
    MeshHandle, SkinnedMesh, TextureHandle, Transform, World, WorldBound, MAX_BONES_PER_MESH,
};
use byroredux_core::math::{Mat4, Vec3, Vec4};
use byroredux_renderer::vulkan::context::DrawCommand;
use std::collections::HashMap;

use crate::components::{AlphaBlend, CellLightingRes, DarkMapHandle, Decal, NormalMapHandle, TwoSided};

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
) -> ([f32; 16], [f32; 3], [f32; 3], [f32; 3], f32, f32) {
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
        for (entity, skin) in skin_q.iter() {
            let offset = bone_palette.len() as u32;
            // World-lookup closure — reads GlobalTransform for each bone
            // entity through the same query guard. Missing bones fall
            // back to identity inside compute_palette.
            let palette = skin.compute_palette(|bone_entity| {
                gt_q.get(bone_entity).map(|gt| gt.to_matrix())
            });
            // Pad every skinned mesh to MAX_BONES_PER_MESH so per-mesh
            // bone offsets are trivially `offset + local_index` and the
            // shader doesn't need a per-mesh bone count.
            for mat in &palette {
                bone_palette.push(mat.to_cols_array_2d());
            }
            for _ in palette.len()..MAX_BONES_PER_MESH {
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
                    (Some(c), Some(t)) => c.projection_matrix() * Camera::view_matrix(t),
                    _ => Mat4::IDENTITY,
                }
            }
            _ => Mat4::IDENTITY,
        };
        let frustum = FrustumPlanes::from_view_proj(vp);
        (vp.to_cols_array(), frustum, vp)
    } else {
        (Mat4::IDENTITY.to_cols_array(), FrustumPlanes::from_view_proj(Mat4::IDENTITY), Mat4::IDENTITY)
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
                    if wb.radius > 0.0
                        && !frustum.contains_sphere(wb.center, wb.radius)
                    {
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
                let alpha_blend = alpha_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
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

                // Material data + PBR classification.
                let mat = mat_q.as_ref().and_then(|q| q.get(entity));
                let (roughness, metalness, emissive_mult, emissive_color, specular_strength, specular_color, alpha_threshold, alpha_test_func) =
                    if let Some(m) = mat {
                        let pbr = m.classify_pbr(m.texture_path.as_deref());
                        let thresh = if m.alpha_test { m.alpha_threshold } else { 0.0 };
                        let func = if m.alpha_test { m.alpha_test_func as u32 } else { 0 };
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
                let sort_depth = clip.w.to_bits();

                draw_commands.push(DrawCommand {
                    mesh_handle: mesh.0,
                    texture_handle: tex_handle,
                    model_matrix: model_mat.to_cols_array(),
                    alpha_blend,
                    two_sided,
                    is_decal,
                    bone_offset,
                    normal_map_index,
                    dark_map_index,
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
                });
            }
        }
    }
    // Sort: opaque → decal → alpha. Within each group:
    //   Opaque: front-to-back (smaller depth first) for early-Z rejection.
    //   Transparent: back-to-front (larger depth first) for correct blending.
    // Within same depth bucket, group by pipeline key and mesh handle so
    // draw.rs can skip redundant pipeline/buffer rebinds. See #50, #241.
    draw_commands.sort_unstable_by_key(|cmd| {
        let depth_key = if cmd.alpha_blend {
            !cmd.sort_depth // back-to-front: invert bits so larger depth sorts first
        } else {
            cmd.sort_depth // front-to-back: smaller depth first
        };
        (
            cmd.alpha_blend,
            cmd.is_decal,
            cmd.two_sided,
            depth_key,
            cmd.texture_handle,
            cmd.mesh_handle,
        )
    });

    // Collect lights from ECS.

    // Add cell directional light. For interior cells the XCLL directional
    // acts as a subtle fill light (not a physical sun), so we scale it down
    // to avoid hard shadow leakage through unsealed interior walls.
    if let Some(cell_lit) = world.try_resource::<CellLightingRes>() {
        let (dir_color, dir_radius) = if cell_lit.is_interior {
            // Interior fill: scale down and flag unshadowed (radius = -1)
            // so the shader skips shadow rays that would hit sealed walls.
            let s = 0.35;
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

    // Add placed point lights from LIGH records.
    if let Some((tq, lq)) = world.query_2_mut::<GlobalTransform, LightSource>() {
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
    let ambient = cell_lit
        .as_ref()
        .map(|l| {
            if l.is_interior {
                // Boost interior ambient — raw XCLL values (0.10-0.15) are
                // authored for Bethesda's legacy fixed-function pipeline which
                // had additional fill contributions we don't replicate.
                [l.ambient[0] * 2.5, l.ambient[1] * 2.5, l.ambient[2] * 2.5]
            } else {
                l.ambient
            }
        })
        .unwrap_or([0.08, 0.08, 0.08]);
    let mut fog_color = cell_lit.as_ref().map(|l| l.fog_color).unwrap_or([0.0; 3]);
    let mut fog_near = cell_lit.as_ref().map(|l| l.fog_near).unwrap_or(0.0);
    let mut fog_far = cell_lit.as_ref().map(|l| l.fog_far).unwrap_or(0.0);
    drop(cell_lit);

    // Procedural fog: when the cell doesn't define fog (near == far == 0),
    // generate atmospheric fog from the ambient color. This adds depth and
    // mood to interiors that the original game achieved via its fixed-function
    // fog pipeline but didn't encode in the cell data.
    if fog_far <= fog_near + 1.0 {
        // Fog color: blend ambient toward a cool desaturated tone.
        // Darker ambients → cooler, more blue-gray fog (dungeons).
        // Brighter ambients → warmer, amber-tinted fog (homes).
        let lum = ambient[0] * 0.299 + ambient[1] * 0.587 + ambient[2] * 0.114;
        let warmth = lum.clamp(0.0, 0.3); // how warm the fog tint is
        fog_color = [
            ambient[0] * 0.4 + warmth * 0.3 + 0.02,
            ambient[1] * 0.4 + warmth * 0.2 + 0.02,
            ambient[2] * 0.4 + warmth * 0.1 + 0.03,
        ];
        // Fog distances: gentle fog starting at ~40% of typical room size,
        // becoming dense at ~200% of room size. Interior cells are typically
        // 500-2000 units across.
        fog_near = 600.0;
        fog_far = 2500.0;
    }

    (view_proj, camera_pos, ambient, fog_color, fog_near, fog_far)
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
