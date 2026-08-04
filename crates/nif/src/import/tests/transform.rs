//! Transform composition and Z-up → Y-up coordinate conversion tests
//! (`import/transform.rs`, `import/coord.rs`).

use super::super::*;
use crate::types::{BlockRef, NiMatrix3, NiPoint3};

use super::{
    identity_transform, make_ni_node, make_ni_tri_shape, make_tri_shape_data, scene_from_blocks,
    translated,
};

#[test]
fn import_inherits_parent_translation() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(translated(10.0, 0.0, 0.0), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Mesh",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert!((m.translation[0] - 10.0).abs() < 1e-6);
    assert!((m.translation[1]).abs() < 1e-6);
    assert!((m.translation[2]).abs() < 1e-6);
}

#[test]
fn import_composes_nested_transforms() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(translated(5.0, 0.0, 0.0), vec![BlockRef(1)])),
        Box::new(make_ni_node(translated(0.0, 3.0, 0.0), vec![BlockRef(2)])),
        Box::new(make_ni_tri_shape(
            "Deep",
            identity_transform(),
            3,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert!((m.translation[0] - 5.0).abs() < 1e-6);
    assert!((m.translation[1] - 0.0).abs() < 1e-6);
    assert!((m.translation[2] - -3.0).abs() < 1e-6);
}

#[test]
fn import_composes_scale() {
    let root_transform = NiTransform {
        scale: 2.0,
        ..NiTransform::default()
    };
    let shape_transform = NiTransform {
        translation: NiPoint3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 3.0,
        ..NiTransform::default()
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(root_transform, vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape("Scaled", shape_transform, 2, Vec::new())),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert!((m.scale - 6.0).abs() < 1e-6);
    assert!((m.translation[0] - 2.0).abs() < 1e-6);
}

#[test]
fn compose_transforms_identity() {
    let a = NiTransform::default();
    let b = NiTransform::default();
    let c = transform::compose_transforms(&a, &b);
    assert_eq!(c.scale, 1.0);
    assert!((c.translation.x).abs() < 1e-6);
}

#[test]
fn compose_transforms_translation_only() {
    let a = translated(1.0, 2.0, 3.0);
    let b = translated(4.0, 5.0, 6.0);
    let c = transform::compose_transforms(&a, &b);
    assert!((c.translation.x - 5.0).abs() < 1e-6);
    assert!((c.translation.y - 7.0).abs() < 1e-6);
    assert!((c.translation.z - 9.0).abs() < 1e-6);
}

#[test]
fn zup_to_yup_vertex_positions() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Test",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    let m = &meshes[0];

    assert_eq!(m.positions[0], [0.0, 0.0, 0.0]);
    assert_eq!(m.positions[1], [1.0, 0.0, 0.0]);
    assert_eq!(m.positions[2], [0.0, 0.0, -1.0]);
}

#[test]
fn zup_to_yup_vertex_normals() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Test",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    for n in &meshes[0].normals {
        assert_eq!(*n, [0.0, 1.0, 0.0]);
    }
}

#[test]
fn zup_to_yup_translation() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(translated(0.0, 0.0, 5.0), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape("Up", identity_transform(), 2, Vec::new())),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert!((meshes[0].translation[0]).abs() < 1e-6);
    assert!((meshes[0].translation[1] - 5.0).abs() < 1e-6);
    assert!((meshes[0].translation[2]).abs() < 1e-6);
}

#[test]
fn zup_to_yup_translation_forward() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(translated(0.0, 7.0, 0.0), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Fwd",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert!((meshes[0].translation[0]).abs() < 1e-6);
    assert!((meshes[0].translation[1]).abs() < 1e-6);
    assert!((meshes[0].translation[2] - -7.0).abs() < 1e-6);
}

#[test]
fn zup_to_yup_identity_rotation_stays_identity() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape("Id", identity_transform(), 2, Vec::new())),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    let q = &meshes[0].rotation;
    assert!(q[0].abs() < 1e-4, "qx={}", q[0]);
    assert!(q[1].abs() < 1e-4, "qy={}", q[1]);
    assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
    assert!((q[3].abs() - 1.0).abs() < 1e-4, "qw={}", q[3]);
}

#[test]
fn zup_to_yup_winding_order_preserved() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Wind",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes[0].indices, vec![0, 1, 2]);
}

