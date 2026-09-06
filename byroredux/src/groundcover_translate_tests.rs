//! EXAL ground-cover translate-boundary tests (Phase 0).
//!
//! Names used here are verbatim from the installed games' `LTEX` corpus (386
//! unique records across Oblivion, FNV, Skyrim and FO3), not invented, so the
//! suite exercises the strings the table will actually meet.

use super::*;
use byroredux_plugin::esm::records::tree::ObjectBounds;

/// Pins *ordering*, never the exact scalars. Design §11.3 calibrates the
/// numbers against real cells with density-histogram telemetry; a suite that
/// asserted `== 0.95` would have to be rewritten by that work rather than
/// protecting it.
fn assert_ranked(higher: &str, lower: &str) {
    let (h, l) = (layer_affinity(higher), layer_affinity(lower));
    assert!(h > l, "expected {higher} ({h}) to outrank {lower} ({l})");
}

#[test]
fn substrate_affinity_is_ordered_vegetated_to_bare() {
    // Real FNV / FO3 / Oblivion / Skyrim editor IDs.
    assert_ranked("LGrassGreenSuburbs", "LDirtWasteland01");
    assert_ranked("LDirtWasteland01", "LRockWasteland05");
    assert_ranked("CHTerrainMoss01", "AnvilSand01");
    assert_ranked("LTundra01", "JMBruSnowStone01");
    assert_ranked("LScrubDirtCanyon01", "LRockCanyonRubble01");
}

#[test]
fn nograss_suffix_suppresses_a_grass_named_layer() {
    // The corpus finding that shapes the whole table: 46 records carry an
    // explicit `NoGrass` suffix. These are authored variants with vegetation
    // deliberately removed — worn paths through meadows, ground under
    // buildings. A `contains("grass")` test would score them *highest*.
    for suppressed in [
        "CHTerrainGrass01NoGrass",
        "LGrassGreenSuburbsNoGrass",
        "DementiaMoss01NoGrass",
        "LTundra01NoGrass",
        "LFrozenMarshLichen01NoGrass",
        "LVolcanicTundraDirt01NoGrass",
    ] {
        assert_eq!(
            layer_affinity(suppressed),
            0.0,
            "{suppressed} must suppress ground cover outright"
        );
    }
}

#[test]
fn suppression_beats_every_positive_keyword() {
    // Structural, not example-driven: the un-suppressed sibling of each of
    // these is positively weighted, so suppression must win regardless of what
    // else the name contains.
    let paired = [
        ("CHTerrainGrass01", "CHTerrainGrass01NoGrass"),
        ("LTundra01", "LTundra01NoGrass"),
        ("DementiaMoss01", "DementiaMoss01NoGrass"),
    ];
    for (plain, suppressed) in paired {
        assert!(layer_affinity(plain) > 0.0, "{plain} should be vegetated");
        assert_eq!(layer_affinity(suppressed), 0.0);
    }
}

#[test]
fn hard_surfaces_get_no_cover_even_when_named_grass() {
    // `LScrubAsphaltStripGRASS` is asphalt with a grass verge painted into the
    // texture, not a lawn — and its uppercase suffix also pins that matching
    // is case-insensitive.
    assert_eq!(layer_affinity("LScrubAsphaltStripGRASS"), 0.0);
    assert_eq!(layer_affinity("Asphalt02"), 0.0);
    assert_eq!(layer_affinity("Pavement"), 0.0);
    assert_eq!(layer_affinity("OblivionOBCaveFloor2Lava"), 0.0);
}

#[test]
fn oblivion_icon_paths_resolve_like_editor_ids() {
    // Oblivion supplies LTEX via ICON (a texture path); every other game via
    // TNAM -> TXST (an editor ID). One code path must handle both shapes.
    assert!(layer_affinity(r"Dementia\DementiaMoss01.dds") > 0.0);
    assert_eq!(layer_affinity(r"Dementia\DementiaMoss01NoGrass.dds"), 0.0);
    assert_ranked(
        r"Ordered\OrderedCrackedEarth01.dds",
        r"Ordered\OrderedRock01.dds",
    );
}

