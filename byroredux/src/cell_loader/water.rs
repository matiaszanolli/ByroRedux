//! Cell-load water-plane spawn.
//!
//! Translates a cell's `XCLW` (water height) + `XCWT` (water-type
//! WATR FormID, Skyrim+) into a `WaterPlane` ECS entity backed by a
//! flat quad mesh and a derived [`WaterMaterial`].
//!
//! Coordinate convention matches the rest of the cell loader: the
//! engine renders in Y-up; Bethesda's Z-up `water_height` therefore
//! becomes the Y coordinate of the water plane.
//!
//! Scope (initial cut):
//!
//! - One `WaterPlane` per cell — interior cells get a centred quad
//!   sized to their reference bounds; exterior cells get a single
//!   per-tile quad spanning the cell's 4096×4096 grid square (so
//!   neighbouring tiles tile seamlessly via the world-XZ UV in
//!   `water.frag`).
//! - `WaterKind` heuristic: defaults to `Calm` for every spawn.
//!   Rivers / rapids / waterfalls land in a follow-up pass once we
//!   parse the WATR record's EDID naming convention + cell flow
//!   metadata (Skyrim has REFR XCWT overrides + `RiverWater` named
//!   WATR records; FNV uses NAM2-suffixed names).
//! - TLAS exclusion: water planes are spawned with no `in_tlas`
//!   semantics because the renderer skips this entity in the regular
//!   draw path (`DrawCommand.is_water == true`), and the water
//!   pipeline doesn't read its own surface from TLAS.
//!
//! Returns the number of water-plane entities spawned (0 or 1 today).

use byroredux_core::ecs::components::water::{WaterPlane, WaterVolume};
use byroredux_core::ecs::{GlobalTransform, MeshHandle, Transform, World};
use byroredux_plugin::esm;
use byroredux_core::math::{Quat, Vec3};
use byroredux_renderer::{Vertex, VulkanContext};
use std::collections::HashMap;

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::NormalMapHandle;
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;

/// Default interior water-plane half-extent in Bethesda units when
/// the cell loader has not yet computed the cell's reference bounds.
///
/// Was 1024 before the first live-engine smoke test surfaced the
/// "everything underwater" failure mode: a 1024-unit plane centred
/// at world origin covered every camera-reachable position in a
/// typical interior, and any cell with a non-zero XCLW height
/// (sewer, flooded ruin, pool) flagged the camera as submerged
/// even when standing on dry floor. 256 wu is the typical
/// Bethesda interior-pool diameter — tighter than the worst case,
/// but the right side of the trade-off until WorldBound
/// aggregation lands.
const DEFAULT_INTERIOR_HALF_EXTENT: f32 = 256.0;

/// Default interior water-volume depth below the surface, in
/// Bethesda units. Most interior pools / baths / sewer channels are
/// well under 200 wu deep; the pre-fix value (4096) was a copy of
/// the exterior open-ocean default and was the second contributor
/// to the spurious-submerged regression — the volume column reached
/// far enough down to engulf any camera that happened to share the
/// interior plane's XZ extent.
const DEFAULT_INTERIOR_VOLUME_DEPTH: f32 = 200.0;

