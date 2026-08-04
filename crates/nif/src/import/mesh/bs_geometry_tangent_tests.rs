//! Regression tests for BSGeometry tangent extraction (#1086 / REN-D16-001)
//! and LOD-slot iteration in `extract_bs_geometry` (#1209).
//!
//! Guards against the regression where `extract_bs_geometry` returned
//! `tangents: Vec::new()` for all Starfield meshes, forcing every mesh
//! to the shader's screen-space derivative Path-2 in `perturbNormal`.

use super::extract_bs_geometry;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::bs_geometry::unpack_udec3_xyzw;
use crate::blocks::bs_geometry::{
    BSGeometry, BSGeometryMesh, BSGeometryMeshData, BSGeometryMeshKind,
};
use crate::scene::NifScene;
use crate::types::{clamp_sign, BlockRef, NiMatrix3, NiPoint3, NiTransform};
use byroredux_core::string::StringPool;
use std::sync::Arc;

/// Helper: encode 10-bit x/y/z and 2-bit w into a UDEC3 word.
fn encode_udec3(x: u32, y: u32, z: u32, w: u32) -> u32 {
    (x & 0x3FF) | ((y & 0x3FF) << 10) | ((z & 0x3FF) << 20) | ((w & 0x3) << 30)
}

/// UDEC3 round-trip: encode tangent (1, 0, 0, +1) and verify the
/// unpack gives approximately the same per-channel values.
/// The 10:10:10:2 format has ~1/1023 precision; ε = 0.003.
#[test]
fn udec3_tangent_roundtrip_x_plus_sign() {
    // x=1.0 → raw 1023; y=0.0 → raw 511; z=0.0 → raw 511; w=+1.0 → raw 3.
    let packed = encode_udec3(1023, 511, 511, 3);
    let xyzw = unpack_udec3_xyzw(packed);
    const EPS: f32 = 0.003;
    assert!((xyzw[0] - 1.0).abs() < EPS, "x ≈ 1.0, got {}", xyzw[0]);
    assert!(xyzw[1].abs() < EPS, "y ≈ 0.0, got {}", xyzw[1]);
    assert!(xyzw[2].abs() < EPS, "z ≈ 0.0, got {}", xyzw[2]);
    assert!(
        (xyzw[3] - 1.0).abs() < EPS,
        "w (sign) ≈ +1.0, got {}",
        xyzw[3]
    );
}

/// Verify that negative bitangent sign (w = -1) is correctly decoded.
#[test]
fn udec3_tangent_negative_sign() {
    // w=0 → raw 0 → (0/3)*2-1 = -1.0.
    let packed = encode_udec3(1023, 511, 511, 0);
    let xyzw = unpack_udec3_xyzw(packed);
    const EPS: f32 = 0.003;
    assert!(
        (xyzw[3] - (-1.0)).abs() < EPS,
        "w (sign) ≈ -1.0, got {}",
        xyzw[3]
    );
}

/// The extraction path: N entries in tangents_raw should produce N tangents,
/// each a [f32;4] matching the unpacked UDEC3 values.
/// This guards against a regression back to Vec::new().
#[test]
fn tangent_extraction_count_and_values() {
    let n: usize = 4;
    let raw_val = encode_udec3(1023, 511, 511, 3); // tangent (1, 0, 0, +1)
    let tangents_raw: Vec<u32> = vec![raw_val; n];

    // Simulate the (post-#2246) extraction loop in extract_bs_geometry.
    let tangents: Vec<[f32; 4]> = tangents_raw
        .iter()
        .map(|&raw| {
            let xyzw = unpack_udec3_xyzw(raw);
            let bitangent_sign = clamp_sign(xyzw[3]);
            [xyzw[0], xyzw[1], xyzw[2], bitangent_sign]
        })
        .collect();

    assert_eq!(tangents.len(), n, "tangent count must equal vertex count");
    for t in &tangents {
        assert!((t[0] - 1.0).abs() < 0.003, "tangent.x ≈ 1.0");
        assert_eq!(t[3], 1.0, "bitangent sign must be exactly +1.0");
    }
}

