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
///
/// #4633 — #4549's parse-time gate is inputs-only: every field here is
/// finite on entry, yet the *products* can still overflow f32 (two
/// finite-but-extreme scales like 1e20 × 1e20, or a near-`f32::MAX` parent
/// scale times a child translation). ~5% of random single-field float
/// corruptions are finite-but-overflowing — the more likely residual than
/// the direct NaN/inf #4549 catches. The composed output is neutralized
/// through the same #4549 policy (translation → 0, scale → 1, rate-limited
/// warn) so the overflow cannot ride a `GlobalTransform` into a
/// non-finite `VkTransformMatrixKHR` at the TLAS instance build. The
/// rotation product needs no gate: `sanitize_rotation` orthonormalizes
/// both inputs, bounding every cell of the product by 3.
pub(super) fn compose_transforms(parent: &NiTransform, child: &NiTransform) -> NiTransform {
    let rot = mul_matrix3(&parent.rotation, &child.rotation);
    let scaled_child_trans = scale_point(child.translation, parent.scale);
    let rotated = mul_matrix3_point(&parent.rotation, scaled_child_trans);
    let mut translation = add_points(parent.translation, rotated);
    let mut scale = parent.scale * child.scale;
    crate::rotation::sanitize_transform_translation_and_scale(&mut translation, &mut scale);

    NiTransform {
        rotation: rot,
        translation,
        scale,
    }
}

pub(super) fn mul_matrix3(a: &NiMatrix3, b: &NiMatrix3) -> NiMatrix3 {
    let mut result = [[0.0f32; 3]; 3];
    for (result_row, a_row) in result.iter_mut().zip(a.rows.iter()) {
        for (j, cell) in result_row.iter_mut().enumerate() {
            *cell = a_row[0] * b.rows[0][j] + a_row[1] * b.rows[1][j] + a_row[2] * b.rows[2][j];
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
