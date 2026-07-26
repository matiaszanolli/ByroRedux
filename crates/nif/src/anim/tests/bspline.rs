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
