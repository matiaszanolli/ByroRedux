//! Z-up -> Y-up conversion plus key conversion / sampling
//! (`anim/coord.rs`, `anim/keys.rs`).

use super::super::*;

use crate::blocks::interpolator::{FloatKey, KeyGroup, KeyType, NiTransformData};
use crate::scene::NifScene;

#[test]
fn cycle_type_from_u32() {
    assert_eq!(CycleType::from_u32(0), CycleType::Clamp);
    assert_eq!(CycleType::from_u32(1), CycleType::Loop);
    assert_eq!(CycleType::from_u32(2), CycleType::Reverse);
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

#[test]
fn euler_to_quat_identity() {
    // All angles zero → identity quaternion (w=1, x=0, y=0, z=0)
    let [w, x, y, z] = euler_to_quat_wxyz(0.0, 0.0, 0.0);
    assert!((w - 1.0).abs() < 1e-6);
    assert!(x.abs() < 1e-6);
    assert!(y.abs() < 1e-6);
    assert!(z.abs() < 1e-6);
}

#[test]
fn euler_to_quat_90_deg_x() {
    use std::f32::consts::FRAC_PI_2;
    // 90° around X: quat = (cos(45°), sin(45°), 0, 0) = (~0.707, ~0.707, 0, 0)
    let [w, x, y, z] = euler_to_quat_wxyz(FRAC_PI_2, 0.0, 0.0);
    let _s = FRAC_PI_2.sin() * 0.5_f32.sqrt(); // sin(45°)
    let _c = FRAC_PI_2.cos() * 0.5_f32.sqrt(); // cos(45°) — but let's just check magnitude
    assert!(
        (w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5,
        "quaternion should be unit"
    );
    assert!(x > 0.5, "x component should be dominant for X rotation");
    assert!(y.abs() < 1e-5);
    assert!(z.abs() < 1e-5);
}

#[test]
fn euler_to_quat_90_deg_y() {
    use std::f32::consts::FRAC_PI_2;
    let [w, x, y, z] = euler_to_quat_wxyz(0.0, FRAC_PI_2, 0.0);
    assert!((w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5);
    assert!(x.abs() < 1e-5);
    assert!(y > 0.5, "y component should be dominant for Y rotation");
    assert!(z.abs() < 1e-5);
}

#[test]
fn euler_to_quat_90_deg_z() {
    use std::f32::consts::FRAC_PI_2;
    let [w, x, y, z] = euler_to_quat_wxyz(0.0, 0.0, FRAC_PI_2);
    assert!((w * w + x * x + y * y + z * z - 1.0).abs() < 1e-5);
    assert!(x.abs() < 1e-5);
    assert!(y.abs() < 1e-5);
    assert!(z > 0.5, "z component should be dominant for Z rotation");
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
