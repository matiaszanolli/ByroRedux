//! Key conversion utilities.
//!
//! Vec3 / quat / Euler key conversion, sample-rate helpers, Euler-to-quat
//! math, `OrderedF32` for time-key deduplication.

use super::*;
use crate::blocks::interpolator::{FloatKey, KeyGroup, KeyType, NiTransformData, Vec3Key};
use std::collections::BTreeSet;

pub fn is_flt_max(v: f32) -> bool {
    v.abs() >= FLT_MAX_SENTINEL
}

/// A keyframe-stream scalar is sane when it is finite and below the
/// FLT_MAX sentinel. The mainline keyframe converters here gate every
/// copied float on it (#1443), and since #4397 the static-pose /
/// B-spline-fallback paths gate on it too — the old `is_flt_max`-only
/// gates were blind to NaN (`is_flt_max(NaN)` is false), letting a NaN
/// pose reach the canonical clip.
pub fn is_key_value_sane(v: f32) -> bool {
    v.is_finite() && !is_flt_max(v)
}

/// The single rotation-sample sanitizer at the NIF→canonical boundary
/// (#4396): every rotation that enters a `RotationKey` — mainline KF
/// keys, static poses, B-spline samples and B-spline static fallbacks —
/// normalizes through here or is skipped, so the bone keeps its bind
/// pose rather than receiving a poisoned key. Input and output are raw
/// NIF `(w, x, y, z)` order; convert through [`zup_to_yup_quat`]
/// afterwards (its re-normalize is idempotent on the unit result).
///
/// Returns `None` (skip the key / sample) when:
///
/// 1. any component fails [`is_key_value_sane`] — non-finite, or the
///    FLT_MAX "axis inactive" sentinel;
/// 2. `len_sq` overflowed to non-finite (#4406): individually-sane
///    components past `~1.84e19` square past `f32::MAX`, and naive
///    normalization then emits the **zero** quaternion (every component
///    is finite, so `finite * 0.0 == 0.0`). A post-normalize
///    `is_key_value_sane` check cannot see that failure — `[0, 0, 0, 0]`
///    is finite — which is why the guard is on `len_sq` itself;
/// 3. `len_sq <= EPSILON` — a genuinely near-zero (typically all-zero
///    authored) quaternion (#4396: core's `normalize_quat` returns
///    zero-length input unchanged, so such a key sailed through as the
///    zero quaternion). Identity is NOT substituted: an invented pose is
///    still fabrication, and the sampler interpolates across the gap or
///    falls back to the bind pose like every other rejection. (Pre-#4396
///    the B-spline path substituted identity here; that exception is
///    retired with the unification.)
///
/// Blast radius if a poisoned value escaped: `GlobalTransform` → skinned
/// vertex positions → BLAS refit / TLAS build with a non-finite AABB,
/// which is undefined behaviour under Vulkan's acceleration-structure
/// build contract rather than a visual glitch (#4166).
///
/// Worth recording because it inverts the intuition (#4166): a NaN
/// control point was already safe before any guard existed — `len_sq`
/// is NaN, `NaN > f32::EPSILON` is false. It is the *infinite* and the
/// *merely huge* inputs that needed guarding, not the NaN ones.
pub(crate) fn normalized_rotation_sample(raw: [f32; 4]) -> Option<[f32; 4]> {
    if !raw.iter().all(|v| is_key_value_sane(*v)) {
        return None;
    }
    let len_sq = raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2] + raw[3] * raw[3];
    if !len_sq.is_finite() || len_sq <= f32::EPSILON {
        return None;
    }
    let inv = 1.0 / len_sq.sqrt();
    Some([
        raw[0] * inv,
        raw[1] * inv,
        raw[2] * inv,
        raw[3] * inv,
    ])
}

/// Clamp a Hermite tangent component: a non-finite / sentinel tangent
/// makes the Quadratic basis emit NaN even when the bracketing key
/// values are clean, so collapse it to 0 (a locally-linear segment)
/// rather than dropping the whole key. See [`is_key_value_sane`].
fn sane_tangent(v: f32) -> f32 {
    if is_key_value_sane(v) {
        v
    } else {
        0.0
    }
}

pub fn convert_vec3_keys(group: &KeyGroup<Vec3Key>) -> (Vec<TranslationKey>, KeyType) {
    let keys = group
        .keys
        .iter()
        // #1443 — drop keys whose value triple is non-finite / FLT_MAX;
        // the sampler interpolates across the gap (or falls back to the
        // bind pose if every key is dropped). Tangents are clamped, not
        // dropped, so a single bad tangent doesn't discard a clean key.
        .filter(|k| k.value.iter().all(|&c| is_key_value_sane(c)))
        .map(|k| TranslationKey {
            time: k.time,
            value: zup_to_yup_pos(k.value),
            forward: zup_to_yup_pos(k.tangent_forward.map(sane_tangent)),
            backward: zup_to_yup_pos(k.tangent_backward.map(sane_tangent)),
            tbc: k.tbc,
        })
        .collect();
    (keys, group.key_type)
}

pub fn convert_quat_keys(data: &NiTransformData) -> (Vec<RotationKey>, KeyType) {
    let rotation_type = data.rotation_type.unwrap_or(KeyType::Linear);

    if rotation_type == KeyType::XyzRotation {
        return convert_xyz_euler_keys(data);
    }

    let keys = data
        .rotation_keys
        .iter()
        // #1443 dropped keys with non-finite / FLT_MAX components. #4396 —
        // the full [`normalized_rotation_sample`] sanitizer replaces the
        // per-component check: an individually-sane quaternion whose
        // components square past f32::MAX (which `normalize_quat` turned
        // into the zero quaternion via `inf → inv 0`) and an authored
        // all-zero quaternion (which `normalize_quat` returns unchanged)
        // are skipped too, and surviving keys are normalized at this
        // boundary instead of relying on the sampler's lazy normalize.
        // Empty result → channel falls back to the bind pose.
        .filter_map(|k| {
            normalized_rotation_sample(k.value).map(|q| RotationKey {
                time: k.time,
                value: zup_to_yup_quat(q),
                tbc: k.tbc,
            })
        })
        .collect();
    (keys, rotation_type)
}

