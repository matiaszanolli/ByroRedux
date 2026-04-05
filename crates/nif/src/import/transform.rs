//! NIF transform composition and degenerate rotation repair.

use crate::types::{NiMatrix3, NiPoint3, NiTransform};

/// Compose parent * child transforms.
///
/// `NiTransform` composition: rotation = parent.rot * child.rot,
/// translation = parent.rot * (parent.scale * child.trans) + parent.trans,
/// scale = parent.scale * child.scale.
pub(super) fn compose_transforms(parent: &NiTransform, child: &NiTransform) -> NiTransform {
    let parent_rot = if is_degenerate_rotation(&parent.rotation) {
        repair_rotation_svd_or_identity(&parent.rotation)
    } else {
        parent.rotation
    };

    let rot = mul_matrix3(&parent_rot, &child.rotation);
    let scaled_child_trans = scale_point(child.translation, parent.scale);
    let rotated = mul_matrix3_point(&parent_rot, scaled_child_trans);
    let translation = add_points(parent.translation, rotated);
    let scale = parent.scale * child.scale;

    NiTransform {
        rotation: rot,
        translation,
        scale,
    }
}

/// Check if a rotation matrix is degenerate (det far from 1.0).
pub(super) fn is_degenerate_rotation(m: &NiMatrix3) -> bool {
    let r = &m.rows;
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
        - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
        + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    (det - 1.0).abs() >= 0.1
}

/// SVD-repair a degenerate rotation matrix, or return identity if the matrix
/// has no meaningful orientation (all singular values near zero).
pub(super) fn repair_rotation_svd_or_identity(m: &NiMatrix3) -> NiMatrix3 {
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

pub(super) fn mul_matrix3(a: &NiMatrix3, b: &NiMatrix3) -> NiMatrix3 {
    let mut result = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            result[i][j] = a.rows[i][0] * b.rows[0][j]
                + a.rows[i][1] * b.rows[1][j]
                + a.rows[i][2] * b.rows[2][j];
        }
    }
    NiMatrix3 { rows: result }
}

pub(super) fn mul_matrix3_point(m: &NiMatrix3, p: NiPoint3) -> NiPoint3 {
    NiPoint3 {
        x: m.rows[0][0] * p.x + m.rows[0][1] * p.y + m.rows[0][2] * p.z,
        y: m.rows[1][0] * p.x + m.rows[1][1] * p.y + m.rows[1][2] * p.z,
        z: m.rows[2][0] * p.x + m.rows[2][1] * p.y + m.rows[2][2] * p.z,
    }
}

pub(super) fn scale_point(p: NiPoint3, s: f32) -> NiPoint3 {
    NiPoint3 {
        x: p.x * s,
        y: p.y * s,
        z: p.z * s,
    }
}

pub(super) fn add_points(a: NiPoint3, b: NiPoint3) -> NiPoint3 {
    NiPoint3 {
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
    }
}
