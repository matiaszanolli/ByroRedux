//! Regression tests for #2357 / SF2D2-03.
//!
//! Stage B (external `.mesh` resolve) has three distinct "no geometry
//! found" exits: no resolver supplied, a per-slot resolve miss, and every
//! slot exhausted with nothing resolvable. All three used to return
//! `None` with no log signal at all — this file doesn't (can't, without a
//! new test-logging dependency) assert on the log text itself, but pins
//! the *behavior* at each exit: still gracefully `None`, not a panic or a
//! wrong-fallback `Some`, now that each site has been rewritten from a
//! bare `?`/`if let` into a `let ... else` that logs before returning.
//! Compare against the sibling `bs_geometry_sentinel_slot_tests.rs`,
//! which covers the *different* failure mode of a resolver returning
//! `Some` with a parseable-but-empty body.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::bs_geometry::{BSGeometry, BSGeometryMesh, BSGeometryMeshKind};
use crate::scene::NifScene;
use crate::types::{BlockRef, NiMatrix3, NiPoint3, NiTransform};
use byroredux_core::string::StringPool;
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

fn av_with_flags(flags: u32) -> NiAVObjectData {
    NiAVObjectData {
        net: NiObjectNETData {
            name: Some(Arc::from("ResolveLogTestShape")),
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

/// Exit 1: no `FLAG_INTERNAL_GEOM_DATA` (so Stage B runs) and `resolver:
/// None`. Pre-#2357 this was a bare `resolver?` with no log at all.
#[test]
fn stage_b_no_resolver_returns_none() {
    let shape = bs_geometry_with_meshes(
        0,
        vec![BSGeometryMesh {
            lod_slot: 0,
            tri_size: 0,
            num_verts: 0,
            flags: 0,
            kind: BSGeometryMeshKind::External {
                mesh_name: "whatever".to_string(),
            },
        }],
    );
    let scene = NifScene::default();
    let mut pool = StringPool::new();
    assert!(extract_bs_geometry(&scene, &shape, &shape.av.transform, &mut pool, None).is_none());
}

/// Test-double resolver that always misses — every `resolve()` call
/// returns `None`, exercising exit 2 (per-slot resolve miss) for every
/// slot, then falling through to exit 3 (every slot exhausted).
struct AlwaysMissResolver;

impl super::super::MeshResolver for AlwaysMissResolver {
    fn resolve(&self, _mesh_name: &str) -> Option<Vec<u8>> {
        None
    }
}

/// Exit 2 + exit 3: a resolver that's present but never has the
/// requested path (archive misconfiguration / missing archive / path
/// convention drift — the exact #1292 symptom class). Pre-#2357 both the
/// per-slot miss and the final `found?` returned `None` with no log.
#[test]
fn stage_b_resolver_always_misses_returns_none() {
    let shape = bs_geometry_with_meshes(
        0,
        vec![
            BSGeometryMesh {
                lod_slot: 0,
                tri_size: 0,
                num_verts: 0,
                flags: 0,
                kind: BSGeometryMeshKind::External {
                    mesh_name: "first".to_string(),
                },
            },
            BSGeometryMesh {
                lod_slot: 1,
                tri_size: 0,
                num_verts: 0,
                flags: 0,
                kind: BSGeometryMeshKind::External {
                    mesh_name: "second".to_string(),
                },
            },
        ],
    );
    let scene = NifScene::default();
    let mut pool = StringPool::new();
    assert!(extract_bs_geometry(
        &scene,
        &shape,
        &shape.av.transform,
        &mut pool,
        Some(&AlwaysMissResolver)
    )
    .is_none());
}

/// A resolver present but with an empty `meshes` list has nothing to
/// iterate — `found` stays `None` via the loop simply not running,
/// still exit 3's `let ... else`, not a panic.
#[test]
fn stage_b_no_external_slots_returns_none() {
    let shape = bs_geometry_with_meshes(0, vec![]);
    let scene = NifScene::default();
    let mut pool = StringPool::new();
    assert!(extract_bs_geometry(
        &scene,
        &shape,
        &shape.av.transform,
        &mut pool,
        Some(&AlwaysMissResolver)
    )
    .is_none());
}
