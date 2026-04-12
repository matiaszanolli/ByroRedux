//! Z-up (Gamebryo) to Y-up (renderer) coordinate conversion.

use crate::types::NiMatrix3;

/// Convert a Z-up NiMatrix3 rotation to a Y-up quaternion [x, y, z, w].
///
/// Gamebryo uses a clockwise-positive rotation convention, so its rotation
/// matrices are the transpose of the standard (CCW) convention. However,
/// the matrix × point multiplication produces the SAME physical result
/// regardless of convention — the matrix IS the rotation. So we can
/// extract a quaternion directly from the NIF matrix without transposing.
///
/// Uses SVD decomposition (via nalgebra) to handle degenerate matrices
/// that Gamebryo NIF files sometimes contain (rank-deficient, det=0).
/// The nearest valid rotation matrix is extracted as U*Vt from the SVD,
/// then the Z-up → Y-up coordinate change is applied.
pub(super) fn zup_matrix_to_yup_quat(m: &NiMatrix3) -> [f32; 4] {
    use nalgebra::{Matrix3, UnitQuaternion};

    let r = &m.rows;

    // Apply the Z-up → Y-up axis swap to the rotation matrix:
    // C: (x,y,z)_zup → (x,z,-y)_yup
    // R_yup = C * R_zup * C^T
    let yup = Matrix3::new(
        r[0][0], r[0][2], -r[0][1], // X row, columns swapped
        r[2][0], r[2][2], -r[2][1], // Z row becomes Y row
        -r[1][0], -r[1][2], r[1][1], // -Y row becomes Z row
    );

    // Fast path: if det ≈ 1.0, the matrix is already a valid rotation and
    // we can extract the quaternion directly. This is the common case (~99%
    // of NIF matrices). SVD is only needed for degenerate matrices (zeroed
    // BSFadeNode rotations, scaled/sheared matrices from bad exports).
    let det = yup.determinant();
    let rotation_matrix = if (det - 1.0).abs() < 0.1 {
        yup
    } else {
        // Degenerate — SVD repair: M = U*Σ*Vt → nearest rotation = U*Vt.
        let svd = yup.svd(true, true);
        let u = svd.u.unwrap();
        let vt = svd.v_t.unwrap();
        let mut nearest = u * vt;

        if nearest.determinant() < 0.0 {
            let mut u_fixed = u;
            u_fixed.column_mut(2).scale_mut(-1.0);
            nearest = u_fixed * vt;
        }
        nearest
    };

    let rot = nalgebra::Rotation3::from_matrix_unchecked(rotation_matrix);
    let q = UnitQuaternion::from_rotation_matrix(&rot);

    [q.i, q.j, q.k, q.w]
}

/// Convert a single sRGB channel value (0.0–1.0) to linear light.
/// Gamebryo stores colors in sRGB space; PBR shaders expect linear.
pub(super) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
