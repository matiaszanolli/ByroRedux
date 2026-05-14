//! B-spline interpolator evaluation (issue #155).
//!
//! Dequantise + de-Boor sampling at 30 Hz for compressed transform / float /
//! point3 interpolators.

use super::*;
use crate::blocks::interpolator::{
    KeyType, NiBSplineBasisData, NiBSplineCompFloatInterpolator, NiBSplineCompTransformInterpolator, NiBSplineData,
};
use crate::scene::NifScene;

// ── B-spline evaluation (issue #155) ──────────────────────────────────────
//
// NiBSplineCompTransformInterpolator stores quantized control points for
// three sub-curves (translation, rotation, scale) that drive one node's
// transform. The data block contains a flat array of i16 control points;
// per-channel handles index into that array. Decompression:
//
//     value = offset + (short / 32767) * half_range
//
// Each channel is an open uniform cubic B-spline (degree 3). Evaluation
// uses a simple De Boor step: given `N` control points (N >= 4), the
// curve is parametrized over `[0, N - 3]` with knots
//     0, 0, 0, 0, 1, 2, ..., N-4, N-3, N-3, N-3, N-3.
// We sample the parameter `u ∈ [0, N - 3]` uniformly in time over
// `[start_time, stop_time]` and emit linear TQS keys — downstream
// evaluation interpolates linearly between samples, which is a visible
// but acceptable approximation of the continuous cubic curve at 30 Hz.
//
// This is the minimum viable implementation. A follow-up could sample
// non-uniformly at tangent breakpoints or emit Hermite tangents instead
// of linear keys.

/// Number of channels a comp-transform interpolator stores per control
/// point per channel: 3 for translation (x,y,z), 4 for rotation
/// (w,x,y,z), 1 for scale.
pub const BSPLINE_TRANS_STRIDE: usize = 3;
pub const BSPLINE_ROT_STRIDE: usize = 4;
pub const BSPLINE_SCALE_STRIDE: usize = 1;

/// Dequantize a single compact control point to an f32.
#[inline]
pub fn dequant(raw: i16, offset: f32, half_range: f32) -> f32 {
    offset + (raw as f32 / 32767.0) * half_range
}

/// Evaluate an open uniform cubic B-spline at parameter `u ∈ [0, N - 3]`
/// given `n` control points (each point is `stride` floats packed
/// contiguously in `control_points`).
///
/// Uses De Boor's algorithm restricted to degree 3. For an open uniform
/// knot vector with clamped endpoints, `u == 0` evaluates exactly to the
/// first control point and `u == n - 3` evaluates exactly to the last.
pub fn deboor_cubic(control_points: &[f32], n: usize, stride: usize, u: f32) -> Vec<f32> {
    debug_assert!(stride > 0);
    if n < BSPLINE_DEGREE + 1 {
        // Underdetermined — just return the first CP (or zeros if empty).
        return control_points
            .get(0..stride)
            .map(|s| s.to_vec())
            .unwrap_or_else(|| vec![0.0; stride]);
    }

    // Clamp u to the valid parameter range.
    let u_max = (n - BSPLINE_DEGREE) as f32;
    let u = u.clamp(0.0, u_max);

    // Find the knot span k such that knots[k] <= u < knots[k+1].
    // For the open uniform knot vector:
    //   knots = [0, 0, 0, 0, 1, 2, ..., n-4, n-3, n-3, n-3, n-3]
    // Internal knots 1..=n-4 correspond to parameter values 1..=n-4.
    // k is in [3, n-1].
    let k = {
        let mut k = (u.floor() as usize) + BSPLINE_DEGREE;
        if k >= n {
            k = n - 1;
        }
        if k < BSPLINE_DEGREE {
            k = BSPLINE_DEGREE;
        }
        k
    };

    // De Boor triangle: start with control points P[k-d..=k] and
    // fold them toward the evaluation point.
    // For the open uniform knot vector, the spans between interior
    // knots are all length 1, so the alpha values simplify.
    let mut d: [Vec<f32>; BSPLINE_DEGREE + 1] = [
        vec![0.0; stride],
        vec![0.0; stride],
        vec![0.0; stride],
        vec![0.0; stride],
    ];
    for j in 0..=BSPLINE_DEGREE {
        let cp_idx = k + j - BSPLINE_DEGREE;
        let cp_idx = cp_idx.min(n - 1);
        let start = cp_idx * stride;
        d[j].copy_from_slice(&control_points[start..start + stride]);
    }

    // Open uniform clamped knot vector for `n` control points and
    // degree `d = BSPLINE_DEGREE`:
    //     knots = [0, 0, 0, 0, 1, 2, ..., n-d-1, n-d, n-d, n-d, n-d]
    // with `n + d + 1` total entries.
    //   - Indices 0..=d all map to value 0 (left clamp).
    //   - Indices d+1..=n-1 are interior knots at values 1..n-d-1.
    //   - Indices n..=n+d all map to value n-d (right clamp).
    let knot_at = |idx: isize| -> f32 {
        let d = BSPLINE_DEGREE as isize;
        let n_d = (n - BSPLINE_DEGREE) as isize;
        if idx <= d {
            0.0
        } else {
            (idx - d).min(n_d) as f32
        }
    };

    for r in 1..=BSPLINE_DEGREE {
        for j in (r..=BSPLINE_DEGREE).rev() {
            let k_i = k as isize;
            let left = knot_at(k_i + j as isize - BSPLINE_DEGREE as isize);
            let right = knot_at(k_i + j as isize - (r as isize - 1));
            let span = right - left;
            let alpha = if span > f32::EPSILON {
                (u - left) / span
            } else {
                0.0
            };
            for c in 0..stride {
                d[j][c] = (1.0 - alpha) * d[j - 1][c] + alpha * d[j][c];
            }
        }
    }

    d[BSPLINE_DEGREE].clone()
}

