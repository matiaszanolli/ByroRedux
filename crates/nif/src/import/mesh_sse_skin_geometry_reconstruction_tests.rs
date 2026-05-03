//! Tests for `sse_skin_geometry_reconstruction_tests` extracted from ../mesh.rs (refactor stage A).
//!
//! Same qualified path preserved (`sse_skin_geometry_reconstruction_tests::FOO`).

//! Regression coverage for #559 — when a Skyrim SE skinned
//! `BsTriShape` ships with empty inline `vertices` / `triangles`,
//! the importer must reconstruct geometry from the linked
//! `NiSkinPartition.global_vertex_data` (the SSE global packed-vertex
//! buffer) plus per-partition `vertex_map` arrays. Pre-fix every
//! Skyrim SE NPC body and creature imported as zero meshes because
//! the parser silently `stream.skip`'d the global buffer and the
//! importer's early-return guard fired on the empty inline arrays.
use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::skin::{
    NiSkinInstance, NiSkinPartition, SkinPartitionEntry, SseSkinGlobalBuffer,
};
use crate::blocks::tri_shape::BsTriShapeKind;
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3};

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

/// Build a 16-byte/vertex SSE packed-position payload — VF_VERTEX
/// only, no UVs / normals / colours / skinning. Each vertex is
/// 12 bytes of f32 position + 4 bytes of `bitangent_x` padding.
fn pack_position_only(positions_zup: &[[f32; 3]]) -> (u64, u32, Vec<u8>) {
    let vertex_size: u32 = 16;
    // vertex_desc: low nibble = vertex_size / 4 = 4. Vertex
    // attribute bitfield (bits 44-55) sets VF_VERTEX (0x001) only.
    let vertex_desc: u64 = (0x001u64 << 44) | 0x4;
    let mut raw = Vec::with_capacity(positions_zup.len() * vertex_size as usize);
    for [x, y, z] in positions_zup {
        raw.extend_from_slice(&x.to_le_bytes());
        raw.extend_from_slice(&y.to_le_bytes());
        raw.extend_from_slice(&z.to_le_bytes());
        raw.extend_from_slice(&0.0f32.to_le_bytes()); // bitangent_x padding
    }
    (vertex_desc, vertex_size, raw)
}

/// Pre-fix this scene imports zero meshes because `extract_bs_tri_shape`
/// returns `None` on empty inline arrays. Post-fix the importer
/// resolves `BsTriShape.skin_ref` → `NiSkinInstance.skin_partition_ref`
/// → `NiSkinPartition.global_vertex_data`, decodes the packed buffer,
/// and concatenates partition triangles (remapped via `vertex_map`)
/// into the final index list.
#[test]
fn empty_inline_bs_tri_shape_with_populated_skin_partition_reconstructs() {
    // 3 vertices in Z-up space; the importer flips them to Y-up
    // (x, z, -y) when emitting `ImportedMesh.positions`.
    let zup_positions = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
    let (vertex_desc, vertex_size, raw_bytes) = pack_position_only(&zup_positions);

    // BsTriShape (block 0) — empty inline arrays. skin_ref → block 1.
    let shape = BsTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: crate::types::NiTransform::default(),
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
        vertex_desc,
        num_triangles: 0,
        num_vertices: 0,
        vertices: Vec::new(),
        uvs: Vec::new(),
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: Vec::new(),
        bone_weights: Vec::new(),
        bone_indices: Vec::new(),
        tangents: Vec::new(),
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    };

    // NiSkinInstance (block 1) → NiSkinPartition (block 2).
    let skin_instance = NiSkinInstance {
        data_ref: BlockRef::NULL,
        skin_partition_ref: BlockRef(2),
        skeleton_root_ref: BlockRef::NULL,
        bone_refs: Vec::new(),
    };

    // Single partition with vertex_map = identity and one
    // triangle. The importer remaps partition-local indices
    // (0, 1, 2) through vertex_map to global indices (0, 1, 2).
    let partition = SkinPartitionEntry {
        num_vertices: 3,
        num_triangles: 1,
        bones: Vec::new(),
        num_weights_per_vertex: 0,
        vertex_map: vec![0, 1, 2],
        vertex_weights: Vec::new(),
        triangles: vec![[0, 1, 2]],
        bone_indices: Vec::new(),
    };

    let skin_partition = NiSkinPartition {
        partitions: vec![partition],
        global_vertex_data: Some(SseSkinGlobalBuffer {
            vertex_desc,
            vertex_size,
            raw_bytes,
        }),
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape));
    scene.blocks.push(Box::new(skin_instance));
    scene.blocks.push(Box::new(skin_partition));

    // Re-borrow the shape (NifScene now owns it).
    let shape_ref = scene
        .get_as::<BsTriShape>(0)
        .expect("block 0 round-trips as BsTriShape");
    let mesh = extract_bs_tri_shape(&scene, shape_ref, &crate::types::NiTransform::default(), &mut byroredux_core::string::StringPool::new())
        .expect("SSE skin partition reconstruction must produce a mesh (#559)");

    assert_eq!(mesh.positions.len(), 3, "all 3 vertices reconstructed");
    // Z-up (1,2,3) → Y-up (1, 3, -2).
    assert_eq!(mesh.positions[0], [1.0, 3.0, -2.0]);
    assert_eq!(mesh.positions[1], [4.0, 6.0, -5.0]);
    assert_eq!(mesh.positions[2], [7.0, 9.0, -8.0]);
    assert_eq!(mesh.indices, vec![0, 1, 2]);
}

