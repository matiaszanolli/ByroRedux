//! Mesh-local bounding sphere, in pre-transform space.
//!
//! Written once at import time (e.g. from NIF `NiTriShapeData::center/radius`
//! or `BsTriShape::center/radius`) and consumed by
//! `world_bound_propagation_system` which composes it with the entity's
//! `GlobalTransform` into a `WorldBound`.
//!
//! SparseSetStorage: only geometry leaves carry a local bound; most
//! scene-graph nodes derive their WorldBound from child unions.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Object-space bounding sphere for a mesh.
///
/// `center` is in the mesh's own local coordinate frame (the same frame
/// `Transform.translation` is applied from); `radius` is uniform and
/// expressed in local units. The world-space bound is derived by
/// transforming the center through `GlobalTransform` and scaling the
/// radius by the global scale.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct LocalBound {
    pub center: Vec3,
    pub radius: f32,
}

impl LocalBound {
    pub const ZERO: Self = Self {
        center: Vec3::ZERO,
        radius: 0.0,
    };

    pub fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }

    /// Compute a mesh-local bounding sphere from a set of vertex positions.
    ///
    /// Uses the naïve centroid + max-distance approach. It is not a
    /// minimal bounding sphere (Ritter's algorithm would be tighter) but
    /// it is deterministic, O(n), and sufficient for frustum culling —
    /// the renderer re-uses this for coarse visibility checks, not for
    /// precise queries.
    pub fn from_points(points: &[[f32; 3]]) -> Self {
        if points.is_empty() {
            return Self::ZERO;
        }

        let mut sum = Vec3::ZERO;
        for p in points {
            sum.x += p[0];
            sum.y += p[1];
            sum.z += p[2];
        }
        let inv_n = 1.0 / points.len() as f32;
        let center = Vec3::new(sum.x * inv_n, sum.y * inv_n, sum.z * inv_n);

        let mut max_sq = 0.0f32;
        for p in points {
            let dx = p[0] - center.x;
            let dy = p[1] - center.y;
            let dz = p[2] - center.z;
            let d_sq = dx * dx + dy * dy + dz * dz;
            if d_sq > max_sq {
                max_sq = d_sq;
            }
        }

        Self {
            center,
            radius: max_sq.sqrt(),
        }
    }
}

impl Default for LocalBound {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Component for LocalBound {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_points_empty_is_zero() {
        let b = LocalBound::from_points(&[]);
        assert_eq!(b.center, Vec3::ZERO);
        assert_eq!(b.radius, 0.0);
    }

    #[test]
    fn from_points_centers_on_centroid() {
        let b = LocalBound::from_points(&[[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]]);
        assert_eq!(b.center, Vec3::ZERO);
        assert!((b.radius - 1.0).abs() < 1e-6);
    }

    #[test]
    fn from_points_single_vertex_has_zero_radius() {
        let b = LocalBound::from_points(&[[3.0, 4.0, 5.0]]);
        assert_eq!(b.center, Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(b.radius, 0.0);
    }

    #[test]
    fn from_points_cube_corners_radius_is_half_diagonal() {
        // Unit cube corners centered at origin — max distance is sqrt(3).
        let b = LocalBound::from_points(&[
            [-0.5, -0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, 0.5, 0.5],
            [0.5, -0.5, -0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, -0.5],
            [0.5, 0.5, 0.5],
        ]);
        assert!((b.center - Vec3::ZERO).length() < 1e-6);
        let expected_radius = (3.0f32).sqrt() / 2.0;
        assert!((b.radius - expected_radius).abs() < 1e-6);
    }
}
