//! #2437 / COORD-4 cross-check: four independent copies of the Z-up →
//! Y-up rotation-similarity transform (`C · R · Cᵀ`), coupled only by
//! comments, not a shared function:
//!   - `coord::zup_matrix_to_yup_quat` (`NiMatrix3` → `[x,y,z,w]`)
//!   - `mesh::skin::ni_transform_to_yup_matrix` (`NiTransform` → 4×4
//!     column-major matrix)
//!   - `collision::havok_quat_to_engine` (raw quaternion → `Quat`)
//!   - `collision::decompose_havok_matrix` (row-major 4×4 → `(Vec3, Quat)`)
//!
//! All four are hand-verified correct today (per the issue's own
//! premise) — this is a regression guard, not a bugfix: a future change
//! to one copy (a handedness/determinant guard, say) has no mechanism
//! to propagate to the other three, and nothing previously caught that
//! divergence. Ground truth is computed independently of all four, via
//! `C · R · Cᵀ` using glam's own (trusted, well-tested) `Mat3` multiply
//! — `C` transcribed directly from `zup_matrix_to_yup_quat`'s own doc
//! comment, not re-derived.

use super::super::coord::zup_matrix_to_yup_quat;
use super::super::collision::{decompose_havok_matrix, havok_quat_to_engine};
use super::super::mesh::ni_transform_to_yup_matrix;
use crate::types::{NiMatrix3, NiPoint3, NiTransform};
use byroredux_core::math::{Mat3, Quat, Vec3};

/// Quaternions have a ±1 double-cover ambiguity for the same rotation —
/// compare the shorter of `|a-b|` / `|a+b|`.
fn quats_agree(a: Quat, b: Quat) -> bool {
    let same_sign = (a.x - b.x).abs() < 1e-4
        && (a.y - b.y).abs() < 1e-4
        && (a.z - b.z).abs() < 1e-4
        && (a.w - b.w).abs() < 1e-4;
    let flipped_sign = (a.x + b.x).abs() < 1e-4
        && (a.y + b.y).abs() < 1e-4
        && (a.z + b.z).abs() < 1e-4
        && (a.w + b.w).abs() < 1e-4;
    same_sign || flipped_sign
}

#[test]
fn all_four_zup_to_yup_rotation_paths_agree() {
    // An arbitrary, fully general (non-axis-aligned) rotation — stresses
    // multi-axis composition the way a permutation/axis-aligned matrix
    // would not.
    let r_zup: Mat3 = Mat3::from_axis_angle(Vec3::new(0.3, 0.5, 0.7).normalize(), 1.1);
    let rows_zup: [[f32; 3]; 3] = [
        r_zup.row(0).to_array(),
        r_zup.row(1).to_array(),
        r_zup.row(2).to_array(),
    ];

    // Ground truth: C · R · Cᵀ via glam's own trusted Mat3 multiply. `C`
    // transcribed verbatim from `zup_matrix_to_yup_quat`'s doc comment:
    // "C = [[1, 0, 0], [0, 0, 1], [0, -1, 0]]" (row-major).
    let c = Mat3::from_cols(
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(0.0, 1.0, 0.0),
    );
    let expected_yup = c * r_zup * c.transpose();
    let expected_quat = Quat::from_mat3(&expected_yup);

    // Path A — coord::zup_matrix_to_yup_quat (NiMatrix3 → [x,y,z,w]).
    let ni_mat = NiMatrix3 { rows: rows_zup };
    let [x, y, z, w] = zup_matrix_to_yup_quat(&ni_mat);
    let quat_a = Quat::from_xyzw(x, y, z, w);
    assert!(
        quats_agree(quat_a, expected_quat),
        "Path A (zup_matrix_to_yup_quat) diverged: got {quat_a:?}, expected {expected_quat:?}"
    );

    // Path B — mesh::skin::ni_transform_to_yup_matrix (NiTransform → 4×4
    // column-major). Identity translation/scale isolates the rotation.
    let transform = NiTransform {
        rotation: ni_mat,
        translation: NiPoint3 { x: 0.0, y: 0.0, z: 0.0 },
        scale: 1.0,
    };
    let mat4 = ni_transform_to_yup_matrix(&transform);
    // `mat4[j]` is column j (function's own "column-major 4x4" doc).
    let mat3_b = Mat3::from_cols(
        Vec3::new(mat4[0][0], mat4[0][1], mat4[0][2]),
        Vec3::new(mat4[1][0], mat4[1][1], mat4[1][2]),
        Vec3::new(mat4[2][0], mat4[2][1], mat4[2][2]),
    );
    let quat_b = Quat::from_mat3(&mat3_b);
    assert!(
        quats_agree(quat_b, expected_quat),
        "Path B (ni_transform_to_yup_matrix) diverged: got {quat_b:?}, expected {expected_quat:?}"
    );

    // Path C — collision::havok_quat_to_engine (raw Z-up quaternion →
    // Quat). Feed the SAME rotation's Z-up quaternion form (computed
    // independently via glam, not via any of the four functions under
    // test).
    let q_zup = Quat::from_mat3(&r_zup);
    let quat_c = havok_quat_to_engine([q_zup.x, q_zup.y, q_zup.z, q_zup.w]);
    assert!(
        quats_agree(quat_c, expected_quat),
        "Path C (havok_quat_to_engine) diverged: got {quat_c:?}, expected {expected_quat:?}"
    );

    // Path D — collision::decompose_havok_matrix (row-major 4×4 → Quat).
    // Havok's on-disk 4×4 (`BhkTransformShape::parse`, read row-by-row
    // straight off the stream) is a ROW-VECTOR-convention matrix
    // (`v' = v * M`), the transpose of the COLUMN-vector convention
    // (`v' = M * v`) `rows_zup` / `zup_matrix_to_yup_quat` use — so the
    // matching raw input here is `rows_zupᵀ`, not `rows_zup` itself.
    // `m[3]` carries translation (zeroed to isolate rotation).
    let havok_m: [[f32; 4]; 4] = [
        [rows_zup[0][0], rows_zup[1][0], rows_zup[2][0], 0.0],
        [rows_zup[0][1], rows_zup[1][1], rows_zup[2][1], 0.0],
        [rows_zup[0][2], rows_zup[1][2], rows_zup[2][2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let (_translation, quat_d) = decompose_havok_matrix(&havok_m, 1.0);
    assert!(
        quats_agree(quat_d, expected_quat),
        "Path D (decompose_havok_matrix) diverged: got {quat_d:?}, expected {expected_quat:?}"
    );
}
