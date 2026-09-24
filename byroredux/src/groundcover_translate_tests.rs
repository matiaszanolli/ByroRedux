//! EXAL ground-cover translate-boundary tests (Phase 0).
//!
//! Names used here are verbatim from the installed games' `LTEX` corpus (386
//! unique records across Oblivion, FNV, Skyrim and FO3), not invented, so the
//! suite exercises the strings the table will actually meet.

use super::*;
use byroredux_core::ecs::components::groundcover::GroundCoverSpecies;

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
    let arid = resolve_palette_for_chain(&["WastelandNV".to_string()]);
    assert_eq!(arid.climate, Climate::Arid);
    assert_eq!(arid.species[0], GroundCoverSpecies::DEFAULT_ARID);

    let temperate = resolve_palette_for_chain(&["Tamriel".to_string()]);
    assert_eq!(temperate.climate, Climate::Temperate);
    assert_eq!(temperate.species[0], GroundCoverSpecies::DEFAULT_TEMPERATE);
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

// ── `GRAS` records and the blade palette (§12.12) ───────────────────

/// §12.12: blades are the engine's own sward. Every climate resolves to its
/// built-in species, and nothing a plugin's `GRAS` records say can reach the
/// blade palette — their models are card clumps, rocks, ferns and kelp, not
/// blades. The resolver no longer takes the records at all, so this pins the
/// resolved palette per climate.
#[test]
fn blade_palette_is_engine_species_for_every_climate() {
    for (chain, expected) in [
        ("WastelandNV", GroundCoverSpecies::DEFAULT_ARID),
        ("Tamriel", GroundCoverSpecies::DEFAULT_TEMPERATE),
        ("Hjaalmarch", GroundCoverSpecies::DEFAULT_TEMPERATE),
        ("WinterholdWorld", GroundCoverSpecies::DEFAULT_TEMPERATE),
    ] {
        let palette = resolve_palette_for_chain(&[chain.to_string()]);
        assert_eq!(palette.species, vec![expected], "{chain}");
    }
}

/// Kept for the authored-model tier (§12.12 Phase C), which weights each
/// record by climate. Editor IDs are verbatim from the installed corpora.
#[test]
fn gras_editor_ids_classify_into_climates() {
    assert_eq!(
        classify_species_name("GrassWasteland06"),
        Some(Climate::Arid)
    );
    assert_eq!(classify_species_name("BWCattail01"), Some(Climate::Wetland));
    // Standing water outranks frost, mirroring the worldspace matcher.
    assert_eq!(
        classify_species_name("FrozenMarshGrass01"),
        Some(Climate::Wetland)
    );
    // No geographic token makes no claim at all.
    assert_eq!(classify_species_name("GCLongGrass01"), None);
}

#[test]
fn climate_weights_favour_the_matched_climate_without_zeroing_others() {
    let w = climate_weights_for(Climate::Arid);
    assert!(w.arid > w.temperate && w.temperate > w.alpine, "{w:?}");
    assert!(
        w.alpine > 0.0 && w.wetland > 0.0,
        "a zero weight is the boolean boundary §1 removes: {w:?}"
    );
}

fn gras(form_id: u32, editor_id: &str, density: u8, water: u32) -> GrasRecord {
    GrasRecord {
        form_id,
        editor_id: editor_id.to_string(),
        model_path: format!("Landscape\\Grass\\{editor_id}.nif"),
        density,
        water_distance_application: water,
        distance_from_water: 40,
        height_range: 0.3,
        position_range: 32.0,
        flags: byroredux_plugin::esm::records::GRAS_FLAG_UNIFORM_SCALING,
        has_data: true,
        ..Default::default()
    }
}

/// #4413 — every placeable `GRAS` record becomes an authored-model record,
/// ordered by FormID, weighted by the climate its editor ID names; records
/// that cannot be placed as authored are dropped rather than guessed at.
#[test]
fn authored_cover_keeps_placeable_records_in_form_id_order() {
    let grasses: HashMap<u32, GrasRecord> = [
        gras(0x30, "WaterKelpGrass01", 60, 2),
        gras(0x10, "TundraGrass01", 45, 0),
        gras(0x20, "FrozenMarshGrass01", 0, 0), // density zero
        gras(0x40, "OddWaterRule01", 30, 7),    // undocumented rule
        GrasRecord {
            has_data: false,
            ..gras(0x50, "NoData01", 30, 0)
        },
    ]
    .into_iter()
    .map(|g| (g.form_id, g))
    .collect();

    let cover = resolve_authored_cover(&grasses, GameKind::Skyrim, Climate::Temperate)
        .expect("placeable records");
    assert_eq!(cover.grid_spacing, 20.0);
    let ids: Vec<u32> = cover.records.iter().map(|r| r.form_id).collect();
    assert_eq!(ids, [0x10, 0x30]);

    let tundra = &cover.records[0];
    assert_eq!(tundra.density, 0.45);
    assert_eq!(tundra.water_rule, CoverWaterRule::AboveAtLeast);
    assert!(tundra.uniform_scaling && !tundra.fit_to_slope);
    // No climate token → generic, weighted like the temperate default.
    assert!(tundra.climate_weight.temperate > tundra.climate_weight.arid);

    let kelp = &cover.records[1];
    assert_eq!(kelp.water_rule, CoverWaterRule::BelowAtLeast);
    assert!(kelp.climate_weight.wetland > kelp.climate_weight.temperate);

    // A game with no measured grid spacing leaves the tier off.
    assert!(resolve_authored_cover(&grasses, GameKind::Starfield, Climate::Temperate).is_none());
}

/// #4413 — every game's candidate grid fits one chunk's placement slab, so
/// the model tier never truncates a chunk's candidates.
#[test]
fn model_slab_holds_every_vanilla_candidate_grid() {
    use byroredux_renderer::shader_constants::{
        GROUNDCOVER_CHUNK_UNITS, GROUNDCOVER_MODEL_POINTS_PER_CHUNK,
    };
    for game in [
        GameKind::Oblivion,
        GameKind::Fallout3NV,
        GameKind::Skyrim,
        GameKind::Fallout4,
    ] {
        let spacing = grass_grid_spacing(game).expect("measured spacing");
        let per_side = (GROUNDCOVER_CHUNK_UNITS / spacing).ceil() as u32;
        assert!(
            per_side * per_side <= GROUNDCOVER_MODEL_POINTS_PER_CHUNK,
            "{game:?}: {per_side}² candidates overflow the slab"
        );
    }
}
