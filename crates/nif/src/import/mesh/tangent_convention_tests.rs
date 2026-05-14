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

use super::synthesize_tangents;
use crate::types::NiPoint3;

/// Construct a triangle in the XY plane (Z-up) with an explicit UV
/// mapping where `U = X` and `V = Y`, so that:
///   - `∂P/∂U = (1, 0, 0)` in Z-up → `(1, 0, 0)` in Y-up
///     (the X axis is unchanged by the Z-up → Y-up swap)
///   - `∂P/∂V = (0, 1, 0)` in Z-up → `(0, 0, -1)` in Y-up
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
