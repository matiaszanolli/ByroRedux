//! EXAL ground-cover canonical type tests (Phase 0).

use super::*;

#[test]
fn default_species_are_well_formed() {
    // These two are the guarantee that a game with no vegetation data still
    // renders ground cover. If either is malformed, `resolve` silently drops
    // it and the palette falls back to... itself, producing an empty palette.
    assert!(GroundCoverSpecies::DEFAULT_TEMPERATE.is_well_formed());
    assert!(GroundCoverSpecies::DEFAULT_ARID.is_well_formed());
}

#[test]
fn malformed_species_are_rejected() {
    let mut bad = GroundCoverSpecies::DEFAULT_TEMPERATE;
    bad.height_range = (14.0, 6.0); // inverted
    assert!(!bad.is_well_formed());

    let mut nan = GroundCoverSpecies::DEFAULT_TEMPERATE;
    nan.bend_stiffness = f32::NAN;
    assert!(!nan.is_well_formed());

    let mut negative = GroundCoverSpecies::DEFAULT_TEMPERATE;
    negative.width_range = (-1.0, 2.0);
    assert!(!negative.is_well_formed());

    let mut inf_colour = GroundCoverSpecies::DEFAULT_TEMPERATE;
    inf_colour.colour_gradient[1][0] = f32::INFINITY;
    assert!(!inf_colour.is_well_formed());
}

/// #6201 — the built-in species' mean and spread are the first two moments of
/// the uniform `height_range` draw they replace, not new numbers. (Of the
/// unclamped log-normal: the shader's clamp to `height_range` trims ~6 % of
/// blades to the limits and narrows σ by ~9.5 %, #6413.)
#[test]
fn default_blade_length_matches_the_moments_of_the_uniform_it_replaced() {
    let (mean, cv) = moment_matched_length((9.0, 21.0));
    assert_eq!(mean, 15.0);
    // σ of U(9, 21) is 12/√12 = 3.4641.
    assert!((cv * mean - 12.0 / 12f32.sqrt()).abs() < 1e-4, "cv {cv}");
    for species in [GroundCoverSpecies::DEFAULT_TEMPERATE, GroundCoverSpecies::DEFAULT_ARID] {
        let (mean, cv) = moment_matched_length(species.height_range);
        assert_eq!((species.blade_length_mean, species.blade_length_cv), (mean, cv));
    }
}

#[test]
fn a_blade_length_mean_outside_the_clamp_or_a_negative_spread_is_rejected() {
    let mut outside = GroundCoverSpecies::DEFAULT_TEMPERATE;
    outside.blade_length_mean = outside.height_range.1 + 1.0;
    assert!(!outside.is_well_formed());

    let mut negative = GroundCoverSpecies::DEFAULT_TEMPERATE;
    negative.blade_length_cv = -0.1;
    assert!(!negative.is_well_formed());

    let mut nan = GroundCoverSpecies::DEFAULT_TEMPERATE;
    nan.blade_length_cv = f32::NAN;
    assert!(!nan.is_well_formed());
}

/// #6201 — a reference of `byroGcBladeLength` (`groundcover_blade.vert`): the
/// log-normal parameters it derives from a mean and CV give back that mean and
/// CV, with the right skew. The GLSL and this share the formula, not the code.
#[test]
fn the_lognormal_blade_length_reproduces_its_mean_and_cv_inside_the_clamp() {
    let species = GroundCoverSpecies::DEFAULT_TEMPERATE;
    let (m, c) = (species.blade_length_mean, species.blade_length_cv);
    let s2 = (1.0 + c * c).ln();
    let draw = |u0: f32, u1: f32| {
        let z = (-2.0 * u0.max(1.0e-6).ln()).sqrt() * (std::f32::consts::TAU * u1).cos();
        ((m.ln() - 0.5 * s2) + s2.sqrt() * z).exp()
    };
    // A fixed low-discrepancy grid over the two streams, no RNG.
    let n = 400usize;
    let mut raw = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in 0..n {
            raw.push(draw((i as f32 + 0.5) / n as f32, (j as f32 + 0.5) / n as f32));
        }
    }
    let count = raw.len() as f32;
    let mean = raw.iter().sum::<f32>() / count;
    let var = raw.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / count;
    assert!((mean / m - 1.0).abs() < 0.01, "mean {mean} vs {m}");
    assert!((var.sqrt() / mean / c - 1.0).abs() < 0.03, "cv {}", var.sqrt() / mean);
    // Right-skewed: the mean is above the median.
    raw.sort_by(f32::total_cmp);
    assert!(mean > raw[raw.len() / 2]);
}