/// vertex_map remap is the partition-local → global translation
/// the SSE skin format depends on. Build a partition whose
/// `vertex_map = [2, 0, 1]` and triangle `[0, 1, 2]` — the
/// emitted indices must be the remapped `[2, 0, 1]`.
#[test]
fn partition_vertex_map_remaps_local_indices_to_global() {
    let zup_positions = [[10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 10.0]];
    let (vertex_desc, vertex_size, raw_bytes) = pack_position_only(&zup_positions);

    let shape = BsTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: crate::types::NiTransform::default(),
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
        vertex_desc,
        num_triangles: 0,
        num_vertices: 0,
        vertices: Vec::new(),
        uvs: Vec::new(),
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: Vec::new(),
        bone_weights: Vec::new(),
        bone_indices: Vec::new(),
        tangents: Vec::new(),
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    };

    let skin_instance = NiSkinInstance {
        data_ref: BlockRef::NULL,
        skin_partition_ref: BlockRef(2),
        skeleton_root_ref: BlockRef::NULL,
        bone_refs: Vec::new(),
    };

    let partition = SkinPartitionEntry {
        num_vertices: 3,
        num_triangles: 1,
        bones: Vec::new(),
        num_weights_per_vertex: 0,
        // Non-identity vertex_map exercises the remap path.
        vertex_map: vec![2, 0, 1],
        vertex_weights: Vec::new(),
        triangles: vec![[0, 1, 2]],
        bone_indices: Vec::new(),
    };

    let skin_partition = NiSkinPartition {
        partitions: vec![partition],
        global_vertex_data: Some(SseSkinGlobalBuffer {
            vertex_desc,
            vertex_size,
            raw_bytes,
        }),
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape));
    scene.blocks.push(Box::new(skin_instance));
    scene.blocks.push(Box::new(skin_partition));

    let shape_ref = scene.get_as::<BsTriShape>(0).unwrap();
    let mesh = extract_bs_tri_shape(&scene, shape_ref, &crate::types::NiTransform::default(), &mut byroredux_core::string::StringPool::new())
        .expect("reconstruction with non-identity vertex_map must succeed");

    // partition triangle [0, 1, 2] remapped via [2, 0, 1] → [2, 0, 1].
    assert_eq!(mesh.indices, vec![2, 0, 1]);
}