/// Dequantize a range of compact control points into a flat f32 vector,
/// preserving the given channel stride.
pub fn dequantize_channel(
    raw: &[i16],
    start: usize,
    count: usize,
    stride: usize,
    offset: f32,
    half_range: f32,
) -> Vec<f32> {
    let end = start + count * stride;
    // #408 — `count` and `stride` originate from `NiBSplineBasisData`
    // / per-channel STRIDE constants. Caller already validates `end`
    // against `raw.len()` via the `channel_slice` callers above, but
    // pre-allocate against the input slice length so a malformed
    // basis can't request more capacity than the data could justify.
    let mut out = Vec::with_capacity((count * stride).min(raw.len()));
    for &r in &raw[start..end] {
        out.push(dequant(r, offset, half_range));
    }
    out
}

/// Extract a `FloatChannel` by sampling a
/// `NiBSplineCompFloatInterpolator`. Same de Boor + `BSPLINE_SAMPLE_HZ`
/// recipe as `extract_transform_channel_bspline` — restricted to a
/// stride-1 scalar channel. Returns `None` when the spline data is
/// missing or under-defined (fewer than `degree + 1` control points,
/// invalid handle, off-end slice) and the caller's downstream behaviour
/// is to leave the channel at its bind value. See #936.
pub fn extract_float_channel_bspline(
    scene: &NifScene,
    interp: &NiBSplineCompFloatInterpolator,
    target: FloatTarget,
) -> Option<FloatChannel> {
    // Single-key static fallback used by every "no usable spline data"
    // branch below (null refs, missing data blocks, under-defined basis,
    // invalid handle). Returns None when the fallback `value` is also
    // FLT_MAX-sentinel, in which case the caller treats it as "no
    // animation" and the channel stays at its bind value.
    let static_fallback = || -> Option<FloatChannel> {
        if is_flt_max(interp.value) {
            return None;
        }
        Some(FloatChannel {
            target,
            keys: vec![AnimFloatKey {
                time: interp.start_time,
                value: interp.value,
            }],
        })
    };

    let (Some(basis_idx), Some(data_idx)) = (
        interp.basis_data_ref.index(),
        interp.spline_data_ref.index(),
    ) else {
        return static_fallback();
    };
    let (Some(basis), Some(data)) = (
        scene.get_as::<NiBSplineBasisData>(basis_idx),
        scene.get_as::<NiBSplineData>(data_idx),
    ) else {
        return static_fallback();
    };

    let n_cp = basis.num_control_points as usize;
    if n_cp < BSPLINE_DEGREE + 1 {
        return static_fallback();
    }

    let channel = channel_slice(
        interp.handle,
        &data.compact_control_points,
        n_cp,
        1, // scalar — stride 1
        interp.float_offset,
        interp.float_half_range,
    );

    let Some(cps) = channel else {
        return static_fallback();
    };

    let duration = (interp.stop_time - interp.start_time).max(0.0);
    let n_samples_f = (duration * BSPLINE_SAMPLE_HZ).ceil();
    let n_samples = (n_samples_f as usize).max(2).min(1_000_000);
    let u_max = (n_cp - BSPLINE_DEGREE) as f32;

    let mut keys = Vec::with_capacity(n_samples);
    for i in 0..n_samples {
        let t = if n_samples > 1 {
            interp.start_time + duration * (i as f32 / (n_samples - 1) as f32)
        } else {
            interp.start_time
        };
        let u = if duration > f32::EPSILON {
            ((t - interp.start_time) / duration) * u_max
        } else {
            0.0
        };
        let p = deboor_cubic(&cps, n_cp, 1, u);
        keys.push(AnimFloatKey {
            time: t,
            value: p[0],
        });
    }

    if keys.is_empty() {
        return None;
    }
    Some(FloatChannel { target, keys })
}

