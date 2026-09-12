//! Compressed B-spline evaluation (`anim/bspline.rs`, issue #155).

use super::super::*;

#[test]
fn bspline_dequant_midpoint() {
    // raw=0 → offset; raw=32767 → offset + half_range; raw=-32767 → offset - half_range
    assert!((dequant(0, 10.0, 5.0) - 10.0).abs() < 1e-5);
    assert!((dequant(32767, 10.0, 5.0) - 15.0).abs() < 1e-4);
    assert!((dequant(-32767, 10.0, 5.0) - 5.0).abs() < 1e-4);
}

#[test]
fn deboor_cubic_clamped_endpoints() {
    // With 4 control points on a single-scalar channel, the cubic
    // B-spline at u=0 should equal CP[0], at u=1 should equal CP[3]
    // because an open uniform knot vector is fully clamped at both
    // ends for the minimum degree-3 case.
    let cps = vec![1.0, 2.0, 3.0, 10.0];
    let v0 = deboor_cubic(&cps, 4, 1, 0.0);
    let v1 = deboor_cubic(&cps, 4, 1, 1.0);
    assert!(
        (v0[0] - 1.0).abs() < 1e-4,
        "u=0 should give CP[0], got {}",
        v0[0]
    );
    assert!(
        (v1[0] - 10.0).abs() < 1e-4,
        "u=1 should give CP[3], got {}",
        v1[0]
    );
}

#[test]
fn deboor_cubic_monotone_between_endpoints() {
    // With a monotone CP sequence and a monotone knot parameter,
    // the evaluated curve should also be monotone (not strictly,
    // but the sign of successive differences should agree).
    let cps = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let n = 5;
    let u_max = (n - BSPLINE_DEGREE) as f32;
    let mut prev = f32::NEG_INFINITY;
    for i in 0..=10 {
        let u = u_max * (i as f32 / 10.0);
        let v = deboor_cubic(&cps, n, 1, u)[0];
        assert!(
            v >= prev - 1e-4,
            "non-monotone: v[{}] = {} < prev {}",
            i,
            v,
            prev
        );
        prev = v;
    }
}

#[test]
fn bspline_channel_slice_invalid_handle() {
    let raw: Vec<i16> = vec![0; 100];
    assert!(channel_slice(u32::MAX, &raw, 4, 3, 0.0, 1.0).is_none());
}

#[test]
fn bspline_channel_slice_out_of_bounds() {
    let raw: Vec<i16> = vec![0; 10];
    // Needs 4 * 3 = 12 slots starting at handle 0 → should fail (only 10).
    assert!(channel_slice(0, &raw, 4, 3, 0.0, 1.0).is_none());
}

#[test]
fn bspline_channel_slice_dequantizes() {
    // 4 CPs × stride 1, raw values [0, 32767, -32767, 0]
    // with offset=10, half_range=5 → [10, 15, 5, 10]
    let raw: Vec<i16> = vec![0, 32767, -32767, 0];
    let out = channel_slice(0, &raw, 4, 1, 10.0, 5.0).unwrap();
    assert_eq!(out.len(), 4);
    assert!((out[0] - 10.0).abs() < 1e-4);
    assert!((out[1] - 15.0).abs() < 1e-4);
    assert!((out[2] - 5.0).abs() < 1e-4);
    assert!((out[3] - 10.0).abs() < 1e-4);
}

// #3765 (SAFE-2026-08-30-D9-01) — `translation_offset` / `translation_half_range`
// (and the rotation/scale/float siblings) are raw f32s read off disk with no
// validation. A NaN/±Inf in either poisoned every dequantized control point
// and `deboor_cubic` propagated it into the sampled channel unfiltered — the
// mainline keyframe converters (`sanitize_keyframe_streams`, #1443) and the
// pose-fallback branches (`is_flt_max`) were already gated; this sampled path,
// the whole point of the block, had neither. `channel_slice` is the single
// choke point all four callers (float channel; transform channel's
// translation/rotation/scale) funnel through, so a fixture here pins all four
// at once.
#[test]
fn bspline_channel_slice_rejects_nan_offset() {
    let raw: Vec<i16> = vec![0, 32767, -32767, 0];
    assert!(
        channel_slice(0, &raw, 4, 1, f32::NAN, 5.0).is_none(),
        "a NaN offset must drop the whole channel to the pose fallback, \
         not produce a NaN-poisoned control point"
    );
}

#[test]
fn bspline_channel_slice_rejects_nan_half_range() {
    let raw: Vec<i16> = vec![0, 32767, -32767, 0];
    assert!(
        channel_slice(0, &raw, 4, 1, 10.0, f32::NAN).is_none(),
        "a NaN half_range must drop the whole channel to the pose fallback"
    );
}

#[test]
fn bspline_channel_slice_rejects_infinite_quantization_params() {
    let raw: Vec<i16> = vec![0, 32767, -32767, 0];
    assert!(channel_slice(0, &raw, 4, 1, f32::INFINITY, 5.0).is_none());
    assert!(channel_slice(0, &raw, 4, 1, f32::NEG_INFINITY, 5.0).is_none());
    assert!(channel_slice(0, &raw, 4, 1, 10.0, f32::INFINITY).is_none());
    assert!(channel_slice(0, &raw, 4, 1, 10.0, f32::NEG_INFINITY).is_none());
}

