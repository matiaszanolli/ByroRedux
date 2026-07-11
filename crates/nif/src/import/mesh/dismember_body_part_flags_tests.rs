//! Regression for #1659 — `BsDismemberSkinInstance`'s per-partition
//! `BodyPartInfo` (dismemberment flags) must reach `ImportedSkin::
//! body_part_flags` on both the NiTriShape and BsTriShape extraction
//! paths. Pre-fix both extractors read only `inst.base.*` and silently
//! dropped `inst.partitions`, so a future slot-hiding consumer had no
//! data to work with even though the parser already captured it.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::node::NiNode;
use crate::blocks::skin::{
    BodyPartInfo, BoneData, BsDismemberSkinInstance, NiSkinData, NiSkinInstance, NiSkinPartition,
    SkinPartitionEntry,
};
use crate::blocks::tri_shape::{BsTriShape, BsTriShapeKind, NiTriShape};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn bone_node() -> Box<dyn crate::blocks::NiObject> {
    Box::new(NiNode {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    })
}

/// `extract_skin_ni_tri_shape` must forward a `BsDismemberSkinInstance`'s
/// partitions onto `ImportedSkin::body_part_flags`, and a plain
/// `NiSkinInstance` (no dismemberment) must leave the vec empty.
#[test]
fn ni_tri_shape_dismember_partitions_reach_imported_skin() {
    let shape = NiTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef(1),
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    };

    let dismember = BsDismemberSkinInstance {
        base: NiSkinInstance {
            data_ref: BlockRef(2),
            skin_partition_ref: BlockRef::NULL,
            skeleton_root_ref: BlockRef::NULL,
            bone_refs: vec![BlockRef(3)],
        },
        partitions: vec![
            BodyPartInfo {
                part_flag: 1,
                body_part: 130, // SBP_30_TORSO
            },
            BodyPartInfo {
                part_flag: 0,
                body_part: 141, // SBP_41_HEAD
            },
        ],
    };

    let skin_data = NiSkinData {
        skin_transform: NiTransform::default(),
        bones: vec![BoneData {
            skin_transform: NiTransform::default(),
            bounding_sphere: [0.0; 4],
            vertex_weights: Vec::new(),
        }],
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape)); // 0
    scene.blocks.push(Box::new(dismember)); // 1
    scene.blocks.push(Box::new(skin_data)); // 2
    scene.blocks.push(bone_node()); // 3

    let shape_ref = scene.get_as::<NiTriShape>(0).unwrap();
    let skin = extract_skin_ni_tri_shape(&scene, shape_ref, 1)
        .expect("BsDismemberSkinInstance-backed shape must build an ImportedSkin");

    assert_eq!(
        skin.body_part_flags,
        vec![
            BodyPartInfo {
                part_flag: 1,
                body_part: 130,
            },
            BodyPartInfo {
                part_flag: 0,
                body_part: 141,
            },
        ],
        "BsDismemberSkinInstance partitions must forward to body_part_flags (#1659)"
    );
}

/// Plain `NiSkinInstance` (no dismemberment extension) must leave
/// `body_part_flags` empty rather than fabricating entries.
#[test]
fn ni_tri_shape_plain_skin_instance_leaves_body_part_flags_empty() {
    let shape = NiTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef(1),
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    };

    let skin_instance = NiSkinInstance {
        data_ref: BlockRef(2),
        skin_partition_ref: BlockRef::NULL,
        skeleton_root_ref: BlockRef::NULL,
        bone_refs: vec![BlockRef(3)],
    };

    let skin_data = NiSkinData {
        skin_transform: NiTransform::default(),
        bones: vec![BoneData {
            skin_transform: NiTransform::default(),
            bounding_sphere: [0.0; 4],
            vertex_weights: Vec::new(),
        }],
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape)); // 0
    scene.blocks.push(Box::new(skin_instance)); // 1
    scene.blocks.push(Box::new(skin_data)); // 2
    scene.blocks.push(bone_node()); // 3

    let shape_ref = scene.get_as::<NiTriShape>(0).unwrap();
    let skin = extract_skin_ni_tri_shape(&scene, shape_ref, 1).unwrap();
    assert!(
        skin.body_part_flags.is_empty(),
        "plain NiSkinInstance has no dismemberment data"
    );
}

/// `extract_skin_bs_tri_shape` (the Skyrim LE NiSkinData branch) must
/// forward `BsDismemberSkinInstance` partitions identically to the
/// NiTriShape path — both geometry containers share the same skin
/// instance types (#1659 SIBLING check).
#[test]
fn bs_tri_shape_dismember_partitions_reach_imported_skin() {
    let shape = BsTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        center: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        radius: 0.0,
        skin_ref: BlockRef(1),
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        vertex_desc: 0,
        num_triangles: 0,
        num_vertices: 1,
        vertices: vec![NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }],
        uvs: Vec::new(),
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: Vec::new(),
        bone_weights: vec![[1.0, 0.0, 0.0, 0.0]],
        bone_indices: vec![[0, 0, 0, 0]],
        tangents: Vec::new(),
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    };

    let dismember = BsDismemberSkinInstance {
        base: NiSkinInstance {
            data_ref: BlockRef(2),
            skin_partition_ref: BlockRef(3),
            skeleton_root_ref: BlockRef::NULL,
            bone_refs: vec![BlockRef(4)],
        },
        partitions: vec![BodyPartInfo {
            part_flag: 1,
            body_part: 102, // SBP_102_RIGHTARM (FO3/FNV convention)
        }],
    };

    let skin_data = NiSkinData {
        skin_transform: NiTransform::default(),
        bones: vec![BoneData {
            skin_transform: NiTransform::default(),
            bounding_sphere: [0.0; 4],
            vertex_weights: Vec::new(),
        }],
    };

    let skin_partition = NiSkinPartition {
        partitions: vec![SkinPartitionEntry {
            num_vertices: 1,
            num_triangles: 0,
            bones: vec![0],
            num_weights_per_vertex: 4,
            vertex_map: vec![0],
            vertex_weights: Vec::new(),
            triangles: Vec::new(),
            bone_indices: Vec::new(),
        }],
        global_vertex_data: None,
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape)); // 0
    scene.blocks.push(Box::new(dismember)); // 1
    scene.blocks.push(Box::new(skin_data)); // 2
    scene.blocks.push(Box::new(skin_partition)); // 3
    scene.blocks.push(bone_node()); // 4

    let shape_ref = scene.get_as::<BsTriShape>(0).unwrap();
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref)
        .expect("BsDismemberSkinInstance-backed BsTriShape must build an ImportedSkin");

    assert_eq!(
        skin.body_part_flags,
        vec![BodyPartInfo {
            part_flag: 1,
            body_part: 102,
        }],
        "BsDismemberSkinInstance partitions must forward on the BsTriShape path too (#1659)"
    );
}