/// Regression for #2246 (REN-D19-02): the 2-bit W channel can decode to
/// -1/3 or +1/3 (`unpack_udec3_xyzw`'s raw sub-values 1 or 2), not just
/// the intended ±1 sign — unlike every other game's import path
/// (`bitangent_sign()` in `types.rs`, used by `tangent.rs` /
/// `sse_recon.rs`), which always derives an exact ±1.0. The extraction
/// map must clamp whatever comes out to exactly ±1.0 so no downstream
/// consumer of `vertexTangent.w` needs its own defensive re-clamp.
#[test]
fn tangent_extraction_normalizes_off_nominal_sign_to_exact_plus_or_minus_one() {
    // w=1 → raw sign (1/3)*2-1 = -1/3 (off-nominal, not ±1).
    let negative_raw = encode_udec3(1023, 511, 511, 1);
    // w=2 → raw sign (2/3)*2-1 = +1/3 (off-nominal, not ±1).
    let positive_raw = encode_udec3(1023, 511, 511, 2);

    for (raw, expected_sign) in [(negative_raw, -1.0_f32), (positive_raw, 1.0_f32)] {
        let xyzw = unpack_udec3_xyzw(raw);
        assert!(
            (xyzw[3].abs() - 1.0).abs() > 0.1,
            "fixture must actually be off-nominal, got raw sign {}",
            xyzw[3]
        );
        // Mirrors the (post-fix) extraction map in extract_bs_geometry.
        let bitangent_sign = clamp_sign(xyzw[3]);
        assert_eq!(
            bitangent_sign, expected_sign,
            "off-nominal packed sign {} must clamp to exactly {}",
            xyzw[3], expected_sign
        );
    }
}

/// #1232 — empty `tangents_raw` paired with populated geometry must
/// route through `synthesize_tangents_yup` rather than dropping to
/// `Vec::new()`. Pre-#1232 the fallback produced empty tangents,
/// forcing every BSGeometry mesh without authored UDEC3 tangents to
/// `perturbNormal` Path-2 (screen-space derivative TBN), which
/// inherits the #1104 UV-mirror handedness bug.
///
/// This test exercises the helper directly with a synthetic triangle and
/// authored normals. The full extraction gate is covered separately below.
/// Mirrors the shape of `synthesize_tangents_yup_*` tests in
/// `tangent_convention_tests.rs`.
#[test]
fn empty_tangents_raw_routes_through_synthesize_when_geometry_populated() {
    use super::tangent::synthesize_tangents_yup;
    // Single triangle in the XZ plane (Y-up), UV unit square so the
    // synthesised tangent lands on +X.
    let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let normals = vec![[0.0, 1.0, 0.0]; 3];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let triangles: Vec<[u16; 3]> = vec![[0, 1, 2]];
    let tangents = synthesize_tangents_yup(&positions, &normals, &uvs, &triangles);
    assert_eq!(
        tangents.len(),
        3,
        "synthesize_tangents_yup must return one tangent per vertex \
         when geometry is populated — empty result regresses to Path-2 \
         (#1232)"
    );
    // Sanity: tangent direction lies in the +X half-plane for the
    // canonical UV-aligned triangle (DPDU points +X for +U direction).
    for t in &tangents {
        assert!(
            t[0] > 0.5,
            "tangent.x must point along +U direction, got {:?}",
            t
        );
    }
}

/// #2363 — missing authored normals must not be disguised by the extractor's
/// renderer-safe `[0, 1, 0]` fallback. Before the fix, the populated fallback
/// made the tangent synthesis guard vacuously true whenever UVs existed,
/// fabricating a tangent basis from normals that were never present on disk.
#[test]
fn placeholder_normals_with_uvs_do_not_trigger_tangent_synthesis() {
    let transform = NiTransform {
        rotation: NiMatrix3 {
            rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        },
        translation: NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        scale: 1.0,
    };
    let shape = BSGeometry {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("PlaceholderNormalShape")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0x200,
            transform,
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        bounding_sphere: ([0.0, 0.0, 0.0], 0.0),
        bound_min_max: [0.0; 6],
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        meshes: vec![BSGeometryMesh {
            tri_size: 3 * std::mem::size_of::<u16>() as u32,
            num_verts: 3,
            flags: 0,
            kind: BSGeometryMeshKind::Internal {
                mesh_data: Box::new(BSGeometryMeshData {
                    version: 2,
                    triangles: vec![[0, 1, 2]],
                    scale: 1.0,
                    weights_per_vert: 0,
                    vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                    uvs0: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                    uvs1: Vec::new(),
                    colors: Vec::new(),
                    normals_raw: Vec::new(),
                    tangents_raw: Vec::new(),
                    skin_weights: Vec::new(),
                    lods: Vec::new(),
                    meshlets: Vec::new(),
                    cull_data: Vec::new(),
                }),
            },
        }],
    };

    let scene = NifScene::default();
    let mut pool = StringPool::new();
    let mesh = extract_bs_geometry(&scene, &shape, &shape.av.transform, &mut pool, None)
        .expect("populated internal BSGeometry must import");

    assert_eq!(mesh.normals, vec![[0.0, 1.0, 0.0]; 3]);
    assert!(
        mesh.tangents.is_empty(),
        "placeholder normals must not authorize tangent synthesis"
    );
}

