//! Z-up -> Y-up conversion plus key conversion / sampling
//! (`anim/coord.rs`, `anim/keys.rs`).

use super::super::*;

use crate::blocks::interpolator::{FloatKey, KeyGroup, KeyType, NiTransformData};
use crate::scene::NifScene;

/// #3097 — pinned against nif.xml's `CycleType` enum ordinals
/// (`CYCLE_LOOP` = 0, `CYCLE_REVERSE` = 1, `CYCLE_CLAMP` = 2), not
/// against whatever `from_u32` happens to return — the pre-fix version
/// of this test asserted the implementation's own (wrong) output and
/// so could never have caught the ordinal rotation it shipped with.
#[test]
fn cycle_type_from_u32() {
    assert_eq!(CycleType::from_u32(0), CycleType::Loop);
    assert_eq!(CycleType::from_u32(1), CycleType::Reverse);
    assert_eq!(CycleType::from_u32(2), CycleType::Clamp);
    assert_eq!(CycleType::from_u32(99), CycleType::Clamp);
}

#[test]
fn zup_to_yup_position() {
    // Gamebryo Z-up (1, 2, 3) → Y-up (1, 3, -2)
    let result = zup_to_yup_pos([1.0, 2.0, 3.0]);
    assert_eq!(result, [1.0, 3.0, -2.0]);
}

