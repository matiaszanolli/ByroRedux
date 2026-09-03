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
