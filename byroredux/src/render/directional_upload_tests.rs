use super::*;

/// Exterior at noon: `sun_intensity == SUN_INTENSITY_PEAK` → ramp
/// is exactly 1.0 → daytime brightness is unchanged from pre-#798.
/// Pins the conservative-normalization invariant: the fix must not
/// regress daytime surface lighting brightness.
#[test]
fn exterior_noon_preserves_pre_fix_brightness() {
    let color = compute_directional_upload(&[0.7, 0.65, 0.55], false, SUN_INTENSITY_PEAK, None);
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
    let color = compute_directional_upload(
        &[0.05, 0.07, 0.12], // typical TOD-NIGHT SKY_SUNLIGHT (dim blue)
        false,
        0.0,
        None,
    );
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
    let color =
        compute_directional_upload(&[0.6, 0.55, 0.40], false, SUN_INTENSITY_PEAK / 2.0, None);
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
    let negative = compute_directional_upload(&[1.0; 3], false, -10.0, None);
    assert_eq!(negative, [0.0; 3], "negative intensity must clamp to zero");
    let over_cap = compute_directional_upload(&[1.0; 3], false, 100.0, None);
    assert_eq!(
        over_cap, [1.0; 3],
        "over-cap intensity must clamp to peak (1.0× ramp)"
    );
}

/// Interior fallback calibration: a missing Directional Fade keeps the
/// established 0.6× scale, independent of `sun_intensity` — XCLL is authored
/// cell lighting, not a TOD-driven weather sun. Classification is pinned by
/// `collect_lights` tests: XCLL still uploads as a shadowable directional key.
#[test]
fn interior_calibration_is_independent_of_exterior_sun() {
    let noon_color = compute_directional_upload(&[0.5, 0.5, 0.5], true, SUN_INTENSITY_PEAK, None);
    let midnight_color = compute_directional_upload(&[0.5, 0.5, 0.5], true, 0.0, None);
    assert_eq!(
        noon_color, midnight_color,
        "interior XCLL source must NOT vary with sun_intensity"
    );
    // 0.6× scale per the established convention.
    assert!((noon_color[0] - 0.30).abs() < 1e-6);
}

#[test]
fn interior_authored_directional_fade_replaces_legacy_scale() {
    let full = compute_directional_upload(&[0.5; 3], true, 0.0, Some(1.0));
    let quarter = compute_directional_upload(&[0.5; 3], true, 0.0, Some(0.25));
    let disabled = compute_directional_upload(&[0.5; 3], true, 0.0, Some(0.0));

    assert_eq!(full, [0.5; 3]);
    assert_eq!(quarter, [0.125; 3]);
    assert_eq!(disabled, [0.0; 3]);
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
    // #2813 — assert against the PRODUCER, not a third hardcoded `4.0`.
    // The pre-fix version compared this constant to a literal, so it went
    // green whenever the producer moved and this constant did not — the
    // exact drift it was written to catch.
    let peak_from_producer =
        crate::systems::weather::compute_sun_arc(12.0, [6.0, 10.0, 18.0, 22.0]).1;
    assert_eq!(
        SUN_INTENSITY_PEAK, peak_from_producer,
        "SUN_INTENSITY_PEAK must equal the daytime ceiling \
         `compute_sun_arc` actually ramps to; a tuning change there must \
         reach this divisor or every exterior surface dims/brightens by \
         the ratio."
    );
    // …and the normalised ramp must therefore span exactly [0, 1].
    assert_eq!(
        (peak_from_producer / SUN_INTENSITY_PEAK).clamp(0.0, 1.0),
        1.0,
        "full daylight must normalise to full directional strength"
    );
}