/// Belt-and-braces layer: even with finite `offset`/`half_range`, a
/// pathologically large (but still finite) `half_range` can overflow
/// `deboor_cubic`'s repeated blending into a non-finite sampled value. This
/// pins that the sampled-value guards added at the three `push` call sites
/// (`extract_float_channel_bspline`, `extract_transform_channel_bspline`'s
/// translation/scale) actually reject such a value rather than propagating
/// it — reproduced directly against `dequant`/`deboor_cubic`, the same
/// primitives those call sites use.
#[test]
fn bspline_dequant_and_deboor_can_overflow_and_the_guard_catches_it() {
    // `dequant`'s ratio term (`raw / 32767.0`) is bounded to [-1, 1], so
    // `offset + ratio * half_range` alone only overflows when `offset` and
    // the scaled `half_range` are both already near f32::MAX and add past
    // it — both individually finite, both individually pass
    // `is_key_value_sane`, exactly the "finite but pathological" case the
    // belt-and-braces guard exists for.
    let poisoned = dequant(32767, f32::MAX, f32::MAX);
    assert!(!poisoned.is_finite(), "test fixture sanity: this must overflow");
    assert!(
        !is_key_value_sane(poisoned),
        "the sampled-value guard must recognize this as unsafe to push as a key"
    );
}

/// Regression: #4166 (NIFAL-D7-2026-09-11-01) — the rotation sub-channel
/// was the one #3765 left unguarded, and the two failure modes it admits
/// are *not* the ones the report predicted. This drives the production
/// guard directly rather than reproducing its arithmetic in the test.
#[test]
fn bspline_rotation_sample_rejects_infinite_control_points() {
    // The genuine NaN path: an infinite component survives to `inf * 0.0`.
    for poisoned in [
        [f32::INFINITY, 0.0, 0.0, 0.0],
        [0.0, f32::NEG_INFINITY, 0.0, 0.0],
        [0.0, 0.0, f32::INFINITY, 0.0],
        [0.0, 0.0, 0.0, f32::INFINITY],
    ] {
        assert!(
            normalized_rotation_sample(poisoned).is_none(),
            "an infinite control point must skip the sample, not push NaN: {poisoned:?}"
        );
    }
    // FLT_MAX sentinel components are rejected on the same sweep.
    assert!(normalized_rotation_sample([f32::MAX, 0.0, 0.0, 0.0]).is_none());
}

/// Regression: #4166. The case the issue named as its headline hazard,
/// with the correction that matters: control points that are individually
/// sane but square past `f32::MAX` do **not** produce NaN — every
/// component is finite, so `finite * 0.0 == 0.0` and the result is the
/// *zero quaternion*.
///
/// That is exactly why the fix is `len_sq.is_finite()` and not the
/// post-normalize `is_key_value_sane` check the issue prescribed:
/// `[0, 0, 0, 0]` passes `is_key_value_sane` on all four components, so a
/// post-normalize guard is blind to it. Asserted here so nobody
/// "simplifies" the guard back to the sibling one-liner.
#[test]
fn bspline_rotation_sample_substitutes_identity_when_squaring_overflows() {
    // sqrt(f32::MAX) ≈ 1.844e19 — just past it, so v*v is inf.
    let huge = 2.0e19f32;
    assert!(
        is_key_value_sane(huge),
        "fixture sanity: each component must individually pass the sibling guard"
    );
    assert!(
        !(huge * huge).is_finite(),
        "fixture sanity: squaring must overflow to inf"
    );

    let q = normalized_rotation_sample([huge, 0.0, 0.0, 0.0])
        .expect("individually-sane components must not skip the sample");
    assert_eq!(
        q,
        [1.0, 0.0, 0.0, 0.0],
        "an overflowing len_sq must fall to identity, never the zero quaternion"
    );

    // The refutation of the prescribed fix, pinned: the zero quaternion
    // the unguarded code produced would have passed a post-normalize check.
    assert!(
        [0.0f32, 0.0, 0.0, 0.0]
            .iter()
            .all(|v| is_key_value_sane(*v)),
        "a post-normalize is_key_value_sane guard cannot see the zero quaternion"
    );
}

/// Regression: #4166. A NaN control point was already safe before this
/// fix — `len_sq` is NaN, `NaN > f32::EPSILON` is false, and the
/// degenerate arm substitutes identity. Pinned so the guard's docs and
/// the code keep agreeing about which inputs were actually broken.
#[test]
fn bspline_rotation_sample_was_already_safe_against_nan_control_points() {
    for raw in [[f32::NAN; 4], [f32::NAN, 1.0, 0.0, 0.0]] {
        // Rejected up front now, but the point is that the *result* was
        // never NaN even before the guard existed.
        assert!(normalized_rotation_sample(raw).is_none());
        let len_sq: f32 = raw.iter().map(|v| v * v).sum();
        assert!(len_sq.is_nan() && !(len_sq > f32::EPSILON));
    }
}

/// Regression: #4166. The guard must not disturb ordinary content — a
/// non-unit but well-scaled quaternion still normalizes as before.
#[test]
fn bspline_rotation_sample_still_normalizes_ordinary_quaternions() {
    let q = normalized_rotation_sample([0.0, 3.0, 4.0, 0.0]).expect("ordinary sample must survive");
    let len = (q.iter().map(|v| v * v).sum::<f32>()).sqrt();
    assert!((len - 1.0).abs() < 1e-5, "must be unit length, got {len}");
    assert!(
        (q[1] - 0.6).abs() < 1e-5 && (q[2] - 0.8).abs() < 1e-5,
        "{q:?}"
    );
    // Near-zero stays identity, the pre-existing degenerate behaviour.
    assert_eq!(
        normalized_rotation_sample([0.0, 0.0, 0.0, 0.0]),
        Some([1.0, 0.0, 0.0, 0.0])
    );
}
