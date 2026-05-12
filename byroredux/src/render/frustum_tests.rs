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
