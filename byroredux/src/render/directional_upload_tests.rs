use super::*;

/// Exterior at noon: `sun_intensity == SUN_INTENSITY_PEAK` → ramp
/// is exactly 1.0 → daytime brightness is unchanged from pre-#798.
/// Pins the conservative-normalization invariant: the fix must not
/// regress daytime surface lighting brightness.
#[test]
fn exterior_noon_preserves_pre_fix_brightness() {
    let (color, radius) =
        compute_directional_upload(&[0.7, 0.65, 0.55], false, SUN_INTENSITY_PEAK);
    assert_eq!(radius, 0.0, "exterior radius must be 0 (shadowed)");
    assert!((color[0] - 0.7).abs() < 1e-6);
    assert!((color[1] - 0.65).abs() < 1e-6);
    assert!((color[2] - 0.55).abs() < 1e-6);
}

/// Exterior at midnight: `sun_intensity == 0` → ramp is exactly
/// 0.0 → directional contribution is zero. THIS IS THE BUG FIX.
/// Pre-#798 the contribution was `directional_color * 1.0`
/// regardless of TOD; ceilings glowed with the TOD-NIGHT
/// `SKY_SUNLIGHT` colour from the (0,-1,0) direction.
#[test]
fn exterior_midnight_zeroes_directional_contribution() {
    let (color, radius) = compute_directional_upload(
        &[0.05, 0.07, 0.12], // typical TOD-NIGHT SKY_SUNLIGHT (dim blue)
        false,
        0.0,
    );
    assert_eq!(radius, 0.0);
    assert_eq!(
        color,
        [0.0, 0.0, 0.0],
        "midnight directional must be zeroed — ceilings/overhangs \
         would otherwise glow with NIGHT SKY_SUNLIGHT from (0,-1,0)"
    );
}

/// Exterior at sunrise (`sun_intensity == SUN_INTENSITY_PEAK / 2`):
/// ramp is 0.5 → contribution is exactly half of daytime. Pin the
/// linear ramp shape — a future change to `smoothstep` or
/// quadratic would regress the smooth dawn/dusk fade.
#[test]
fn exterior_sunrise_half_intensity_half_contribution() {
    let (color, _) =
        compute_directional_upload(&[0.6, 0.55, 0.40], false, SUN_INTENSITY_PEAK / 2.0);
    assert!((color[0] - 0.30).abs() < 1e-6);
    assert!((color[1] - 0.275).abs() < 1e-6);
    assert!((color[2] - 0.20).abs() < 1e-6);
}

/// Out-of-range `sun_intensity` (negative or > peak) clamps to
/// [0, 1]. Defends against a future `weather_system` regression
/// that produces an out-of-range value (e.g. an HDR multiplier
/// that bumps peak past 4.0 without updating SUN_INTENSITY_PEAK).
/// Negative clamps to 0 → no directional contribution; over-cap
/// clamps to 1 → daytime equivalent.
#[test]
fn exterior_out_of_range_intensity_is_clamped() {
    let (negative, _) = compute_directional_upload(&[1.0; 3], false, -10.0);
    assert_eq!(negative, [0.0; 3], "negative intensity must clamp to zero");
    let (over_cap, _) = compute_directional_upload(&[1.0; 3], false, 100.0);
    assert_eq!(
        over_cap, [1.0; 3],
        "over-cap intensity must clamp to peak (1.0× ramp)"
    );
}

/// Interior fill: 0.6× constant scale, `radius == -1` for
/// shader-side shadow-skip. Independent of `sun_intensity` — the
/// XCLL fill is an aesthetic constant, not a TOD-driven sun. Pin
/// the independence: if a future change accidentally couples the
/// interior arm to the sun ramp, every interior would dim/brighten
/// with the wall-clock hour.
#[test]
fn interior_uses_fixed_fill_independent_of_sun_intensity() {
    let (noon_color, noon_radius) =
        compute_directional_upload(&[0.5, 0.5, 0.5], true, SUN_INTENSITY_PEAK);
    let (midnight_color, midnight_radius) =
        compute_directional_upload(&[0.5, 0.5, 0.5], true, 0.0);
    assert_eq!(
        noon_color, midnight_color,
        "interior fill must NOT vary with sun_intensity"
    );
    assert_eq!(
        noon_radius, -1.0,
        "interior radius must be -1 (unshadowed fill)"
    );
    assert_eq!(midnight_radius, -1.0);
    // 0.6× scale per the established convention.
    assert!((noon_color[0] - 0.30).abs() < 1e-6);
}

/// Sanity check that the constant matches `weather_system`'s
/// ramp peak. If `weather_system` is retuned to a different peak
/// (e.g., 5.0 for HDR headroom) without updating
/// `SUN_INTENSITY_PEAK` here, daytime surface lighting would
/// silently regress. This test fires whenever the two values
/// drift — pulling the live peak via systems.rs reflection isn't
/// possible (computed inline), so the cross-check is a literal
/// match against the known-good value.
#[test]
fn directional_upload_peak_matches_weather_system() {
    // weather_system at byroredux/src/systems.rs:1446-1454 uses
    // 4.0 as the daytime ceiling; the linear ramps at 6-7h /
    // 17-18h reach this peak at the steady-state hours. If that
    // value changes, this test is the canary.
    assert_eq!(
        SUN_INTENSITY_PEAK, 4.0,
        "SUN_INTENSITY_PEAK must match weather_system's daytime peak \
         (`systems.rs:1446-1454`); a tuning change there must update \
         this constant in the same commit or every exterior surface \
         dims/brightens by the ratio."
    );
}
