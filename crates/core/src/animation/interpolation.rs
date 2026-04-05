//! Keyframe interpolation: linear, Hermite (quadratic), and TBC (Kochanek-Bartels).

use crate::math::{Quat, Vec3};

use super::types::{BoolChannel, ColorChannel, FloatChannel, KeyType, TransformChannel};

/// Binary search for the key pair bracketing `time`.
/// Returns (index_before, index_after, normalized_t).
pub(super) fn find_key_pair(times: &[f32], time: f32) -> (usize, usize, f32) {
    if times.is_empty() {
        return (0, 0, 0.0);
    }
    if times.len() == 1 || time <= times[0] {
        return (0, 0, 0.0);
    }
    if time >= *times.last().unwrap() {
        let last = times.len() - 1;
        return (last, last, 0.0);
    }

    // Binary search for the interval
    let mut lo = 0;
    let mut hi = times.len() - 1;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if times[mid] <= time {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let dt = times[hi] - times[lo];
    let t = if dt > 0.0 {
        (time - times[lo]) / dt
    } else {
        0.0
    };
    (lo, hi, t)
}

/// Cubic Hermite interpolation: p(t) = h00*p0 + h10*m0 + h01*p1 + h11*m1
fn hermite(t: f32) -> (f32, f32, f32, f32) {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    (h00, h10, h01, h11)
}

/// Compute Kochanek-Bartels tangents for a key given its neighbors.
/// Returns (incoming_tangent, outgoing_tangent).
fn tbc_tangents_f32(
    prev: Option<(f32, f32)>, // (time, value)
    curr: (f32, f32),
    next: Option<(f32, f32)>,
    tension: f32,
    bias: f32,
    continuity: f32,
) -> (f32, f32) {
    let a = (1.0 - tension) * (1.0 + bias) * (1.0 + continuity) / 2.0;
    let b = (1.0 - tension) * (1.0 - bias) * (1.0 - continuity) / 2.0;
    let c = (1.0 - tension) * (1.0 + bias) * (1.0 - continuity) / 2.0;
    let d = (1.0 - tension) * (1.0 - bias) * (1.0 + continuity) / 2.0;

    let (d_in, d_out) = match (prev, next) {
        (Some(p), Some(n)) => {
            let incoming = a * (curr.1 - p.1) + b * (n.1 - curr.1);
            let outgoing = c * (curr.1 - p.1) + d * (n.1 - curr.1);
            (incoming, outgoing)
        }
        (Some(p), None) => {
            let diff = curr.1 - p.1;
            (diff, diff)
        }
        (None, Some(n)) => {
            let diff = n.1 - curr.1;
            (diff, diff)
        }
        (None, None) => (0.0, 0.0),
    };

    (d_in, d_out)
}

/// Sample translation at a given time.
pub fn sample_translation(channel: &TransformChannel, time: f32) -> Option<Vec3> {
    let keys = &channel.translation_keys;
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].value);
    }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 {
        return Some(keys[i0].value);
    }

    let k0 = &keys[i0];
    let k1 = &keys[i1];

    match channel.translation_type {
        KeyType::Linear => Some(k0.value.lerp(k1.value, t)),
        KeyType::Quadratic => {
            let (h00, h10, h01, h11) = hermite(t);
            let dt = k1.time - k0.time;
            Some(Vec3::new(
                h00 * k0.value.x
                    + h10 * k0.forward.x * dt
                    + h01 * k1.value.x
                    + h11 * k1.backward.x * dt,
                h00 * k0.value.y
                    + h10 * k0.forward.y * dt
                    + h01 * k1.value.y
                    + h11 * k1.backward.y * dt,
                h00 * k0.value.z
                    + h10 * k0.forward.z * dt
                    + h01 * k1.value.z
                    + h11 * k1.backward.z * dt,
            ))
        }
        KeyType::Tbc => {
            // For TBC, compute Hermite tangents from TBC parameters
            let prev = if i0 > 0 { Some(i0 - 1) } else { None };
            let next = if i1 + 1 < keys.len() { Some(i1) } else { None };

            let tbc0 = k0.tbc.unwrap_or([0.0, 0.0, 0.0]);
            let mut m0 = Vec3::ZERO;
            for axis in 0..3 {
                let p = prev.map(|pi| (keys[pi].time, keys[pi].value[axis]));
                let c = (k0.time, k0.value[axis]);
                let n = Some((k1.time, k1.value[axis]));
                let (_, out) = tbc_tangents_f32(p, c, n, tbc0[0], tbc0[1], tbc0[2]);
                m0[axis] = out;
            }

            let tbc1 = k1.tbc.unwrap_or([0.0, 0.0, 0.0]);
            let mut m1 = Vec3::ZERO;
            for axis in 0..3 {
                let p = Some((k0.time, k0.value[axis]));
                let c = (k1.time, k1.value[axis]);
                let n = next.map(|ni| (keys[ni].time, keys[ni].value[axis]));
                let (inc, _) = tbc_tangents_f32(p, c, n, tbc1[0], tbc1[1], tbc1[2]);
                m1[axis] = inc;
            }

            let (h00, h10, h01, h11) = hermite(t);
            let dt = k1.time - k0.time;
            Some(Vec3::new(
                h00 * k0.value.x + h10 * m0.x * dt + h01 * k1.value.x + h11 * m1.x * dt,
                h00 * k0.value.y + h10 * m0.y * dt + h01 * k1.value.y + h11 * m1.y * dt,
                h00 * k0.value.z + h10 * m0.z * dt + h01 * k1.value.z + h11 * m1.z * dt,
            ))
        }
    }
}

