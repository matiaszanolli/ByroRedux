//! Regression tests for #1203 — `BSGeometry::skin_instance_ref` must
//! resolve to an `ImportedSkin` via the FO4+ `BsSkinInstance` +
//! `BsSkinBoneData` chain. Pre-fix every Starfield NPC mesh imported with
//! `skin: None` and the renderer fell through to the rigid-placement
//! path, leaving every character in bind pose.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::bs_geometry::{BSGeometry, BSGeometryMesh, BSGeometryMeshKind};
use crate::blocks::node::NiNode;
use crate::blocks::skin::{BsSkinBoneData, BsSkinBoneTrans, BsSkinInstance};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiMatrix3, NiPoint3, NiTransform};
use std::sync::Arc;

fn identity_transform() -> NiTransform {
    NiTransform {
        rotation: NiMatrix3 {
            rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        },
        translation: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 1.0,
    }
}

fn named_net(name: &str) -> NiObjectNETData {
    NiObjectNETData {
        name: Some(Arc::from(name)),
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn av_with_name(name: &str) -> NiAVObjectData {
    NiAVObjectData {
        net: named_net(name),
        flags: 0,
        transform: identity_transform(),
        properties: Vec::new(),
        collision_ref: BlockRef::NULL,
    }
}

fn bone_node(name: &str) -> NiNode {
    NiNode {
        av: av_with_name(name),
        children: Vec::new(),
        effects: Vec::new(),
    }
}

fn bone_trans(idx: usize) -> BsSkinBoneTrans {
    // Distinct per-bone values so the round-trip test can verify
    // bone[i] in the import maps to bone[i] in the input.
    BsSkinBoneTrans {
        bounding_sphere: [idx as f32, 0.0, 0.0, 1.0],
        rotation: [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        translation: [idx as f32 * 10.0, 0.0, 0.0],
        scale: 1.0,
    }
}

fn bs_geometry_with_skin(skin_idx: u32) -> BSGeometry {
    BSGeometry {
        av: av_with_name("MeshShape"),
        bounding_sphere: ([0.0, 0.0, 0.0], 0.0),
        bound_min_max: [0.0; 6],
        skin_instance_ref: BlockRef(skin_idx),
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        meshes: vec![BSGeometryMesh {
            tri_size: 0,
            num_verts: 0,
            flags: 0,
            kind: BSGeometryMeshKind::External {
                mesh_name: "ignored.mesh".to_string(),
            },
        }],
    }
}

/// Bug case (pre-#1203): a Starfield BSGeometry that wires up a
/// `BsSkinInstance` + `BsSkinBoneData` pair must resolve to an
/// `ImportedSkin` with the expected bone count and resolved names.
#[test]
fn bs_geometry_skin_instance_resolves_to_imported_skin() {
    let scene = NifScene {
        blocks: vec![
            Box::new(bone_node("Bip01")),    // block 0 — skeleton root
            Box::new(bone_node("Spine")),    // block 1
            Box::new(bone_node("Head")),     // block 2
            Box::new(BsSkinInstance {        // block 3
                skeleton_root_ref: BlockRef(0),
                bone_data_ref: BlockRef(4),
                bone_refs: vec![BlockRef(1), BlockRef(2)],
                scales: Vec::new(),
            }),
            Box::new(BsSkinBoneData {        // block 4
                bones: vec![bone_trans(1), bone_trans(2)],
            }),
        ],
        ..NifScene::default()
    };
    let shape = bs_geometry_with_skin(3);
    let skin = extract_skin_bs_geometry(&scene, &shape)
        .expect("BSGeometry with valid skin_instance_ref must resolve");
    assert_eq!(skin.bones.len(), 2, "bone count must match BsSkinInstance");
    assert_eq!(skin.bones[0].name.as_ref(), "Spine");
    assert_eq!(skin.bones[1].name.as_ref(), "Head");
    assert_eq!(
        skin.skeleton_root.as_deref().map(|s| s as &str),
        Some("Bip01"),
        "skeleton root must resolve to its node's name",
    );
    // Per-vertex bone indices + weights deferred until the BSGeometry
    // parser surfaces them (#1203 deferred scope).
    assert!(skin.vertex_bone_indices.is_empty());
    assert!(skin.vertex_bone_weights.is_empty());
}

/// Mismatched bone counts return None (defensive — same behaviour
/// as `extract_skin_ni_tri_shape` / `extract_skin_bs_tri_shape`).
#[test]
fn mismatched_bone_counts_return_none() {
    let scene = NifScene {
        blocks: vec![
            Box::new(bone_node("Root")),
            Box::new(bone_node("Spine")),
            Box::new(BsSkinInstance {
                skeleton_root_ref: BlockRef(0),
                bone_data_ref: BlockRef(3),
                bone_refs: vec![BlockRef(1)], // 1 bone ref
                scales: Vec::new(),
            }),
            Box::new(BsSkinBoneData {
                bones: vec![bone_trans(0), bone_trans(1)], // 2 bone transforms
            }),
        ],
        ..NifScene::default()
    };
    let shape = bs_geometry_with_skin(2);
    assert!(extract_skin_bs_geometry(&scene, &shape).is_none());
}

/// NULL skin_instance_ref returns None (rigid geometry — the common
/// case for static / clutter / world meshes).
#[test]
fn null_skin_instance_ref_returns_none() {
    let scene = NifScene::default();
    let mut shape = bs_geometry_with_skin(0);
    shape.skin_instance_ref = BlockRef::NULL;
    assert!(extract_skin_bs_geometry(&scene, &shape).is_none());
}

/// Dangling skin_instance_ref (points at a non-existent block) returns
/// None rather than panicking.
#[test]
fn dangling_skin_instance_ref_returns_none() {
    let scene = NifScene::default();
    let shape = bs_geometry_with_skin(99); // points at block 99, scene has 0
    assert!(extract_skin_bs_geometry(&scene, &shape).is_none());
}

/// Wrong block type at skin_instance_ref returns None (e.g., points at
/// an NiNode instead of a BsSkinInstance).
#[test]
fn wrong_block_type_at_skin_instance_ref_returns_none() {
    let scene = NifScene {
        blocks: vec![Box::new(bone_node("NotASkinInstance"))],
        ..NifScene::default()
    };
    let shape = bs_geometry_with_skin(0); // points at the NiNode
    assert!(extract_skin_bs_geometry(&scene, &shape).is_none());
}

/// Bone refs that don't resolve to a named NiObjectNET-bearing block
/// still surface — they fall back to `BoneN` synthetic names so the
/// import isn't fully blocked by a single missing bone node.
#[test]
fn unresolvable_bone_ref_falls_back_to_synthetic_name() {
    let scene = NifScene {
        blocks: vec![
            Box::new(bone_node("Bip01")),
            Box::new(bone_node("Spine")),
            Box::new(BsSkinInstance {
                skeleton_root_ref: BlockRef(0),
                bone_data_ref: BlockRef(3),
                // Bone 0 resolves, bone 1 dangles
                bone_refs: vec![BlockRef(1), BlockRef(42)],
                scales: Vec::new(),
            }),
            Box::new(BsSkinBoneData {
                bones: vec![bone_trans(1), bone_trans(2)],
            }),
        ],
        ..NifScene::default()
    };
    let shape = bs_geometry_with_skin(2);
    let skin = extract_skin_bs_geometry(&scene, &shape).expect("must still resolve");
    assert_eq!(skin.bones.len(), 2);
    assert_eq!(skin.bones[0].name.as_ref(), "Spine");
    assert_eq!(
        skin.bones[1].name.as_ref(),
        "Bone1",
        "dangling bone ref must fall back to synthetic Bone{{index}}",
    );
}
