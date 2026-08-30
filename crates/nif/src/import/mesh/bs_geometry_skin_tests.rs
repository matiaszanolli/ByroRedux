//! Regression tests for #1203 — `BSGeometry::skin_instance_ref` must
//! resolve to an `ImportedSkin` via the FO4+ `BsSkinInstance` +
//! `BsSkinBoneData` chain. Pre-fix every Starfield NPC mesh imported with
//! `skin: None` and the renderer fell through to the rigid-placement
//! path, leaving every character in bind pose.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::bs_geometry::{
    BSGeometry, BSGeometryMesh, BSGeometryMeshData, BSGeometryMeshKind, BoneWeight,
};
use crate::blocks::node::NiNode;
use crate::blocks::skin::{BsSkinBoneData, BsSkinBoneTrans, BsSkinInstance};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiMatrix3, NiPoint3, NiTransform};
use std::sync::Arc;

/// Empty `BSGeometryMeshData` — every field default. Used by tests that
/// don't exercise the #2613 per-vertex weight plumbing (skin
/// resolve/bone-count/name tests only care about the `BsSkinInstance` +
/// `BsSkinBoneData` chain).
fn empty_mesh_data() -> BSGeometryMeshData {
    BSGeometryMeshData::default()
}

fn bone_weight(bone_index: u16, weight: u16) -> BoneWeight {
    BoneWeight { bone_index, weight }
}

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
        rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
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
            lod_slot: 0,
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
            Box::new(bone_node("Bip01")), // block 0 — skeleton root
            Box::new(bone_node("Spine")), // block 1
            Box::new(bone_node("Head")),  // block 2
            Box::new(BsSkinInstance {
                // block 3
                skeleton_root_ref: BlockRef(0),
                bone_data_ref: BlockRef(4),
                bone_refs: vec![BlockRef(1), BlockRef(2)],
                scales: Vec::new(),
            }),
            Box::new(BsSkinBoneData {
                // block 4
                bones: vec![bone_trans(1), bone_trans(2)],
            }),
        ],
        ..NifScene::default()
    };
    let shape = bs_geometry_with_skin(3);
    let skin = extract_skin_bs_geometry(&scene, &shape, &empty_mesh_data(), None)
        .expect("BSGeometry with valid skin_instance_ref must resolve");
    assert_eq!(skin.bones.len(), 2, "bone count must match BsSkinInstance");
    assert_eq!(skin.bones[0].name.as_ref(), "Spine");
    assert_eq!(skin.bones[1].name.as_ref(), "Head");
    assert_eq!(
        skin.skeleton_root.as_deref().map(|s| s as &str),
        Some("Bip01"),
        "skeleton root must resolve to its node's name",
    );
    // No `skin_weights` on this fixture's (empty) mesh data — vertex
    // arrays stay empty (the engine's rigid-fallback sentinel), same as
    // a genuinely-unskinned shape. See `bs_geometry_skin_weights_*`
    // below for the #2613 populated-weights path.
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
    assert!(extract_skin_bs_geometry(&scene, &shape, &empty_mesh_data(), None).is_none());
}

/// NULL skin_instance_ref returns None (rigid geometry — the common
/// case for static / clutter / world meshes).
#[test]
fn null_skin_instance_ref_returns_none() {
    let scene = NifScene::default();
    let mut shape = bs_geometry_with_skin(0);
    shape.skin_instance_ref = BlockRef::NULL;
    assert!(extract_skin_bs_geometry(&scene, &shape, &empty_mesh_data(), None).is_none());
}

/// Dangling skin_instance_ref (points at a non-existent block) returns
/// None rather than panicking.
#[test]
fn dangling_skin_instance_ref_returns_none() {
    let scene = NifScene::default();
    let shape = bs_geometry_with_skin(99); // points at block 99, scene has 0
    assert!(extract_skin_bs_geometry(&scene, &shape, &empty_mesh_data(), None).is_none());
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
    assert!(extract_skin_bs_geometry(&scene, &shape, &empty_mesh_data(), None).is_none());
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
    let skin = extract_skin_bs_geometry(&scene, &shape, &empty_mesh_data(), None)
        .expect("must still resolve");
    assert_eq!(skin.bones.len(), 2);
    assert_eq!(skin.bones[0].name.as_ref(), "Spine");
    assert_eq!(
        skin.bones[1].name.as_ref(),
        "Bone1",
        "dangling bone ref must fall back to synthetic Bone{{index}}",
    );
}

// ── #2613 — per-vertex skin_weights plumbing ───────────────────────────
//
// Pre-fix `extract_skin_bs_geometry` hardcoded `vertex_bone_indices` /
// `vertex_bone_weights` to `Vec::new()` on the stale premise that the
// BSGeometry parser didn't decode per-vertex skin data — it does
// (`skin_weights`, since #873), it just was never passed to the
// extractor. These tests exercise the fix: real-data-shaped (naked_f.nif
// per the issue) two-bone skin + populated `skin_weights`.