/// Sample rotation at a given time.
pub fn sample_rotation(channel: &TransformChannel, time: f32) -> Option<Quat> {
    let keys = &channel.rotation_keys;
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].value);
    }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 {
        return Some(keys[i0].value);
    }

    // For all rotation key types, use SLERP between the two bracketing keys.
    // TBC rotation uses the same SLERP — TBC tangents affect timing but
    // quaternion interpolation is always spherical.
    let q0 = keys[i0].value;
    let q1 = keys[i1].value;

    // Ensure shortest path
    let q1 = if q0.dot(q1) < 0.0 { -q1 } else { q1 };
    Some(q0.slerp(q1, t))
}

/// Sample scale at a given time.
pub fn sample_scale(channel: &TransformChannel, time: f32) -> Option<f32> {
    let keys = &channel.scale_keys;
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].value);
    }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 {
        return Some(keys[i0].value);
    }

    let k0 = &keys[i0];
    let k1 = &keys[i1];

    match channel.scale_type {
        KeyType::Linear => Some(k0.value + (k1.value - k0.value) * t),
        KeyType::Quadratic => {
            let (h00, h10, h01, h11) = hermite(t);
            let dt = k1.time - k0.time;
            Some(h00 * k0.value + h10 * k0.forward * dt + h01 * k1.value + h11 * k1.backward * dt)
        }
        KeyType::Tbc => {
            let prev = if i0 > 0 {
                Some((keys[i0 - 1].time, keys[i0 - 1].value))
            } else {
                None
            };
            let next = if i1 + 1 < keys.len() {
                Some((keys[i1 + 1].time, keys[i1 + 1].value))
            } else {
                None
            };

            let tbc0 = k0.tbc.unwrap_or([0.0, 0.0, 0.0]);
            let (_, m0) = tbc_tangents_f32(
                prev,
                (k0.time, k0.value),
                Some((k1.time, k1.value)),
                tbc0[0],
                tbc0[1],
                tbc0[2],
            );

            let tbc1 = k1.tbc.unwrap_or([0.0, 0.0, 0.0]);
            let (m1, _) = tbc_tangents_f32(
                Some((k0.time, k0.value)),
                (k1.time, k1.value),
                next,
                tbc1[0],
                tbc1[1],
                tbc1[2],
            );

            let (h00, h10, h01, h11) = hermite(t);
            let dt = k1.time - k0.time;
            Some(h00 * k0.value + h10 * m0 * dt + h01 * k1.value + h11 * m1 * dt)
        }
    }
}

/// Sample a float channel at a given time.
pub fn sample_float_channel(channel: &FloatChannel, time: f32) -> f32 {
    let keys = &channel.keys;
    if keys.is_empty() {
        return 0.0;
    }
    if keys.len() == 1 || time <= keys[0].time {
        return keys[0].value;
    }
    if time >= keys.last().unwrap().time {
        return keys.last().unwrap().value;
    }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 {
        return keys[i0].value;
    }
    keys[i0].value + (keys[i1].value - keys[i0].value) * t
}

/// Sample a color channel at a given time (linear interpolation).
pub fn sample_color_channel(channel: &ColorChannel, time: f32) -> Vec3 {
    let keys = &channel.keys;
    if keys.is_empty() {
        return Vec3::ONE;
    }
    if keys.len() == 1 || time <= keys[0].time {
        return keys[0].value;
    }
    if time >= keys.last().unwrap().time {
        return keys.last().unwrap().value;
    }

    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i0, i1, t) = find_key_pair(&times, time);
    if i0 == i1 {
        return keys[i0].value;
    }
    keys[i0].value.lerp(keys[i1].value, t)
}

/// Sample a bool channel at a given time (step — no interpolation).
pub fn sample_bool_channel(channel: &BoolChannel, time: f32) -> bool {
    let keys = &channel.keys;
    if keys.is_empty() {
        return true;
    }
    // Step function: use the last key whose time <= current time.
    let mut result = keys[0].value;
    for key in keys {
        if key.time <= time {
            result = key.value;
        } else {
            break;
        }
    }
    result
}
