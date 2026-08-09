//! Physical radiometry and colorimetry shared by the emissive-media path.
//!
//! Fire, explosions, glowing embers, and hot gas are all *thermal emitters*:
//! their colour is not authored, it follows from temperature. This module is
//! the single place that turns a temperature in kelvin into linear-RGB
//! radiance, so the fog/fire translation boundary, the derived light sources,
//! and (later) the voxel-fire simulation's LUT all agree by construction.
//!
//! # Why this is not "just a colour ramp"
//!
//! A two-point RGB LERP — the legacy `ParticleEmitter::start_color` /
//! `end_color` model — cannot reproduce a flame, because a flame's apparent
//! colour and its apparent *brightness* are not independent. Both fall out of
//! one number:
//!
//! - **Chromaticity** comes from Planck's law integrated against the CIE 1931
//!   colour-matching functions. This is exact, not a fit to reference imagery.
//! - **Magnitude** comes from the *visible-band* luminance of the same
//!   integral — [`blackbody_visible_radiance_ratio`].
//!
//! The magnitude law deserves care, because the obvious choice is wrong. The
//! Stefan-Boltzmann law (`M ∝ T^4`, [`stefan_boltzmann_ratio`]) governs
//! **total** radiant exitance across all wavelengths, and a 900 K ember emits
//! almost all of it as invisible infrared. As temperature rises the spectrum
//! also *shifts into* the visible band, so visible luminance climbs far
//! faster than `T^4` — for a 900 K to 1800 K step, by well over the 16x that
//! `T^4` alone predicts. Scaling a luminance-normalized colour by `T^4` would
//! silently discard that shift and flatten exactly the flame-core-versus-
//! cooling-tail contrast that makes fire read as hot.
//!
//! So [`blackbody_radiance_srgb`] takes hue from the normalized chromaticity
//! and magnitude from the CIE `Y` integral, which captures both effects at
//! once and remains exact. `stefan_boltzmann_ratio` is retained as the
//! total-power law for radiated-energy budgeting and as an independent
//! physics cross-check, not as the rendering magnitude.
//!
//! # Colour space
//!
//! Output is **linear** sRGB-primary RGB (Rec. 709 primaries, D65 white),
//! which is the renderer's working space all the way to the ACES tone map.
//!
//! This deliberately does *not* contradict the engine rule that authored
//! Gamebryo colours are raw monitor-space floats and must never be run
//! through `srgb_to_linear`. That rule governs **authored** colour data
//! arriving from game records. The values here are not authored — they are
//! computed from a physical spectrum, and a spectrum resolves to linear
//! tristimulus values by definition. No transfer function is applied or
//! implied.
//!
//! # References
//!
//! - Planck's law and the Stefan-Boltzmann law; CODATA 2018 / SI-2019
//!   constants (`h`, `c`, `k_B` are all exact by definition since the 2019
//!   SI redefinition).
//! - Wyman, Sloan & Shirley, "Simple Analytic Approximations to the CIE XYZ
//!   Colour Matching Functions", *Journal of Computer Graphics Techniques*
//!   vol. 2 no. 2, 2013 — the multi-lobe piecewise-Gaussian fit used by
//!   [`cie_1931_xyz_bar`]. Chosen over a tabulated CMF because it is
//!   branch-cheap, allocation-free, and portable verbatim into GLSL when the
//!   voxel-fire simulation needs a shader-side temperature LUT.
//! - CIE XYZ to linear sRGB matrix: IEC 61966-2-1 (sRGB), D65 white point.

/// Planck constant `h`, joule-seconds. Exact by SI definition (2019).
const PLANCK_CONSTANT: f64 = 6.626_070_15e-34;
/// Speed of light in vacuum `c`, metres per second. Exact by SI definition.
const SPEED_OF_LIGHT: f64 = 2.997_924_58e8;
/// Boltzmann constant `k_B`, joules per kelvin. Exact by SI definition (2019).
const BOLTZMANN_CONSTANT: f64 = 1.380_649e-23;

/// Lower bound of the CMF integration domain, nanometres. The CIE 1931
/// observer is defined over 360-830 nm; outside it the response is zero.
const SPECTRUM_MIN_NM: f64 = 360.0;
/// Upper bound of the CMF integration domain, nanometres.
const SPECTRUM_MAX_NM: f64 = 830.0;
/// Integration step, nanometres. 1 nm over the 470 nm domain is 471 samples —
/// far finer than the analytic CMF fit's own accuracy, and this runs at
/// translation time (or once per LUT build), never per frame.
const SPECTRUM_STEP_NM: f64 = 1.0;