/// When the linked `NiSkinPartition` has no global vertex data
/// (e.g. legacy Oblivion / FNV / FO3 path), the importer must
/// still apply the original early-return so the existing inline
/// path is unaffected. Locks the negative branch.
#[test]
fn empty_inline_with_no_global_buffer_returns_none() {
    let shape = BsTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: crate::types::NiTransform::default(),
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
        num_vertices: 0,
        vertices: Vec::new(),
        uvs: Vec::new(),
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: Vec::new(),
        bone_weights: Vec::new(),
        bone_indices: Vec::new(),
        tangents: Vec::new(),
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    };
    let skin_instance = NiSkinInstance {
        data_ref: BlockRef::NULL,
        skin_partition_ref: BlockRef(2),
        skeleton_root_ref: BlockRef::NULL,
        bone_refs: Vec::new(),
    };
    let skin_partition = NiSkinPartition {
        partitions: Vec::new(),
        global_vertex_data: None, // legacy / non-SSE — reconstructor must bail
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape));
    scene.blocks.push(Box::new(skin_instance));
    scene.blocks.push(Box::new(skin_partition));

    let shape_ref = scene.get_as::<BsTriShape>(0).unwrap();
    assert!(
        extract_bs_tri_shape(&scene, shape_ref, &crate::types::NiTransform::default(), &mut byroredux_core::string::StringPool::new()).is_none(),
        "empty inline + no global buffer must remain a no-op (early return)"
    );
}

