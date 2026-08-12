//! Trigger-volume extraction from collision primitives.
//!
//! Extracted from `references/mod.rs`'s inline `mod tests`
//! (#2409 / TD1-006). Contents unchanged.

use super::*;
// Test-only symbols not referenced by production code in this module
// (they'd warn as unused at file scope). #1877 split.

/// `XPRM` box primitive → world-space `TriggerVolume`: bounds are
/// z-up half-extents, permuted to engine y-up `[x, z, y]` and scaled
/// by the REFR scale. Center / rotation pass through verbatim.
#[test]
fn trigger_volume_from_box_primitive_permutes_and_scales() {
    let prim = esm::cell::PrimitiveBounds {
        bounds: [10.0, 20.0, 30.0], // z-up: x=10, y=20, z=30
        color: [1.0, 0.0, 0.0],
        unknown: 0.0,
        shape_type: 1, // Box
    };
    let center = Vec3::new(100.0, 5.0, -50.0);
    let v = trigger_volume_from_primitive(&prim, center, Quat::IDENTITY, 2.0)
        .expect("box primitive yields a volume");
    assert_eq!(v.shape, byroredux_scripting::TriggerShape::Box);
    assert_eq!(v.center, center);
    // y-up half-extents = [x, z, y] * scale = [10, 30, 20] * 2.
    assert_eq!(v.half_extents, Vec3::new(20.0, 60.0, 40.0));
}

/// Sphere primitive (shape 3): `bounds[0]` is the radius, carried in
/// `half_extents.x` and scaled.
#[test]
fn trigger_volume_from_sphere_primitive_uses_radius() {
    let prim = esm::cell::PrimitiveBounds {
        bounds: [15.0, 0.0, 0.0],
        color: [0.0; 3],
        unknown: 0.0,
        shape_type: 3, // Sphere
    };
    let v = trigger_volume_from_primitive(&prim, Vec3::ZERO, Quat::IDENTITY, 3.0)
        .expect("sphere primitive yields a volume");
    assert_eq!(v.shape, byroredux_scripting::TriggerShape::Sphere);
    assert_eq!(v.half_extents.x, 45.0); // 15 * 3
}

/// #1742 / SCR-D7-02 — the audit flagged (verify-not-confirmed) that
/// `half_extents` is permuted z-up→y-up (`[x, z, y]`) while `rotation`
/// passes through verbatim, and worried the two might not be in the
/// same frame for a non-axis-aligned trigger box.
///
/// They ARE the same frame. `rotation` here is exactly
/// `euler_zup_to_quat_yup_refr`'s output — the same conversion every
/// other REFR placement in this loader uses — derived specifically so
/// it rotates y-up-frame vectors consistently with `zup_to_yup_pos`'s
/// position conversion. This test proves it end-to-end without
/// leaning on that self-consistency: it rotates a point in the box's
/// OWN z-up local frame (on the z-up +Y face — `bounds[1]`, one of the
/// two permuted axes) using Bethesda's independently-implemented
/// clockwise convention, converts the ROTATED point to y-up via the
/// canonical `zup_to_yup_pos` (not via `rotation`, and not via
/// anything `trigger_volume_from_primitive` touches), and checks
/// `TriggerVolume::contains` classifies points just inside/outside
/// that rotated face correctly. Deliberately probes `bounds[1]`
/// (z-up Y), not `bounds[0]` (z-up X, untouched by the permutation) —
/// a swapped `[x, z, y]` → `[x, y, z]` regression wouldn't move this
/// axis and this test would falsely pass.
#[test]
fn rotated_box_trigger_composes_rotation_in_same_frame_as_permuted_extents() {
    use std::f32::consts::FRAC_PI_2;

    // z-up: x=10, y=20 (the axis under test), z=30.
    let prim = esm::cell::PrimitiveBounds {
        bounds: [10.0, 20.0, 30.0],
        color: [0.0; 3],
        unknown: 0.0,
        shape_type: 1, // Box
    };
    let center = Vec3::ZERO;
    // A pure 90° yaw (Bethesda's "rz" Euler component) — same helper,
    // same shipping mode (1 = CW+ZYX), as every other placed REFR.
    let ref_rot = euler_zup_to_quat_yup_refr(0.0, 0.0, FRAC_PI_2);
    let v = trigger_volume_from_primitive(&prim, center, ref_rot, 1.0)
        .expect("box primitive yields a volume");

    // Bethesda's clockwise rotation about z-up's Z axis, applied
    // directly to a z-up local point — independent of `ref_rot`.
    let cw_rotate_zup_by_z = |p: Vec3, theta: f32| {
        let (s, c) = theta.sin_cos();
        Vec3::new(p.x * c + p.y * s, -p.x * s + p.y * c, p.z)
    };
    let just_inside_zup = cw_rotate_zup_by_z(Vec3::new(0.0, 19.9, 0.0), FRAC_PI_2);
    let just_outside_zup = cw_rotate_zup_by_z(Vec3::new(0.0, 20.1, 0.0), FRAC_PI_2);
    let just_inside_world = Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(
        just_inside_zup.to_array(),
    ));
    let just_outside_world = Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(
        just_outside_zup.to_array(),
    ));

    assert!(
        v.contains(just_inside_world),
        "a point 0.1 units inside the box's rotated z-up +Y face must test \
         inside (just_inside_world = {just_inside_world:?})"
    );
    assert!(
        !v.contains(just_outside_world),
        "a point 0.1 units outside the same rotated face must test outside \
         (just_outside_world = {just_outside_world:?})"
    );
}

/// Non-containment shapes (line / portal / plane) don't become
/// trigger volumes — they're not solids a point can be inside.
#[test]
fn trigger_volume_rejects_non_containment_shapes() {
    for shape_type in [2u32, 4, 5] {
        let prim = esm::cell::PrimitiveBounds {
            bounds: [1.0, 1.0, 1.0],
            color: [0.0; 3],
            unknown: 0.0,
            shape_type,
        };
        assert!(
            trigger_volume_from_primitive(&prim, Vec3::ZERO, Quat::IDENTITY, 1.0).is_none(),
            "shape_type {shape_type} must not yield a containment volume",
        );
    }
}