fn two_bone_skin_scene() -> NifScene {
    NifScene {
        blocks: vec![
            Box::new(bone_node("Bip01")), // block 0 — skeleton root
            Box::new(bone_node("Spine")), // block 1
            Box::new(bone_node("Head")),  // block 2
            Box::new(BsSkinInstance {
                // block 3
                skeleton_root_ref: BlockRef(0),
                bone_data_ref: BlockRef(4),
                bone_refs: vec![BlockRef(1), BlockRef(2)],
                scales: Vec::new(),
            }),
            Box::new(BsSkinBoneData {
                // block 4
                bones: vec![bone_trans(1), bone_trans(2)],
            }),
        ],
        ..NifScene::default()
    }
}

fn mesh_data_with_weights(
    num_vertices: usize,
    weights_per_vert: u32,
    skin_weights: Vec<Vec<BoneWeight>>,
) -> BSGeometryMeshData {
    BSGeometryMeshData {
        weights_per_vert,
        vertices: vec![[0.0, 0.0, 0.0]; num_vertices],
        skin_weights,
        ..BSGeometryMeshData::default()
    }
}

/// Basic case: 2 vertices, 2 weights each. Vertex 0's weights already
/// sum to exactly 1.0 NORM (no renormalization); vertex 1's weights are
/// deliberately far from unit sum to exercise
/// `renormalize_skin_weights`. Both cases also confirm descending
/// (highest-weight-first) ordering.
#[test]
fn bs_geometry_skin_weights_plumbed_through_when_present() {
    let scene = two_bone_skin_scene();
    let shape = bs_geometry_with_skin(3);
    let mesh_data = mesh_data_with_weights(
        2,
        2,
        vec![
            vec![bone_weight(0, 65535), bone_weight(1, 0)],
            vec![bone_weight(1, 6), bone_weight(0, 4)],
        ],
    );
    let skin = extract_skin_bs_geometry(&scene, &shape, &mesh_data, None)
        .expect("BSGeometry with valid skin_instance_ref must resolve");

    assert_eq!(skin.vertex_bone_indices.len(), 2);
    assert_eq!(skin.vertex_bone_weights.len(), 2);

    // Vertex 0: full weight on bone 0, already unit-sum.
    assert_eq!(skin.vertex_bone_indices[0], [0, 1, 0, 0]);
    assert!((skin.vertex_bone_weights[0][0] - 1.0).abs() < 1e-5);
    assert!(skin.vertex_bone_weights[0][1].abs() < 1e-5);

    // Vertex 1: raw NORM weights (6, 4) sum far below 1.0 — must be
    // renormalized to the same 6:4 (0.6:0.4) ratio, sorted so bone 1
    // (the larger raw weight) leads.
    assert_eq!(skin.vertex_bone_indices[1], [1, 0, 0, 0]);
    assert!(
        (skin.vertex_bone_weights[1][0] - 0.6).abs() < 1e-4,
        "got {}",
        skin.vertex_bone_weights[1][0]
    );
    assert!(
        (skin.vertex_bone_weights[1][1] - 0.4).abs() < 1e-4,
        "got {}",
        skin.vertex_bone_weights[1][1]
    );
    let sum: f32 = skin.vertex_bone_weights[1].iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-4,
        "renormalized sum must be ~1.0, got {sum}"
    );
}

/// A vertex authoring more than 4 influences keeps only the top 4 by
/// weight — the lowest two (bones 0 and 1) must be dropped entirely.
#[test]
fn bs_geometry_skin_weights_keeps_top_four_by_weight() {
    let scene = two_bone_skin_scene();
    let shape = bs_geometry_with_skin(3);
    let mesh_data = mesh_data_with_weights(
        1,
        6,
        vec![vec![
            bone_weight(0, 100),
            bone_weight(1, 200),
            bone_weight(2, 300),
            bone_weight(3, 400),
            bone_weight(4, 500),
            bone_weight(5, 600),
        ]],
    );
    let skin = extract_skin_bs_geometry(&scene, &shape, &mesh_data, None)
        .expect("BSGeometry with valid skin_instance_ref must resolve");

    assert_eq!(
        skin.vertex_bone_indices[0],
        [5, 4, 3, 2],
        "must keep the 4 highest-weight bones (5,4,3,2), descending"
    );
    let w = skin.vertex_bone_weights[0];
    assert!(
        w[0] > w[1] && w[1] > w[2] && w[2] > w[3],
        "weights must stay descending: {w:?}"
    );
    let sum: f32 = w.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-4,
        "renormalized sum must be ~1.0, got {sum}"
    );
}

/// A vertex count mismatch between `skin_weights` and the mesh's own
/// `vertices` falls back to the engine's empty-vec rigid-fallback
/// sentinel — the skin still resolves (bones intact) rather than
/// failing outright, matching a genuinely-unskinned shape's contract.
#[test]
fn bs_geometry_skin_weights_vertex_count_mismatch_falls_back_to_empty() {
    let scene = two_bone_skin_scene();
    let shape = bs_geometry_with_skin(3);
    // 1 skin_weights row but 2 vertices — mismatch.
    let mesh_data = mesh_data_with_weights(2, 1, vec![vec![bone_weight(0, 65535)]]);
    let skin = extract_skin_bs_geometry(&scene, &shape, &mesh_data, None)
        .expect("bone resolution must still succeed despite the weight mismatch");

    assert!(skin.vertex_bone_indices.is_empty());
    assert!(skin.vertex_bone_weights.is_empty());
    // Bones themselves are unaffected by the mismatch.
    assert_eq!(skin.bones.len(), 2);
}
