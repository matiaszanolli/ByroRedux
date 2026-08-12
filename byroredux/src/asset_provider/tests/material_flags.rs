//! Conductor diffuse-tint chromaticity (#1591) + `bgsm_metalness` (#1987).
//!
//! Extracted from the 2051-LOC `asset_provider/tests.rs` (#2411 / TD1-010)
//! at the topic-divider comments that file already carried. Contents
//! unchanged.

use super::super::*;

// ── #1591 — conductor diffuse-tint must use mult-free chromaticity ──

/// The blend uses `specular_color` (chromaticity), not
/// `specular_color × specular_mult`. The real vanilla strong-metal cases
/// the audit sampled: pre-fix the blend target was `spec × mult`, which
/// darkened toward black (mult<1) or overshot past 1.0 (mult>1). Because
/// `conductor_diffuse_tint` takes no `mult` argument, the tint is
/// structurally mult-invariant — these assert the exact mult-free values.
#[test]
fn conductor_tint_is_mult_free() {
    // spec=[1.0,0.255,0.255]; diffuse=[0.5,0.5,0.5].
    // Mult-free target: 0.5*diffuse + 0.5*spec.
    let got = conductor_diffuse_tint([0.5, 0.5, 0.5], [1.0, 0.255, 0.255]);
    assert!((got[0] - 0.75).abs() < 1e-6, "{got:?}");
    assert!((got[1] - 0.3775).abs() < 1e-6, "{got:?}");
    assert!((got[2] - 0.3775).abs() < 1e-6, "{got:?}");
    // The old mult=0.25 fold would have blended toward [0.25,0.064,0.064]
    // → diffuse ≈ [0.375,0.282,0.282], strictly darker than the above.
    assert!(got[0] > 0.375, "mult<1 must NOT darken the tint: {got:?}");
}

/// `mult > 1` previously overshot a channel past 1.0 unclamped into
/// `GpuMaterial.diffuse_*`. The mult-free blend of two `[0,1]` inputs is
/// already in range, and the `[0,1]` clamp guards a >1 diffuse input.
#[test]
#[allow(clippy::approx_constant)] // 0.318 is authored specular data, not 1/pi.
fn conductor_tint_clamps_to_unit_range() {
    // Both inputs in range → result in range (no overshoot).
    let in_range = conductor_diffuse_tint([1.0, 1.0, 1.0], [1.0, 0.467, 0.318]);
    assert!(
        in_range.iter().all(|&c| (0.0..=1.0).contains(&c)),
        "{in_range:?}"
    );
    // A >1 diffuse input (defensive) is clamped.
    let clamped = conductor_diffuse_tint([2.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    assert_eq!(clamped[0], 1.0, "0.5*2.0 + 0.5*1.0 = 1.5 → clamped to 1.0");
}

// ── #1987 — `bgsm_metalness` pin against the #1476 luminance regression ──

/// Legacy (non-pbr) branch: white/achromatic spec is a dielectric, not a
/// conductor. This is the exact case (`paintpeelingconcrete`-style
/// `spec=[1,1,1]`) that the luminance formula got backwards — it must
/// read ~0.0, never the mirror-chrome `1.0` the pre-fix code produced.
#[test]
fn bgsm_metalness_legacy_white_spec_is_dielectric() {
    let m = bgsm_metalness([1.0, 1.0, 1.0], false);
    assert!(m < 1.0e-6, "white spec must classify as dielectric: {m}");
}

/// Legacy branch: tinted spec (e.g. `metallocker`-style `[1,0.85,0.70]`)
/// is a conductor — saturation-derived metalness must read clearly above
/// zero.
#[test]
fn bgsm_metalness_legacy_tinted_spec_is_conductor() {
    let m = bgsm_metalness([1.0, 0.85, 0.70], false);
    assert!(m > 0.1, "tinted spec must classify as metallic: {m}");
}

/// Legacy branch is mult-invariant by construction (`mult` is folded in
/// by the caller before pbr F0-luminance, never before this saturation
/// formula) — pass the same white spec regardless of authored mult and
/// confirm it still reads dielectric.
#[test]
fn bgsm_metalness_legacy_near_zero_spec_is_dielectric() {
    let m = bgsm_metalness([0.0, 0.0, 0.0], false);
    assert_eq!(
        m, 0.0,
        "near-zero spec magnitude must not divide-by-zero into metallic"
    );
}

/// pbr branch: F0 at the dielectric floor (0.04 achromatic) reads ~0.0.
#[test]
fn bgsm_metalness_pbr_dielectric_floor_is_zero() {
    let m = bgsm_metalness([0.04, 0.04, 0.04], true);
    assert!(
        m.abs() < 1.0e-5,
        "F0=0.04 must read as dielectric floor: {m}"
    );
}

/// pbr branch: full-white F0 is a fully metallic conductor.
#[test]
fn bgsm_metalness_pbr_white_f0_is_metallic() {
    let m = bgsm_metalness([1.0, 1.0, 1.0], true);
    assert!(
        (m - 1.0).abs() < 1.0e-6,
        "F0=1.0 must read as fully metallic: {m}"
    );
}