#[test]
fn more_specific_keywords_are_not_shadowed() {
    // Ordering inside the table is load-bearing: `cobblestone` must not be
    // matched by `stone`, and the compound grass/dirt tokens must not be
    // swallowed by bare `grass` or bare `dirt`.
    let cobble = layer_affinity("BrumaCobbleStone01");
    let stone = layer_affinity("JMBruStone");
    assert!(
        cobble < stone || (cobble - stone).abs() < f32::EPSILON,
        "cobblestone ({cobble}) should not outrank bare stone ({stone})"
    );
    let blended = layer_affinity("CHTerrainDirtGrass01");
    assert!(
        blended < layer_affinity("CHTerrainGrass01") && blended > layer_affinity("CHTerrainDirt01"),
        "a dirt/grass blend must sit between its two components, got {blended}"
    );
}

#[test]
fn unknown_layers_get_a_low_but_nonzero_default() {
    // Zero would make an unrecognised layer a hard vegetation hole — exactly
    // the boolean-boundary artifact the design exists to remove.
    let unknown = layer_affinity("SomeModAddedSurfaceXYZ");
    assert_eq!(unknown, DEFAULT_AFFINITY);
    assert!(unknown > 0.0 && unknown < 0.5);
}

#[test]
fn every_affinity_is_a_valid_weight() {
    // Sweep the real corpus shape: nothing may produce a negative, >1, or
    // non-finite weight, since these are dotted against splat weights on the GPU.
    for name in [
        "LGrassGreenSuburbs",
        "CHTerrainMoss01",
        "AnvilSand01",
        "LRockWasteland05",
        "JMBruSnowStone01",
        "LTundra01NoGrass",
        "Asphalt02",
        "MudSlimeLowlands01",
        "RiverBedLowlands01",
        "GroundLitterHeavy01",
        "TerrainHDMS14Canvas01",
        "LWaterGravelSandNV01",
        "OrderedScorchedEarth01",
        "",
        "\\/..",
    ] {
        let a = layer_affinity(name);
        assert!(
            a.is_finite() && (0.0..=1.0).contains(&a),
            "{name:?} produced invalid affinity {a}"
        );
    }
}

#[test]
fn layer_affinities_preserves_order_and_handles_gaps() {
    let names = [Some("LGrassGreenSuburbs"), None, Some("LRockWasteland05")];
    let out = layer_affinities(&names);
    assert_eq!(out.len(), 3);
    assert!(out[0] > out[2]);
    // An unnamed layer must not read as a hole.
    assert_eq!(out[1], DEFAULT_AFFINITY);
}

// ── climate + palette ───────────────────────────────────

#[test]
fn worldspace_names_classify_into_climates() {
    assert_eq!(
        climate_for_worldspace_chain(&["WastelandNV".to_string()]),
        Climate::Arid
    );
    assert_eq!(
        climate_for_worldspace_chain(&["MegatonWorld".to_string()]),
        Climate::Temperate
    );
    assert_eq!(
        climate_for_worldspace_chain(&["Hjaalmarch".to_string()]),
        Climate::Wetland
    );
    assert_eq!(
        climate_for_worldspace_chain(&["WinterholdWorld".to_string()]),
        Climate::Alpine
    );
    assert_eq!(
        climate_for_worldspace_chain(&["Tamriel".to_string()]),
        Climate::Temperate
    );
}

#[test]
fn wetland_and_alpine_outrank_arid_when_a_name_matches_both() {
    // "FrozenMarsh" contains both an alpine and a wetland token; the more
    // specific vegetation signal (standing water) must win.
    assert_eq!(
        climate_for_worldspace_chain(&["FrozenMarshHold".to_string()]),
        Climate::Wetland
    );
}

