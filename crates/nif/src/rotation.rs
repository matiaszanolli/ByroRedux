//! Rotation matrix sanitization.
//!
//! Gamebryo NIF files occasionally contain degenerate rotation matrices
//! (rank-deficient, det≈0, or sheared/scaled) from bad exports or zeroed
//! BSFadeNode transforms. We repair these ONCE at parse time via SVD
//! ("nearest orthogonal matrix"), so downstream code (compose_transforms,
//! zup_matrix_to_yup_quat) can skip per-composition checks. See #277.

use crate::types::NiMatrix3;

/// Check if a rotation matrix is degenerate (det far from 1.0).
#[inline]
pub fn is_degenerate_rotation(m: &NiMatrix3) -> bool {
    let r = &m.rows;
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
        - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
        + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    (det - 1.0).abs() >= 0.1
}

/// SVD-repair a degenerate rotation matrix, or return identity if the matrix
/// has no meaningful orientation (all singular values near zero).
pub fn repair_rotation_svd_or_identity(m: &NiMatrix3) -> NiMatrix3 {
    use nalgebra::Matrix3;

    let r = &m.rows;
    let mat = Matrix3::new(
        r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2],
    );

    let svd = mat.svd(true, true);

    let max_sv = svd.singular_values.max();
    if max_sv < 0.01 {
        return NiMatrix3::default();
    }

    let u = svd.u.unwrap();
    let vt = svd.v_t.unwrap();
    let mut nearest = u * vt;

    if nearest.determinant() < 0.0 {
        let mut u_fixed = u;
        u_fixed.column_mut(2).scale_mut(-1.0);
        nearest = u_fixed * vt;
    }

    NiMatrix3 {
        rows: [
            [nearest[(0, 0)], nearest[(0, 1)], nearest[(0, 2)]],
            [nearest[(1, 0)], nearest[(1, 1)], nearest[(1, 2)]],
            [nearest[(2, 0)], nearest[(2, 1)], nearest[(2, 2)]],
        ],
    }
}

/// Sanitize a rotation matrix: pass-through for valid rotations (~99.9% of
/// NIF content), SVD-repair for degenerate ones. Call once at parse time
/// so downstream code can assume the matrix is a valid rotation.
#[inline]
pub fn sanitize_rotation(m: NiMatrix3) -> NiMatrix3 {
    if is_degenerate_rotation(&m) {
        repair_rotation_svd_or_identity(&m)
    } else {
        m
    }
}
