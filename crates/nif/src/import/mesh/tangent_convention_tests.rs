//! Regression tests for #786 / R-N2 — `synthesize_tangents` and
//! `extract_tangents_from_extra_data` must store `Vertex.tangent.xyz`
//! as `∂P/∂U` (textbook Lengyel convention) so the renderer's
//! `mat3(T, B, N) * tangentNormal` evaluates `tangentNormal.x` along
//! the texture U axis.
//!
//! Pre-#786 our import ported nifly's swap verbatim and stored
//! `∂P/∂V` in the tangent slot, mismatching the shader's standard-
//! convention TBN construction and producing the chrome-walls
//! regression on FNV `GSDocMitchellHouse` (DBG_VIZ_TANGENT confirmed
//! Path 1 firing on chrome fragments — the swap-induced 90° rotation
//! of the normal-map basis).

use super::{extract_tangents_from_extra_data, synthesize_tangents, synthesize_tangents_yup};
use crate::blocks::extra_data::NiExtraData;
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3};

/// Construct a triangle in the XY plane (Z-up) with an explicit UV
/// mapping where `U = X` and `V = Y`, so that:
///   - `∂P/∂U = (1, 0, 0)` in Z-up → `(1, 0, 0)` in Y-up
///     (the X axis is unchanged by the Z-up → Y-up swap)
///   - `∂P/∂V = (0, 1, 0)` in Z-up → `(0, 0, -1)` in Y-up
///
/// The vertex normal is the +Z axis (Z-up) → +Y axis (Y-up).
///
/// This is a 1-triangle fixture chosen so the `tangent_yup`
/// computation reduces to copying axis-aligned vectors and the
/// expected output has no floating-point ambiguity. A pre-#786
/// build of `synthesize_tangents` returns `(0, 0, -1)` for the
/// tangent (= ∂P/∂V); a fixed build returns `(1, 0, 0)` (= ∂P/∂U).
#[test]
fn synthesize_tangents_stores_dpdu_not_dpdv() {
    let vertices = vec![
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
    ];
    let normals = vec![
        NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 1.0
        };
        3
    ];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let triangles = vec![[0u16, 1u16, 2u16]];

    let out = synthesize_tangents(&vertices, &normals, &uvs, &triangles);
    assert_eq!(out.len(), 3, "one tangent per vertex");

    for (i, t) in out.iter().enumerate() {
        // ∂P/∂U in Z-up is (1,0,0); the (x,y,z) → (x,z,-y) swap
        // leaves it at (1,0,0) in Y-up.
        assert!(
            (t[0] - 1.0).abs() < 1e-5,
            "vertex {i} tangent.x = {} expected 1.0 (∂P/∂U), \
             not 0.0 (∂P/∂V)",
            t[0]
        );
        assert!(
            t[1].abs() < 1e-5,
            "vertex {i} tangent.y = {} expected 0",
            t[1]
        );
        assert!(
            t[2].abs() < 1e-5,
            "vertex {i} tangent.z = {} expected 0 (a -1 here \
             would mean we stored ∂P/∂V — the pre-#786 bug)",
            t[2]
        );
        // Right-handed mesh + standard convention → bitangent sign +1.
        // `cross(N=+Y, T=+X)` = -Z = `(0, 0, -1)` in Y-up which
        // equals `∂P/∂V` in Y-up coordinates → `dot(B, cross_nt) > 0`.
        assert!(
            (t[3] - 1.0).abs() < 1e-5,
            "vertex {i} bitangent_sign = {} expected +1 for \
             standard right-handed UV winding",
            t[3]
        );
    }
}

