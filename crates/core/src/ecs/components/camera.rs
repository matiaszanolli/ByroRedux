//! Camera component and ActiveCamera resource.

use crate::ecs::resource::Resource;
use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};
use crate::math::{Mat4, Vec3};

use super::transform::Transform;

/// Perspective camera parameters.
///
/// Attach to an entity that also has a [`Transform`] component.
/// The entity's Transform determines the camera's position and orientation.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Near clipping plane distance.
    pub near: f32,
    /// Far clipping plane distance.
    pub far: f32,
    /// Viewport aspect ratio (width / height). Updated on window resize.
    pub aspect: f32,
}

impl Camera {
    pub fn new(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        Self {
            fov_y,
            near,
            far,
            aspect,
        }
    }

    /// Build a perspective projection matrix (Vulkan clip space: Y-down, Z 0..1).
    pub fn projection_matrix(&self) -> Mat4 {
        // glam's perspective_rh produces right-handed with Z in [-1, 1].
        // For Vulkan we need Z in [0, 1] — use perspective_rh with a
        // correction or use the infinite variant. glam has a dedicated
        // Vulkan-friendly function when using the right setup.
        //
        // Manual Vulkan correction: flip Y and remap Z.
        let mut proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far);
        // Vulkan Y is inverted compared to OpenGL.
        proj.col_mut(1).y *= -1.0;
        proj
    }

    /// Build a view matrix from the camera entity's transform.
    ///
    /// The transform's translation is the camera position.
    /// The transform's rotation determines the look direction
    /// (forward is -Z in the camera's local space).
    pub fn view_matrix(transform: &Transform) -> Mat4 {
        let position = transform.translation;
        let forward = transform.rotation * -Vec3::Z;
        let up = transform.rotation * Vec3::Y;
        Mat4::look_at_rh(position, position + forward, up)
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            fov_y: std::f32::consts::FRAC_PI_4, // 45°
            near: 0.1,
            far: 1000.0,
            aspect: 16.0 / 9.0,
        }
    }
}

impl Component for Camera {
    type Storage = SparseSetStorage<Self>;
}

/// Resource indicating which entity is the active camera.
pub struct ActiveCamera(pub EntityId);
impl Resource for ActiveCamera {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Quat, Vec3, Vec4};
    use std::f32::consts::FRAC_PI_4;

    #[test]
    fn default_camera() {
        let cam = Camera::default();
        assert!((cam.fov_y - FRAC_PI_4).abs() < 1e-6);
        assert!((cam.near - 0.1).abs() < 1e-6);
        assert!((cam.far - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn projection_matrix_is_valid() {
        let cam = Camera::new(FRAC_PI_4, 16.0 / 9.0, 0.1, 100.0);
        let proj = cam.projection_matrix();

        // Near plane should map to Z=0 in Vulkan clip space.
        // Far plane should map to Z=1.
        // Check that the matrix is not all zeros.
        assert!(proj.col(0).x.abs() > 0.0);
        assert!(proj.col(1).y.abs() > 0.0);

        // Y should be flipped for Vulkan (negative).
        assert!(proj.col(1).y < 0.0);
    }

    #[test]
    fn view_matrix_at_origin_looking_forward() {
        let transform = Transform::IDENTITY;
        let view = Camera::view_matrix(&transform);

        // Camera at origin looking down -Z.
        // Point at (0, 0, -5) should be in front of the camera.
        let point = view * Vec4::new(0.0, 0.0, -5.0, 1.0);
        // In view space, the point should have negative Z (in front).
        assert!(point.z < 0.0);
    }

    #[test]
    fn view_matrix_translated() {
        let transform = Transform::from_translation(Vec3::new(0.0, 0.0, 5.0));
        let view = Camera::view_matrix(&transform);

        // Camera at (0, 0, 5) looking down -Z.
        // Origin (0, 0, 0) should be 5 units in front.
        let point = view * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!((point.z + 5.0).abs() < 1e-4);
    }

    #[test]
    fn view_matrix_rotated() {
        // Camera rotated 90° around Y — now looking down -X.
        let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let transform = Transform::from_rotation(rotation);
        let view = Camera::view_matrix(&transform);

        // Point at (-5, 0, 0) should be in front of the camera.
        let point = view * Vec4::new(-5.0, 0.0, 0.0, 1.0);
        assert!(point.z < 0.0);
    }
}