#[test]
fn compose_degenerate_zero_matrix_uses_identity() {
    // Since #277, degenerate rotations are repaired at parse time
    // (read_ni_transform → sanitize_rotation). This test mirrors that
    // pipeline by sanitizing manually before composition.
    let zero_rot = NiMatrix3 {
        rows: [[0.0; 3]; 3],
    };
    let parent = NiTransform {
        rotation: crate::rotation::sanitize_rotation(zero_rot),
        translation: NiPoint3 {
            x: 10.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 1.0,
    };
    let child = translated(5.0, 0.0, 0.0);
    let result = transform::compose_transforms(&parent, &child);

    assert!((result.translation.x - 15.0).abs() < 1e-4);
    assert!((result.translation.y).abs() < 1e-4);
    assert!((result.translation.z).abs() < 1e-4);
}

#[test]
fn compose_degenerate_scaled_rotation_uses_svd() {
    let scaled_identity = NiMatrix3 {
        rows: [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]],
    };
    let parent = NiTransform {
        rotation: crate::rotation::sanitize_rotation(scaled_identity),
        translation: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 1.0,
    };
    let child = translated(3.0, 4.0, 5.0);
    let result = transform::compose_transforms(&parent, &child);

    assert!((result.translation.x - 3.0).abs() < 1e-4);
    assert!((result.translation.y - 4.0).abs() < 1e-4);
    assert!((result.translation.z - 5.0).abs() < 1e-4);
}

#[test]
fn compose_degenerate_scaled_rotation_rotates_child() {
    let scaled_rot_z90 = NiMatrix3 {
        rows: [[0.0, -2.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]],
    };
    let parent = NiTransform {
        rotation: crate::rotation::sanitize_rotation(scaled_rot_z90),
        translation: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 1.0,
    };
    let child = translated(1.0, 0.0, 0.0);
    let result = transform::compose_transforms(&parent, &child);

    assert!(
        (result.translation.x).abs() < 1e-4,
        "x={}",
        result.translation.x
    );
    assert!(
        (result.translation.y - 1.0).abs() < 1e-4,
        "y={}",
        result.translation.y
    );
    assert!(
        (result.translation.z).abs() < 1e-4,
        "z={}",
        result.translation.z
    );
}

#[test]
fn zup_to_yup_90deg_ccw_rotation_around_z() {
    let rot_z90 = NiMatrix3 {
        rows: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let q = coord::zup_matrix_to_yup_quat(&rot_z90);
    let sin45 = std::f32::consts::FRAC_PI_4.sin();
    let cos45 = std::f32::consts::FRAC_PI_4.cos();
    assert!(q[0].abs() < 1e-4, "qx={}", q[0]);
    assert!((q[1].abs() - sin45).abs() < 1e-4, "qy={}", q[1]);
    assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
    assert!((q[3].abs() - cos45).abs() < 1e-4, "qw={}", q[3]);
}

/// Regression: #333 / D4-05. Export-tool drift can produce matrices
/// whose determinant is in the (1.0, 1.07] window that the fast-path
/// gate admits; without normalisation the Shepperd extraction
/// produced a quaternion up to ~3.5% off unity, which downstream
/// consumers (`scene.rs`, `cell_loader.rs`) feed directly into
/// `Quat::from_xyzw` without normalising. The post-fix output is
/// always unit-length regardless of the input matrix's scale drift.
#[test]
fn zup_to_yup_drifted_rotation_returns_unit_quaternion() {
    // Identity-around-Z rotation scaled by 1.03 — 6% determinant
    // drift, still inside the fast path. Pre-fix |q| ≈ 1.03; post-fix
    // |q| == 1.0 to f32 precision.
    let drift = 1.03f32;
    let scaled_identity = NiMatrix3 {
        rows: [[drift, 0.0, 0.0], [0.0, drift, 0.0], [0.0, 0.0, drift]],
    };
    let q = coord::zup_matrix_to_yup_quat(&scaled_identity);
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    assert!(
        (len - 1.0).abs() < 1e-5,
        "fast-path quaternion must be unit-length; got {len} (q={q:?})"
    );
}

#[test]
fn zup_to_yup_90deg_ccw_rotation_around_x() {
    let rot_x90 = NiMatrix3 {
        rows: [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
    };
    let q = coord::zup_matrix_to_yup_quat(&rot_x90);
    let sin45 = std::f32::consts::FRAC_PI_4.sin();
    let cos45 = std::f32::consts::FRAC_PI_4.cos();
    assert!((q[0].abs() - sin45).abs() < 1e-4, "qx={}", q[0]);
    assert!(q[1].abs() < 1e-4, "qy={}", q[1]);
    assert!(q[2].abs() < 1e-4, "qz={}", q[2]);
    assert!((q[3].abs() - cos45).abs() < 1e-4, "qw={}", q[3]);
}
