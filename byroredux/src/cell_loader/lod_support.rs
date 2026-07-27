//! Shared reconciliation policy and geometry helpers for the three
//! distant-LOD spawn paths — terrain, object LOD (`.bto`,
//! [`super::object_lod`]), and placement LOD (`_far.nif`,
//! [`super::placement_lod`]).
//!
//! The format-specific streaming logic (`.bto` vs `.lod` discovery, ring
//! math, per-game gating) is deliberately kept separate in each module.
//! Format-agnostic budget accounting, closest-first ordering, shared inputs,
//! and `ImportedMesh` → renderer conversion live here so providers cannot
//! drift on scheduling, vertex defaults, or bound math (TD2-105 / #2064).

use byroredux_core::math::Vec3;
use byroredux_nif::import::ImportedMesh;
use byroredux_renderer::Vertex;

use crate::asset_provider::TextureProvider;

use super::exterior::ExteriorWorldContext;

/// Immutable inputs shared by terrain and both game-specific object LOD
/// providers during one ring reconciliation.
pub(crate) struct LodReconcileInput<'a> {
    pub(crate) tex_provider: &'a TextureProvider,
    pub(crate) wctx: &'a ExteriorWorldContext,
    pub(crate) player_grid: (i32, i32),
    /// The full-detail streaming hysteresis boundary (`radius_unload`).
    pub(crate) max_full_cell_radius: i32,
}

/// Bounded main-thread work allowance shared by every distant-LOD provider.
///
/// One unit represents one archive/import/upload attempt for a terrain block,
/// object quad, or placement cell. Reclaims do not consume the budget: stale
/// geometry must disappear immediately when the player moves, while new
/// geometry may fill in progressively over later frames.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LodWorkBudget {
    remaining: usize,
    spent: usize,
}

impl LodWorkBudget {
    pub(crate) fn new(max_attempts: usize) -> Self {
        Self {
            remaining: max_attempts,
            spent: 0,
        }
    }

    pub(crate) fn unlimited() -> Self {
        Self::new(usize::MAX)
    }

    /// Reserve one potentially expensive provider attempt.
    pub(crate) fn try_take(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        self.spent += 1;
        true
    }

    pub(crate) fn spent(self) -> usize {
        self.spent
    }
}

/// Apply the shared closest-first ordering used by every LOD provider.
///
/// Providers supply their own distance metric (terrain block distance, object
/// quad minimum-cell distance, or placement-cell distance); Y/X tie-breakers
/// keep the result stable across runs and archive/hash iteration order.
pub(crate) fn sort_lod_coords_nearest(
    coords: &mut [(i32, i32)],
    distance: impl Fn((i32, i32)) -> i32,
) {
    coords.sort_unstable_by_key(|&(x, y)| (distance((x, y)), y, x));
}

/// Convert an [`ImportedMesh`]'s parallel per-vertex arrays into renderer
/// [`Vertex`]es, applying the LOD fallback defaults (opaque white colour,
/// +Y normal, zero UV) for any array shorter than `positions` and copying
/// authored tangents where present.
///
/// Both LOD paths upload the result via `upload_scene_mesh_global_only`
/// (global-SSBO-only, no per-mesh buffers / no BLAS), so this never needs
/// the RT-capable vertex layout.
pub(crate) fn imported_mesh_to_vertices(mesh: &ImportedMesh) -> Vec<Vertex> {
    (0..mesh.positions.len())
        .map(|i| {
            let color = mesh.colors.get(i).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let normal = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
            let mut v = Vertex::new_rgba(mesh.positions[i], color, normal, uv);
            if let Some(t) = mesh.tangents.get(i) {
                v.tangent = *t;
            }
            v
        })
        .collect()
}

/// Local-space bounding sphere `(centre, radius)` of a mesh's positions:
/// the min/max AABB midpoint and the distance from it to the far corner.
///
/// Callers guard against empty position lists before calling — an empty
/// slice would leave the min/max sentinels un-updated and yield a NaN
/// centre, matching the pre-extraction inline behaviour.
pub(crate) fn local_aabb_center_radius(positions: &[[f32; 3]]) -> (Vec3, f32) {
    let mut lmin = Vec3::splat(f32::INFINITY);
    let mut lmax = Vec3::splat(f32::NEG_INFINITY);
    for p in positions {
        let v = Vec3::from_array(*p);
        lmin = lmin.min(v);
        lmax = lmax.max(v);
    }
    let centre = (lmin + lmax) * 0.5;
    let radius = (lmax - centre).length();
    (centre, radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mesh with only positions authored — every other per-vertex array
    /// empty, so `imported_mesh_to_vertices` must apply its fallbacks.
    fn positions_only(positions: Vec<[f32; 3]>) -> ImportedMesh {
        ImportedMesh::from_geometry(
            positions,
            Vec::new(), // colors
            Vec::new(), // normals
            Vec::new(), // tangents
            Vec::new(), // uvs
            Vec::new(), // indices
        )
    }

    #[test]
    fn vertices_apply_lod_fallback_defaults_when_arrays_short() {
        // Only positions authored — colour/normal/uv fall back, no tangent.
        let mesh = positions_only(vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let verts = imported_mesh_to_vertices(&mesh);
        assert_eq!(verts.len(), 2);
        assert_eq!(verts[0].position, [1.0, 2.0, 3.0]);
        // Vertex::new_rgba stores colour as rgba; the fallback is opaque white.
        assert_eq!(verts[0].color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(verts[0].normal, [0.0, 1.0, 0.0]);
        assert_eq!(verts[0].uv, [0.0, 0.0]);
        assert_eq!(verts[0].tangent, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn aabb_center_radius_is_midpoint_and_corner_distance() {
        // Unit cube corners → centre (0.5,0.5,0.5), radius = |(0.5,0.5,0.5)|.
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        let (centre, radius) = local_aabb_center_radius(&positions);
        assert_eq!(centre, Vec3::splat(0.5));
        assert!((radius - Vec3::splat(0.5).length()).abs() < 1e-6);
    }

    #[test]
    fn lod_work_budget_caps_attempts_and_tracks_spend() {
        let mut budget = LodWorkBudget::new(2);
        assert!(budget.try_take());
        assert!(budget.try_take());
        assert!(!budget.try_take());
        assert_eq!(budget.spent(), 2);

        let mut unlimited = LodWorkBudget::unlimited();
        for _ in 0..1_000 {
            assert!(unlimited.try_take());
        }
        assert_eq!(unlimited.spent(), 1_000);
    }

    #[test]
    fn lod_coordinate_order_is_nearest_first_with_stable_ties() {
        let mut coords = vec![(2, 0), (0, 1), (-1, 0), (1, 0), (0, -1)];
        sort_lod_coords_nearest(&mut coords, |coord| coord.0.abs().max(coord.1.abs()));
        assert_eq!(coords, vec![(0, -1), (-1, 0), (1, 0), (0, 1), (2, 0)]);
    }
}