/// Convert XYZ euler rotation key groups to quaternion keys.
///
/// Each axis has its own `KeyGroup<FloatKey>` with potentially different key counts
/// and interpolation types. We collect all unique timestamps, sample each axis at
/// each time, compose the euler angles into a quaternion, and apply Z-up→Y-up conversion.
pub fn convert_xyz_euler_keys(data: &NiTransformData) -> (Vec<RotationKey>, KeyType) {
    let Some(ref xyz) = data.xyz_rotations else {
        return (Vec::new(), KeyType::Linear);
    };

    // Collect all unique timestamps across all 3 axes using ordered floats.
    let mut times = BTreeSet::new();
    for axis_group in xyz {
        for key in &axis_group.keys {
            times.insert(OrderedF32(key.time));
        }
    }

    if times.is_empty() {
        return (Vec::new(), KeyType::Linear);
    }

    let keys: Vec<RotationKey> = times
        .iter()
        .filter_map(|&OrderedF32(time)| {
            let x = sample_float_key_group(&xyz[0], time);
            let y = sample_float_key_group(&xyz[1], time);
            let z = sample_float_key_group(&xyz[2], time);

            // #2434 / COORD-1 — Gamebryo euler angles are in radians,
            // Z-up coordinate system, XYZ intrinsic order, and (per the
            // vendor header `efd/Matrix3.h`: "positive angles are
            // associated with clockwise rotations") CLOCKWISE-positive.
            // Route through the same core SoT every other Gamebryo
            // Euler consumer uses (REFR placement, XCLL directional
            // lighting) instead of a second, independently-signed
            // private formula: the prior `euler_to_quat_wxyz` composed
            // the standard CCW-positive product with no negation,
            // which — conjugated through the Z-up→Y-up swap — is
            // exactly `--rotation-mode 3`
            // (`byroredux/src/cell_loader/euler.rs`), a non-shipping
            // diagnostic variant, not the canonical CW convention every
            // other consumer honours. `euler_zup_to_quat_yup` composes
            // AND axis-swaps in one step, so no separate
            // `zup_to_yup_quat` call is needed here.
            let q = byroredux_core::math::coord::euler_zup_to_quat_yup(x, y, z);
            let yup = [q.x, q.y, q.z, q.w];

            // #1443 — a non-finite euler sample (NaN / ±inf) makes
            // `sin_cos` emit NaN and poisons the quaternion; drop the
            // sample rather than bake a NaN rotation into the bone matrix.
            if !yup.iter().all(|&c| is_key_value_sane(c)) {
                return None;
            }

            Some(RotationKey {
                time,
                value: yup,
                tbc: None, // Euler→quat bakes interpolation; SLERP between samples
            })
        })
        .collect();

    // Output as Linear (SLERP between the pre-composed quaternion samples)
    (keys, KeyType::Linear)
}

/// Linearly sample a float key group at a given time.
/// Supports Linear, Quadratic (Hermite), and TBC interpolation.
pub fn sample_float_key_group(group: &KeyGroup<FloatKey>, time: f32) -> f32 {
    let keys = &group.keys;
    if keys.is_empty() {
        return 0.0;
    }
    if keys.len() == 1 || time <= keys[0].time {
        return keys[0].value;
    }
    if time >= keys.last().unwrap().time {
        return keys.last().unwrap().value;
    }

    // Binary search for bracketing pair.
    let mut lo = 0;
    let mut hi = keys.len() - 1;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if keys[mid].time <= time {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let k0 = &keys[lo];
    let k1 = &keys[hi];
    let dt = k1.time - k0.time;
    let t = if dt > 0.0 { (time - k0.time) / dt } else { 0.0 };

    match group.key_type {
        KeyType::Constant => k0.value, // Step: hold value until next key
        KeyType::Linear => k0.value + (k1.value - k0.value) * t,
        KeyType::Quadratic => {
            let t2 = t * t;
            let t3 = t2 * t;
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            h00 * k0.value
                + h10 * k0.tangent_forward * dt
                + h01 * k1.value
                + h11 * k1.tangent_backward * dt
        }
        KeyType::Tbc | KeyType::XyzRotation => {
            // TBC: fall back to linear for euler axis sampling (rare edge case)
            k0.value + (k1.value - k0.value) * t
        }
    }
}

/// Wrapper for f32 that implements Ord for use in BTreeSet.
/// NaN-safe: treats NaN as equal and less than all values.
#[derive(Clone, Copy, PartialEq)]
pub struct OrderedF32(f32);

impl Eq for OrderedF32 {}

impl PartialOrd for OrderedF32 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF32 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

pub fn convert_float_keys(group: &KeyGroup<FloatKey>) -> (Vec<ScaleKey>, KeyType) {
    let keys = group
        .keys
        .iter()
        // #1443 — drop non-finite / FLT_MAX scale values (a NaN scale
        // collapses or explodes the skinning matrix); clamp bad tangents.
        .filter(|k| is_key_value_sane(k.value))
        .map(|k| ScaleKey {
            time: k.time,
            value: k.value,
            forward: sane_tangent(k.tangent_forward),
            backward: sane_tangent(k.tangent_backward),
            tbc: k.tbc,
        })
        .collect();
    (keys, group.key_type)
}