/// Mirror UV winding (V flipped) — `dt2 < 0` flips the determinant
/// sign in the per-triangle accumulator. The output tangent should
/// still be `∂P/∂U` after the sign correction `r = sign(det)` runs,
/// but the bitangent sign flips to -1 because the authored bitangent
/// (= ∂P/∂V_authored) now points opposite to `cross(N, T)`. This
/// pins both halves of the convention against the existing
/// post-Gram-Schmidt pipeline.
#[test]
fn synthesize_tangents_flips_bitangent_sign_on_mirrored_uvs() {
    let vertices = vec![
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
    ];
    let normals = vec![
        NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 1.0
        };
        3
    ];
    // V axis flipped: vertex (0,1,0) gets v = -1 instead of +1.
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [0.0, -1.0]];
    let triangles = vec![[0u16, 1u16, 2u16]];

    let out = synthesize_tangents(&vertices, &normals, &uvs, &triangles);
    assert_eq!(out.len(), 3);
    for (i, t) in out.iter().enumerate() {
        // Tangent magnitude still along ±X — the determinant sign
        // correction keeps the U-axis derivative pointing the same
        // way as the actual U axis.
        assert!(
            (t[0].abs() - 1.0).abs() < 1e-5,
            "vertex {i} tangent.x magnitude = {} expected 1",
            t[0]
        );
        // Bitangent sign flips on mirrored UVs.
        assert!(
            (t[3] + 1.0).abs() < 1e-5,
            "vertex {i} bitangent_sign = {} expected -1 for \
             V-flipped UV winding",
            t[3]
        );
    }
}

// ── #1204 — Y-up synthesis sibling for already-Y-up inputs ─────────
//
// `synthesize_tangents_yup` is the SSE-reconstructed BSTriShape /
// Starfield BSGeometry counterpart of `synthesize_tangents`. The same
// fixture as the Z-up flavour test, but the inputs ARE already in Y-up
// (renderer) space — the function must NOT apply a second Z-up→Y-up
// swap. Expected outputs are the Y-up image of the Z-up test.

/// Y-up positions: same triangle as the Z-up test after the swap.
/// Z-up (0,0,0), (1,0,0), (0,1,0) → Y-up (0,0,0), (1,0,0), (0,0,-1).
/// Normal +Y, UVs (U=X_yup, V=-Z_yup). Expected tangent = ∂P/∂U =
/// (1, 0, 0) Y-up; bitangent sign = +1.
#[test]
fn synthesize_tangents_yup_stores_dpdu_not_dpdv() {
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]];
    let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 3];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let triangles = vec![[0u16, 1u16, 2u16]];

    let out = synthesize_tangents_yup(&positions, &normals, &uvs, &triangles);
    assert_eq!(out.len(), 3);
    for (i, t) in out.iter().enumerate() {
        assert!(
            (t[0] - 1.0).abs() < 1e-5,
            "vertex {i} tangent.x = {} expected 1.0 (∂P/∂U in Y-up)",
            t[0]
        );
        assert!(t[1].abs() < 1e-5, "vertex {i} tangent.y = {}", t[1]);
        assert!(t[2].abs() < 1e-5, "vertex {i} tangent.z = {}", t[2]);
        assert!(
            (t[3] - 1.0).abs() < 1e-5,
            "vertex {i} bitangent_sign = {} expected +1",
            t[3]
        );
    }
}

#[test]
fn synthesize_tangents_yup_flips_bitangent_sign_on_mirrored_uvs() {
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]];
    let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 3];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [0.0, -1.0]];
    let triangles = vec![[0u16, 1u16, 2u16]];

    let out = synthesize_tangents_yup(&positions, &normals, &uvs, &triangles);
    assert_eq!(out.len(), 3);
    for (i, t) in out.iter().enumerate() {
        assert!(
            (t[0].abs() - 1.0).abs() < 1e-5,
            "vertex {i} tangent.x magnitude = {}",
            t[0]
        );
        assert!(
            (t[3] + 1.0).abs() < 1e-5,
            "vertex {i} bitangent_sign = {} expected -1",
            t[3]
        );
    }
}