/// Extract a TransformChannel by sampling a NiBSplineCompTransformInterpolator.
pub fn extract_transform_channel_bspline(
    scene: &NifScene,
    interp: &NiBSplineCompTransformInterpolator,
) -> Option<TransformChannel> {
    let basis_idx = interp.basis_data_ref.index()?;
    let basis = scene.get_as::<NiBSplineBasisData>(basis_idx)?;
    let data_idx = interp.spline_data_ref.index()?;
    let data = scene.get_as::<NiBSplineData>(data_idx)?;

    let n_cp = basis.num_control_points as usize;
    if n_cp < BSPLINE_DEGREE + 1 {
        // Not enough control points for a cubic B-spline — fall back to
        // the static transform stored on the interpolator.
        return Some(static_transform_channel(interp));
    }

    // Determine number of samples from the animation duration.
    // #408 — clamp to a 1 M sample ceiling per channel (~9 hours of
    // animation at 30 Hz) so a malicious or corrupt `stop_time` can't
    // request `usize::MAX` slots and OOM the importer. Real anims top
    // out at a few thousand samples even for the longest cinematics.
    let duration = (interp.stop_time - interp.start_time).max(0.0);
    let n_samples_f = (duration * BSPLINE_SAMPLE_HZ).ceil();
    let n_samples = (n_samples_f as usize).max(2).min(1_000_000);

    // Per-channel setup. Each handle is an offset in i16 units into
    // `data.compact_control_points` where that channel's run of
    // `n_cp * stride` quantized values begins. INVALID = u32::MAX.
    let trans_q = channel_slice(
        interp.translation_handle,
        &data.compact_control_points,
        n_cp,
        BSPLINE_TRANS_STRIDE,
        interp.translation_offset,
        interp.translation_half_range,
    );
    let rot_q = channel_slice(
        interp.rotation_handle,
        &data.compact_control_points,
        n_cp,
        BSPLINE_ROT_STRIDE,
        interp.rotation_offset,
        interp.rotation_half_range,
    );
    let scale_q = channel_slice(
        interp.scale_handle,
        &data.compact_control_points,
        n_cp,
        BSPLINE_SCALE_STRIDE,
        interp.scale_offset,
        interp.scale_half_range,
    );

    let u_max = (n_cp - BSPLINE_DEGREE) as f32;

    let mut translation_keys = Vec::with_capacity(n_samples);
    let mut rotation_keys = Vec::with_capacity(n_samples);
    let mut scale_keys = Vec::with_capacity(n_samples);

    for i in 0..n_samples {
        let t = if n_samples > 1 {
            interp.start_time + duration * (i as f32 / (n_samples - 1) as f32)
        } else {
            interp.start_time
        };
        // Parameter u in [0, n-d] corresponding to t in [start, stop].
        let u = if duration > f32::EPSILON {
            ((t - interp.start_time) / duration) * u_max
        } else {
            0.0
        };

        // Translation. Bspline payload absent → pose-value fallback. If
        // the pose itself is the FLT_MAX sentinel ("axis inactive"),
        // skip the key so `sample_translation` returns None and the
        // bone keeps its bind-pose translation.
        if let Some(ref cps) = trans_q {
            let p = deboor_cubic(cps, n_cp, BSPLINE_TRANS_STRIDE, u);
            let zup = [p[0], p[1], p[2]];
            translation_keys.push(TranslationKey {
                time: t,
                value: zup_to_yup_pos(zup),
                forward: [0.0, 0.0, 0.0],
                backward: [0.0, 0.0, 0.0],
                tbc: None,
            });
        } else {
            let pose = [
                interp.transform.translation.x,
                interp.transform.translation.y,
                interp.transform.translation.z,
            ];
            if !(is_flt_max(pose[0]) || is_flt_max(pose[1]) || is_flt_max(pose[2])) {
                translation_keys.push(TranslationKey {
                    time: t,
                    value: zup_to_yup_pos(pose),
                    forward: [0.0, 0.0, 0.0],
                    backward: [0.0, 0.0, 0.0],
                    tbc: None,
                });
            }
        }

        // Rotation — normalize after sampling since the B-spline doesn't
        // enforce unit length on quaternions. Same FLT_MAX gate as
        // translation: an FLT_MAX-valued quaternion would normalise to
        // NaN and rotate the bone to garbage.
        if let Some(ref cps) = rot_q {
            let p = deboor_cubic(cps, n_cp, BSPLINE_ROT_STRIDE, u);
            let [mut w, mut x, mut y, mut z] = [p[0], p[1], p[2], p[3]];
            let len_sq = w * w + x * x + y * y + z * z;
            if len_sq > f32::EPSILON {
                let inv = 1.0 / len_sq.sqrt();
                w *= inv;
                x *= inv;
                y *= inv;
                z *= inv;
            } else {
                w = 1.0;
                x = 0.0;
                y = 0.0;
                z = 0.0;
            }
            rotation_keys.push(RotationKey {
                time: t,
                value: zup_to_yup_quat([w, x, y, z]),
                tbc: None,
            });
        } else {
            let q = interp.transform.rotation;
            if !(is_flt_max(q[0]) || is_flt_max(q[1]) || is_flt_max(q[2]) || is_flt_max(q[3])) {
                rotation_keys.push(RotationKey {
                    time: t,
                    value: zup_to_yup_quat(q),
                    tbc: None,
                });
            }
        }

        // Scale. Same FLT_MAX gate.
        if let Some(ref cps) = scale_q {
            let p = deboor_cubic(cps, n_cp, BSPLINE_SCALE_STRIDE, u);
            scale_keys.push(ScaleKey {
                time: t,
                value: p[0],
                forward: 0.0,
                backward: 0.0,
                tbc: None,
            });
        } else if !is_flt_max(interp.transform.scale) {
            scale_keys.push(ScaleKey {
                time: t,
                value: interp.transform.scale,
                forward: 0.0,
                backward: 0.0,
                tbc: None,
            });
        }
    }

    Some(TransformChannel {
        translation_keys,
        translation_type: KeyType::Linear,
        rotation_keys,
        rotation_type: KeyType::Linear,
        scale_keys,
        scale_type: KeyType::Linear,
        priority: 0,
    })
}

