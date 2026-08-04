//! Basic import-pipeline sanity tests: empty/single/multiple shapes,
//! missing data ref, and `extract_root_flags` — the general-purpose
//! tests that don't target one specific concern covered by a sibling.

use super::super::*;
use crate::types::BlockRef;

use super::{
    identity_transform, make_ni_node, make_ni_tri_shape, make_tri_shape_data, scene_from_blocks,
    translated,
};

#[test]
fn import_empty_scene() {
    let scene = NifScene::default();
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    assert!(meshes.is_empty());
}

#[test]
fn import_single_shape_under_root() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Triangle",
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
    assert_eq!(m.name, Some(Arc::from("Triangle")));
    assert_eq!(m.positions.len(), 3);
    assert_eq!(m.indices, vec![0, 1, 2]);
    assert_eq!(m.uvs.len(), 3);
    assert_eq!(m.translation, [0.0, 0.0, 0.0]);
    assert_eq!(m.scale, 1.0);
}

#[test]
fn import_multiple_shapes() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(
            identity_transform(),
            vec![BlockRef(1), BlockRef(3)],
        )),
        Box::new(make_ni_tri_shape(
            "A",
            translated(1.0, 0.0, 0.0),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
        Box::new(make_ni_tri_shape(
            "B",
            translated(-1.0, 0.0, 0.0),
            4,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 2);
    assert_eq!(meshes[0].name, Some(Arc::from("A")));
    assert_eq!(meshes[1].name, Some(Arc::from("B")));
}

#[test]
fn import_shape_with_no_data_ref_is_skipped() {
    let mut shape = make_ni_tri_shape("NoData", identity_transform(), 0, Vec::new());
    shape.data_ref = BlockRef::NULL;

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(shape),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    assert!(meshes.is_empty());
}

/// #1235 / LC-D1-NEW-01 — `extract_root_flags` returns the root NiNode's
/// raw `NiAVObject.flags` so the cell-loader spawn site can attach a
/// `SceneFlags` ECS row on the placement root (parity with the loose-NIF
/// loader at `byroredux/src/scene/nif_loader.rs:450-452`).
#[test]
fn extract_root_flags_returns_root_av_flags() {
    let mut root = make_ni_node(identity_transform(), Vec::new());
    // DISABLE_SORTING (0x0040) | IS_NODE (0x0100) — arbitrary realistic mix.
    root.av.flags = 0x0140;
    let scene = scene_from_blocks(vec![Box::new(root)]);
    assert_eq!(extract_root_flags(&scene), 0x0140);
}

/// Empty scene → 0 (the SpeedTree `.spt` placeholder + generated-content
/// paths in `cell_loader/references.rs` rely on this fall-through).
#[test]
fn extract_root_flags_returns_zero_when_no_root() {
    let scene = scene_from_blocks(Vec::new());
    assert_eq!(extract_root_flags(&scene), 0);
}

/// Root present but isn't a NiNode (synthetic — vanilla content always
/// roots in a NiNode subclass, but we want graceful degradation) → 0.
#[test]
fn extract_root_flags_returns_zero_when_root_is_not_a_ninode() {
    let data = make_tri_shape_data();
    let scene = scene_from_blocks(vec![Box::new(data)]);
    assert_eq!(extract_root_flags(&scene), 0);
}