#[test]
fn synthesize_tangents_yup_rejects_mismatched_inputs() {
    let positions: Vec<[f32; 3]> = vec![[0.0; 3]; 3];
    let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 2]; // mismatched length
    let uvs = vec![[0.0, 0.0]; 3];
    let triangles = vec![[0u16, 1u16, 2u16]];
    assert!(synthesize_tangents_yup(&positions, &normals, &uvs, &triangles).is_empty());

    let normals = vec![[0.0, 1.0, 0.0]; 3];
    let uvs = vec![[0.0, 0.0]; 2]; // mismatched length
    assert!(synthesize_tangents_yup(&positions, &normals, &uvs, &triangles).is_empty());
}

/// #2632 / SF2D2-D2-04 — the degenerate fallback (permute-N-components)
/// branch must produce a UNIT tangent that is ORTHOGONAL to N, even when
/// the input normal is non-unit-length (as a UDEC3-decoded Starfield
/// `BSGeometry` normal is, to quantization) and not axis-aligned (so the
/// permutation isn't already trivially orthogonal to N — this exercises
/// the real Gram-Schmidt projection, not a no-op).
///
/// Degenerate branch trigger: every vertex shares the identical UV, so
/// the per-triangle `sdir`/`tdir` UV-derivative accumulators are exactly
/// zero for every vertex (`vec3_is_zero` fires), regardless of vertex
/// positions.
#[test]
fn synthesize_tangents_yup_degenerate_fallback_normalizes_and_orthogonalizes_against_n() {
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.3]];
    // Non-unit (magnitude 5) and NOT axis-aligned, so a raw cyclic
    // permutation of its components is not already orthogonal to it —
    // pre-fix this would leave the tangent both non-unit AND
    // non-orthogonal to N.
    let raw_normal = [0.0f32, 4.0, 3.0];
    let normals: Vec<[f32; 3]> = vec![raw_normal; 3];
    // Identical UVs on every vertex → zero UV-derivative accumulation →
    // every vertex takes the degenerate fallback branch.
    let uvs = vec![[0.5, 0.5]; 3];
    let triangles = vec![[0u16, 1u16, 2u16]];

    let out = synthesize_tangents_yup(&positions, &normals, &uvs, &triangles);
    assert_eq!(out.len(), 3);

    // The same normalization the function must apply internally, computed
    // independently here for the orthogonality check below.
    let len = (raw_normal[0] * raw_normal[0]
        + raw_normal[1] * raw_normal[1]
        + raw_normal[2] * raw_normal[2])
        .sqrt();
    let n_unit = [
        raw_normal[0] / len,
        raw_normal[1] / len,
        raw_normal[2] / len,
    ];

    for (i, t) in out.iter().enumerate() {
        let tangent = [t[0], t[1], t[2]];
        let mag2 = tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2];
        assert!(
            (mag2 - 1.0).abs() < 1e-5,
            "vertex {i} degenerate-fallback tangent must be unit length, got |T|^2={mag2}"
        );
        let dot_nt = n_unit[0] * tangent[0] + n_unit[1] * tangent[1] + n_unit[2] * tangent[2];
        assert!(
            dot_nt.abs() < 1e-5,
            "vertex {i} degenerate-fallback tangent must be orthogonal to (normalized) N, \
             got dot(N, T)={dot_nt}"
        );
        assert!(
            (t[3].abs() - 1.0).abs() < 1e-5,
            "vertex {i} bitangent_sign must be exactly +-1, got {}",
            t[3]
        );
    }
}

