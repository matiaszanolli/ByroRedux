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

/// Shortest-path quaternion: flip `q` if it points away from `reference`.
/// Keeps interpolation on the hemisphere nearest the reference to avoid
/// long-way-around artifacts.
fn shortest_path(reference: Quat, q: Quat) -> Quat {
    if reference.dot(q) < 0.0 {
        -q
    } else {
        q
    }
}

/// Quaternion log relative to `base`: returns `log(base^-1 * q)` as an
/// angle-axis vector with magnitude = half the rotation angle times the
/// unit axis. The factor of 0.5 matches `exp_map_rel` below; both use
/// the "quaternion = (cos(θ/2), sin(θ/2)·axis)" convention.
///
/// Flips `q` to the shortest-path hemisphere first so rotations above
/// 180° aren't introduced by the rebase.
fn quat_log_rel(base: Quat, q: Quat) -> Vec3 {
    let q = shortest_path(base, q);
    let rel = base.conjugate() * q;
    let w = rel.w.clamp(-1.0, 1.0);
    let sin_half = (1.0 - w * w).max(0.0).sqrt();
    if sin_half < 1.0e-6 {
        return Vec3::ZERO;
    }
    let half_angle = w.acos(); // θ/2, since w = cos(θ/2)
    let axis = Vec3::new(rel.x, rel.y, rel.z) / sin_half;
    axis * half_angle
}

/// Inverse of [`quat_log_rel`]: `exp(v)` as a quaternion with the
/// same "half-angle axis" convention. `base * exp_map_rel(v)` gives
/// back a unit quaternion displaced from `base` by `v`.
fn exp_map_rel(v: Vec3) -> Quat {
    let half_angle = v.length();
    if half_angle < 1.0e-6 {
        return Quat::IDENTITY;
    }
    let axis = v / half_angle;
    let (sin_h, cos_h) = half_angle.sin_cos();
    Quat::from_xyzw(axis.x * sin_h, axis.y * sin_h, axis.z * sin_h, cos_h)
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

    let k0 = &keys[i0];
    let k1 = &keys[i1];
    let q0 = k0.value;
    // Shortest-path flip for q1 relative to q0 — keeps SLERP on the
    // near hemisphere for Linear, and keeps the q0-local log finite
    // for TBC.
    let q1 = shortest_path(q0, k1.value);

    match channel.rotation_type {
        KeyType::Linear | KeyType::Quadratic => {
            // Quadratic rotations in nif.xml (QuaternionKey) carry
            // forward/backward control quats; RotationKey doesn't store
            // them today, so fall back to SLERP. Linear always uses
            // SLERP — the standard Gamebryo behavior. See #230.
            Some(q0.slerp(q1, t))
        }
        KeyType::Tbc => {
            // Cubic Hermite in the log space of q0. We rebase every
            // neighbor rotation into q0-local space (so q0 sits at the
            // origin), run the scalar TBC-Hermite from
            // `tbc_tangents_f32` axis-by-axis, and convert the result
            // back via `exp_map_rel` before multiplying by q0. This is
            // a standard log-space approximation of SQUAD that respects
            // the TBC tension/bias/continuity parameters — the straight
            // SLERP path ignored them entirely. See #230.
            let prev = if i0 > 0 { Some(i0 - 1) } else { None };
            let next = if i1 + 1 < keys.len() { Some(i1 + 1) } else { None };

            // q0-local log of neighbors (always finite because
            // `quat_log_rel` applies a shortest-path flip).
            let log_prev =
                prev.map(|pi| (keys[pi].time, quat_log_rel(q0, keys[pi].value)));
            let log_k0 = (k0.time, Vec3::ZERO);
            let log_k1 = (k1.time, quat_log_rel(q0, k1.value));
            let log_next =
                next.map(|ni| (keys[ni].time, quat_log_rel(q0, keys[ni].value)));

            let tbc0 = k0.tbc.unwrap_or([0.0, 0.0, 0.0]);
            let tbc1 = k1.tbc.unwrap_or([0.0, 0.0, 0.0]);

            let mut m0 = Vec3::ZERO;
            let mut m1 = Vec3::ZERO;
            for axis in 0..3 {
                let pv = log_prev.map(|(t, v)| (t, v[axis]));
                let cv0 = (log_k0.0, log_k0.1[axis]);
                let nv0 = Some((log_k1.0, log_k1.1[axis]));
                let (_, out0) = tbc_tangents_f32(pv, cv0, nv0, tbc0[0], tbc0[1], tbc0[2]);
                m0[axis] = out0;

                let pv1 = Some((log_k0.0, log_k0.1[axis]));
                let cv1 = (log_k1.0, log_k1.1[axis]);
                let nv1 = log_next.map(|(t, v)| (t, v[axis]));
                let (in1, _) = tbc_tangents_f32(pv1, cv1, nv1, tbc1[0], tbc1[1], tbc1[2]);
                m1[axis] = in1;
            }

            let (h00, h10, h01, h11) = hermite(t);
            let dt = k1.time - k0.time;
            let p0 = log_k0.1; // zero
            let p1 = log_k1.1;
            let log_result = Vec3::new(
                h00 * p0.x + h10 * m0.x * dt + h01 * p1.x + h11 * m1.x * dt,
                h00 * p0.y + h10 * m0.y * dt + h01 * p1.y + h11 * m1.y * dt,
                h00 * p0.z + h10 * m0.z * dt + h01 * p1.z + h11 * m1.z * dt,
            );

            Some((q0 * exp_map_rel(log_result)).normalize())
        }
    }
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
