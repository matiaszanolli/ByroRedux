//! Tests for `bs_tri_shape_partition_remap_tests` extracted from ../mesh.rs (refactor stage A).
//!
//! Same qualified path preserved (`bs_tri_shape_partition_remap_tests::FOO`).

//! Packed indices are skin-global, unlike the separate partition-local
//! channel. Earlier #613/#2577 fixtures asserted the opposite and masked
//! the extra-remap bug. Installed-data cross-checks live in
//! `sse_skin_index_space_tests` and compare the two independent channels.
use super::*;
use crate::blocks::node::NiNode;
use crate::blocks::skin::{NiSkinData, NiSkinInstance, NiSkinPartition};
use crate::blocks::tri_shape::BsTriShape;
use crate::blocks::tri_shape::BsTriShapeKind;
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};

use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::skin::SkinPartitionEntry;

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

/// Non-identity partition palettes must not transform packed global IDs.
#[test]
fn multi_partition_shape_preserves_packed_global_bone_indices() {
    // Bone refs used by the skin (4 NiNode blocks at indices 5..9).
    let bone_node = || -> Box<dyn crate::blocks::NiObject> {
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
    };

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
        num_vertices: 2,
        vertices: vec![
            NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            NiPoint3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
        ],
        uvs: Vec::new(),
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: Vec::new(),
        // Global IDs for two different subsets. Applying either palette
        // again would alias or zero these valid indices.
        bone_weights: vec![[0.4, 0.3, 0.2, 0.1], [0.4, 0.3, 0.2, 0.1]],
        bone_indices: vec![[2, 3, 2, 3], [1, 3, 1, 3]],
        tangents: Vec::new(),
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    };

    let skin_instance = NiSkinInstance {
        data_ref: BlockRef(2),
        skin_partition_ref: BlockRef(3),
        skeleton_root_ref: BlockRef::NULL,
        // 4 global bones — indices 0..=3.
        bone_refs: vec![BlockRef(5), BlockRef(6), BlockRef(7), BlockRef(8)],
    };

    // NiSkinData with bind transforms for each bone — needed so
    // `extract_skin_bs_tri_shape` succeeds beyond the bone-refs
    // check. Per-bone vertex_weights stay empty: the BsTriShape
    // vertex buffer carries the inline weights instead.
    let skin_data = NiSkinData {
        skin_transform: NiTransform::default(),
        bones: vec![
            crate::blocks::skin::BoneData {
                skin_transform: NiTransform::default(),
                bounding_sphere: [0.0; 4],
                vertex_weights: Vec::new(),
            },
            crate::blocks::skin::BoneData {
                skin_transform: NiTransform::default(),
                bounding_sphere: [0.0; 4],
                vertex_weights: Vec::new(),
            },
            crate::blocks::skin::BoneData {
                skin_transform: NiTransform::default(),
                bounding_sphere: [0.0; 4],
                vertex_weights: Vec::new(),
            },
            crate::blocks::skin::BoneData {
                skin_transform: NiTransform::default(),
                bounding_sphere: [0.0; 4],
                vertex_weights: Vec::new(),
            },
        ],
    };

    let skin_partition = NiSkinPartition {
        partitions: vec![
            // Partition 0: covers vertex 0; bones palette = [2, 3]
            // (so partition-local index 0 → global bone 2).
            SkinPartitionEntry {
                num_vertices: 1,
                num_triangles: 0,
                bones: vec![2, 3],
                num_weights_per_vertex: 4,
                vertex_map: vec![0],
                vertex_weights: Vec::new(),
                triangles: Vec::new(),
                bone_indices: Vec::new(),
            },
            // Partition 1: covers vertex 1; bones palette = [1, 3]
            // (so partition-local index 0 → global bone 1).
            SkinPartitionEntry {
                num_vertices: 1,
                num_triangles: 0,
                bones: vec![1, 3],
                num_weights_per_vertex: 4,
                vertex_map: vec![1],
                vertex_weights: Vec::new(),
                triangles: Vec::new(),
                bone_indices: Vec::new(),
            },
        ],
        global_vertex_data: None,
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape));
    scene.blocks.push(Box::new(skin_instance));
    scene.blocks.push(Box::new(skin_data));
    scene.blocks.push(Box::new(skin_partition));
    scene.blocks.push(bone_node()); // 4
    scene.blocks.push(bone_node()); // 5
    scene.blocks.push(bone_node()); // 6
    scene.blocks.push(bone_node()); // 7
    scene.blocks.push(bone_node()); // 8

    let shape_ref = scene.get_as::<BsTriShape>(0).unwrap();
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref, &[])
        .expect("multi-partition skin must build an ImportedSkin");

    assert_eq!(
        skin.vertex_bone_indices.len(),
        2,
        "both vertices must retain their packed influences"
    );
    // Packed IDs already include partition 0's global bone identities.
    assert_eq!(
        skin.vertex_bone_indices[0],
        [2, 3, 2, 3],
        "vertex 0 must not remap global IDs through [2,3] again"
    );
    // Vertex 1 likewise keeps its own global subset.
    assert_eq!(
        skin.vertex_bone_indices[1],
        [1, 3, 1, 3],
        "vertex 1 must not remap global IDs through [1,3] again"
    );
}