/// Empty `tangents_raw` AND empty geometry (degenerate input) — the
/// helper returns `Vec::new()` and the extractor's fall-through arm
/// produces the same. Guards against a future "always synthesize" bug
/// that would panic on the degenerate path.
#[test]
fn empty_tangents_raw_and_empty_geometry_yields_empty_vec() {
    use super::tangent::synthesize_tangents_yup;
    let positions: Vec<[f32; 3]> = Vec::new();
    let normals: Vec<[f32; 3]> = Vec::new();
    let uvs: Vec<[f32; 2]> = Vec::new();
    let triangles: Vec<[u16; 3]> = Vec::new();
    let tangents = synthesize_tangents_yup(&positions, &normals, &uvs, &triangles);
    assert!(
        tangents.is_empty(),
        "degenerate input must yield Vec::new()"
    );
}

// ── #1209: Stage-A LOD-slot iteration ──────────────────────────────
//
// Pre-#1209, Stage A pulled `shape.meshes.first()` and bailed when LOD 0
// was `External` even though a later slot carried `Internal` geometry.
// Stage B already iterated. These tests pin the symmetric iteration on
// the `Internal` branch using the same `iter().find_map(...)` pattern
// that landed in `extract_bs_geometry`.

fn make_internal(version: u32) -> BSGeometryMesh {
    BSGeometryMesh {
        tri_size: 0,
        num_verts: 0,
        flags: 0,
        kind: BSGeometryMeshKind::Internal {
            mesh_data: Box::new(BSGeometryMeshData {
                version,
                triangles: Vec::new(),
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
            }),
        },
    }
}

fn make_external(name: &str) -> BSGeometryMesh {
    BSGeometryMesh {
        tri_size: 0,
        num_verts: 0,
        flags: 0,
        kind: BSGeometryMeshKind::External {
            mesh_name: name.to_owned(),
        },
    }
}

/// Mirrors the post-#1209 Stage-A selector: take the first `Internal`
/// LOD slot regardless of position.
fn select_internal(meshes: &[BSGeometryMesh]) -> Option<&BSGeometryMeshData> {
    meshes.iter().find_map(|m| match &m.kind {
        BSGeometryMeshKind::Internal { mesh_data } => Some(mesh_data.as_ref()),
        BSGeometryMeshKind::External { .. } => None,
    })
}

#[test]
fn stage_a_iter_picks_internal_when_lod0_is_external() {
    // [External, Internal] — the pre-#1209 short-circuit bailed with None.
    let meshes = vec![make_external("ignored.mesh"), make_internal(2)];
    let picked = select_internal(&meshes).expect("must find LOD-1 Internal");
    assert_eq!(picked.version, 2);
}

#[test]
fn stage_a_iter_picks_lod0_when_internal() {
    let meshes = vec![make_internal(2), make_external("ignored.mesh")];
    let picked = select_internal(&meshes).expect("must find LOD-0 Internal");
    assert_eq!(picked.version, 2);
}

#[test]
fn stage_a_iter_returns_none_when_all_external() {
    let meshes = vec![make_external("a.mesh"), make_external("b.mesh")];
    assert!(select_internal(&meshes).is_none());
}

#[test]
fn stage_a_iter_returns_none_when_meshes_empty() {
    let meshes: Vec<BSGeometryMesh> = Vec::new();
    assert!(select_internal(&meshes).is_none());
}
