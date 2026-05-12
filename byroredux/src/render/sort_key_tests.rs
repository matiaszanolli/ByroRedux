use super::f32_sortable_u32;

/// Helper — assert `a < b` implies `key(a) < key(b)` for a sorted
/// reference slice of f32 values, covering negatives, zero,
/// positives, and infinities.
#[test]
fn sortable_u32_preserves_finite_ordering() {
    let sorted = [
        f32::NEG_INFINITY,
        -1.0e38,
        -1.0,
        -f32::MIN_POSITIVE, // smallest-magnitude negative normal
        -0.0,
        0.0,
        f32::MIN_POSITIVE,
        1.0,
        1.0e38,
        f32::INFINITY,
    ];
    for window in sorted.windows(2) {
        let ka = f32_sortable_u32(window[0]);
        let kb = f32_sortable_u32(window[1]);
        assert!(
            ka <= kb,
            "f32 order {} < {} must map to u32 order {} <= {}",
            window[0],
            window[1],
            ka,
            kb
        );
    }
}

/// The negative sign branch was the whole point of #306. A naive
/// `bits` sort would invert the order on negatives — positive
/// `-1.0.to_bits()` is larger than `-1000.0.to_bits()` because
/// IEEE 754 stores magnitude in the low bits regardless of sign.
#[test]
fn sortable_u32_orders_negatives_correctly() {
    // -1000 < -1 < 0 must produce key(-1000) < key(-1) < key(0)
    let k_minus_1000 = f32_sortable_u32(-1000.0);
    let k_minus_1 = f32_sortable_u32(-1.0);
    let k_zero = f32_sortable_u32(0.0);
    assert!(
        k_minus_1000 < k_minus_1,
        "-1000 should sort below -1 (got {k_minus_1000} vs {k_minus_1})"
    );
    assert!(
        k_minus_1 < k_zero,
        "-1 should sort below 0 (got {k_minus_1} vs {k_zero})"
    );
    // Raw `to_bits` would reverse this — -1000 has smaller
    // magnitude bits than -1, so `(-1000.0).to_bits() > (-1.0).to_bits()`.
    assert!(
        (-1000f32).to_bits() > (-1f32).to_bits(),
        "sanity: raw to_bits DOES reverse negatives, so the fix is load-bearing"
    );
}

/// +0.0 and -0.0 differ only in the sign bit but must hit the
/// same ordering bucket (they compare equal in IEEE 754).
#[test]
fn sortable_u32_handles_signed_zero() {
    let k_pos = f32_sortable_u32(0.0);
    let k_neg = f32_sortable_u32(-0.0);
    // -0.0 sorts strictly below +0.0 under our total order
    // (that's what `max_by(normalized_weight)`-style code expects
    // when both appear — deterministic placement without special-casing).
    // Specifically: -0.0 has sign bit set, so key = !bits =
    // !0x80000000 = 0x7FFFFFFF. +0.0 has sign bit clear, so key =
    // bits ^ 0x80000000 = 0x80000000. Hence k_neg < k_pos.
    assert_eq!(k_neg, 0x7FFF_FFFF);
    assert_eq!(k_pos, 0x8000_0000);
    assert!(k_neg < k_pos);
}

/// Infinities land at the extreme ends of the u32 range — no
/// wraparound, no overlap with any finite value.
#[test]
fn sortable_u32_places_infinities_at_extremes() {
    let k_neg_inf = f32_sortable_u32(f32::NEG_INFINITY);
    let k_pos_inf = f32_sortable_u32(f32::INFINITY);
    let k_huge_neg = f32_sortable_u32(-1.0e38);
    let k_huge_pos = f32_sortable_u32(1.0e38);
    assert!(k_neg_inf < k_huge_neg);
    assert!(k_huge_pos < k_pos_inf);
}

/// Transparent back-to-front path inverts the key via `!` — that
/// inversion must still produce a legal total ordering (i.e., the
/// opposite of the forward ordering). Tests the actual usage
/// pattern the sort in `build_render_data` relies on.
#[test]
fn sortable_u32_invertible_for_back_to_front() {
    let near = f32_sortable_u32(1.0); // close to camera
    let far = f32_sortable_u32(100.0); // far
                                       // Forward: near < far (front-to-back opaque path)
    assert!(near < far);
    // Back-to-front (transparent path): !near > !far so `far`
    // draws first, `near` last — exactly what alpha-compositing
    // needs.
    assert!(!near > !far);
}

/// Denormal (subnormal) values must still sort strictly between
/// zero and the smallest normal positive. A prior implementation
/// that treated subnormals specially or clamped to zero would
/// collapse a band of distinct depths into one sort bucket.
#[test]
fn sortable_u32_orders_denormals() {
    // f32 smallest subnormal = 1e-45 (approximately).
    let denorm = f32::from_bits(1); // positive denormal
    let k_zero = f32_sortable_u32(0.0);
    let k_denorm = f32_sortable_u32(denorm);
    let k_min_normal = f32_sortable_u32(f32::MIN_POSITIVE);
    assert!(k_zero < k_denorm);
    assert!(k_denorm < k_min_normal);
}

/// NaN has a well-defined position in the total ordering — it
/// doesn't interfere with the finite range. Any NaN produced by
/// an out-of-frustum clip-space projection won't silently drop
/// into a random sort bucket; it ends up at the positive end
/// alongside +infinity.
#[test]
fn sortable_u32_places_canonical_nan_past_positive_infinity() {
    let k_nan = f32_sortable_u32(f32::NAN);
    let k_pos_inf = f32_sortable_u32(f32::INFINITY);
    // Canonical f32::NAN has the sign bit clear, so it falls in
    // the `bits ^ 0x80000000` branch and sorts above +infinity.
    assert!(k_nan > k_pos_inf);
}
