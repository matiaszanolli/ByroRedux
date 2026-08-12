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
