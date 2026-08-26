//! IEEE 754 binary16 edge-case pins for `decode::half_to_f32` (#1597 / FO4-D4-LOW-01).
//!
//! The canonical half decoder feeds the FO4 BSVER >= 130 vertex path; a future
//! refactor of the bit-twiddling could silently regress denormal/NaN/Inf
//! handling with no test to catch it. One canonical impl pins all consumers.

use super::*;

#[test]
fn half_to_f32_pins_ieee754_binary16_edge_classes() {
    // +0 and -0
    assert_eq!(half_to_f32(0x0000).to_bits(), 0x0000_0000, "+0");
    assert_eq!(half_to_f32(0x8000).to_bits(), 0x8000_0000, "-0");

    // Smallest positive denormal: half 0x0001 = 2^-24
    assert_eq!(half_to_f32(0x0001), 2.0_f32.powi(-24), "smallest denormal");
    // Largest denormal: half 0x03FF = (1023/1024) * 2^-14
    assert_eq!(
        half_to_f32(0x03FF),
        (1023.0_f32 / 1024.0) * 2.0_f32.powi(-14),
        "largest denormal"
    );

    // +Inf / -Inf (exp == 31, mant == 0)
    assert!(
        half_to_f32(0x7C00).is_infinite() && half_to_f32(0x7C00) > 0.0,
        "+Inf"
    );
    assert!(
        half_to_f32(0xFC00).is_infinite() && half_to_f32(0xFC00) < 0.0,
        "-Inf"
    );

    // NaN with a payload (exp == 31, mant != 0); payload preserved in mantissa.
    let nan = half_to_f32(0x7E00);
    assert!(nan.is_nan(), "0x7E00 must decode to NaN");
    // Mantissa bit 9 (0x200) shifts left 13 -> f32 bit 22 set.
    assert_eq!(nan.to_bits() & (1 << 22), 1 << 22, "NaN payload preserved");

    // Smallest normal: half 0x0400 = 2^-14
    assert_eq!(half_to_f32(0x0400), 2.0_f32.powi(-14), "smallest normal");
    // Normal mid-range: half 0x3C00 = 1.0
    assert_eq!(half_to_f32(0x3C00), 1.0, "1.0");
    // Normal: half 0xC000 = -2.0
    assert_eq!(half_to_f32(0xC000), -2.0, "-2.0");
    // Max finite half 0x7BFF = 65504.0
    assert_eq!(half_to_f32(0x7BFF), 65504.0, "max finite half");
}

/// #2599 / FO4-D4-03 — `crates/facegen` carries a third independent copy of
/// this decoder (`byroredux_facegen::half_to_f32`). It can't call the
/// canonical one: this impl is `pub(crate)`, and facegen is deliberately
/// dependency-light (production deps: `thiserror` only), so pulling in
/// `byroredux-nif` — or hoisting the shared 20 lines into `byroredux-core`
/// and pulling *that* into facegen — costs far more than the duplication.
///
/// The real risk the audit named is not the duplication itself but that
/// nothing would catch a *divergence*: a future subnormal / NaN-payload
/// correction applied here would silently not propagate to facegen. This
/// pins the two bit-for-bit across the entire input domain, so any such
/// edit fails here until facegen is updated to match.
///
/// Exhaustive rather than sampled: `u16` is only 65_536 values, which
/// covers every sign / exponent / mantissa class (including all denormals
/// and every NaN payload) in well under a millisecond.
#[test]
fn facegen_half_to_f32_copy_matches_canonical_bit_for_bit() {
    for raw in 0..=u16::MAX {
        let canonical = half_to_f32(raw);
        let facegen = byroredux_facegen::half_to_f32(raw);
        // Compare raw bits, not values: `==` is false for NaN and would
        // also let a -0.0 / +0.0 divergence through. Bit equality is the
        // only assertion that catches a payload or sign-of-zero drift.
        assert_eq!(
            canonical.to_bits(),
            facegen.to_bits(),
            "half 0x{raw:04X} diverged: canonical 0x{:08X} vs facegen 0x{:08X}",
            canonical.to_bits(),
            facegen.to_bits(),
        );
    }
}