/// Spawn one water-plane entity for the given cell.
///
/// `xclw_height` is the cell's parsed `water_height` in Bethesda
/// Z-up. `xcwt_form` is the optional `water_type_form` FormID into
/// the parsed `WATR` records table. `cell_origin_world_xz` is the
/// already-converted Y-up origin of the cell (X, Z components) —
/// for interior cells, this is `(0, 0)`; for exterior cells, the
/// renderer's grid translation.
///
/// Returns `Some(1)` on a successful spawn, `None` when mesh upload
/// fails (matches the terrain helper's signature).
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_water_plane(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    waters: &HashMap<u32, esm::records::misc::WatrRecord>,
    xclw_height: f32,
    xcwt_form: Option<u32>,
    cell_origin_world_xz: (f32, f32),
    half_extent: f32,
    blas_specs: &mut Vec<(u32, u32, u32)>,
) -> Option<usize> {
    // ── Resolve WATR → engine WaterMaterial (EXAL boundary) ──
    let (material, kind, flow, normal_texture_path) =
        crate::env_translate::resolve_water_material(waters, xcwt_form);

    let allocator = ctx.allocator.as_ref()?;

    // ── Build the flat quad mesh ──
    // Local-space mesh: 4 verts on Y=0, square covering [-1, 1] × [-1, 1].
    // The entity's Transform places the quad at world Y = xclw_height
    // and scales it by `half_extent`. Normal map UV scrolls in world
    // space, so the mesh UVs don't matter visually — they're set to
    // [-half_extent, half_extent] so the perturbed-normal blend in
    // the fragment shader has a consistent UV-derivative magnitude
    // across the plane.
    let uv = half_extent;
    let vertices = vec![
        // Position is local space; the model matrix scales by
        // half_extent on X/Z. UV mirrors local position so world-
        // space UV derivatives behave.
        Vertex {
            position: [-1.0, 0.0, -1.0],
            color: [1.0, 1.0, 1.0],
            normal: [0.0, 1.0, 0.0],
            uv: [-uv, -uv],
            bone_indices: [0, 0, 0, 0],
            bone_weights: [0.0, 0.0, 0.0, 0.0],
            splat_weights_0: [0, 0, 0, 0],
            splat_weights_1: [0, 0, 0, 0],
            // World +X tangent — water.frag re-orthogonalises against
            // the world normal, so any non-degenerate tangent works.
            tangent: [1.0, 0.0, 0.0, 1.0],
        },
        Vertex {
            position: [1.0, 0.0, -1.0],
            color: [1.0, 1.0, 1.0],
            normal: [0.0, 1.0, 0.0],
            uv: [uv, -uv],
            bone_indices: [0, 0, 0, 0],
            bone_weights: [0.0, 0.0, 0.0, 0.0],
            splat_weights_0: [0, 0, 0, 0],
            splat_weights_1: [0, 0, 0, 0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        },
        Vertex {
            position: [-1.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0],
            normal: [0.0, 1.0, 0.0],
            uv: [-uv, uv],
            bone_indices: [0, 0, 0, 0],
            bone_weights: [0.0, 0.0, 0.0, 0.0],
            splat_weights_0: [0, 0, 0, 0],
            splat_weights_1: [0, 0, 0, 0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        },
        Vertex {
            position: [1.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0],
            normal: [0.0, 1.0, 0.0],
            uv: [uv, uv],
            bone_indices: [0, 0, 0, 0],
            bone_weights: [0.0, 0.0, 0.0, 0.0],
            splat_weights_0: [0, 0, 0, 0],
            splat_weights_1: [0, 0, 0, 0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        },
    ];
    // Two triangles, CCW after the engine's Z→Y up swizzle would
    // negate winding — emit CW so it becomes CCW post-conversion.
    // For this mesh we don't apply the negate (we author already in
    // Y-up local), so emit CCW directly.
    let indices = vec![0u32, 2, 1, 1, 2, 3];

    let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
        &ctx.device,
        allocator,
        &ctx.graphics_queue,
        ctx.transfer_pool,
        &vertices,
        &indices,
        // Water meshes do NOT need BLAS — they're skipped from TLAS
        // (water-on-water self-hits are avoided by the CP2077-style
        // terminate-on-hit policy on water rays).
        false,
        None,
    ) {
        Ok(h) => h,
        Err(e) => {
            log::warn!("Water plane mesh upload failed: {e}");
            return None;
        }
    };
    // Suppress unused warning when ray tracing is on — water never
    // adds a BLAS entry, but other spawn helpers in the same call
    // chain accumulate into the same vec.
    let _ = blas_specs;

    // Texture resolve — the water material's normal_map_index points
    // here. When the WATR record's TNAM is unset (e.g., default
    // interior water with no XCWT), fall back to the canonical
    // engine water normal map path.
    let resolved_normal_idx = if let Some(path) = normal_texture_path {
        resolve_texture(ctx, tex_provider, Some(path.as_str()))
    } else {
        // Empty path → resolve_texture returns 0 (placeholder), which
        // the shader interprets as `u32::MAX` via floatBitsToUint —
        // *but* we want the procedural fallback in that case. Encode
        // u32::MAX directly into the material instead of letting it
        // pass through the texture registry.
        0
    };

    let mut material = material;
    if resolved_normal_idx != 0 {
        material.normal_map_index = resolved_normal_idx;
    } // else material.normal_map_index stays at u32::MAX (default — triggers shader procedural)

    // ── Spawn the entity ──
    let position = Vec3::new(
        cell_origin_world_xz.0,
        xclw_height,
        // Bethesda Z-up → Y-up: world_y → −Z. `cell_origin_world_xz.1`
        // is already pre-converted by the caller, so we use it as-is
        // (callers from interior path pass `(0, 0)`; exterior callers
        // pass `(grid_x * 4096, grid_y * 4096 * -1)` already swizzled).
        cell_origin_world_xz.1,
    );
    let scale = half_extent;

    let entity = world.spawn();
    world.insert(
        entity,
        Transform::new(position, Quat::IDENTITY, scale),
    );
    world.insert(
        entity,
        GlobalTransform::new(position, Quat::IDENTITY, scale),
    );
    world.insert(entity, MeshHandle(mesh_handle));
    world.insert(entity, WaterPlane { kind, material });
    // #1338 — pair the normal-map `resolve_texture` refcount bump above
    // with a handle component the cell-unload victim walk can reach.
    // The water plane is drawn by the water pipeline from
    // `WaterPlane.material.normal_map_index` (its static `DrawCommand`
    // is skipped via the `is_water` flag in `reemit_water_planes`), so
    // this handle is consumed only by `unload_cell`'s `NormalMapHandle`
    // sweep — without it the texture refcount + bindless slot leak on
    // every cell unload. Gated on `!= 0` to mirror the acquire gate
    // (the procedural-fallback path leaves `resolved_normal_idx == 0`).
    if resolved_normal_idx != 0 {
        world.insert(entity, NormalMapHandle(resolved_normal_idx));
    }
    if let Some(flow) = flow {
        world.insert(entity, flow);
    }
    // Volume extends from the surface down to a per-mode floor.
    // Interior planes get a tight 200-wu column (typical pool
    // depth); exterior planes get the full cell-width column so deep
    // ocean cells remain detectable. The exterior heuristic is
    // "half-extent > 1024 = exterior" — captures the spawn caller
    // contract without an explicit flag.
    let volume_depth = if half_extent > 1024.0 {
        EXTERIOR_CELL_UNITS
    } else {
        DEFAULT_INTERIOR_VOLUME_DEPTH
    };
    let volume_floor_y = xclw_height - volume_depth;
    world.insert(
        entity,
        WaterVolume {
            min: [
                position.x - half_extent,
                volume_floor_y,
                position.z - half_extent,
            ],
            max: [
                position.x + half_extent,
                xclw_height,
                position.z + half_extent,
            ],
        },
    );

    // RenderLayer::Decal pushes water onto a slightly biased depth
    // ladder so it stays above coincident architectural geometry
    // (lake floor mesh, river bed) without z-fighting. The engine-
    // wide depth-bias ladder treats `Decal` as a soft over-bias.
    world.insert(
        entity,
        byroredux_core::ecs::components::RenderLayer::Decal,
    );

    log::debug!(
        "Water plane spawned: pos={:?}, half_extent={}, kind={:?}, normalIdx={}",
        position,
        half_extent,
        kind,
        material.normal_map_index
    );

    Some(1)
}

/// Convenience for the interior path — picks a default half-extent
/// when the cell-load step doesn't yet know the actual reference
/// bounds (most interior cells with water are small pools or
/// flooded rooms; the half-extent is generous so the plane covers
/// the whole room).
#[inline]
pub(super) fn default_interior_half_extent() -> f32 {
    DEFAULT_INTERIOR_HALF_EXTENT
}

/// Convenience for the exterior path — one exterior cell quad.
#[inline]
pub(super) fn exterior_half_extent() -> f32 {
    EXTERIOR_CELL_UNITS * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::water::{WaterKind, WaterMaterial};

    // `resolve_water_material` (+ its WATR reflection-tint / default-tint
    // regressions for #1069) moved to the EXAL boundary in
    // `crate::env_translate`; the tests moved with it.

    /// Regression for #1338 / D3-01 — the normal map `spawn_water_plane`
    /// resolves (bumping a texture refcount) must be reachable by the
    /// SAME `NormalMapHandle` query `unload_cell`'s victim walk uses, so
    /// the refcount is released on cell unload. Pre-fix the index lived
    /// only in `WaterMaterial.normal_map_index`, which the walk can't
    /// reach → one leaked texture + bindless slot per water cell unload.
    ///
    /// The Vulkan half of spawn/unload can't run in a unit test (no
    /// headless `VulkanContext`), so we assert the reachability invariant
    /// directly: a water entity built like `spawn_water_plane` (MeshHandle
    /// + WaterPlane + NormalMapHandle) is found by both the mesh-drop and
    /// the texture-drop queries the walk fans out to.
    #[test]
    fn water_normal_map_handle_reachable_by_unload_walk_query() {
        let mut world = World::new();
        let entity = world.spawn();
        // Mirror the production component set attached by spawn_water_plane.
        world.insert(entity, MeshHandle(7));
        world.insert(
            entity,
            WaterPlane {
                kind: WaterKind::Calm,
                material: WaterMaterial::default(),
            },
        );
        // The fix: a non-zero resolved normal index becomes a handle.
        let resolved_normal_idx: u32 = 42;
        if resolved_normal_idx != 0 {
            world.insert(entity, NormalMapHandle(resolved_normal_idx));
        }

        // `unload_cell` reaches mesh handles via `query::<MeshHandle>()`
        // and texture handles via `query::<NormalMapHandle>()`. Both must
        // find this water entity for cleanup to be complete.
        let mq = world.query::<MeshHandle>().expect("MeshHandle storage");
        assert_eq!(
            mq.get(entity).map(|m| m.0),
            Some(7),
            "water entity's mesh handle must be reachable by the unload walk"
        );
        let nq = world
            .query::<NormalMapHandle>()
            .expect("NormalMapHandle storage");
        assert_eq!(
            nq.get(entity).map(|n| n.0),
            Some(resolved_normal_idx),
            "water entity's normal-map handle must be reachable by the unload \
             walk's NormalMapHandle query so the texture refcount is released"
        );
    }
}