/// Z-up counterpart of the #2632 regression guard above. The legacy NIF
/// producer must apply the same Gram-Schmidt projection after converting its
/// permuted normal into renderer space.
#[test]
fn synthesize_tangents_zup_degenerate_fallback_normalizes_and_orthogonalizes_against_n() {
    let vertices = vec![
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
            z: 0.3,
        },
    ];
    let raw_normal = NiPoint3 {
        x: 0.0,
        y: 0.8,
        z: 0.6,
    };
    let normals = vec![raw_normal; 3];
    let uvs = vec![[0.5, 0.5]; 3];
    let triangles = vec![[0u16, 1u16, 2u16]];
    let out = synthesize_tangents(&vertices, &normals, &uvs, &triangles);
    assert_eq!(out.len(), 3);

    // Z-up (x,y,z) maps to renderer Y-up (x,z,-y).
    let n_unit = [0.0f32, 0.6, -0.8];
    for (i, t) in out.iter().enumerate() {
        let mag2 = t[0] * t[0] + t[1] * t[1] + t[2] * t[2];
        assert!((mag2 - 1.0).abs() < 1e-5, "vertex {i} tangent not unit");
        let dot_nt = n_unit[0] * t[0] + n_unit[1] * t[1] + n_unit[2] * t[2];
        assert!(
            dot_nt.abs() < 1e-5,
            "vertex {i} tangent not orthogonal: {dot_nt}"
        );
        assert!((t[3].abs() - 1.0).abs() < 1e-5);
    }
}

/// #3176 — the degenerate fallback's primary seed is a cyclic permutation
/// of N, Gram-Schmidt-projected against N. For `N = (1,1,1)/sqrt(3)` that
/// permutation is N itself (the case the #2632 comment on the branch
/// names as its own motivation), so the projection removes the entire
/// vector and pre-fix the branch shipped a zero tangent. The fallback
/// seed must produce a unit, N-orthogonal tangent instead.
#[test]
fn synthesize_tangents_yup_degenerate_fallback_handles_all_equal_normal_components() {
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.3]];
    let c = 1.0f32 / 3.0f32.sqrt();
    let normals: Vec<[f32; 3]> = vec![[c, c, c]; 3];
    let uvs = vec![[0.5, 0.5]; 3]; // identical UVs → every vertex takes the degenerate branch
    let triangles = vec![[0u16, 1u16, 2u16]];

    let out = synthesize_tangents_yup(&positions, &normals, &uvs, &triangles);
    assert_eq!(out.len(), 3);

    for (i, t) in out.iter().enumerate() {
        let tangent = [t[0], t[1], t[2]];
        let mag2 = tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2];
        assert!(
            mag2 > 1e-5,
            "vertex {i} fallback tangent must be non-zero (pre-#3176 this was [0,0,0])"
        );
        assert!(
            (mag2 - 1.0).abs() < 1e-5,
            "vertex {i} fallback tangent must be unit length, got |T|^2={mag2}"
        );
        let dot_nt = c * tangent[0] + c * tangent[1] + c * tangent[2];
        assert!(
            dot_nt.abs() < 1e-5,
            "vertex {i} fallback tangent must be orthogonal to N, got dot(N, T)={dot_nt}"
        );
    }
}

/// Z-up counterpart of the #3176 regression guard above.
#[test]
fn synthesize_tangents_zup_degenerate_fallback_handles_all_equal_normal_components() {
    let vertices = vec![
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
            z: 0.3,
        },
    ];
    let c = 1.0f32 / 3.0f32.sqrt();
    let normals = vec![NiPoint3 { x: c, y: c, z: c }; 3];
    let uvs = vec![[0.5, 0.5]; 3];
    let triangles = vec![[0u16, 1u16, 2u16]];

    let out = synthesize_tangents(&vertices, &normals, &uvs, &triangles);
    assert_eq!(out.len(), 3);

    // Z-up (x,y,z) → Y-up (x,z,-y): (c,c,c) → (c,c,-c), still all-equal
    // in magnitude, so the Z-up permutation self-collapses too.
    let n_unit = [c, c, -c];
    for (i, t) in out.iter().enumerate() {
        let mag2 = t[0] * t[0] + t[1] * t[1] + t[2] * t[2];
        assert!(
            mag2 > 1e-5,
            "vertex {i} fallback tangent must be non-zero (pre-#3176 this was [0,0,0])"
        );
        assert!((mag2 - 1.0).abs() < 1e-5, "vertex {i} tangent not unit");
        let dot_nt = n_unit[0] * t[0] + n_unit[1] * t[1] + n_unit[2] * t[2];
        assert!(
            dot_nt.abs() < 1e-5,
            "vertex {i} fallback tangent must be orthogonal to N, got dot(N, T)={dot_nt}"
        );
    }
}