#[test]
fn palette_is_never_empty() {
    // The scatter pass must not need an empty-palette branch; getting that
    // wrong is a divide-by-zero in the species draw.
    let empty = GroundCoverPalette::resolve(Vec::new(), Climate::Temperate);
    assert_eq!(empty.species.len(), 1);
    assert_eq!(empty.species[0], GroundCoverSpecies::DEFAULT_TEMPERATE);

    let arid = GroundCoverPalette::resolve(Vec::new(), Climate::Arid);
    assert_eq!(arid.species[0], GroundCoverSpecies::DEFAULT_ARID);
}

#[test]
fn palette_drops_malformed_species_but_keeps_good_ones() {
    let mut bad = GroundCoverSpecies::DEFAULT_TEMPERATE;
    bad.height_range = (f32::NAN, 1.0);
    let palette = GroundCoverPalette::resolve(
        vec![GroundCoverSpecies::DEFAULT_ARID, bad],
        Climate::Temperate,
    );
    assert_eq!(palette.species, vec![GroundCoverSpecies::DEFAULT_ARID]);
}

#[test]
fn palette_falls_back_when_every_species_is_malformed() {
    // A wholly corrupt GRAS set must not produce an empty palette.
    let mut bad = GroundCoverSpecies::DEFAULT_TEMPERATE;
    bad.width_range = (0.0, 0.0);
    let palette = GroundCoverPalette::resolve(vec![bad; 3], Climate::Temperate);
    assert_eq!(palette.species.len(), 1);
    assert!(palette.species[0].is_well_formed());
}

#[test]
fn climate_weights_select_per_climate() {
    let arid = GroundCoverSpecies::DEFAULT_ARID.climate_weight;
    assert!(arid.weight_for(Climate::Arid) > arid.weight_for(Climate::Temperate));
    assert!(arid.weight_for(Climate::Temperate) > arid.weight_for(Climate::Alpine));
    // Uniform must not accidentally favour anything.
    for c in [
        Climate::Temperate,
        Climate::Arid,
        Climate::Alpine,
        Climate::Wetland,
    ] {
        assert_eq!(ClimateWeights::UNIFORM.weight_for(c), 1.0);
    }
}

#[test]
fn total_weight_is_positive_for_the_default_palette() {
    for climate in [
        Climate::Temperate,
        Climate::Arid,
        Climate::Alpine,
        Climate::Wetland,
    ] {
        let palette = GroundCoverPalette::resolve(Vec::new(), climate);
        assert!(
            palette.total_weight() > 0.0,
            "default palette has zero weight in {climate:?}"
        );
    }
}

// ── wind ────────────────────────────────────────────────

#[test]
fn calm_is_well_formed_and_still() {
    assert!(WindField::CALM.is_well_formed());
    assert_eq!(WindField::CALM.speed, 0.0);
    assert_eq!(WindField::default(), WindField::CALM);
}

#[test]
fn wind_scales_monotonically_with_the_weather_byte() {
    let mut previous = -1.0;
    for byte in [0_u8, 32, 64, 128, 200, 255] {
        let wind = WindField::from_weather_byte(byte, [1.0, 0.0]);
        assert!(wind.is_well_formed(), "byte {byte} produced {wind:?}");
        assert!(
            wind.speed > previous,
            "speed not monotonic at byte {byte}: {} <= {previous}",
            wind.speed
        );
        previous = wind.speed;
    }
    assert_eq!(
        WindField::from_weather_byte(255, [1.0, 0.0]).speed,
        MAX_WIND_SPEED
    );
}

#[test]
fn gusts_grow_faster_than_base_speed() {
    // Still air is genuinely steady; strong wind is turbulent. A constant gust
    // ratio would make calm weather shimmer, which reads as broken.
    let light = WindField::from_weather_byte(40, [1.0, 0.0]);
    let heavy = WindField::from_weather_byte(240, [1.0, 0.0]);
    let light_ratio = light.gust_amplitude / light.speed;
    let heavy_ratio = heavy.gust_amplitude / heavy.speed;
    assert!(
        heavy_ratio > light_ratio,
        "gust ratio did not grow: {light_ratio} -> {heavy_ratio}"
    );
    assert!(heavy.gust_frequency > light.gust_frequency);
}

#[test]
fn zero_wind_byte_produces_a_dead_calm_that_is_still_valid() {
    let wind = WindField::from_weather_byte(0, [1.0, 0.0]);
    assert_eq!(wind.speed, 0.0);
    assert_eq!(wind.gust_amplitude, 0.0);
    // Direction must still be unit-length — the shader normalises nothing.
    assert!(wind.is_well_formed());
}

#[test]
fn wind_direction_is_always_normalised() {
    for dir in [
        [3.0, 4.0],
        [-1.0, 0.0],
        [0.0, 0.0],
        [1e-20, 1e-20],
        [f32::NAN, 1.0],
        [f32::INFINITY, 0.0],
    ] {
        let wind = WindField::from_weather_byte(128, dir);
        assert!(
            wind.is_well_formed(),
            "direction {dir:?} produced malformed {wind:?}"
        );
    }
}

