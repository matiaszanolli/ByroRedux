//! Regression tests for SF2-01 / #1828 and SF2-02 / #1829.
//!
//! Both bugs share one root cause: a `BSGeometryMeshData` slot can parse
//! successfully (`Ok`) yet be the `scale <= 0.0` sentinel body — empty
//! `vertices`, but `triangles` still populated (the on-disk layout reads
//! triangles before scale). Vanilla Starfield uses these sentinel slots
//! for segment-only / skin-weight-only LODs that share a parent
//! `BSGeometry` with a populated slot. Pre-fix, both Stage A (inline
//! `Internal` slots) and Stage B (external `.mesh` slots) accepted the
//! first slot that merely *parsed*, dropping the whole mesh if that slot
//! happened to be the sentinel and a later slot carried real geometry.
//!
//! These tests exercise the real `extract_bs_geometry` entry point (not
//! a duplicated selection helper) with a sentinel-first slot ordering,
//! asserting a mesh is still produced from the later populated slot.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::bs_geometry::{
    BSGeometry, BSGeometryMesh, BSGeometryMeshData, BSGeometryMeshKind,
};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiMatrix3, NiPoint3, NiTransform};
use byroredux_core::string::StringPool;
use std::sync::Arc;

const FLAG_INTERNAL_GEOM_DATA: u32 = 0x200;

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

