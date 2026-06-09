//! Regression tests for #1207 (BSLODTriShape `bs_lod_cutoffs`) and
//! #1206 (BSSubIndexTriShape `bs_sub_index`) — the parser captured the
//! `BsTriShapeKind` discriminator but `extract_bs_tri_shape` dropped it
//! entirely. Both fields now ride through to `ImportedMesh` for the
//! eventual M35 LOD selector + dismemberment system to consume.

use super::*;
use crate::blocks::tri_shape::{
    BsGeometryPerSegmentSharedData, BsGeometrySegmentData, BsGeometrySegmentSharedData,
    BsGeometrySubSegment, BsSubIndexTriShapeData, BsTriShape, BsTriShapeKind,
};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3};

use super::super::ImportedMesh;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

/// Build a renderable `BsTriShape` (one triangle, three vertices) with
/// the given `kind` discriminator. No shader bound — the test focuses
/// on the discriminator passthrough.
fn renderable_shape_with_kind(kind: BsTriShapeKind) -> BsTriShape {
    BsTriShape {
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
        skin_ref: BlockRef::NULL,
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
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        normals: Vec::new(),
        vertex_colors: Vec::new(),
        triangles: vec![[0, 1, 2]],
        bone_weights: Vec::new(),
        bone_indices: Vec::new(),
        tangents: Vec::new(),
        kind,
        data_size: 0,
    }
}

fn import(shape: &BsTriShape) -> ImportedMesh {
    let scene = NifScene::default();
    let mut pool = byroredux_core::string::StringPool::new();
    extract_bs_tri_shape(
        &scene,
        shape,
        &crate::types::NiTransform::default(),
        &mut pool,
    )
    .expect("renderable shape must produce ImportedMesh")
}

// ── #1207: BSLODTriShape cutoffs ────────────────────────────────

#[test]
fn lod_kind_surfaces_three_cutoffs() {
    let shape = renderable_shape_with_kind(BsTriShapeKind::LOD {
        lod0: 1000,
        lod1: 500,
        lod2: 100,
    });
    let mesh = import(&shape);
    assert_eq!(
        mesh.bs_lod_cutoffs,
        Some([1000, 500, 100]),
        "BsTriShapeKind::LOD must surface the triple verbatim"
    );
    assert!(
        mesh.bs_sub_index.is_none(),
        "LOD variant must not synthesize a SubIndex payload"
    );
}

#[test]
fn plain_kind_drops_lod_cutoffs() {
    let mesh = import(&renderable_shape_with_kind(BsTriShapeKind::Plain));
    assert_eq!(mesh.bs_lod_cutoffs, None);
}

#[test]
fn mesh_lod_kind_drops_lod_cutoffs() {
    // BSMeshLODTriShape (Skyrim SE DLC variant) shares the wire format
    // with BSLODTriShape but the engine doesn't consult the cutoffs;
    // the parser tracks the discriminator via `MeshLOD` and the import
    // surface intentionally drops the triple. The current parser variant
    // does not embed the trio in `MeshLOD` so the importer returns None.
    let mesh = import(&renderable_shape_with_kind(BsTriShapeKind::MeshLOD));
    assert_eq!(mesh.bs_lod_cutoffs, None);
}

// ── #1206: BSSubIndexTriShape segmentation ──────────────────────

fn sample_subindex_data() -> BsSubIndexTriShapeData {
    BsSubIndexTriShapeData {
        num_primitives: 42,
        num_segments: 2,
        total_segments: 3,
        segments: vec![
            BsGeometrySegmentData {
                flags: None,
                start_index: 0,
                num_primitives: 21,
                parent_array_index: Some(0xFFFFFFFF),
                sub_segments: Vec::new(),
            },
            BsGeometrySegmentData {
                flags: None,
                start_index: 21,
                num_primitives: 21,
                parent_array_index: Some(0xFFFFFFFF),
                sub_segments: vec![BsGeometrySubSegment {
                    start_index: 21,
                    num_primitives: 21,
                    parent_array_index: 1,
                    unused: 0,
                }],
            },
        ],
        shared: Some(BsGeometrySegmentSharedData {
            num_segments: 2,
            total_segments: 3,
            segment_starts: vec![0, 21],
            per_segment_data: vec![BsGeometryPerSegmentSharedData {
                user_index: 0,
                bone_id: 0xDEADBEEF,
                cut_offsets: vec![0.25, 0.5, 0.75],
            }],
            ssf_filename: "actors\\character\\test.ssf".to_owned(),
        }),
    }
}

#[test]
fn subindex_kind_surfaces_payload_verbatim() {
    let payload = sample_subindex_data();
    let shape = renderable_shape_with_kind(BsTriShapeKind::SubIndex(Box::new(payload.clone())));
    let mesh = import(&shape);
    let surfaced = mesh
        .bs_sub_index
        .as_ref()
        .expect("BsTriShapeKind::SubIndex must surface segmentation payload");
    assert_eq!(surfaced, &payload);
    assert!(
        mesh.bs_lod_cutoffs.is_none(),
        "SubIndex variant must not synthesize LOD cutoffs"
    );
}

#[test]
fn plain_kind_drops_subindex_payload() {
    let mesh = import(&renderable_shape_with_kind(BsTriShapeKind::Plain));
    assert!(mesh.bs_sub_index.is_none());
}

#[test]
fn lod_kind_drops_subindex_payload() {
    let mesh = import(&renderable_shape_with_kind(BsTriShapeKind::LOD {
        lod0: 0,
        lod1: 0,
        lod2: 0,
    }));
    assert!(mesh.bs_sub_index.is_none());
}

#[test]
fn dynamic_kind_drops_both() {
    let mesh = import(&renderable_shape_with_kind(BsTriShapeKind::Dynamic));
    assert!(mesh.bs_lod_cutoffs.is_none());
    assert!(mesh.bs_sub_index.is_none());
}