#[test]
fn zup_to_yup_identity_quat() {
    // Gamebryo identity (w=1, x=0, y=0, z=0) → glam (x=0, y=0, z=0, w=1)
    let result = zup_to_yup_quat([1.0, 0.0, 0.0, 0.0]);
    assert_eq!(result, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn empty_scene_produces_no_clips() {
    let scene = NifScene::default();
    let clips = import_kf(&scene);
    assert!(clips.is_empty());
}

// #2434 / COORD-1 — `euler_to_quat_wxyz` (the private, independently-signed
// CCW-composed formula these tests used to exercise directly) is gone;
// `convert_xyz_euler_keys` now routes through the core SoT
// `byroredux_core::math::coord::euler_zup_to_quat_yup`, already covered by
// that function's own test suite. The tests below exercise the
// KF-specific plumbing (sampling + Z-up→Y-up handoff) and, critically,
// PIN THE SIGN — the four tests removed here asserted only unit length
// and axis dominance, which passes under either the correct CW-positive
// convention or the (bug's) CCW-positive one. See
// `convert_xyz_euler_keys_matches_core_sot_for_asymmetric_multi_axis`
// below for the actual regression pin.

/// Sign-discriminating regression for #2434 / COORD-1. A single-axis 90°
/// rotation is directionally unambiguous: CW-positive (correct) and
/// CCW-positive (the bug) place the swept quadrant on OPPOSITE sides,
/// which shows up as an opposite-signed vector component — unlike the
/// pre-fix tests, which only checked magnitude/dominance.
#[test]
fn convert_xyz_euler_keys_90_deg_x_matches_cw_positive_sign() {
    use std::f32::consts::FRAC_PI_2;
    let x_keys = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![FloatKey {
            time: 0.0,
            value: FRAC_PI_2,
            tangent_forward: 0.0,
            tangent_backward: 0.0,
            tbc: None,
        }],
    };
    let empty_keys = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![FloatKey {
            time: 0.0,
            value: 0.0,
            tangent_forward: 0.0,
            tangent_backward: 0.0,
            tbc: None,
        }],
    };
    let data = NiTransformData {
        rotation_type: Some(KeyType::XyzRotation),
        rotation_keys: Vec::new(),
        xyz_rotations: Some([x_keys, empty_keys.clone(), empty_keys]),
        translations: KeyGroup {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
        scales: KeyGroup {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
    };
    let (keys, _) = convert_xyz_euler_keys(&data);
    assert_eq!(keys.len(), 1);

    // Ground truth: the core SoT every other Gamebryo Euler consumer
    // uses, applied to the same (rx=FRAC_PI_2, ry=0, rz=0) input.
    let expected = byroredux_core::math::coord::euler_zup_to_quat_yup(FRAC_PI_2, 0.0, 0.0);
    let got = keys[0].value; // [x, y, z, w], glam order
    assert!(
        (got[0] - expected.x).abs() < 1e-5,
        "x: got {got:?}, expected {expected:?}"
    );
    assert!(
        (got[1] - expected.y).abs() < 1e-5,
        "y: got {got:?}, expected {expected:?}"
    );
    assert!(
        (got[2] - expected.z).abs() < 1e-5,
        "z: got {got:?}, expected {expected:?}"
    );
    assert!(
        (got[3] - expected.w).abs() < 1e-5,
        "w: got {got:?}, expected {expected:?}"
    );

    // X is invariant under the Z-up→Y-up swap (`zup_to_yup_pos([x,y,z])
    // = (x,z,-y)`), so a pure-X-axis Gamebryo rotation stays on the
    // glam X axis post-conversion — the sign is the whole story here.
    // The pre-fix (CCW, un-negated) formula lands on `x = +0.707`; the
    // correct CW-positive formula lands on `x = -0.707`. Pin the sign
    // explicitly so a regression back to the CCW formula fails loudly
    // rather than passing on the `expected`-comparison alone (which
    // would also fail, but this makes the failure mode legible without
    // cross-referencing `euler_zup_to_quat_yup`).
    assert!(
        got[0] < -0.5,
        "expected a negative (CW-positive) X component for a +90° \
         X-axis Gamebryo rotation, got {got:?} — a positive value here \
         means the CCW-positive bug (#2434) has regressed"
    );
}

/// Multi-axis regression: pins `convert_xyz_euler_keys`'s full output
/// against `euler_zup_to_quat_yup` for an asymmetric (rx, ry, rz) triple
/// where sign AND composition-order errors both produce a materially
/// different quaternion — the strongest single check against a
/// regression to either bug class.
#[test]
fn convert_xyz_euler_keys_matches_core_sot_for_asymmetric_multi_axis() {
    let (rx, ry, rz) = (0.3_f32, 0.5_f32, 0.7_f32);
    let one_key = |v: f32| KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![FloatKey {
            time: 0.0,
            value: v,
            tangent_forward: 0.0,
            tangent_backward: 0.0,
            tbc: None,
        }],
    };
    let data = NiTransformData {
        rotation_type: Some(KeyType::XyzRotation),
        rotation_keys: Vec::new(),
        xyz_rotations: Some([one_key(rx), one_key(ry), one_key(rz)]),
        translations: KeyGroup {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
        scales: KeyGroup {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
    };
    let (keys, _) = convert_xyz_euler_keys(&data);
    assert_eq!(keys.len(), 1);

    let expected = byroredux_core::math::coord::euler_zup_to_quat_yup(rx, ry, rz);
    let got = keys[0].value;
    assert!(
        (got[0] - expected.x).abs() < 1e-5,
        "x: got {got:?}, expected {expected:?}"
    );
    assert!(
        (got[1] - expected.y).abs() < 1e-5,
        "y: got {got:?}, expected {expected:?}"
    );
    assert!(
        (got[2] - expected.z).abs() < 1e-5,
        "z: got {got:?}, expected {expected:?}"
    );
    assert!(
        (got[3] - expected.w).abs() < 1e-5,
        "w: got {got:?}, expected {expected:?}"
    );
}

#[test]
fn sample_float_key_group_linear() {
    let group = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![
            FloatKey {
                time: 0.0,
                value: 0.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
            FloatKey {
                time: 1.0,
                value: 1.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
        ],
    };
    assert!((sample_float_key_group(&group, 0.5) - 0.5).abs() < 1e-5);
    assert!((sample_float_key_group(&group, 0.0) - 0.0).abs() < 1e-5);
    assert!((sample_float_key_group(&group, 1.0) - 1.0).abs() < 1e-5);
}

#[test]
fn sample_float_key_group_empty() {
    let group = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![],
    };
    assert_eq!(sample_float_key_group(&group, 0.5), 0.0);
}

#[test]
fn sample_float_key_group_single() {
    let group = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![FloatKey {
            time: 0.5,
            value: 42.0,
            tangent_forward: 0.0,
            tangent_backward: 0.0,
            tbc: None,
        }],
    };
    assert_eq!(sample_float_key_group(&group, 0.0), 42.0);
    assert_eq!(sample_float_key_group(&group, 1.0), 42.0);
}

#[test]
fn convert_xyz_euler_keys_produces_rotation_keys() {
    use std::f32::consts::FRAC_PI_2;
    // Create NiTransformData with XYZ euler rotation keys:
    // At t=0: all angles 0 (identity)
    // At t=1: 90° around X
    let x_keys = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![
            FloatKey {
                time: 0.0,
                value: 0.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
            FloatKey {
                time: 1.0,
                value: FRAC_PI_2,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
        ],
    };
    let empty_keys = KeyGroup {
        key_type: KeyType::Linear,
        keys: vec![
            FloatKey {
                time: 0.0,
                value: 0.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
            FloatKey {
                time: 1.0,
                value: 0.0,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            },
        ],
    };

    let data = NiTransformData {
        rotation_type: Some(KeyType::XyzRotation),
        rotation_keys: Vec::new(),
        xyz_rotations: Some([x_keys, empty_keys.clone(), empty_keys]),
        translations: KeyGroup {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
        scales: KeyGroup {
            key_type: KeyType::Linear,
            keys: Vec::new(),
        },
    };

    let (keys, key_type) = convert_xyz_euler_keys(&data);
    assert_eq!(key_type, KeyType::Linear);
    assert_eq!(
        keys.len(),
        2,
        "should have 2 rotation keys (one per unique timestamp)"
    );

    // First key (t=0): identity → after Z-up to Y-up, glam format (x, y, z, w)
    let k0 = &keys[0];
    assert!((k0.time).abs() < 1e-5);
    // Identity quat in glam: (0, 0, 0, 1)
    assert!(
        (k0.value[3] - 1.0).abs() < 1e-4,
        "w should be ~1 for identity: {:?}",
        k0.value
    );

    // Second key (t=1): 90° around X in Z-up, then converted to Y-up
    let k1 = &keys[1];
    assert!((k1.time - 1.0).abs() < 1e-5);
    // Should be a unit quaternion
    let len_sq = k1.value.iter().map(|v| v * v).sum::<f32>();
    assert!(
        (len_sq - 1.0).abs() < 1e-4,
        "quaternion should be unit: {:?}",
        k1.value
    );
}

// ── B-spline evaluator tests (issue #155) ──────────────────────────
