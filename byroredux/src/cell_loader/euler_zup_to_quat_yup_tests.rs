//! Tests for `euler_zup_to_quat_yup_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`euler_zup_to_quat_yup_tests::FOO`).

//! Regression tests for XCLL's dedicated azimuth/elevation conversion.
//! These angles are not a REFR Euler triple and must not use the shared
//! placement helper.
use super::*;

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

fn xcll_dir_yup(azimuth: f32, elevation: f32) -> Vec3 {
    load::xcll_direction_yup(azimuth, elevation)
}

/// Baseline: `(rx, ry) = (0, 0)` must leave the model direction
/// at `(1, 0, 0)` — no rotation, no drift, identity pass-through.
/// Z-up default `(1, 0, 0)` maps to Y-up `(1, 0, 0)` because the
/// x axis is invariant under the Z-up → Y-up coord swap.
#[test]
fn zero_rotation_returns_model_direction_unchanged() {
    let dir = xcll_dir_yup(0.0, 0.0);
    assert!(approx_eq(dir.x, 1.0), "x should be 1, got {}", dir.x);
    assert!(approx_eq(dir.y, 0.0), "y should be 0, got {}", dir.y);
    assert!(approx_eq(dir.z, 0.0), "z should be 0, got {}", dir.z);
}

/// Rotation XY is azimuth: a quarter turn must move the horizontal source
/// direction from +X to source +Y, which maps to renderer -Z.
#[test]
fn azimuth_quarter_turn_moves_to_renderer_negative_z() {
    let dir = xcll_dir_yup(std::f32::consts::FRAC_PI_2, 0.0);
    assert!(approx_eq(dir.x, 0.0), "x should be 0, got {}", dir.x);
    assert!(approx_eq(dir.y, 0.0), "y should be 0, got {}", dir.y);
    assert!(approx_eq(dir.z, -1.0), "z should be -1, got {}", dir.z);
}

/// The FNV population resolves the elevation sign: 96 of 252 active XCLL
/// directionals use 270 degrees as the overhead key-light preset.
#[test]
fn elevation_270_degrees_points_to_renderer_up() {
    let dir = xcll_dir_yup(0.0, 3.0 * std::f32::consts::FRAC_PI_2);
    assert!(approx_eq(dir.x, 0.0), "x should be 0, got {}", dir.x);
    assert!(approx_eq(dir.y, 1.0), "y should be 1, got {}", dir.y);
    assert!(approx_eq(dir.z, 0.0), "z should be 0, got {}", dir.z);
}

/// Output vector must always be unit length — XCLL rotations are
/// rigid, so the direction magnitude must not drift. Exercises a
/// non-trivial `(rx, ry)` pair to avoid hitting the axis-invariant
/// corners.
#[test]
fn output_is_unit_length_for_arbitrary_angles() {
    let dir = xcll_dir_yup(0.3, 0.7);
    let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
    assert!(
        (len - 1.0).abs() < 1e-5,
        "quaternion rotation must preserve length (got {})",
        len
    );
}
