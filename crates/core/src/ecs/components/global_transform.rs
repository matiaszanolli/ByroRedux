//! Computed world-space transform.
//!
//! PackedStorage: read every frame by the renderer for model matrices.
//! Written only by `transform_propagation_system`, never by user code.

use crate::ecs::packed::PackedStorage;
use crate::ecs::storage::Component;
use crate::math::{Mat4, Quat, Vec3};

/// World-space transform computed from the local `Transform` and parent chain.
///
/// For root entities (no `Parent`), this equals the entity's `Transform`.
/// For child entities, this is the composed result of all ancestor transforms.
///
/// The renderer reads this (not `Transform`) for the model matrix.
#[derive(Debug, Clone, Copy)]
pub struct GlobalTransform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: f32,
}

impl GlobalTransform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: 1.0,
    };

    pub fn new(translation: Vec3, rotation: Quat, scale: f32) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Build a 4x4 model matrix: scale → rotate → translate.
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            self.rotation,
            self.translation,
        )
    }

    /// Compose a parent's global transform with a child's local transform.
    pub fn compose(
        parent: &GlobalTransform,
        local_translation: Vec3,
        local_rotation: Quat,
        local_scale: f32,
    ) -> Self {
        Self {
            translation: parent.translation + parent.rotation * (parent.scale * local_translation),
            rotation: parent.rotation * local_rotation,
            scale: parent.scale * local_scale,
        }
    }
}

impl Default for GlobalTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Component for GlobalTransform {
    type Storage = PackedStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn identity_matrix() {
        let g = GlobalTransform::IDENTITY;
        assert_eq!(g.to_matrix(), Mat4::IDENTITY);
    }

    #[test]
    fn compose_translation_only() {
        let parent = GlobalTransform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let child =
            GlobalTransform::compose(&parent, Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        assert!((child.translation.x - 15.0).abs() < 1e-5);
    }

    #[test]
    fn compose_with_scale() {
        let parent = GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 2.0);
        let child =
            GlobalTransform::compose(&parent, Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        // Parent scale 2.0 * child local offset 5.0 = 10.0
        assert!((child.translation.x - 10.0).abs() < 1e-5);
        assert!((child.scale - 2.0).abs() < 1e-5);
    }

    #[test]
    fn compose_with_rotation() {
        let parent = GlobalTransform::new(Vec3::ZERO, Quat::from_rotation_y(FRAC_PI_2), 1.0);
        // Child at (1, 0, 0) local → parent rotates 90° around Y → (0, 0, -1) world
        let child =
            GlobalTransform::compose(&parent, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        assert!(child.translation.x.abs() < 1e-4);
        assert!((child.translation.z + 1.0).abs() < 1e-4);
    }

    #[test]
    fn compose_chain_three_levels() {
        let root = GlobalTransform::new(Vec3::new(100.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let mid = GlobalTransform::compose(&root, Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 2.0);
        let leaf = GlobalTransform::compose(&mid, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        // root(100) + mid_local(10) = 110, mid_scale=2, leaf_local(1)*2 = 2 → 112
        assert!((leaf.translation.x - 112.0).abs() < 1e-4);
        assert!((leaf.scale - 2.0).abs() < 1e-5);
    }
}