/// #3177 — `synthesize_tangents`'s Z-up producer receives `BSTriShape`
/// normbyte normals (`byte_to_normal`), which are unit-length only to
/// quantization. Pre-fix the ordinary (non-degenerate) branch's
/// Gram-Schmidt projection used the raw, non-unit N directly instead of
/// normalizing it first (unlike its `synthesize_tangents_yup` #2632
/// sibling), leaving the emitted tangent measurably non-orthogonal to
/// the real (normalized) shading normal.
///
/// `raw_normal` is deliberately far off unit (magnitude 5, not
/// axis-aligned with the UV-derived tangent) so the orthogonality
/// violation is unambiguous in floating point — real normbyte
/// quantization error is much smaller (~1.5%) but exercises the exact
/// same code path.
#[test]
fn synthesize_tangents_zup_normalizes_nonunit_normal_before_orthogonalizing() {
    let vertices = vec![
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
    ];
    let raw_normal = NiPoint3 {
        x: 3.0,
        y: 4.0,
        z: 0.0,
    };
    let normals = vec![raw_normal; 3];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]; // distinct UVs → ordinary branch
    let triangles = vec![[0u16, 1u16, 2u16]];

    let out = synthesize_tangents(&vertices, &normals, &uvs, &triangles);
    assert_eq!(out.len(), 3);

    // Z-up (3,4,0) → Y-up (3,0,-4), normalized → (0.6, 0, -0.8).
    let n_unit = [0.6f32, 0.0, -0.8];
    for (i, t) in out.iter().enumerate() {
        let dot_nt = n_unit[0] * t[0] + n_unit[1] * t[1] + n_unit[2] * t[2];
        assert!(
            dot_nt.abs() < 1e-4,
            "vertex {i} tangent must be orthogonal to the NORMALIZED normal, \
             got dot(N_unit, T)={dot_nt} (pre-#3177 the projection used the \
             raw non-unit N)"
        );
    }
}

#[test]
fn synthesize_tangents_yup_empty_inputs_return_empty() {
    let empty_positions: Vec<[f32; 3]> = Vec::new();
    let empty_normals: Vec<[f32; 3]> = Vec::new();
    let empty_uvs: Vec<[f32; 2]> = Vec::new();
    let empty_triangles: Vec<[u16; 3]> = Vec::new();
    assert!(synthesize_tangents_yup(
        &empty_positions,
        &empty_normals,
        &empty_uvs,
        &empty_triangles
    )
    .is_empty());
}

// ── #2818 (REN-D19-06) — `extract_tangents_from_extra_data` coverage ──
//
// The load-bearing #786 half-swap (Bethesda's on-disk "tangents" field
// holds ∂P/∂V, "bitangents" holds ∂P/∂U — our decoder reads the SECOND
// 12-byte half into `Vertex.tangent.xyz`) had no direct test coverage
// before this file: only the downstream `synthesize_tangents` fallback
// was pinned. These tests exercise the extractor itself: the half-swap,
// the exact-size gate, the extra-data name match, and the Z-up → Y-up
// conversion applied to both halves.

