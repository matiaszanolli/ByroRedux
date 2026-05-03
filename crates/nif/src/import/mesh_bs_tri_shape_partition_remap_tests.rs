//! Tests for `bs_tri_shape_partition_remap_tests` extracted from ../mesh.rs (refactor stage A).
//!
//! Same qualified path preserved (`bs_tri_shape_partition_remap_tests::FOO`).

//! Regression coverage for #613 / SK-D1-01 — `BsTriShape` inline
//! `bone_indices` (`[u8; 4]` per vertex) are partition-LOCAL
//! indices into each `NiSkinPartition.partitions[i].bones` palette,
//! not global indices into the skin's bone list. The importer
//! must walk the partition table and remap before exposing the
//! values to downstream consumers, otherwise multi-partition
//! shapes (Skyrim Argonian/Khajiit body + worn armour, modded
//! 256+ bone skins) silently alias every vertex past partition 0
//! to the wrong bones.
use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::node::NiNode;
use crate::blocks::skin::{NiSkinData, NiSkinInstance, NiSkinPartition, SkinPartitionEntry};
use crate::blocks::tri_shape::BsTriShapeKind;
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3, NiTransform};

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

/// Build a 2-vertex skinned BsTriShape whose inline bone_indices
/// are partition-local. The skin instance points at a SkinPartition
/// with two partitions whose `bones` palettes pick distinct global
/// bones — so a `[0, 0, 0, 0]` partition-local index resolves to
/// **different** global indices depending on which partition the
/// vertex belongs to. Pre-#613 the importer cloned the partition-
/// local indices verbatim and both vertices ended up "bound to
/// bone 0 globally" — wrong.
#[test]
fn multi_partition_remap_picks_correct_global_per_vertex() {
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
        // Vertex 0 → partition 0; per-vertex partition-local
        // bone slots are [0, 1, 0, 1] — exercises BOTH palette
        // entries so the remap is observable.
        // Vertex 1 → partition 1, same shape.
        bone_weights: vec![[0.4, 0.3, 0.2, 0.1], [0.4, 0.3, 0.2, 0.1]],
        bone_indices: vec![[0, 1, 0, 1], [0, 1, 0, 1]],
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
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref)
        .expect("multi-partition skin must build an ImportedSkin");

    assert_eq!(
        skin.vertex_bone_indices.len(),
        2,
        "both vertices must remap"
    );
    // Vertex 0: partition 0's palette is [2, 3]. Local slots
    // [0, 1, 0, 1] remap to globals [2, 3, 2, 3].
    assert_eq!(
        skin.vertex_bone_indices[0],
        [2, 3, 2, 3],
        "vertex 0 partition-local [0,1,0,1] must remap via [2,3]"
    );
    // Vertex 1: partition 1's palette is [1, 3]. Local slots
    // [0, 1, 0, 1] remap to globals [1, 3, 1, 3]. Pre-#613 the
    // partition-local indices were cloned verbatim and widened —
    // vertex 1 would have come back as [0, 1, 0, 1] (aliasing to
    // global bones 0 and 1) instead of the intended [1, 3, 1, 3].
    assert_eq!(
        skin.vertex_bone_indices[1],
        [1, 3, 1, 3],
        "vertex 1 partition-local [0,1,0,1] must remap via [1,3] \
             (pre-#613 aliased to bones 0 and 1 because it cloned the \
             partition-local indices verbatim)"
    );
}

/// Single-partition shapes still take the identity-widen fast
/// path: partition-local indices coincide with global indices
/// because the partition's `bones` palette is the full skin list.
/// Locks the no-regression case.
#[test]
fn single_partition_shape_widens_indices_directly() {
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
        bone_refs: vec![BlockRef(5), BlockRef(6), BlockRef(7), BlockRef(8)],
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

    let skin_partition = NiSkinPartition {
        partitions: vec![SkinPartitionEntry {
            num_vertices: 1,
            num_triangles: 0,
            // Single partition → bones palette is identity over
            // the global bone list. Indices already match.
            bones: vec![0, 1, 2, 3],
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

    let shape_ref = scene.get_as::<BsTriShape>(0).unwrap();
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref).unwrap();
    // [3, 0, 0, 0] u8 widens to [3, 0, 0, 0] u16 — single-partition
    // identity. No remap surprise.
    assert_eq!(skin.vertex_bone_indices[0], [3u16, 0, 0, 0]);
}

/// When the linked NiSkinPartition is missing entirely (synthetic
/// or mod malformation), the importer falls back to identity
/// widening rather than failing or aliasing. Locks the defensive
/// fallback path.
#[test]
fn missing_skin_partition_falls_back_to_identity_widen() {
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
        bone_indices: vec![[7, 0, 0, 0]],
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
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref).unwrap();
    // No partition table to remap through — identity widen [7] → [7u16].
    assert_eq!(skin.vertex_bone_indices[0], [7u16, 0, 0, 0]);
}
