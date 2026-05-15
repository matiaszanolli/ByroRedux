//! Z-up (Gamebryo) to Y-up (renderer) coordinate conversion — NIF
//! flavour. Array-form primitives live in
//! [`byroredux_core::math::coord`]; this file wraps them with the
//! NIF-internal types (`NiPoint3` / `NiMatrix3`) the import path
//! works in. The matrix-path Shepperd + SVD repair stays here because
//! it depends on NIF types and isn't shared with any other consumer.
//! See #1044 / TD3-002 for the consolidation.

use crate::types::{NiMatrix3, NiPoint3};

/// Convert a Z-up `NiPoint3` to Y-up `[x, y, z]`: `(x, y, z) → (x, z, -y)`.
///
/// Thin wrapper over [`byroredux_core::math::coord::zup_to_yup_pos`].
/// Applied at every import boundary (mesh vertices, mesh normals, node
/// translations, bound centers, bone translations, light positions, …).
/// Pre-#232 the same `[.x, .z, -.y]` literal appeared in ~13 sites
/// across `mesh.rs` and `walk.rs`; pre-#1044 the array-form sibling
/// in `crates/nif/src/anim/coord.rs` was a copy. Both now route here.
#[inline]
pub(super) fn zup_point_to_yup(p: &NiPoint3) -> [f32; 3] {
    byroredux_core::math::coord::zup_to_yup_pos([p.x, p.y, p.z])
}

/// Convert a Z-up NiMatrix3 rotation to a Y-up quaternion [x, y, z, w].
///
/// Gamebryo uses a clockwise-positive rotation convention, so its rotation
/// matrices are the transpose of the standard (CCW) convention. However,
/// the matrix × point multiplication produces the SAME physical result
/// regardless of convention — the matrix IS the rotation. So we can
/// extract a quaternion directly from the NIF matrix without transposing.
///
/// Fast path (~99% of NIF matrices): hand-rolled Shepperd method for
/// quaternion extraction (~20 FLOPs, no nalgebra). Falls back to nalgebra
/// SVD only for degenerate matrices (rank-deficient, det≈0).
pub(super) fn zup_matrix_to_yup_quat(m: &NiMatrix3) -> [f32; 4] {
    let r = &m.rows;

    // Apply the Z-up → Y-up axis swap to the rotation matrix:
    // C: (x,y,z)_zup → (x,z,-y)_yup
    // R_yup = C * R_zup * C^T
    let yup = [
        [r[0][0], r[0][2], -r[0][1]],  // X row, columns swapped
        [r[2][0], r[2][2], -r[2][1]],  // Z row becomes Y row
        [-r[1][0], -r[1][2], r[1][1]], // -Y row becomes Z row
    ];

    // Determinant — same formula as rotation::is_degenerate_rotation.
    let det = yup[0][0] * (yup[1][1] * yup[2][2] - yup[1][2] * yup[2][1])
        - yup[0][1] * (yup[1][0] * yup[2][2] - yup[1][2] * yup[2][0])
        + yup[0][2] * (yup[1][0] * yup[2][1] - yup[1][1] * yup[2][0]);

    if (det - 1.0).abs() < 0.1 {
        // Fast path: valid rotation matrix — Shepperd method for quaternion
        // extraction. Numerically stable for all rotation angles.
        // Reference: Shepperd, "Quaternion from Rotation Matrix", JGCD 1978.
        matrix3_to_quat(&yup)
    } else {
        // Degenerate — SVD repair via nalgebra.
        svd_repair_to_quat(&yup)
    }
}

/// Shepperd method: extract a unit quaternion from a 3×3 rotation matrix.
///
/// Picks the largest diagonal element to avoid division by near-zero,
/// ensuring numerical stability for all rotation angles. ~20 FLOPs total.
///
/// Shepperd's formula only produces a unit quaternion when the input is
/// a proper rotation; the fast-path gate in `zup_matrix_to_yup_quat`
/// admits matrices with determinant in ~[0.93, 1.07] (scaled-by-drift
/// rotations from export-tool quirks or hand-authored content), so the
/// raw output can be up to ~3.5% off unity. Downstream consumers build
/// `glam::Quat::from_xyzw` without normalising, which would propagate
/// a shear/scale error into the ECS Transform rotation. Normalise here
/// so the invariant holds regardless of input drift. See #333.
fn matrix3_to_quat(m: &[[f32; 3]; 3]) -> [f32; 4] {
    let trace = m[0][0] + m[1][1] + m[2][2];

    let q = if trace > 0.0 {
        // w is largest
        let s = (trace + 1.0).sqrt();
        let w = s * 0.5;
        let inv = 0.5 / s;
        let x = (m[2][1] - m[1][2]) * inv;
        let y = (m[0][2] - m[2][0]) * inv;
        let z = (m[1][0] - m[0][1]) * inv;
        [x, y, z, w]
    } else if m[0][0] >= m[1][1] && m[0][0] >= m[2][2] {
        // x is largest
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt();
        let x = s * 0.5;
        let inv = 0.5 / s;
        let y = (m[0][1] + m[1][0]) * inv;
        let z = (m[0][2] + m[2][0]) * inv;
        let w = (m[2][1] - m[1][2]) * inv;
        [x, y, z, w]
    } else if m[1][1] >= m[2][2] {
        // y is largest
        let s = (1.0 - m[0][0] + m[1][1] - m[2][2]).sqrt();
        let y = s * 0.5;
        let inv = 0.5 / s;
        let x = (m[0][1] + m[1][0]) * inv;
        let z = (m[1][2] + m[2][1]) * inv;
        let w = (m[0][2] - m[2][0]) * inv;
        [x, y, z, w]
    } else {
        // z is largest
        let s = (1.0 - m[0][0] - m[1][1] + m[2][2]).sqrt();
        let z = s * 0.5;
        let inv = 0.5 / s;
        let x = (m[0][2] + m[2][0]) * inv;
        let y = (m[1][2] + m[2][1]) * inv;
        let w = (m[1][0] - m[0][1]) * inv;
        [x, y, z, w]
    };

    byroredux_core::math::coord::normalize_quat(q)
}

/// SVD-repair a degenerate matrix and extract a quaternion.
/// Only called for ~1% of NIF matrices (zeroed BSFadeNode rotations, etc.).
fn svd_repair_to_quat(yup: &[[f32; 3]; 3]) -> [f32; 4] {
    use nalgebra::Matrix3;

    let mat = Matrix3::new(
        yup[0][0], yup[0][1], yup[0][2], yup[1][0], yup[1][1], yup[1][2], yup[2][0], yup[2][1],
        yup[2][2],
    );

    let svd = mat.svd(true, true);
    let u = svd.u.unwrap();
    let vt = svd.v_t.unwrap();
    let mut nearest = u * vt;

    if nearest.determinant() < 0.0 {
        let mut u_fixed = u;
        u_fixed.column_mut(2).scale_mut(-1.0);
        nearest = u_fixed * vt;
    }

    let repaired = [
        [nearest[(0, 0)], nearest[(0, 1)], nearest[(0, 2)]],
        [nearest[(1, 0)], nearest[(1, 1)], nearest[(1, 2)]],
        [nearest[(2, 0)], nearest[(2, 1)], nearest[(2, 2)]],
    ];
    matrix3_to_quat(&repaired)
}