#[test]
fn palette_resolution_matches_the_climate() {
    let arid = resolve_palette_for_chain(&["WastelandNV".to_string()], Vec::new());
    assert_eq!(arid.climate, Climate::Arid);
    assert_eq!(arid.species[0], GroundCoverSpecies::DEFAULT_ARID);

    let temperate = resolve_palette_for_chain(&["Tamriel".to_string()], Vec::new());
    assert_eq!(temperate.climate, Climate::Temperate);
    assert_eq!(temperate.species[0], GroundCoverSpecies::DEFAULT_TEMPERATE);
}

#[test]
fn authored_species_take_precedence_over_the_default() {
    let authored = GroundCoverSpecies {
        height_range: (20.0, 30.0),
        ..GroundCoverSpecies::DEFAULT_TEMPERATE
    };
    let palette = resolve_palette_for_chain(&["Tamriel".to_string()], vec![authored]);
    assert_eq!(palette.species, vec![authored]);
}

#[test]
fn wind_direction_is_stable_per_worldspace_but_differs_between_them() {
    // Grass must not change direction when you reload a save, and two
    // worldspaces should not share a direction by construction.
    let a1 = resolve_wind("Tamriel", 128);
    let a2 = resolve_wind("Tamriel", 128);
    let b = resolve_wind("WastelandNV", 128);
    assert_eq!(a1.direction, a2.direction);
    assert_ne!(a1.direction, b.direction);
    assert!(a1.is_well_formed() && b.is_well_formed());
}

#[test]
fn wind_direction_is_case_insensitive_per_worldspace() {
    // WRLD editor IDs are normalised to lowercase by the record index, so the
    // same worldspace must not get two different wind directions depending on
    // which spelling reached the translate boundary.
    assert_eq!(
        resolve_wind("WastelandNV", 90).direction,
        resolve_wind("wastelandnv", 90).direction
    );
}

#[test]
fn authored_wind_direction_wins_at_installation_boundary() {
    let wind = resolve_wind_with_direction("Tamriel", 128, Some([0.0, 1.0]));
    assert!(wind.direction[0].abs() < 1.0e-6);
    assert!((wind.direction[1] - 1.0).abs() < 1.0e-6);
}

#[test]
fn malformed_authored_wind_direction_uses_stable_fallback() {
    let expected = resolve_wind("Tamriel", 128);
    let malformed = resolve_wind_with_direction("Tamriel", 128, Some([f32::NAN, 0.0]));
    assert_eq!(malformed.direction, expected.direction);
}

#[test]
fn resolved_wind_is_always_well_formed() {
    for name in ["Tamriel", "", "WastelandNV", "MegatonWorld"] {
        for speed in [0_u8, 1, 127, 255] {
            let wind = resolve_wind(name, speed);
            assert!(wind.is_well_formed(), "{name}/{speed} -> {wind:?}");
        }
    }
}

#[test]
fn worn_paths_outrank_their_substrate() {
    // `LDirtPathWasteland01` is a trail worn through dirt. Matching `dirt`
    // first would grow grass straight across the trail — the path is precisely
    // why cover is absent there. Caught by sweeping the real corpus.
    assert!(layer_affinity("LDirtPathWasteland01") < layer_affinity("LDirtWasteland01"));
    assert_eq!(layer_affinity("AnvilStreet01"), 0.0);
    assert!(layer_affinity("JMPath01") < DEFAULT_AFFINITY);
}

#[test]
fn scorched_is_not_read_as_plain_earth() {
    // Ordering regression: `OrderedScorchedEarth01` contains both `scorched`
    // and `earth`, and matched the wrong one until the table was reordered.
    assert!(
        layer_affinity("OrderedScorchedEarth01") < layer_affinity("OrderedCrackedEarth01"),
        "scorched earth must be more hostile than merely cracked earth"
    );
    assert!(layer_affinity("OrderedScorchedEarth01") < 0.05);
}