/// Temperatures at or below this are treated as non-emissive. Well below any
/// physical incandescence threshold (~800 K for barely-visible dull red), so
/// this only rejects "unset"/garbage values, never a real cooling curve.
pub const MIN_EMISSIVE_TEMPERATURE_K: f32 = 1.0;

/// Spectral radiance of an ideal blackbody at wavelength `lambda_nm` and
/// temperature `temperature_k`, per Planck's law.
///
/// Returns W · sr⁻¹ · m⁻³ (radiance per unit wavelength). The absolute unit
/// cancels in [`blackbody_chromaticity_srgb`], which normalizes by luminance;
/// it is retained here so the function is independently meaningful and
/// testable against the Wien displacement law.
pub fn planck_spectral_radiance(lambda_nm: f64, temperature_k: f64) -> f64 {
    // `is_finite` first so NaN is rejected explicitly rather than relying on
    // a negated comparison to catch it.
    if !lambda_nm.is_finite() || lambda_nm <= 0.0 {
        return 0.0;
    }
    if !temperature_k.is_finite() || temperature_k <= 0.0 {
        return 0.0;
    }
    let lambda_m = lambda_nm * 1.0e-9;
    let numerator = 2.0 * PLANCK_CONSTANT * SPEED_OF_LIGHT * SPEED_OF_LIGHT
        / (lambda_m * lambda_m * lambda_m * lambda_m * lambda_m);
    let exponent =
        PLANCK_CONSTANT * SPEED_OF_LIGHT / (lambda_m * BOLTZMANN_CONSTANT * temperature_k);
    // exp() overflows to +inf for cold/short-wavelength combinations; the
    // limit of the whole expression there is 0, which `numerator / inf`
    // already yields. Guard only the exp(x) - 1 -> 0 cancellation at the
    // opposite extreme, where the series limit is numerator/(exponent) but
    // the division would land on a denormal.
    let denominator = exponent.exp_m1();
    if !denominator.is_finite() || denominator <= 0.0 {
        return 0.0;
    }
    numerator / denominator
}

/// One lobe of the Wyman-Sloan-Shirley piecewise-Gaussian CMF fit.
///
/// `inv_sigma_low` / `inv_sigma_high` are *inverse* widths (they multiply the
/// wavelength offset), matching the paper's formulation exactly.
fn cmf_lobe(
    wavelength: f64,
    amplitude: f64,
    mean: f64,
    inv_sigma_low: f64,
    inv_sigma_high: f64,
) -> f64 {
    let inv_sigma = if wavelength < mean {
        inv_sigma_low
    } else {
        inv_sigma_high
    };
    let t = (wavelength - mean) * inv_sigma;
    amplitude * (-0.5 * t * t).exp()
}

/// CIE 1931 2° standard-observer colour-matching functions `(x̄, ȳ, z̄)` at
/// `lambda_nm`, via the Wyman-Sloan-Shirley multi-lobe analytic fit.
///
/// Coefficients are transcribed from the JCGT 2013 paper; see the module
/// header. The `x̄` fit's third lobe is genuinely negative-amplitude — that
/// is the fit reproducing the real CMF's shape, not a sign error.
pub fn cie_1931_xyz_bar(lambda_nm: f64) -> [f64; 3] {
    let x = cmf_lobe(lambda_nm, 0.362, 442.0, 0.0624, 0.0374)
        + cmf_lobe(lambda_nm, 1.056, 599.8, 0.0264, 0.0323)
        + cmf_lobe(lambda_nm, -0.065, 501.1, 0.0490, 0.0382);
    let y = cmf_lobe(lambda_nm, 0.821, 568.8, 0.0213, 0.0247)
        + cmf_lobe(lambda_nm, 0.286, 530.9, 0.0613, 0.0322);
    let z = cmf_lobe(lambda_nm, 1.217, 437.0, 0.0845, 0.0278)
        + cmf_lobe(lambda_nm, 0.681, 459.0, 0.0385, 0.0725);
    [x, y, z]
}