/// A single non-identity palette also must not remap packed global IDs.
#[test]
fn single_partition_shape_preserves_global_index_with_non_identity_palette() {
    let bone_node = || -> Box<dyn crate::blocks::NiObject> {
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
    };

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
        bone_indices: vec![[3, 0, 0, 0]],
        tangents: Vec::new(),
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    };

    let skin_instance = NiSkinInstance {
        data_ref: BlockRef(2),
        skin_partition_ref: BlockRef(3),
        skeleton_root_ref: BlockRef::NULL,
        bone_refs: vec![
            BlockRef(4),
            BlockRef(5),
            BlockRef(6),
            BlockRef(7),
            BlockRef(8),
            BlockRef(9),
            BlockRef(10),
        ],
    };

    let skin_data = NiSkinData {
        skin_transform: NiTransform::default(),
        bones: (0..7)
            .map(|_| crate::blocks::skin::BoneData {
                skin_transform: NiTransform::default(),
                bounding_sphere: [0.0; 4],
                vertex_weights: Vec::new(),
            })
            .collect(),
    };

    let skin_partition = NiSkinPartition {
        partitions: vec![SkinPartitionEntry {
            num_vertices: 1,
            num_triangles: 0,
            // Vanilla FaceGen-style subset: local slot 2 maps to
            // global bone 3 rather than widening to global bone 2.
            bones: vec![0, 1, 3, 4, 5, 6],
            num_weights_per_vertex: 4,
            vertex_map: vec![0],
            vertex_weights: Vec::new(),
            triangles: Vec::new(),
            bone_indices: Vec::new(),
        }],
        global_vertex_data: None,
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape));
    scene.blocks.push(Box::new(skin_instance));
    scene.blocks.push(Box::new(skin_data));
    scene.blocks.push(Box::new(skin_partition));
    scene.blocks.push(bone_node()); // 4
    scene.blocks.push(bone_node()); // 5
    scene.blocks.push(bone_node()); // 6
    scene.blocks.push(bone_node()); // 7
    scene.blocks.push(bone_node()); // 8
    scene.blocks.push(bone_node()); // 9
    scene.blocks.push(bone_node()); // 10

    let shape_ref = scene.get_as::<BsTriShape>(0).unwrap();
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref, &[]).unwrap();
    // Packed global ID 3 stays 3, not palette[3] == 4.
    assert_eq!(skin.vertex_bone_indices[0], [3u16, 0, 0, 0]);
}

/// Packed influences do not require a partition table at all.
#[test]
fn missing_skin_partition_preserves_packed_indices() {
    let bone_node = || -> Box<dyn crate::blocks::NiObject> {
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
    };

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
        bone_indices: vec![[3, 0, 0, 0]],
        tangents: Vec::new(),
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    };

    let skin_instance = NiSkinInstance {
        data_ref: BlockRef(2),
        // No partition — `skin_partition_ref` is null.
        skin_partition_ref: BlockRef::NULL,
        skeleton_root_ref: BlockRef::NULL,
        bone_refs: vec![BlockRef(4), BlockRef(5), BlockRef(6), BlockRef(7)],
    };

    let skin_data = NiSkinData {
        skin_transform: NiTransform::default(),
        bones: (0..4)
            .map(|_| crate::blocks::skin::BoneData {
                skin_transform: NiTransform::default(),
                bounding_sphere: [0.0; 4],
                vertex_weights: Vec::new(),
            })
            .collect(),
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape));
    scene.blocks.push(Box::new(skin_instance));
    scene.blocks.push(Box::new(skin_data));
    // Pad to keep block-ref math sane.
    scene.blocks.push(bone_node()); // 3
    scene.blocks.push(bone_node()); // 4
    scene.blocks.push(bone_node()); // 5
    scene.blocks.push(bone_node()); // 6
    scene.blocks.push(bone_node()); // 7

    let shape_ref = scene.get_as::<BsTriShape>(0).unwrap();
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref, &[]).unwrap();
    assert_eq!(skin.vertex_bone_indices[0], [3u16, 0, 0, 0]);
}