#[test]
fn grass_overrides_an_otherwise_barren_base_name() {
    // `RootsBarrenWastesGrass01` and `ChemicalBarrenWastes01Grass` are the
    // barren base with grass painted over it; the vegetated reading is the
    // correct one, so `grass` must outrank `roots`, `barren` and `chemical`.
    assert!(layer_affinity("RootsBarrenWastesGrass01") > layer_affinity("RootsBarrenWastes01"));
    assert!(
        layer_affinity("ChemicalBarrenWastes01Grass") > layer_affinity("ChemicalBarrenWastes01")
    );
}

#[test]
fn conifer_needles_rank_below_broadleaf_litter() {
    // Needle drop is acidic and matted — real conifer understory is sparse,
    // where broadleaf litter supports growth.
    assert!(layer_affinity("TerrainHDEvergreenNeedles01") < layer_affinity("GroundLitterHeavy01"));
}

#[test]
fn cultivated_and_clover_ground_is_strongly_vegetated() {
    assert!(layer_affinity("TerrainTilledSoil") > layer_affinity("CHTerrainDirt01"));
    assert!(layer_affinity("TerrainHDClover01SU") > 0.5);
}

#[test]
fn ancestry_supplies_a_climate_the_leaf_name_lacks() {
    // The FO3 case that forced the chain walk: `MegatonWorld` carries no
    // geographic signal, but Megaton is a Capital Wasteland settlement and its
    // parent does. Classified on the leaf alone this returned Temperate.
    assert_eq!(
        climate_for_worldspace_chain(&["megatonworld".to_string()]),
        Climate::Temperate
    );
    assert_eq!(
        climate_for_worldspace_chain(&["megatonworld".to_string(), "wasteland".to_string()]),
        Climate::Arid
    );
}

#[test]
fn a_childs_own_signal_overrides_its_parents() {
    // Most-specific-first: a marsh inside a temperate province stays wetland,
    // and an alpine holdout inside the wasteland stays alpine.
    assert_eq!(
        climate_for_worldspace_chain(&["hjaalmarch".to_string(), "tamriel".to_string()]),
        Climate::Wetland
    );
    assert_eq!(
        climate_for_worldspace_chain(&["frozenworld".to_string(), "wasteland".to_string()]),
        Climate::Alpine
    );
}

#[test]
fn an_empty_or_signalless_chain_falls_back_to_temperate() {
    assert_eq!(climate_for_worldspace_chain(&[]), Climate::Temperate);
    assert_eq!(
        climate_for_worldspace_chain(&["tamriel".to_string(), "someroot".to_string()]),
        Climate::Temperate
    );
}

// ── Phase 5: `GRAS` → species (#3807) ───────────────────────────────
//
// Editor IDs, FormIDs and field values below are verbatim from the four
// installed corpora (168 vanilla `GRAS` records, census 2026-09-06), so
// the suite exercises the records the boundary will actually meet.

/// Oblivion `BWCattail01` (0x000984C8): `MODB` 127.96, height_range 0.2,
/// no `OBND` — the Oblivion shape.
fn cattail() -> GrasRecord {
    GrasRecord {
        form_id: 0x0009_84C8,
        editor_id: "BWCattail01".to_string(),
        model_path: r"Plants\BWCattail01.NIF".to_string(),
        bound_radius: 127.964_78,
        height_range: 0.2,
        density: 50,
        max_slope: 45,
        wave_period: 10.0,
        has_data: true,
        ..Default::default()
    }
}

/// FO3/FNV `GrassWasteland06` (0x00061EB1): `OBND` z extent 70,
/// height_range 0.375 — the FO3+ shape.
fn wasteland06() -> GrasRecord {
    GrasRecord {
        form_id: 0x0006_1EB1,
        editor_id: "GrassWasteland06".to_string(),
        model_path: r"Landscape\Grass\GrassWasteland06.NIF".to_string(),
        bounds: Some(ObjectBounds {
            min: [-34, -31, 0],
            max: [32, 36, 70],
        }),
        height_range: 0.375,
        density: 30,
        max_slope: 40,
        wave_period: 15.0,
        has_data: true,
        ..Default::default()
    }
}