/// Integrate a blackbody spectrum at `temperature_k` against the CIE 1931
/// observer, returning unnormalized tristimulus `(X, Y, Z)`.
pub fn blackbody_xyz(temperature_k: f32) -> [f64; 3] {
    let temperature = temperature_k as f64;
    if !temperature.is_finite() || temperature <= 0.0 {
        return [0.0; 3];
    }
    let mut xyz = [0.0f64; 3];
    let mut lambda = SPECTRUM_MIN_NM;
    while lambda <= SPECTRUM_MAX_NM {
        let radiance = planck_spectral_radiance(lambda, temperature);
        let bar = cie_1931_xyz_bar(lambda);
        xyz[0] += radiance * bar[0];
        xyz[1] += radiance * bar[1];
        xyz[2] += radiance * bar[2];
        lambda += SPECTRUM_STEP_NM;
    }
    // The Riemann weight is a constant factor across all three channels, so
    // it cancels under the luminance normalization every caller applies. It
    // is applied anyway so this function's output is meaningful standalone.
    for channel in &mut xyz {
        *channel *= SPECTRUM_STEP_NM;
    }
    xyz
}

/// Convert CIE XYZ to linear sRGB-primary RGB (Rec. 709 primaries, D65).
///
/// Matrix from IEC 61966-2-1. No transfer function is applied — the result is
/// linear, which is the renderer's working space.
pub fn xyz_to_linear_srgb(xyz: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = xyz;
    [
        3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z,
        -0.969_266_0 * x + 1.876_010_8 * y + 0.041_556_0 * z,
        0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z,
    ]
}

/// Linear-sRGB chromaticity of an ideal blackbody at `temperature_k`,
/// normalized to unit luminance (`Y = 1`).
///
/// This is pure hue — all magnitude information is deliberately removed so
/// the caller applies the Stefan-Boltzmann `T^4` scaling separately and both
/// halves stay independently testable.
///
/// Very low and very high temperatures fall outside the sRGB gamut, where one
/// channel goes negative. Clamping that channel to zero *raises* the realized
/// luminance slightly above 1 at those extremes (removing a negative term
/// increases the weighted sum). That is the correct trade for a
/// display-referred working space: the alternative — desaturating toward
/// white to stay in gamut — would misrepresent a deep-red ember as pink.
/// Callers that need magnitude use [`blackbody_visible_radiance_ratio`],
/// which is computed from the unclamped CIE integral and is therefore immune
/// to this.
///
/// Returns `None` when the temperature is non-finite, non-positive, or below
/// [`MIN_EMISSIVE_TEMPERATURE_K`].
pub fn blackbody_chromaticity_srgb(temperature_k: f32) -> Option<[f32; 3]> {
    if !temperature_k.is_finite() || temperature_k < MIN_EMISSIVE_TEMPERATURE_K {
        return None;
    }
    let xyz = blackbody_xyz(temperature_k);
    let luminance = xyz[1];
    if !luminance.is_finite() || luminance <= 0.0 {
        return None;
    }
    let normalized = [xyz[0] / luminance, 1.0, xyz[2] / luminance];
    let rgb = xyz_to_linear_srgb(normalized);
    let clamped = [
        rgb[0].max(0.0) as f32,
        rgb[1].max(0.0) as f32,
        rgb[2].max(0.0) as f32,
    ];
    if !clamped.iter().all(|c| c.is_finite()) {
        return None;
    }
    Some(clamped)
}

/// Relative **visible** radiance of a blackbody at `temperature_k` against a
/// reference temperature: the ratio of their CIE `Y` (luminance) integrals.
///
/// This is the magnitude law the renderer wants. Unlike
/// [`stefan_boltzmann_ratio`] it accounts for the spectral shift into and out
/// of the visible band, so a cooling flame dims at the rate an observer
/// actually perceives rather than at the much gentler total-power rate. See
/// the module header for why the distinction is load-bearing.
///
/// Returns `1.0` when `temperature_k == reference_k`, and `0.0` for
/// non-physical inputs.
pub fn blackbody_visible_radiance_ratio(temperature_k: f32, reference_k: f32) -> f32 {
    if !temperature_k.is_finite() || !reference_k.is_finite() || reference_k <= 0.0 {
        return 0.0;
    }
    let reference_luminance = blackbody_xyz(reference_k)[1];
    if !reference_luminance.is_finite() || reference_luminance <= 0.0 {
        return 0.0;
    }
    let luminance = blackbody_xyz(temperature_k)[1];
    if !luminance.is_finite() || luminance <= 0.0 {
        return 0.0;
    }
    (luminance / reference_luminance) as f32
}