fn av_with_flags(flags: u32) -> NiAVObjectData {
    NiAVObjectData {
        net: NiObjectNETData {
            name: Some(Arc::from("SentinelSlotShape")),
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        flags,
        transform: identity_transform(),
        properties: Vec::new(),
        collision_ref: BlockRef::NULL,
    }
}

fn bs_geometry_with_meshes(flags: u32, meshes: Vec<BSGeometryMesh>) -> BSGeometry {
    BSGeometry {
        av: av_with_flags(flags),
        bounding_sphere: ([0.0, 0.0, 0.0], 0.0),
        bound_min_max: [0.0; 6],
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        meshes,
    }
}

/// A sentinel slot: `scale <= 0.0` body — `vertices`/`normals_raw`/etc all
/// empty, but `triangles` populated (matches the real on-disk layout,
/// where triangles are read before the scale sentinel is checked).
fn sentinel_mesh_data() -> BSGeometryMeshData {
    BSGeometryMeshData {
        version: 2,
        triangles: vec![[0, 1, 2]],
        scale: 0.0,
        weights_per_vert: 0,
        vertices: Vec::new(),
        uvs0: Vec::new(),
        uvs1: Vec::new(),
        colors: Vec::new(),
        normals_raw: Vec::new(),
        tangents_raw: Vec::new(),
        skin_weights: Vec::new(),
        lods: Vec::new(),
        meshlets: Vec::new(),
        cull_data: Vec::new(),
    }
}

/// A populated slot with a single triangle.
fn populated_mesh_data() -> BSGeometryMeshData {
    BSGeometryMeshData {
        version: 2,
        triangles: vec![[0, 1, 2]],
        scale: 1.0,
        weights_per_vert: 0,
        vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        uvs0: Vec::new(),
        uvs1: Vec::new(),
        colors: Vec::new(),
        normals_raw: Vec::new(),
        tangents_raw: Vec::new(),
        skin_weights: Vec::new(),
        lods: Vec::new(),
        meshlets: Vec::new(),
        cull_data: Vec::new(),
    }
}

// ── SF2-02 / #1829: Stage A (inline `Internal` slots) ──────────────────

#[test]
fn stage_a_skips_sentinel_first_internal_slot_and_finds_populated_one() {
    let shape = bs_geometry_with_meshes(
        FLAG_INTERNAL_GEOM_DATA,
        vec![
            BSGeometryMesh {
                tri_size: 0,
                num_verts: 0,
                flags: 0,
                kind: BSGeometryMeshKind::Internal {
                    mesh_data: Box::new(sentinel_mesh_data()),
                },
            },
            BSGeometryMesh {
                tri_size: 0,
                num_verts: 0,
                flags: 0,
                kind: BSGeometryMeshKind::Internal {
                    mesh_data: Box::new(populated_mesh_data()),
                },
            },
        ],
    );
    let scene = NifScene::default();
    let mut pool = StringPool::new();
    let mesh = extract_bs_geometry(&scene, &shape, &shape.av.transform, &mut pool, None)
        .expect("sentinel-first Internal slot order must not drop a later populated slot");
    assert_eq!(mesh.positions.len(), 3);
    assert_eq!(mesh.indices.len(), 3);
}

#[test]
fn stage_a_all_sentinel_internal_slots_returns_none() {
    let shape = bs_geometry_with_meshes(
        FLAG_INTERNAL_GEOM_DATA,
        vec![BSGeometryMesh {
            tri_size: 0,
            num_verts: 0,
            flags: 0,
            kind: BSGeometryMeshKind::Internal {
                mesh_data: Box::new(sentinel_mesh_data()),
            },
        }],
    );
    let scene = NifScene::default();
    let mut pool = StringPool::new();
    assert!(extract_bs_geometry(&scene, &shape, &shape.av.transform, &mut pool, None).is_none());
}

// ── SF2-01 / #1828: Stage B (external `.mesh` slots) ────────────────────

/// Test-double resolver: maps a canonical `geometries\<name>.mesh` path
/// to raw `BSGeometryMeshData` bytes for the named slot.
struct FakeResolver {
    sentinel_name: String,
    populated_name: String,
}

impl super::super::MeshResolver for FakeResolver {
    fn resolve(&self, mesh_name: &str) -> Option<Vec<u8>> {
        if mesh_name == format!("geometries\\{}.mesh", self.sentinel_name) {
            Some(encode_mesh_data(&sentinel_mesh_data()))
        } else if mesh_name == format!("geometries\\{}.mesh", self.populated_name) {
            Some(encode_mesh_data(&populated_mesh_data()))
        } else {
            None
        }
    }
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}
fn write_f32(buf: &mut Vec<u8>, v: f32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

/// Encode a `BSGeometryMeshData` back into the on-disk byte layout that
/// `BSGeometryMeshData::parse_from_bytes` reads. Only supports the
/// field combinations these tests exercise (no UVs/colors/normals/
/// tangents/skin-weights/LODs/meshlets/cull-data).
fn encode_mesh_data(data: &BSGeometryMeshData) -> Vec<u8> {
    let mut buf = Vec::new();
    write_u32(&mut buf, data.version);
    write_u32(&mut buf, (data.triangles.len() * 3) as u32);
    for tri in &data.triangles {
        write_u16(&mut buf, tri[0]);
        write_u16(&mut buf, tri[1]);
        write_u16(&mut buf, tri[2]);
    }
    write_f32(&mut buf, data.scale);
    if data.scale <= 0.0 {
        return buf;
    }
    write_u32(&mut buf, data.weights_per_vert);
    write_u32(&mut buf, data.vertices.len() as u32);
    for _ in &data.vertices {
        // These tests only assert vertex *count*, not exact decoded
        // coordinates, so raw NORM i16 value doesn't matter.
        for _ in 0..3 {
            write_u16(&mut buf, 0);
        }
    }
    write_u32(&mut buf, 0); // n_uv1
    write_u32(&mut buf, 0); // n_uv2
    write_u32(&mut buf, 0); // n_colors
    write_u32(&mut buf, 0); // n_normals
    write_u32(&mut buf, 0); // n_tangents
    write_u32(&mut buf, 0); // n_total_weights
    write_u32(&mut buf, 0); // n_lods
    write_u32(&mut buf, 0); // n_meshlets
    write_u32(&mut buf, 0); // n_cull_data
    buf
}

#[test]
fn stage_b_skips_sentinel_first_external_slot_and_finds_populated_one() {
    let resolver = FakeResolver {
        sentinel_name: "sentinel".to_string(),
        populated_name: "populated".to_string(),
    };
    let shape = bs_geometry_with_meshes(
        0, // no FLAG_INTERNAL_GEOM_DATA — Stage B (external)
        vec![
            BSGeometryMesh {
                tri_size: 0,
                num_verts: 0,
                flags: 0,
                kind: BSGeometryMeshKind::External {
                    mesh_name: "sentinel".to_string(),
                },
            },
            BSGeometryMesh {
                tri_size: 0,
                num_verts: 0,
                flags: 0,
                kind: BSGeometryMeshKind::External {
                    mesh_name: "populated".to_string(),
                },
            },
        ],
    );
    let scene = NifScene::default();
    let mut pool = StringPool::new();
    let mesh = extract_bs_geometry(
        &scene,
        &shape,
        &shape.av.transform,
        &mut pool,
        Some(&resolver),
    )
    .expect("sentinel-first external slot order must not drop a later populated slot");
    assert_eq!(mesh.positions.len(), 3);
    assert_eq!(mesh.indices.len(), 3);
}

#[test]
fn stage_b_all_sentinel_external_slots_returns_none() {
    let resolver = FakeResolver {
        sentinel_name: "sentinel".to_string(),
        populated_name: "unused".to_string(),
    };
    let shape = bs_geometry_with_meshes(
        0,
        vec![BSGeometryMesh {
            tri_size: 0,
            num_verts: 0,
            flags: 0,
            kind: BSGeometryMeshKind::External {
                mesh_name: "sentinel".to_string(),
            },
        }],
    );
    let scene = NifScene::default();
    let mut pool = StringPool::new();
    assert!(extract_bs_geometry(
        &scene,
        &shape,
        &shape.av.transform,
        &mut pool,
        Some(&resolver)
    )
    .is_none());
}
