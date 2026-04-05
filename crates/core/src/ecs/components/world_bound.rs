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
}