/// Relative **total** radiant exitance of a blackbody at `temperature_k`
/// against a reference temperature, per the Stefan-Boltzmann law (`M ∝ T^4`).
///
/// This is total emitted power across all wavelengths, most of which is
/// infrared for flame-range temperatures. Use it for radiated-energy
/// budgeting (an explosion's heat output, ignition thresholds), **not** for
/// rendering magnitude — see [`blackbody_visible_radiance_ratio`].
///
/// Returns `1.0` when `temperature_k == reference_k`.
pub fn stefan_boltzmann_ratio(temperature_k: f32, reference_k: f32) -> f32 {
    if !temperature_k.is_finite() || !reference_k.is_finite() || reference_k <= 0.0 {
        return 0.0;
    }
    let ratio = (temperature_k.max(0.0) / reference_k) as f64;
    (ratio * ratio * ratio * ratio) as f32
}

/// Linear-sRGB radiance of an ideal blackbody at `temperature_k`.
///
/// Combines the exact chromaticity from [`blackbody_chromaticity_srgb`] with
/// the exact visible-band magnitude law from
/// [`blackbody_visible_radiance_ratio`], anchored so that a blackbody at
/// `reference_k` has luminance `reference_radiance`.
///
/// The two temperatures and the physics between them are exact; only
/// `reference_radiance` is an exposure choice, and it is a caller-supplied
/// parameter precisely so that choice stays explicit and tunable rather than
/// baked into a magic constant here.
pub fn blackbody_radiance_srgb(
    temperature_k: f32,
    reference_k: f32,
    reference_radiance: f32,
) -> Option<[f32; 3]> {
    if !reference_radiance.is_finite() || reference_radiance < 0.0 {
        return None;
    }
    let chromaticity = blackbody_chromaticity_srgb(temperature_k)?;
    let scale = reference_radiance * blackbody_visible_radiance_ratio(temperature_k, reference_k);
    if !scale.is_finite() {
        return None;
    }
    let radiance = [
        chromaticity[0] * scale,
        chromaticity[1] * scale,
        chromaticity[2] * scale,
    ];
    radiance.iter().all(|c| c.is_finite()).then_some(radiance)
}