#[test]
fn gras_dimensions_become_the_species_height_range() {
    let species = species_from_gras(&wasteland06(), Climate::Arid).expect("dimensioned record");
    // 70 units ± 37.5%.
    assert!((species.height_range.0 - 43.75).abs() < 1e-3);
    assert!((species.height_range.1 - 96.25).abs() < 1e-3);
    assert!(species.is_well_formed());
}

/// Oblivion carries no `OBND` at all, so the `MODB` bounding radius is the
/// only size signal its records have. The boundary must read it without
/// the caller knowing which game it came from.
#[test]
fn oblivion_bound_radius_feeds_the_same_height_range_path() {
    let species = species_from_gras(&cattail(), Climate::Temperate).expect("dimensioned record");
    assert!((species.height_range.0 - 127.964_78 * 0.8).abs() < 1e-2);
    assert!((species.height_range.1 - 127.964_78 * 1.2).abs() < 1e-2);
}

/// The whole point of design §1: placement authority does not cross the
/// boundary. Changing every placement field must not change the species.
#[test]
fn gras_placement_fields_do_not_reach_the_species() {
    let base = species_from_gras(&wasteland06(), Climate::Arid).expect("dimensioned record");
    let repainted = GrasRecord {
        density: 100,
        min_slope: 10,
        max_slope: 90,
        distance_from_water: 390,
        water_distance_application: 3,
        position_range: 90.0,
        colour_range: 0.5,
        wave_period: 600.0,
        flags: 0x07,
        ..wasteland06()
    };
    assert_eq!(
        species_from_gras(&repainted, Climate::Arid),
        Some(base),
        "a placement/appearance field leaked into the canonical species"
    );
}

/// A record whose bounds were never computed contributes nothing over the
/// built-in default — 15 of FNV's 24 records and 6 of Skyrim's 27.
#[test]
fn gras_without_usable_dimensions_yields_no_species() {
    let unset = GrasRecord {
        bounds: Some(ObjectBounds {
            min: [0; 3],
            max: [0; 3],
        }),
        bound_radius: 0.0,
        ..wasteland06()
    };
    assert_eq!(species_from_gras(&unset, Climate::Arid), None);
}

/// Trees and shrubs have their own authority (§10), and a `MODB` can be
/// authored to anything. The tallest real record is 279 units.
#[test]
fn gras_taller_than_ground_cover_is_rejected() {
    let tree_sized = GrasRecord {
        bound_radius: 4096.0,
        bounds: None,
        ..cattail()
    };
    assert_eq!(species_from_gras(&tree_sized, Climate::Temperate), None);

    let tallest_real = GrasRecord {
        bound_radius: 279.0,
        bounds: None,
        ..cattail()
    };
    assert!(species_from_gras(&tallest_real, Climate::Temperate).is_some());
}

/// A non-finite `height_range` survives `clamp` as NaN, which would put a
/// NaN height in the blade shader — a blade that silently vanishes from
/// the raster with no diagnostic.
#[test]
fn gras_with_a_non_finite_height_range_is_rejected() {
    for poison in [f32::NAN, f32::INFINITY] {
        let bad = GrasRecord {
            height_range: poison,
            ..wasteland06()
        };
        assert_eq!(
            species_from_gras(&bad, Climate::Arid),
            None,
            "non-finite height_range {poison} reached the palette"
        );
    }
}