/// Regression: #638 — when the SSE global buffer carries
/// `VF_SKINNED`, `decode_sse_packed_buffer` must surface the
/// per-vertex bone weights + indices into `DecodedPackedBuffer`,
/// and `extract_skin_bs_tri_shape` must fall back to those values
/// when the inline `shape.bone_weights` / `shape.bone_indices` are
/// empty (the canonical state for Skyrim SE NPC bodies, whose
/// `data_size == 0` BSTriShape ships geometry only via the global
/// buffer). Pre-fix every vertex hit the renderer's rigid fallback
/// at `triangle.vert:151` and rendered in bind pose.
///
/// Fixture builds a 28-byte/vertex packed buffer with `VF_VERTEX |
/// VF_SKINNED`: 16 bytes position + 12 bytes skin (4 × half-float
/// weights + 4 × u8 indices). Two vertices, distinct skin payloads
/// per vertex so the parser can't accidentally pass with a static
/// pattern.
#[test]
fn sse_global_buffer_skin_payload_reaches_imported_skin() {
    // 28 bytes per vertex = 7 quads. vertex_attrs = VF_VERTEX (0x001)
    // | VF_SKINNED (0x040) = 0x041.
    let vertex_size: u32 = 28;
    let vertex_desc: u64 = (0x041u64 << 44) | 0x7;

    // Half-float bit patterns: 0x3C00 = 1.0, 0x3800 = 0.5,
    // 0x0000 = 0.0, 0x3400 = 0.25.
    // Vertex 0: weights [1.0, 0.0, 0.0, 0.0], indices [3, 0, 0, 0]
    // Vertex 1: weights [0.5, 0.5, 0.0, 0.0], indices [1, 7, 0, 0]
    let mut raw = Vec::with_capacity(vertex_size as usize * 2);
    // Vertex 0
    raw.extend_from_slice(&1.0f32.to_le_bytes()); // x
    raw.extend_from_slice(&2.0f32.to_le_bytes()); // y
    raw.extend_from_slice(&3.0f32.to_le_bytes()); // z
    raw.extend_from_slice(&0.0f32.to_le_bytes()); // bitangent_x pad
    raw.extend_from_slice(&0x3C00u16.to_le_bytes()); // weight 1.0
    raw.extend_from_slice(&0x0000u16.to_le_bytes()); // weight 0
    raw.extend_from_slice(&0x0000u16.to_le_bytes()); // weight 0
    raw.extend_from_slice(&0x0000u16.to_le_bytes()); // weight 0
    raw.extend_from_slice(&[3u8, 0, 0, 0]); // indices
                                            // Vertex 1
    raw.extend_from_slice(&4.0f32.to_le_bytes());
    raw.extend_from_slice(&5.0f32.to_le_bytes());
    raw.extend_from_slice(&6.0f32.to_le_bytes());
    raw.extend_from_slice(&0.0f32.to_le_bytes());
    raw.extend_from_slice(&0x3800u16.to_le_bytes()); // weight 0.5
    raw.extend_from_slice(&0x3800u16.to_le_bytes()); // weight 0.5
    raw.extend_from_slice(&0x0000u16.to_le_bytes());
    raw.extend_from_slice(&0x0000u16.to_le_bytes());
    raw.extend_from_slice(&[1u8, 7, 0, 0]);

    let shape = BsTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: crate::types::NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        center: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        radius: 0.0,
        // skin_ref → block 1 (NiSkinInstance), data_ref → block 3
        // (NiSkinData) so the legacy bone-resolution path lands
        // and `extract_skin_bs_tri_shape` reaches the per-vertex
        // assertions below.
        skin_ref: BlockRef(1),
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        vertex_desc,
        num_triangles: 0,
        num_vertices: 0,
        vertices: Vec::new(),
        uvs: Vec::new(),
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: Vec::new(),
        // CRITICAL: empty inline arrays — the whole point of #638.
        bone_weights: Vec::new(),
        bone_indices: Vec::new(),
        tangents: Vec::new(),
        kind: BsTriShapeKind::Plain,
        data_size: 0,
    };

    let skin_instance = NiSkinInstance {
        data_ref: BlockRef(3),
        skin_partition_ref: BlockRef(2),
        skeleton_root_ref: BlockRef::NULL,
        // 8 bones in the global skin list — enough that the highest
        // partition-local index (7) is in range and the test can
        // distinguish slots.
        bone_refs: vec![
            BlockRef::NULL,
            BlockRef::NULL,
            BlockRef::NULL,
            BlockRef::NULL,
            BlockRef::NULL,
            BlockRef::NULL,
            BlockRef::NULL,
            BlockRef::NULL,
        ],
    };
    let partition = SkinPartitionEntry {
        num_vertices: 2,
        num_triangles: 0,
        // Single-partition shape with all 8 bones in palette →
        // remap is identity (palette[i] = i).
        bones: (0u16..8).collect(),
        num_weights_per_vertex: 4,
        vertex_map: vec![0, 1],
        vertex_weights: Vec::new(),
        triangles: Vec::new(),
        bone_indices: Vec::new(),
    };
    let skin_partition = NiSkinPartition {
        partitions: vec![partition],
        global_vertex_data: Some(SseSkinGlobalBuffer {
            vertex_desc,
            vertex_size,
            raw_bytes: raw,
        }),
    };
    // Build a NiSkinData with one bone entry per ref; the function
    // checks lengths agree before reading bone transforms.
    use crate::blocks::skin::BoneData;
    let bone_data = NiSkinData {
        skin_transform: crate::types::NiTransform::default(),
        bones: (0..8)
            .map(|_| BoneData {
                skin_transform: crate::types::NiTransform::default(),
                bounding_sphere: [0.0, 0.0, 0.0, 0.0],
                vertex_weights: Vec::new(),
            })
            .collect(),
    };

    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(shape));
    scene.blocks.push(Box::new(skin_instance));
    scene.blocks.push(Box::new(skin_partition));
    scene.blocks.push(Box::new(bone_data));

    let shape_ref = scene.get_as::<BsTriShape>(0).unwrap();
    let skin = extract_skin_bs_tri_shape(&scene, shape_ref)
        .expect("global-buffer skin payload must reach ImportedSkin (#638)");

    assert_eq!(skin.vertex_bone_weights.len(), 2);
    assert_eq!(skin.vertex_bone_indices.len(), 2);

    // Vertex 0: full weight on bone 3.
    assert!((skin.vertex_bone_weights[0][0] - 1.0).abs() < 1e-3);
    assert_eq!(skin.vertex_bone_weights[0][1], 0.0);
    assert_eq!(skin.vertex_bone_indices[0], [3, 0, 0, 0]);

    // Vertex 1: 50/50 between bones 1 and 7.
    assert!((skin.vertex_bone_weights[1][0] - 0.5).abs() < 1e-3);
    assert!((skin.vertex_bone_weights[1][1] - 0.5).abs() < 1e-3);
    assert_eq!(skin.vertex_bone_weights[1][2], 0.0);
    assert_eq!(skin.vertex_bone_indices[1], [1, 7, 0, 0]);
}
