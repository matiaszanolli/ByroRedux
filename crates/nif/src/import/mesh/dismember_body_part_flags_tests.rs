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
    SkinPartitionEntry, SseSkinGlobalBuffer,
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
                body_part: 130, // section-cap variant of SBP_30_HEAD
            },
            BodyPartInfo {
                part_flag: 0,
                body_part: 141, // section-cap variant of SBP_41_LONGHAIR
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
    let skin = extract_skin_ni_tri_shape(&scene, shape_ref, 1, &[])
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
    let skin = extract_skin_ni_tri_shape(&scene, shape_ref, 1, &[]).unwrap();
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
        num_triangles: 1,
        num_vertices: 3,
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
            NiPoint3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ],
        uvs: Vec::new(),
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: vec![[0, 1, 2]],
        bone_weights: vec![[1.0, 0.0, 0.0, 0.0]; 3],
        bone_indices: vec![[0, 0, 0, 0]; 3],
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
            body_part: 102, // section-cap variant of FO3/FNV BP_HEAD2
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
            num_vertices: 3,
            num_triangles: 1,
            bones: vec![0],
            num_weights_per_vertex: 4,
            vertex_map: vec![0, 1, 2],
            vertex_weights: Vec::new(),
            triangles: vec![[0, 1, 2]],
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
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref, &[0, 1, 2])
        .expect("BsDismemberSkinInstance-backed BsTriShape must build an ImportedSkin");

    assert_eq!(
        skin.body_part_flags,
        vec![BodyPartInfo {
            part_flag: 1,
            body_part: 102,
        }],
        "BsDismemberSkinInstance partitions must forward on the BsTriShape path too (#1659)"
    );
    assert_eq!(
        skin.triangle_body_parts,
        vec![102],
        "the final draw triangle must retain its dismember partition identity"
    );
}

/// #3360 — `triangle_body_parts` must read SSE partition triangles as
/// GLOBAL indices, matching what `try_reconstruct_sse_geometry` now emits
/// (#3355). The two are a matched pair: before both were fixed they applied
/// the identical wrong `vertex_map` remap and cancelled out, and fixing
/// either alone makes every key miss, drops the whole map to
/// `UNASSIGNED_BODY_PART`, and returns empty — at which point
/// `hide_skin_partitions` stops hiding and every NPC renders bare body skin
/// through their armour.
///
/// `vertex_map` here maps onto a disjoint index range (`[5, 6, 7]`) rather
/// than a permutation: `canonical_triangle` sorts, so a permutation of the
/// same three indices would be invisible to the lookup and the test would
/// pass either way.
#[test]
fn sse_global_partition_triangles_are_not_remapped_for_body_parts() {
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
        num_triangles: 1,
        num_vertices: 3,
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
            NiPoint3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ],
        uvs: Vec::new(),
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: vec![[0, 1, 2]],
        bone_weights: vec![[1.0, 0.0, 0.0, 0.0]; 3],
        bone_indices: vec![[0, 0, 0, 0]; 3],
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
            body_part: 32,
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
            num_vertices: 3,
            num_triangles: 1,
            bones: vec![0],
            num_weights_per_vertex: 4,
            // Disjoint from the triangle's own indices: if this were
            // (wrongly) applied the keys would become {5,6,7} and never
            // match the {0,1,2} draw list.
            vertex_map: vec![5, 6, 7],
            vertex_weights: Vec::new(),
            triangles: vec![[0, 1, 2]],
            bone_indices: Vec::new(),
        }],
        // The SSE marker that gates the global reading.
        global_vertex_data: Some(SseSkinGlobalBuffer {
            vertex_desc: 0,
            vertex_size: 0,
            raw_bytes: Vec::new(),
        }),
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape)); // 0
    scene.blocks.push(Box::new(dismember)); // 1
    scene.blocks.push(Box::new(skin_data)); // 2
    scene.blocks.push(Box::new(skin_partition)); // 3
    scene.blocks.push(bone_node()); // 4

    let shape_ref = scene.get_as::<BsTriShape>(0).unwrap();
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref, &[0, 1, 2])
        .expect("SSE dismember shape must build an ImportedSkin");

    assert_eq!(
        skin.triangle_body_parts,
        vec![32],
        "SSE partition triangles are global — remapping them through \
         vertex_map makes every body-part key miss, so hide_skin_partitions \
         silently stops hiding armour-covered skin (#3360)"
    );
}

/// The legacy path must keep the remap: nifly's `bMappedIndices` defaults
/// to `true` and only flips for `Stream() == 100`, so Oblivion/FO3/FNV
/// partition triangles genuinely are vertex_map-local. Same disjoint map,
/// opposite expectation — this is why #3360 is a version gate, not a
/// deletion.
#[test]
fn legacy_partition_triangles_still_go_through_vertex_map_for_body_parts() {
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
            skin_partition_ref: BlockRef(3),
            skeleton_root_ref: BlockRef::NULL,
            bone_refs: vec![BlockRef(4)],
        },
        partitions: vec![BodyPartInfo {
            part_flag: 1,
            body_part: 32,
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
            num_vertices: 3,
            num_triangles: 1,
            bones: vec![0],
            num_weights_per_vertex: 4,
            vertex_map: vec![5, 6, 7],
            vertex_weights: Vec::new(),
            triangles: vec![[0, 1, 2]],
            bone_indices: Vec::new(),
        }],
        // No global buffer -> legacy game -> triangles are local.
        global_vertex_data: None,
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape)); // 0
    scene.blocks.push(Box::new(dismember)); // 1
    scene.blocks.push(Box::new(skin_data)); // 2
    scene.blocks.push(Box::new(skin_partition)); // 3
    scene.blocks.push(bone_node()); // 4

    let shape_ref = scene.get_as::<NiTriShape>(0).unwrap();
    // The draw list is the REMAPPED triangle, which is what the legacy
    // geometry path emits.
    let skin = extract_skin_ni_tri_shape(&scene, shape_ref, 8, &[5, 6, 7])
        .expect("legacy dismember shape must build an ImportedSkin");

    assert_eq!(
        skin.triangle_body_parts,
        vec![32],
        "legacy partition triangles are vertex_map-local and must still be \
         remapped (#3360 is a gate, not a deletion)"
    );
}
