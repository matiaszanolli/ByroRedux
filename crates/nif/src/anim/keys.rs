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
/// FLT_MAX sentinel. The static-pose / B-spline-fallback paths already
/// gate on `is_flt_max` (`transform.rs`, `bspline.rs`); the mainline
/// keyframe converters here copy raw NIF floats, so a corrupt value
/// (NaN / ±inf / ±FLT_MAX) would otherwise reach the sampler and poison
/// the bone/shader uniform — a NaN skinning matrix that vanishes the
/// mesh (#1443, same finite-guard family as #772 / #1434 / #1409).
pub fn is_key_value_sane(v: f32) -> bool {
    v.is_finite() && !is_flt_max(v)
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
        // #1443 — drop keys whose quaternion has any non-finite / FLT_MAX
        // component; a NaN rotation propagates straight into the bone
        // matrix. Empty result → channel falls back to the bind pose.
        .filter(|k| k.value.iter().all(|&c| is_key_value_sane(c)))
        .map(|k| RotationKey {
            time: k.time,
            value: zup_to_yup_quat(k.value),
            tbc: k.tbc,
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

            // Gamebryo euler angles are in radians, Z-up coordinate system.
            // Compose euler → quaternion in Gamebryo space, then convert to Y-up.
            // Gamebryo uses XYZ intrinsic euler order.
            let qw = euler_to_quat_wxyz(x, y, z);
            let yup = zup_to_yup_quat(qw);

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

/// Convert XYZ intrinsic euler angles (radians) to quaternion (w, x, y, z).
pub fn euler_to_quat_wxyz(x: f32, y: f32, z: f32) -> [f32; 4] {
    let (sx, cx) = (x * 0.5).sin_cos();
    let (sy, cy) = (y * 0.5).sin_cos();
    let (sz, cz) = (z * 0.5).sin_cos();

    // XYZ intrinsic rotation composition
    let w = cx * cy * cz - sx * sy * sz;
    let qx = sx * cy * cz + cx * sy * sz;
    let qy = cx * sy * cz - sx * cy * sz;
    let qz = cx * cy * sz + sx * sy * cz;

    [w, qx, qy, qz]
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