/// Build a `NiBinaryExtraData` block with the canonical tangent-space
/// name and a caller-supplied blob, registered as block 0 of a fresh
/// scene. Returns the scene plus the `extra_data_refs` slice pointing
/// at it, ready to hand to `extract_tangents_from_extra_data`.
fn scene_with_tangent_blob(name: &str, type_name: &str, binary_data: Option<Vec<u8>>) -> NifScene {
    let extra = NiExtraData {
        type_name: type_name.to_string(),
        name: Some(std::sync::Arc::from(name)),
        string_value: None,
        integer_value: None,
        float_value: None,
        binary_data,
        strings_array: None,
        integers_array: None,
        floats_array: None,
        bone_lods: None,
        skin_attach_bones: None,
        bone_translations: None,
    };
    let mut scene = NifScene::default();
    scene.blocks.push(Box::new(extra));
    scene
}

/// The exact canonical name `extract_tangents_from_extra_data` matches
/// against — kept as a constant so a future rename of the fixture can't
/// silently drift from the production string.
const TANGENT_SPACE_NAME: &str = "Tangent space (binormal & tangent vectors)";

/// Pack two vertices' worth of Bethesda-layout tangent-space extra
/// data: `[v0_tangent, v1_tangent, v0_bitangent, v1_bitangent]`, each
/// a 12-byte Z-up `Vector3` — the on-disk layout per nifly's
/// `Geometry.cpp:81-84` (`[tangents..., bitangents...]`).
fn pack_bethesda_tangent_blob(tangents: &[[f32; 3]], bitangents: &[[f32; 3]]) -> Vec<u8> {
    let mut blob = Vec::with_capacity((tangents.len() + bitangents.len()) * 12);
    for [x, y, z] in tangents {
        blob.extend_from_slice(&x.to_le_bytes());
        blob.extend_from_slice(&y.to_le_bytes());
        blob.extend_from_slice(&z.to_le_bytes());
    }
    for [x, y, z] in bitangents {
        blob.extend_from_slice(&x.to_le_bytes());
        blob.extend_from_slice(&y.to_le_bytes());
        blob.extend_from_slice(&z.to_le_bytes());
    }
    blob
}

/// #786 half-swap + Z-up → Y-up conversion, exercised end-to-end
/// through `extract_tangents_from_extra_data` with two vertices (so
/// the per-vertex offset math — `i * 12` for the tangent half,
/// `num_verts * 12 + i * 12` for the bitangent half — is pinned, not
/// just the single-vertex degenerate case).
///
/// Vertex 0: on-disk tangent (∂P/∂V, Z-up) = (0, 1, 0); on-disk
/// bitangent (∂P/∂U, Z-up) = (1, 0, 0).
/// Vertex 1: on-disk tangent = (0, -1, 0); on-disk bitangent = (-1, 0, 0).
/// Both vertex normals point Z-up +Z (straight up).
///
/// `Vertex.tangent.xyz` must equal `zup_to_yup(bitangent)` — the
/// SECOND half, not the first — and the bitangent-sign convention
/// (`sign(dot(B, cross(N, T)))`) must land on the on-disk tangent half
/// converted the same way.
#[test]
fn extract_tangents_from_extra_data_applies_bethesda_half_swap_and_zup_to_yup() {
    let normals_zup = [
        NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        NiPoint3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
    ];
    let blob = pack_bethesda_tangent_blob(
        &[[0.0, 1.0, 0.0], [0.0, -1.0, 0.0]],
        &[[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]],
    );
    assert_eq!(
        blob.len(),
        2 * 24,
        "fixture must be exactly num_verts * 24 bytes"
    );

    let scene = scene_with_tangent_blob(TANGENT_SPACE_NAME, "NiBinaryExtraData", Some(blob));
    let refs = [BlockRef(0)];
    let tangents = extract_tangents_from_extra_data(&scene, &refs, &normals_zup, 2);

    assert_eq!(tangents.len(), 2, "one tangent per vertex");
    // Vertex 0: bitangent half (1,0,0) Z-up → (1,0,0) Y-up (X axis is
    // identity under the swap); sign +1 for this right-handed fixture.
    assert!(
        (tangents[0][0] - 1.0).abs() < 1e-6,
        "v0.x = {}",
        tangents[0][0]
    );
    assert!(tangents[0][1].abs() < 1e-6, "v0.y = {}", tangents[0][1]);
    assert!(tangents[0][2].abs() < 1e-6, "v0.z = {}", tangents[0][2]);
    assert_eq!(tangents[0][3], 1.0, "v0 sign");
    // Vertex 1: bitangent half (-1,0,0) Z-up → (-1,0,0) Y-up. A
    // different offset than vertex 0 proves the `num_verts * 12`
    // second-half stride, not a stale first-vertex read repeated.
    assert!(
        (tangents[1][0] + 1.0).abs() < 1e-6,
        "v1.x = {}",
        tangents[1][0]
    );
    assert!(tangents[1][1].abs() < 1e-6, "v1.y = {}", tangents[1][1]);
    assert!(tangents[1][2].abs() < 1e-6, "v1.z = {}", tangents[1][2]);
    assert_eq!(tangents[1][3], 1.0, "v1 sign");
}

