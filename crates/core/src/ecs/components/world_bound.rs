//! World-space bounding sphere for frustum culling and spatial queries.
//!
//! PackedStorage: read every frame by culling/spatial query systems.
//! Written by import (from NIF bounding data) and transform propagation.

use crate::ecs::packed::PackedStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// World-space bounding sphere.
///
/// Equivalent to Gamebryo's NiAVObject::m_kWorldBound.
/// Center is in world-space coordinates; radius is uniform (max axis scale).
///
/// Used for:
/// - Frustum culling (renderer skips entities outside the view frustum)
/// - Spatial queries (collision broadphase, AI visibility checks)
/// - LOD selection (distance from camera to bound center)
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WorldBound {
    pub center: Vec3,
    pub radius: f32,
}

impl WorldBound {
    pub const ZERO: Self = Self {
        center: Vec3::ZERO,
        radius: 0.0,
    };

    pub fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }

    /// Test whether a point is inside the bounding sphere.
    pub fn contains_point(&self, point: Vec3) -> bool {
        (point - self.center).length_squared() <= self.radius * self.radius
    }

    /// Test whether two bounding spheres overlap.
    pub fn intersects(&self, other: &WorldBound) -> bool {
        let dist_sq = (other.center - self.center).length_squared();
        let radii_sum = self.radius + other.radius;
        dist_sq <= radii_sum * radii_sum
    }

    /// Merge two world-space bounding spheres into one that covers both.
    ///
    /// Uses the "smallest enclosing sphere of two spheres" construction:
    ///   - If one sphere already contains the other, return the larger.
    ///   - Otherwise, the new sphere's diameter equals the distance between
    ///     the two centers plus both radii, and its center sits on the line
    ///     between them weighted by radii.
    ///
    /// Not a minimum-volume bound when more than two spheres are involved —
    /// the caller (bound propagation) reduces over children pairwise, which
    /// can over-estimate by a few percent vs. Welzl's algorithm but stays
    /// O(n) and has no branching.
    ///
    /// A zero-radius sphere is treated as "empty" and ignored so that
    /// `ZERO.merge(b)` returns `b` — this lets propagation start from
    /// `WorldBound::ZERO` and fold children without a special case.
    pub fn merge(&self, other: &WorldBound) -> WorldBound {
        if self.radius <= 0.0 {
            return *other;
        }
        if other.radius <= 0.0 {
            return *self;
        }

        let delta = other.center - self.center;
        let dist = delta.length();

        // One sphere contains the other.
        if dist + other.radius <= self.radius {
            return *self;
        }
        if dist + self.radius <= other.radius {
            return *other;
        }

        let new_radius = (dist + self.radius + other.radius) * 0.5;
        // Interpolate along the line from self.center toward other.center.
        // When dist == 0 we'd divide by zero; handle by returning the
        // bigger radius centered on either (concentric case).
        if dist < 1.0e-6 {
            return WorldBound {
                center: self.center,
                radius: self.radius.max(other.radius),
            };
        }
        let direction = delta / dist;
        let new_center = self.center + direction * (new_radius - self.radius);
        WorldBound {
            center: new_center,
            radius: new_radius,
        }
    }
}

impl Default for WorldBound {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Component for WorldBound {
    type Storage = PackedStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_point_inside() {
        let b = WorldBound::new(Vec3::new(10.0, 0.0, 0.0), 5.0);
        assert!(b.contains_point(Vec3::new(12.0, 0.0, 0.0)));
    }

    #[test]
    fn contains_point_outside() {
        let b = WorldBound::new(Vec3::ZERO, 1.0);
        assert!(!b.contains_point(Vec3::new(2.0, 0.0, 0.0)));
    }

    #[test]
    fn intersects_overlapping() {
        let a = WorldBound::new(Vec3::ZERO, 2.0);
        let b = WorldBound::new(Vec3::new(3.0, 0.0, 0.0), 2.0);
        assert!(a.intersects(&b));
    }

    #[test]
    fn intersects_separated() {
        let a = WorldBound::new(Vec3::ZERO, 1.0);
        let b = WorldBound::new(Vec3::new(5.0, 0.0, 0.0), 1.0);
        assert!(!a.intersects(&b));
    }

    #[test]
    fn default_is_zero() {
        let b = WorldBound::default();
        assert_eq!(b.center, Vec3::ZERO);
        assert_eq!(b.radius, 0.0);
    }

    #[test]
    fn merge_with_zero_returns_other() {
        let a = WorldBound::ZERO;
        let b = WorldBound::new(Vec3::new(5.0, 0.0, 0.0), 2.0);
        let merged = a.merge(&b);
        assert_eq!(merged.center, b.center);
        assert!((merged.radius - b.radius).abs() < 1e-6);
    }

    #[test]
    fn merge_inner_sphere_returns_outer() {
        let outer = WorldBound::new(Vec3::ZERO, 10.0);
        let inner = WorldBound::new(Vec3::new(2.0, 0.0, 0.0), 3.0);
        let merged = outer.merge(&inner);
        assert_eq!(merged.center, outer.center);
        assert!((merged.radius - outer.radius).abs() < 1e-6);
    }

    #[test]
    fn merge_two_disjoint_spheres_covers_both() {
        // Two unit spheres at x=±10: merged sphere has diameter 22, radius 11, center origin.
        let a = WorldBound::new(Vec3::new(-10.0, 0.0, 0.0), 1.0);
        let b = WorldBound::new(Vec3::new(10.0, 0.0, 0.0), 1.0);
        let merged = a.merge(&b);
        assert!((merged.center - Vec3::ZERO).length() < 1e-6);
        assert!((merged.radius - 11.0).abs() < 1e-6);
        // The merged sphere must contain both inputs.
        assert!(merged.contains_point(a.center));
        assert!(merged.contains_point(b.center));
    }

    #[test]
    fn merge_concentric_returns_larger_radius() {
        let a = WorldBound::new(Vec3::new(2.0, 3.0, 4.0), 1.5);
        let b = WorldBound::new(Vec3::new(2.0, 3.0, 4.0), 0.5);
        let merged = a.merge(&b);
        assert_eq!(merged.center, a.center);
        assert!((merged.radius - 1.5).abs() < 1e-6);
    }
}
