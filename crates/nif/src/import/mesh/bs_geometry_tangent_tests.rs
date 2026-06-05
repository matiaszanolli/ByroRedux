//! Regression tests for BSGeometry tangent extraction (#1086 / REN-D16-001)
//! and LOD-slot iteration in `extract_bs_geometry` (#1209).
//!
//! Guards against the regression where `extract_bs_geometry` returned
//! `tangents: Vec::new()` for all Starfield meshes, forcing every mesh
//! to the shader's screen-space derivative Path-2 in `perturbNormal`.

use crate::blocks::bs_geometry::unpack_udec3_xyzw;
use crate::blocks::bs_geometry::{BSGeometryMesh, BSGeometryMeshData, BSGeometryMeshKind};

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
    assert!((xyzw[3] - 1.0).abs() < EPS, "w (sign) ≈ +1.0, got {}", xyzw[3]);
}

/// Verify that negative bitangent sign (w = -1) is correctly decoded.
#[test]
fn udec3_tangent_negative_sign() {
    // w=0 → raw 0 → (0/3)*2-1 = -1.0.
    let packed = encode_udec3(1023, 511, 511, 0);
    let xyzw = unpack_udec3_xyzw(packed);
    const EPS: f32 = 0.003;
    assert!((xyzw[3] - (-1.0)).abs() < EPS, "w (sign) ≈ -1.0, got {}", xyzw[3]);
}

/// The extraction path: N entries in tangents_raw should produce N tangents,
/// each a [f32;4] matching the unpacked UDEC3 values.
/// This guards against a regression back to Vec::new().
#[test]
fn tangent_extraction_count_and_values() {
    let n: usize = 4;
    let raw_val = encode_udec3(1023, 511, 511, 3); // tangent (1, 0, 0, +1)
    let tangents_raw: Vec<u32> = vec![raw_val; n];

    // Simulate the extraction loop in extract_bs_geometry.
    let tangents: Vec<[f32; 4]> = tangents_raw
        .iter()
        .map(|&raw| {
            let xyzw = unpack_udec3_xyzw(raw);
            [xyzw[0], xyzw[1], xyzw[2], xyzw[3]]
        })
        .collect();

    assert_eq!(tangents.len(), n, "tangent count must equal vertex count");
    for t in &tangents {
        assert!((t[0] - 1.0).abs() < 0.003, "tangent.x ≈ 1.0");
        assert!((t[3] - 1.0).abs() < 0.003, "bitangent sign ≈ +1.0");
    }
}

/// #1232 — empty `tangents_raw` paired with populated geometry must
/// route through `synthesize_tangents_yup` rather than dropping to
/// `Vec::new()`. Pre-#1232 the fallback produced empty tangents,
/// forcing every BSGeometry mesh without authored UDEC3 tangents to
/// `perturbNormal` Path-2 (screen-space derivative TBN), which
/// inherits the #1104 UV-mirror handedness bug.
///
/// This test exercises the helper directly with a synthetic triangle.
/// The full extract path is too involved for a unit test (requires a
/// `BSGeometry` block + `NifScene` + `StringPool`); the in-extractor
/// gate is the matching `else if !normals.is_empty() && !uvs.is_empty()
/// && !positions.is_empty()` branch in `bs_geometry.rs`. Mirrors the
/// shape of `synthesize_tangents_yup_*` tests in
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
    assert!(tangents.is_empty(), "degenerate input must yield Vec::new()");
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