/// The `blob.len() != num_verts * 24` size gate must skip the blob
/// (log + `continue`) rather than risk a partial/misaligned decode —
/// the caller falls back to `synthesize_tangents`.
#[test]
fn extract_tangents_from_extra_data_size_mismatch_returns_empty() {
    let normals_zup = [NiPoint3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    }];
    // One byte short of the required 24 for num_verts = 1.
    let blob = vec![0u8; 23];
    let scene = scene_with_tangent_blob(TANGENT_SPACE_NAME, "NiBinaryExtraData", Some(blob));
    let refs = [BlockRef(0)];
    let tangents = extract_tangents_from_extra_data(&scene, &refs, &normals_zup, 1);
    assert!(
        tangents.is_empty(),
        "size-mismatched blob must be skipped, not partially decoded"
    );
}

/// A `NiBinaryExtraData` block whose `name` doesn't exactly match
/// `"Tangent space (binormal & tangent vectors)"` must be skipped —
/// the function has no fuzzy-match fallback.
#[test]
fn extract_tangents_from_extra_data_wrong_name_returns_empty() {
    let normals_zup = [NiPoint3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    }];
    let blob = pack_bethesda_tangent_blob(&[[0.0, 1.0, 0.0]], &[[1.0, 0.0, 0.0]]);
    let scene = scene_with_tangent_blob("Some other extra data", "NiBinaryExtraData", Some(blob));
    let refs = [BlockRef(0)];
    let tangents = extract_tangents_from_extra_data(&scene, &refs, &normals_zup, 1);
    assert!(
        tangents.is_empty(),
        "non-matching extra-data name must be skipped"
    );
}

/// A block with the right name but the wrong `type_name` (not
/// `NiBinaryExtraData`) must also be skipped — the name check alone
/// isn't sufficient.
#[test]
fn extract_tangents_from_extra_data_wrong_type_name_returns_empty() {
    let normals_zup = [NiPoint3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    }];
    let blob = pack_bethesda_tangent_blob(&[[0.0, 1.0, 0.0]], &[[1.0, 0.0, 0.0]]);
    let scene = scene_with_tangent_blob(TANGENT_SPACE_NAME, "NiStringExtraData", Some(blob));
    let refs = [BlockRef(0)];
    let tangents = extract_tangents_from_extra_data(&scene, &refs, &normals_zup, 1);
    assert!(
        tangents.is_empty(),
        "non-NiBinaryExtraData type must be skipped even with a matching name"
    );
}

/// `num_verts == 0` must short-circuit before any block lookup.
#[test]
fn extract_tangents_from_extra_data_zero_num_verts_returns_empty() {
    let scene = scene_with_tangent_blob(TANGENT_SPACE_NAME, "NiBinaryExtraData", Some(Vec::new()));
    let refs = [BlockRef(0)];
    let tangents = extract_tangents_from_extra_data(&scene, &refs, &[], 0);
    assert!(tangents.is_empty());
}