/// Build a static single-key TransformChannel from an interpolator's
/// fallback `NiQuatTransform`. FLT_MAX-encoded axes drop to empty key
/// lists so the bone keeps its bind-pose value (see FLT_MAX_SENTINEL).
pub fn static_transform_channel(interp: &NiBSplineCompTransformInterpolator) -> TransformChannel {
    let pose_t = [
        interp.transform.translation.x,
        interp.transform.translation.y,
        interp.transform.translation.z,
    ];
    let translation_keys =
        if is_flt_max(pose_t[0]) || is_flt_max(pose_t[1]) || is_flt_max(pose_t[2]) {
            Vec::new()
        } else {
            vec![TranslationKey {
                time: interp.start_time,
                value: zup_to_yup_pos(pose_t),
                forward: [0.0, 0.0, 0.0],
                backward: [0.0, 0.0, 0.0],
                tbc: None,
            }]
        };
    let q = interp.transform.rotation;
    let rotation_keys =
        if is_flt_max(q[0]) || is_flt_max(q[1]) || is_flt_max(q[2]) || is_flt_max(q[3]) {
            Vec::new()
        } else {
            vec![RotationKey {
                time: interp.start_time,
                value: zup_to_yup_quat(q),
                tbc: None,
            }]
        };
    let scale_keys = if is_flt_max(interp.transform.scale) {
        Vec::new()
    } else {
        vec![ScaleKey {
            time: interp.start_time,
            value: interp.transform.scale,
            forward: 0.0,
            backward: 0.0,
            tbc: None,
        }]
    };
    TransformChannel {
        translation_keys,
        translation_type: KeyType::Linear,
        rotation_keys,
        rotation_type: KeyType::Linear,
        scale_keys,
        scale_type: KeyType::Linear,
        priority: 0,
    }
}

/// Slice the compact control-point array for a single channel and
/// dequantize it. Returns `None` when the handle is invalid (`u32::MAX`)
/// or when the slice would run off the end of the data buffer.
pub fn channel_slice(
    handle: u32,
    raw: &[i16],
    n_cp: usize,
    stride: usize,
    offset: f32,
    half_range: f32,
) -> Option<Vec<f32>> {
    if handle == u32::MAX {
        return None;
    }
    let start = handle as usize;
    let needed = n_cp * stride;
    if start
        .checked_add(needed)
        .map_or(true, |end| end > raw.len())
    {
        log::debug!(
            "NiBSplineCompTransformInterpolator: handle {} + {} > data len {}",
            handle,
            needed,
            raw.len(),
        );
        return None;
    }
    Some(dequantize_channel(
        raw, start, n_cp, stride, offset, half_range,
    ))
}

