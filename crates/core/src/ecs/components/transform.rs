//! Transform component — local-space position, rotation, and scale.
//!
//! PackedStorage: read every frame by renderer, physics, animation.
//! Maps to Gamebryo's NiTransform (but uses Quat instead of Matrix3).

use crate::ecs::packed::PackedStorage;
use crate::ecs::storage::Component;
use crate::math::{Mat4, Quat, Vec3};

/// Local-space transform for an entity.
///
/// Uses quaternion rotation (16 bytes) instead of Gamebryo's 3x3 matrix
/// (36 bytes) — more compact, better for interpolation (SLERP), standard
/// in modern engines. Convert from matrix on NIF import.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: f32,
}

impl Transform {
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

    pub fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    pub fn from_rotation(rotation: Quat) -> Self {
        Self {
            rotation,
            ..Self::IDENTITY
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
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Component for Transform {
    type Storage = PackedStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn identity_matrix() {
        let t = Transform::IDENTITY;
        let m = t.to_matrix();
        assert_eq!(m, Mat4::IDENTITY);
    }

    #[test]
    fn translation_only() {
        let t = Transform::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let m = t.to_matrix();
        // Last column is the translation.
        let col3 = m.col(3);
        assert!((col3.x - 1.0).abs() < 1e-6);
        assert!((col3.y - 2.0).abs() < 1e-6);
        assert!((col3.z - 3.0).abs() < 1e-6);
    }

    #[test]
    fn scale_uniform() {
        let t = Transform {
            scale: 2.0,
            ..Transform::IDENTITY
        };
        let m = t.to_matrix();
        // Diagonal should be 2.0 (except w=1).
        assert!((m.col(0).x - 2.0).abs() < 1e-6);
        assert!((m.col(1).y - 2.0).abs() < 1e-6);
        assert!((m.col(2).z - 2.0).abs() < 1e-6);
    }

    #[test]
    fn rotation_90_degrees_y() {
        let t = Transform::from_rotation(Quat::from_rotation_y(FRAC_PI_2));
        let m = t.to_matrix();
        // Rotating (1, 0, 0) by 90° around Y should give approximately (0, 0, -1).
        let result = m.transform_point3(Vec3::X);
        assert!((result.x).abs() < 1e-5);
        assert!((result.z + 1.0).abs() < 1e-5);
    }

    #[test]
    fn combined_transform() {
        let t = Transform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 3.0);
        let m = t.to_matrix();
        // Point at origin should end up at (10, 0, 0) after transform.
        let result = m.transform_point3(Vec3::ZERO);
        assert!((result.x - 10.0).abs() < 1e-5);
        // Point at (1, 0, 0) should end up at (13, 0, 0) after scale+translate.
        let result2 = m.transform_point3(Vec3::X);
        assert!((result2.x - 13.0).abs() < 1e-5);
    }

    #[test]
    fn default_is_identity() {
        assert_eq!(Transform::default().to_matrix(), Mat4::IDENTITY);
    }
}
