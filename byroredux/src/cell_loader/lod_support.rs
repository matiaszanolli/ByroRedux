//! Shared geometry helpers for the two distant-LOD spawn paths — object
//! LOD (`.bto`, [`super::object_lod`]) and placement LOD (`_far.nif`,
//! [`super::placement_lod`]).
//!
//! The format-specific streaming logic (`.bto` vs `.lod` discovery, ring
//! math, per-game gating) is deliberately kept separate in each module.
//! Only the format-agnostic `ImportedMesh` → renderer conversion lives
//! here, so the two paths can't drift on vertex defaults or bound math
//! (TD2-105 / #2064).

use byroredux_core::math::Vec3;
use byroredux_nif::import::ImportedMesh;
use byroredux_renderer::Vertex;

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
}