/// Photometric luminance of a linear-sRGB triple (Rec. 709 luma weights).
pub fn linear_srgb_luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wien's displacement law is an independent analytic consequence of
    /// Planck's law: the spectral radiance peak sits at
    /// `lambda_peak = b / T` with `b ≈ 2.897771955e-3 m·K`. Recovering it by
    /// brute-force search over `planck_spectral_radiance` validates the
    /// Planck implementation against physics rather than against itself.
    #[test]
    fn planck_peak_matches_wien_displacement_law() {
        const WIEN_B_NM_K: f64 = 2.897_771_955e6; // b in nm·K
        for temperature in [1000.0_f64, 2500.0, 5772.0, 12000.0] {
            let expected_peak = WIEN_B_NM_K / temperature;
            let mut best_lambda = 0.0;
            let mut best_radiance = f64::NEG_INFINITY;
            // Sweep well outside the visible band so the peak is genuinely
            // found, not clipped to a domain edge.
            let mut lambda = 10.0;
            while lambda <= 20_000.0 {
                let radiance = planck_spectral_radiance(lambda, temperature);
                if radiance > best_radiance {
                    best_radiance = radiance;
                    best_lambda = lambda;
                }
                lambda += 0.5;
            }
            let relative_error = (best_lambda - expected_peak).abs() / expected_peak;
            assert!(
                relative_error < 1.0e-3,
                "Planck peak at {temperature} K was {best_lambda} nm, Wien predicts \
                 {expected_peak} nm (relative error {relative_error})"
            );
        }
    }

    /// The CIE 1931 `ȳ` curve is the photopic luminous-efficiency function,
    /// which peaks at 555 nm by definition. This pins the CMF fit's
    /// transcription — a mistyped mean or inverse-width would move it.
    #[test]
    fn luminous_efficiency_curve_peaks_near_555_nm() {
        let mut best_lambda = 0.0;
        let mut best_value = f64::NEG_INFINITY;
        let mut lambda = SPECTRUM_MIN_NM;
        while lambda <= SPECTRUM_MAX_NM {
            let y = cie_1931_xyz_bar(lambda)[1];
            if y > best_value {
                best_value = y;
                best_lambda = lambda;
            }
            lambda += 0.1;
        }
        assert!(
            (best_lambda - 555.0).abs() < 5.0,
            "ȳ peak at {best_lambda} nm; the photopic curve peaks at 555 nm"
        );
    }

    /// Chromaticity must be pure hue: the *luminance* of the result is 1,
    /// which is a statement about the Rec. 709 weighted sum, not about any
    /// single channel. In-gamut temperatures hit it exactly; out-of-gamut
    /// ones overshoot slightly because clamping a negative channel to zero
    /// removes a negative term from that sum.
    #[test]
    fn chromaticity_is_luminance_normalized() {
        for temperature in [3000.0_f32, 4000.0, 5000.0, 6500.0] {
            let rgb = blackbody_chromaticity_srgb(temperature).expect("emissive");
            let luminance = linear_srgb_luminance(rgb);
            assert!(
                (luminance - 1.0).abs() < 1.0e-3,
                "{temperature} K: expected unit luminance, got {luminance} from {rgb:?}"
            );
        }

        // Deep red is outside the sRGB gamut; the documented direction of the
        // clamping error is upward, never downward.
        let ember = blackbody_chromaticity_srgb(1000.0).expect("emissive");
        assert_eq!(ember[2], 0.0, "1000 K blue channel must clamp to zero");
        assert!(
            linear_srgb_luminance(ember) >= 1.0,
            "gamut clamping must not lose luminance, got {ember:?}"
        );
    }

    /// The whole point of the module: colour must track temperature the way
    /// a real fire does. Cool bodies are red-dominant, hot bodies are
    /// blue-dominant, and the crossover is monotonic in between.
    #[test]
    fn blue_to_red_ratio_increases_monotonically_with_temperature() {
        // Below roughly 1500 K the blue primary is out of gamut and clamps to
        // zero, so the ratio is pinned at 0 and can only be asserted
        // non-decreasing. Above that it must strictly rise.
        const STRICT_ABOVE_K: f32 = 2000.0;
        let temperatures = [1000.0_f32, 1500.0, 2000.0, 3000.0, 4500.0, 6500.0, 10000.0];
        let mut previous_ratio = f32::NEG_INFINITY;
        for temperature in temperatures {
            let rgb = blackbody_chromaticity_srgb(temperature).expect("emissive");
            let ratio = rgb[2] / rgb[0].max(1.0e-6);
            if temperature >= STRICT_ABOVE_K {
                assert!(
                    ratio > previous_ratio,
                    "B/R ratio must strictly rise above {STRICT_ABOVE_K} K: \
                     {temperature} K gave {ratio}, previous was {previous_ratio}"
                );
            } else {
                assert!(
                    ratio >= previous_ratio,
                    "B/R ratio must never fall as temperature rises: \
                     {temperature} K gave {ratio}, previous was {previous_ratio}"
                );
            }
            previous_ratio = ratio;
        }
    }

    /// A candle-flame-temperature body must read as orange: red clearly
    /// dominant, blue strongly suppressed. This is the sanity check that the
    /// XYZ->RGB matrix orientation is right (a transposed matrix would
    /// invert the hue).
    #[test]
    fn flame_temperature_is_red_dominant() {
        let rgb = blackbody_chromaticity_srgb(1800.0).expect("emissive");
        assert!(
            rgb[0] > rgb[1] && rgb[1] > rgb[2],
            "1800 K must be R > G > B, got {rgb:?}"
        );
        // Across the entire flame/ember range the blue primary is out of the
        // sRGB gamut and clamps to exactly zero. Consumers comparing the hue
        // of two flame-range temperatures must therefore use green-over-red,
        // not blue-over-red, which is identically zero for both.
        assert_eq!(
            rgb[2], 0.0,
            "1800 K blue is out of sRGB gamut and must clamp to zero, got {rgb:?}"
        );
        assert!(
            rgb[1] > 0.1 && rgb[1] < rgb[0],
            "green must remain in gamut and below red at flame temperature, got {rgb:?}"
        );
    }

    /// Around 6500 K the Planckian locus passes close to the sRGB white
    /// point, so the result should be near-neutral. Not exactly neutral —
    /// D65 is a daylight illuminant that sits slightly off the locus — so
    /// this asserts a loose neighbourhood, not equality.
    #[test]
    fn daylight_temperature_is_near_neutral() {
        let rgb = blackbody_chromaticity_srgb(6500.0).expect("emissive");
        let spread = rgb[0].max(rgb[2]) / rgb[0].min(rgb[2]).max(1.0e-6);
        assert!(
            spread < 1.25,
            "6500 K should be within a quarter-stop of neutral, got {rgb:?} (spread {spread})"
        );
    }

    /// The total-power law is exact and easy: doubling `T` is 16x.
    #[test]
    fn total_exitance_follows_fourth_power_law() {
        assert!((stefan_boltzmann_ratio(1000.0, 1000.0) - 1.0).abs() < 1.0e-6);
        assert!((stefan_boltzmann_ratio(2000.0, 1000.0) - 16.0).abs() < 1.0e-3);
        assert!((stefan_boltzmann_ratio(500.0, 1000.0) - 0.0625).abs() < 1.0e-6);
    }

    /// The load-bearing claim of the module: *visible* radiance grows much
    /// faster than total power, because the spectrum shifts into the visible
    /// band as the body heats. If this ever collapses toward 16x, the
    /// magnitude path has silently reverted to the `T^4` model and flame
    /// cores will stop reading as hotter than their cooling tails.
    #[test]
    fn visible_radiance_outruns_the_fourth_power_law() {
        let visible = blackbody_visible_radiance_ratio(1800.0, 900.0);
        let total = stefan_boltzmann_ratio(1800.0, 900.0);
        assert!(
            (total - 16.0).abs() < 1.0e-3,
            "control: total-power ratio should be 16, got {total}"
        );
        assert!(
            visible > total * 4.0,
            "visible-band ratio ({visible}) must far exceed the total-power \
             ratio ({total}); a 900 K body radiates almost entirely in infrared"
        );

        // Self-consistency: the ratio is reflexive and inverts cleanly.
        assert!((blackbody_visible_radiance_ratio(1600.0, 1600.0) - 1.0).abs() < 1.0e-4);
        let inverse = blackbody_visible_radiance_ratio(900.0, 1800.0);
        assert!(
            (inverse * visible - 1.0).abs() < 1.0e-3,
            "ratio must invert: {visible} * {inverse} != 1"
        );
    }

    /// The full radiance path must carry that magnitude law through to RGB.
    #[test]
    fn radiance_magnitude_tracks_the_visible_ratio() {
        let cool = blackbody_radiance_srgb(900.0, 1800.0, 1.0).expect("emissive");
        let hot = blackbody_radiance_srgb(1800.0, 1800.0, 1.0).expect("emissive");
        let ratio = linear_srgb_luminance(hot) / linear_srgb_luminance(cool);
        assert!(
            ratio > 16.0,
            "hot/cool luminance ratio must exceed the 16x total-power ratio, got {ratio}"
        );
        // The anchor holds: the reference temperature lands on the reference
        // luminance, up to the documented gamut-clamp overshoot.
        let anchor = linear_srgb_luminance(hot);
        assert!(
            (anchor - 1.0).abs() < 0.05,
            "reference temperature should render at the reference luminance, got {anchor}"
        );
    }

    /// The reference anchor is the one exposure choice in the module, so it
    /// must behave as a plain linear scale with no hidden coupling.
    #[test]
    fn reference_radiance_scales_linearly() {
        let unit = blackbody_radiance_srgb(1600.0, 1600.0, 1.0).expect("emissive");
        let scaled = blackbody_radiance_srgb(1600.0, 1600.0, 7.5).expect("emissive");
        for channel in 0..3 {
            assert!(
                (scaled[channel] - unit[channel] * 7.5).abs() < 1.0e-4,
                "channel {channel} did not scale linearly: {unit:?} vs {scaled:?}"
            );
        }
    }

    /// Garbage in must not produce a black-body-coloured NaN in a GPU
    /// buffer. Every rejection path returns `None` rather than a sentinel.
    #[test]
    fn non_physical_inputs_are_rejected() {
        assert!(blackbody_chromaticity_srgb(f32::NAN).is_none());
        assert!(blackbody_chromaticity_srgb(f32::INFINITY).is_none());
        assert!(blackbody_chromaticity_srgb(0.0).is_none());
        assert!(blackbody_chromaticity_srgb(-500.0).is_none());
        assert!(blackbody_radiance_srgb(1600.0, 1600.0, f32::NAN).is_none());
        assert!(blackbody_radiance_srgb(1600.0, 1600.0, -1.0).is_none());
        assert_eq!(stefan_boltzmann_ratio(1000.0, 0.0), 0.0);
        assert_eq!(planck_spectral_radiance(0.0, 1000.0), 0.0);
        assert_eq!(planck_spectral_radiance(500.0, 0.0), 0.0);
    }
}