#[test]
fn species_climate_weighting_follows_the_editor_id() {
    let arid = species_from_gras(&wasteland06(), Climate::Arid).expect("dimensioned");
    let w = arid.climate_weight;
    assert!(
        w.arid > w.temperate && w.temperate > w.alpine,
        "'GrassWasteland06' should favour arid: {w:?}"
    );
    assert!(w.alpine > 0.0, "a zero weight is the boolean boundary §1 removes");

    // `BWCattail01` — a wetland plant in a temperate worldspace.
    let wet = species_from_gras(&cattail(), Climate::Temperate).expect("dimensioned");
    assert!(wet.climate_weight.wetland > wet.climate_weight.arid);
}

/// Mirrors `classify_worldspace_name`'s precedence: standing water is the
/// strongest signal, so Skyrim's `FrozenMarshGrass01` is wetland, not
/// alpine, even though it names frost first.
#[test]
fn wetland_species_keywords_outrank_alpine_ones() {
    let frozen_marsh = GrasRecord {
        editor_id: "FrozenMarshGrass01".to_string(),
        ..wasteland06()
    };
    let w = species_from_gras(&frozen_marsh, Climate::Temperate)
        .expect("dimensioned")
        .climate_weight;
    assert!(
        w.wetland > w.alpine,
        "FrozenMarshGrass01 should read as wetland: {w:?}"
    );
}

/// A species with no geographic token makes no claim at all rather than
/// being pushed toward the worldspace's own climate.
#[test]
fn species_with_no_climate_signal_stays_uniform() {
    let plain = GrasRecord {
        editor_id: "GCLongGrass01".to_string(),
        ..cattail()
    };
    let species = species_from_gras(&plain, Climate::Alpine).expect("dimensioned");
    assert_eq!(species.climate_weight, ClimateWeights::UNIFORM);
}

/// The scatter pass indexes species by position, so a `HashMap` iteration
/// order would make a blade change species between sessions.
#[test]
fn authored_species_order_is_stable_across_runs() {
    let grasses: HashMap<u32, GrasRecord> = [
        (0x0009_84C8, cattail()),
        (0x0006_1EB1, wasteland06()),
        (
            0x0001_CC70,
            GrasRecord {
                form_id: 0x0001_CC70,
                editor_id: "SnowGrass01".to_string(),
                bounds: Some(ObjectBounds {
                    min: [-29, -23, -2],
                    max: [29, 27, 61],
                }),
                height_range: 0.33,
                has_data: true,
                ..Default::default()
            },
        ),
    ]
    .into_iter()
    .collect();

    let first = authored_species(&grasses, Climate::Temperate);
    assert_eq!(first.len(), 3);
    for _ in 0..8 {
        assert_eq!(authored_species(&grasses, Climate::Temperate), first);
    }
    // Sorted by FormID: SnowGrass01 (0x1CC70) < GrassWasteland06 (0x61EB1)
    // < BWCattail01 (0x984C8). Heights 63, 70, 127.96.
    assert!(first[0].height_range.1 < first[1].height_range.1);
    assert!(first[1].height_range.1 < first[2].height_range.1);
}

/// The full boundary: a worldspace with real `GRAS` records must resolve
/// to those species, not to the built-in fallback.
#[test]
fn palette_from_grasses_uses_authored_species_and_still_falls_back() {
    let grasses: HashMap<u32, GrasRecord> =
        [(0x0006_1EB1, wasteland06())].into_iter().collect();
    let palette = resolve_palette_from_grasses(&["WastelandNV".to_string()], &grasses);
    assert_eq!(palette.climate, Climate::Arid);
    assert_eq!(palette.species.len(), 1);
    assert_ne!(palette.species[0], GroundCoverSpecies::DEFAULT_ARID);
    assert!(palette.total_weight() > 0.0);

    // A worldspace whose plugin has no GRAS at all keeps the built-in.
    let empty = resolve_palette_from_grasses(&["WastelandNV".to_string()], &HashMap::new());
    assert_eq!(empty.species, vec![GroundCoverSpecies::DEFAULT_ARID]);
}
