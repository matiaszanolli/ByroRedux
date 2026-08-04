//! `NiBillboardNode` mode-propagation tests (#2206 / NIFAL-D4-02).

use super::super::*;
use crate::types::BlockRef;

use super::{
    identity_transform, make_ni_node, make_ni_tri_shape, make_tri_shape_data, scene_from_blocks,
};

/// #2206 / NIFAL-D4-02 — `import_nif` (the flat, cell-loader-style walk)
/// must propagate a `NiBillboardNode` ancestor's mode onto every mesh in
/// its subtree. Pre-fix `walk_node_flat` unwrapped `NiBillboardNode` to a
/// plain `NiNode` via `as_ni_node` and never recorded that the node was a
/// billboard at all, so `ImportedMesh` had no field to receive it and
/// `CachedNifImport::placement_root_billboard` stayed hardcoded `None` on
/// this path — measured live at 213–1,527 `NiBillboardNode` instances per
/// vanilla archive across every cell-loaded game, none of which ever
/// rotated to face the camera.
#[test]
fn import_propagates_billboard_mode_to_descendant_meshes() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(crate::blocks::node::NiBillboardNode {
            base: make_ni_node(identity_transform(), vec![BlockRef(1)]),
            billboard_mode: 1, // ROTATE_ABOUT_UP
        }),
        Box::new(make_ni_tri_shape(
            "Leaf",
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
    assert_eq!(
        meshes[0].billboard_mode,
        Some(1),
        "mesh under a NiBillboardNode ancestor must carry its mode"
    );
}

/// Sibling of the propagation test: a mesh with no `NiBillboardNode`
/// ancestor anywhere on its path must stay `None` — this is the
/// overwhelming majority case and must not regress to "everything is a
/// billboard".
#[test]
fn import_leaves_billboard_mode_none_without_ancestor() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(make_ni_tri_shape(
            "Leaf",
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
    assert_eq!(meshes[0].billboard_mode, None);
}

/// A `NiBillboardNode` subtree must not leak its mode to an unrelated
/// sibling subtree — the same isolation `inherited_props` already gets
/// via push/truncate around the recursive descent.
#[test]
fn import_billboard_mode_does_not_leak_to_sibling_subtree() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(
            identity_transform(),
            vec![BlockRef(1), BlockRef(3)],
        )),
        Box::new(crate::blocks::node::NiBillboardNode {
            base: make_ni_node(identity_transform(), vec![BlockRef(2)]),
            billboard_mode: 0, // ALWAYS_FACE_CAMERA — still a billboard, see extract_billboard_mode
        }),
        Box::new(make_ni_tri_shape(
            "BillboardLeaf",
            identity_transform(),
            4,
            Vec::new(),
        )),
        Box::new(make_ni_tri_shape(
            "PlainLeaf",
            identity_transform(),
            4,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(meshes.len(), 2);
    let billboard_leaf = meshes
        .iter()
        .find(|m| m.name.as_deref() == Some("BillboardLeaf"))
        .expect("BillboardLeaf must import");
    let plain_leaf = meshes
        .iter()
        .find(|m| m.name.as_deref() == Some("PlainLeaf"))
        .expect("PlainLeaf must import");
    assert_eq!(
        billboard_leaf.billboard_mode,
        Some(0),
        "mode 0 is still ALWAYS_FACE_CAMERA, not absence of a billboard ancestor"
    );
    assert_eq!(
        plain_leaf.billboard_mode, None,
        "the billboard subtree must not leak its mode to an unrelated sibling"
    );
}
