//! NIF transform composition.
//!
//! Rotation matrices are sanitized at parse time (see `crate::rotation`),
//! so this module can assume all input rotations are valid.

use crate::types::{NiMatrix3, NiPoint3, NiTransform};

/// Compose parent * child transforms.
///
/// `NiTransform` composition: rotation = parent.rot * child.rot,
/// translation = parent.rot * (parent.scale * child.trans) + parent.trans,
/// scale = parent.scale * child.scale.
pub(super) fn compose_transforms(parent: &NiTransform, child: &NiTransform) -> NiTransform {
    let rot = mul_matrix3(&parent.rotation, &child.rotation);
    let scaled_child_trans = scale_point(child.translation, parent.scale);
    let rotated = mul_matrix3_point(&parent.rotation, scaled_child_trans);
    let translation = add_points(parent.translation, rotated);
    let scale = parent.scale * child.scale;

    NiTransform {
        rotation: rot,
        translation,
        scale,
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