#[test]
fn degenerate_direction_falls_back_to_east_not_nan() {
    // A zero or non-finite direction must not propagate into the blade shader;
    // EX-05 hard-fails non-finite environment values, and the translate
    // boundary is the place to catch it.
    assert_eq!(
        WindField::from_weather_byte(128, [0.0, 0.0]).direction,
        [1.0, 0.0]
    );
    assert_eq!(
        WindField::from_weather_byte(128, [f32::NAN, f32::NAN]).direction,
        [1.0, 0.0]
    );
}

fn record(form_id: u32, density: f32) -> AuthoredCoverRecord {
    AuthoredCoverRecord {
        form_id,
        editor_id: String::new(),
        model_path: String::new(),
        climate_weight: ClimateWeights::UNIFORM,
        density,
        water_rule: CoverWaterRule::AboveAtLeast,
        water_distance: 0.0,
        height_range: 0.0,
        position_range: 0.0,
        uniform_scaling: true,
        fit_to_slope: false,
        nominal_height: None,
    }
}

/// A layer's authored density is the union of its listed records' densities —
/// the number the blade tier's per-layer affinity now comes from (#5719-era
/// grass overhaul): empty is "no grass", one record is its own density, two are
/// `1 - (1-a)(1-b)`, and unplaceable (absent) records contribute nothing.
#[test]
fn union_density_combines_the_listed_records() {
    let cover = AuthoredCover {
        records: vec![record(10, 0.30), record(20, 0.50), record(30, 1.0)],
        grid_spacing: 80.0,
        climate: Climate::Arid,
        localised: true,
        generation: 1,
    };
    assert_eq!(cover.union_density(&[]), 0.0);
    assert!((cover.union_density(&[10]) - 0.30).abs() < 1.0e-6);
    assert!((cover.union_density(&[10, 20]) - 0.65).abs() < 1.0e-6);
    assert_eq!(cover.union_density(&[99]), 0.0, "an unplaceable record adds nothing");
    assert!((cover.union_density(&[20, 99]) - 0.50).abs() < 1.0e-6);
    assert_eq!(cover.union_density(&[10, 30]), 1.0);
}

/// #6413 — the documented consequence of clamping the log-normal to `height_range`: it is only
/// ±√3 σ of the matched distribution, so ~5.6 % of blades pin at the maximum, ~1.7 % at the minimum
/// and σ falls ~9.5 % short. Replays `groundcover_blade.vert`'s `byroGcBladeLength` (Box–Muller over
/// a stratified grid of the two seed streams, so the result is deterministic) and pins the numbers
/// the field docs and `exal-groundcover.md` §12.16 quote.
#[test]
fn the_clamped_blade_length_draw_is_narrower_than_the_matched_moments() {
    const GRID: usize = 600;
    for species in [GroundCoverSpecies::DEFAULT_TEMPERATE, GroundCoverSpecies::DEFAULT_ARID] {
        let (lo, hi) = species.height_range;
        let (mean, cv) = (species.blade_length_mean, species.blade_length_cv);
        let s2 = (1.0 + cv as f64 * cv as f64).ln();
        let (mut sum, mut sum_sq, mut at_max, mut at_min) = (0.0f64, 0.0f64, 0usize, 0usize);
        for i in 0..GRID {
            for j in 0..GRID {
                let u0 = ((i as f64 + 0.5) / GRID as f64).max(1.0e-6);
                let u1 = (j as f64 + 0.5) / GRID as f64;
                let z = (-2.0 * u0.ln()).sqrt() * (std::f64::consts::TAU * u1).cos();
                let raw = ((mean as f64).ln() - 0.5 * s2 + s2.sqrt() * z).exp();
                let v = raw.clamp(lo as f64, hi as f64);
                sum += v;
                sum_sq += v * v;
                at_max += usize::from(v >= hi as f64);
                at_min += usize::from(v <= lo as f64);
            }
        }
        let n = (GRID * GRID) as f64;
        let (m, sigma) = (sum / n, (sum_sq / n - (sum / n).powi(2)).sqrt());
        let target_sigma = (cv * mean) as f64;
        assert!((m - mean as f64).abs() / (mean as f64) < 0.02, "mean moves by under 2 %: {m} vs {mean}");
        let shortfall = 1.0 - sigma / target_sigma;
        assert!((0.08..0.11).contains(&shortfall), "sigma ~9.5 % low, got {:.1} %", shortfall * 100.0);
        let (max_share, min_share) = (at_max as f64 / n, at_min as f64 / n);
        assert!((0.05..0.065).contains(&max_share), "~5.6 % at the maximum, got {:.1} %", max_share * 100.0);
        assert!((0.010..0.025).contains(&min_share), "~1.7 % at the minimum, got {:.1} %", min_share * 100.0);
    }
}
