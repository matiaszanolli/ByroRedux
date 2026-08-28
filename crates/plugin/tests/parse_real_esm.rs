//! Per-game ESM record-count integration tests.
//!
//! Mirrors the `crates/nif/tests/parse_real_nifs.rs` pattern: walk a
//! real game's master file, assert the total parsed-record count stays
//! at or above the M24 Phase 1 baseline, and sanity-check the
//! per-category floors. `#[ignore]`-gated because they require real
//! game data (CI has none). Opt in with:
//!
//! ```sh
//! cargo test -p byroredux-plugin --test parse_real_esm -- --ignored
//! ```
//!
//! Override the `BYROREDUX_*_DATA` env vars to point at a non-default
//! install path (see the `data_dir` helper below for defaults).
//!
//! See issue #488 — pre-existing inline test at
//! `records/mod.rs::tests::parse_real_fnv_esm_record_counts` was
//! hardcoded-path only and had no `total >= 13_684` floor.

use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::reader::GameKind;
use std::path::PathBuf;

/// Resolve a `Data/` directory from an env var, falling back to the
/// canonical Steam install path on the dev machine. Returns `None` when
/// neither resolves — the test then skips cleanly. Mirrors the pattern
/// from `crates/nif/tests/common/mod.rs::game_data_dir`.
fn data_dir(env_var: &str, fallback: &str) -> Option<PathBuf> {
    if let Ok(v) = std::env::var(env_var) {
        let p = PathBuf::from(&v);
        if p.is_dir() {
            return Some(p);
        }
        eprintln!("{env_var} points to {v:?} which is not a directory; falling back to default");
    }
    let p = PathBuf::from(fallback);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// FNV: 60,000 records floor — covers the 13,684 M24 Phase 1 baseline
/// plus the 7 categories added in #446/#447 (PACK 4163, QUST 436,
/// DIAL 18215, MESG 1144, PERK 176, SPEL 270, MGEF 289 = +24,693).
/// Observed 2026-04: 62,219. Floor sits a few percent below to absorb
/// DLC patch drift without masking regressions.
const FNV_TOTAL_FLOOR: usize = 60_000;

/// FO3: 30,000 records floor — covers the original 18,007 baseline +
/// the 7 categories added in #446/#447. Observed 2026-04: 31,101.
const FO3_TOTAL_FLOOR: usize = 30_000;

/// FO4: 70,000 records floor — observed 2026-05-04 on vanilla
/// Fallout4.esm: 76,468 (with #817 categories landed: cells 964 +
/// statics 31,989 + scols 2,617 + packins 872 + material_swaps 2,537 +
/// texture_sets 379 + items 4,076 + NPCs 3,015 + game_settings 2,039,
/// along with globals 1,346 + LVLI 2,098 + factions 699 + weathers 71
/// and many smaller categories). Floor at 70 K absorbs DLC/patch drift without
/// masking a category-wipe regression.
const FO4_TOTAL_FLOOR: usize = 70_000;

/// #2904 — Far Harbor ships HEDR 0.95 (overlapping the old FO3/FNV band)
/// but uses FO4's TES5+ record-header version 131.
#[test]
#[ignore]
fn dlccoast_header_classifies_as_fallout4() {
    let Some(data) = data_dir(
        "BYROREDUX_FO4_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
    ) else {
        eprintln!("[FO4/DLCCoast] skipping: game data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("DLCCoast.esm")).expect("read DLCCoast.esm");
    let index = parse_esm(&bytes).expect("parse DLCCoast.esm");
    assert_eq!(index.game, GameKind::Fallout4);
    assert!(
        !index.cells.packins.is_empty(),
        "FO4-only PKIN records prove the DLC used the FO4 schema"
    );
}

/// #2905 — FNAM is a display/coercion hint; FLTV is always an IEEE f32.
#[test]
#[ignore]
fn fnv_karma_good_global_decodes_float_payload_before_narrowing() {
    let Some(data) = data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    ) else {
        eprintln!("[FNV/GLOB] skipping: game data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("FalloutNV.esm")).expect("read FalloutNV.esm");
    let index = parse_esm(&bytes).expect("parse FalloutNV.esm");
    let karma = index
        .globals
        .values()
        .find(|global| global.editor_id == "KarmaGood")
        .expect("KarmaGood GLOB");
    assert_eq!(karma.value.as_f32(), 250.0);
}

#[test]
#[ignore]
fn oblivion_spawn_time_global_decodes_float_payload_before_narrowing() {
    let Some(data) = data_dir(
        "BYROREDUX_OBL_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    ) else {
        eprintln!("[Oblivion/GLOB] skipping: game data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("Oblivion.esm")).expect("read Oblivion.esm");
    let index = parse_esm(&bytes).expect("parse Oblivion.esm");
    let spawn_time = index
        .globals
        .values()
        .find(|global| global.editor_id == "SEKnightSpawnTime")
        .expect("SEKnightSpawnTime GLOB");
    assert_eq!(spawn_time.value.as_f32(), 4.0);
}

#[test]
#[ignore]
fn fnv_actor_value_roster_and_health_resolve_on_shipped_master() {
    let Some(data) = data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    ) else {
        eprintln!("[FNV AVIF] skipping: game data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("FalloutNV.esm")).expect("read FalloutNV.esm");
    let index = parse_esm(&bytes).expect("parse FalloutNV.esm");

    assert_eq!(index.actor_value_form_id("Strength"), Some(0x0000_03E8));
    assert_eq!(index.health_actor_value_key(), Some(0x0000_0450));
    assert_eq!(
        index.character_rules,
        byroredux_core::character::CharacterRulesProfile::FALLOUT_NEW_VEGAS
    );
    for skill in byroredux_core::character::SkillSet::FALLOUT_NV.skills() {
        assert!(
            index.actor_value_form_id(skill.editor_id).is_some(),
            "FNV roster entry '{}' must resolve against FalloutNV.esm",
            skill.editor_id
        );
    }
    assert!(
        byroredux_core::character::SkillSet::FALLOUT_NV
            .get("BigGuns")
            .is_none(),
        "the shipped AVBigGuns record is explicitly obsolete"
    );

    let (npc, pairs) = index
        .npcs
        .values()
        .filter(|npc| index.classes.contains_key(&npc.class_form_id))
        .find_map(|npc| {
            let pairs = byroredux_plugin::esm::records::derive_npc_actor_values(npc, &index);
            pairs
                .iter()
                .any(|(form_id, value)| *form_id == 0x450 && *value > 0.0)
                .then_some((npc, pairs))
        })
        .expect("at least one class-backed FNV NPC must derive positive Health");
    assert!(
        pairs.len() >= 21,
        "{} derived only {} actor values",
        npc.editor_id,
        pairs.len()
    );
}

#[test]
#[ignore]
fn skyrim_health_resolves_to_authored_avif_form_id() {
    let Some(data) = data_dir(
        "BYROREDUX_SKYRIMSE_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
    ) else {
        eprintln!("[Skyrim AVIF] skipping: game data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).expect("read Skyrim.esm");
    let index = parse_esm(&bytes).expect("parse Skyrim.esm");
    assert_eq!(index.health_actor_value_key(), Some(0x0000_03E8));
    assert!(index.npcs.values().any(|npc| {
        byroredux_plugin::esm::records::derive_npc_actor_values(npc, &index)
            .iter()
            .any(|(form_id, value)| *form_id == 0x3E8 && *value > 0.0)
    }));
}

// ── #3172 — CHARAL rosters, falsified against every shipped master ────────
//
// #3095 added one such loop, for `SkillSet::FALLOUT_NV`. The other four
// rosters, and the derived-row *output* keys on every game, still relied
// exclusively on hand-written `full()` fixtures in
// `crates/core/src/character/` — resolvers that enumerate the very strings
// the builders pass, so they cannot falsify a roster. `CHAR-2026-08-20-D2-01`
// is the demonstration: `SkillSet::SKYRIM`'s `Illusion` entry (vanilla
// `Skyrim.esm` authors `AVMysticism`) survived the commit that closed #3095,
// in a roster the new loop did not cover.
//
// Existence is the whole assertion here — no values. The arithmetic stays
// with the synthetic fixtures, where a hand-written resolver is the right
// tool.

/// One game's CHARAL roster surface, checked against its own shipped master.
struct RosterCase {
    label: &'static str,
    env: &'static str,
    fallback: &'static str,
    master: &'static str,
    profile: byroredux_core::character::CharacterRulesProfile,
    /// The primary-attribute roster this family's ruleset attaches. Empty for
    /// Skyrim (attributes were removed) and for FO4's skill-less profile it is
    /// still the 7 SPECIAL.
    attributes: byroredux_core::character::AttributeSet,
    /// Expected `derived_row_len()` when `build_ruleset` is driven by the
    /// **real** `EsmIndex` resolver.
    ///
    /// This is the derived-row *output key* assertion: every row is pushed
    /// only `if let Some(out) = resolve(<output editor id>)`, so an output key
    /// that does not exist on disk silently drops its row and this count
    /// falls. `None` = the family has no wired `RulesetBuilder` arm yet, in
    /// which case `build_ruleset` itself must return `None`.
    derived_rows: Option<usize>,
    /// Vanilla Oblivion ships **no `AVIF` records at all** — TES4 predates the
    /// record type and hardwires actor-value indices in the engine (verified:
    /// zero `AVIF` byte occurrences in `Oblivion.esm`). Its rosters therefore
    /// cannot be falsified against a master; asserting the empty AVIF set is
    /// the strongest honest claim, and it flips the moment that stops being
    /// true.
    authors_actor_values: bool,
}

const ROSTER_CASES: &[RosterCase] = &[
    RosterCase {
        label: "FNV",
        env: "BYROREDUX_FNV_DATA",
        fallback: "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
        master: "FalloutNV.esm",
        profile: byroredux_core::character::CharacterRulesProfile::FALLOUT_NEW_VEGAS,
        attributes: byroredux_core::character::AttributeSet::FALLOUT,
        derived_rows: Some(8),
        authors_actor_values: true,
    },
    RosterCase {
        label: "FO3",
        env: "BYROREDUX_FO3_DATA",
        fallback: "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
        master: "Fallout3.esm",
        profile: byroredux_core::character::CharacterRulesProfile::FALLOUT3,
        attributes: byroredux_core::character::AttributeSet::FALLOUT,
        derived_rows: Some(8),
        authors_actor_values: true,
    },
    RosterCase {
        label: "FO4",
        env: "BYROREDUX_FO4_DATA",
        fallback: "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
        master: "Fallout4.esm",
        profile: byroredux_core::character::CharacterRulesProfile::FALLOUT4,
        attributes: byroredux_core::character::AttributeSet::FALLOUT,
        // Health + Action Points + Carry Weight. FO4 authors no `MeleeDamage`
        // AVIF (#3093), so the shared FO3/FNV rows have no FO4 counterpart.
        derived_rows: Some(3),
        authors_actor_values: true,
    },
    RosterCase {
        label: "Skyrim",
        env: "BYROREDUX_SKYRIMSE_DATA",
        fallback: "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
        master: "Skyrim.esm",
        profile: byroredux_core::character::CharacterRulesProfile::SKYRIM,
        attributes: byroredux_core::character::AttributeSet::SKYRIM,
        derived_rows: None,
        authors_actor_values: true,
    },
    RosterCase {
        label: "Oblivion",
        env: "BYROREDUX_OBL_DATA",
        fallback: "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
        master: "Oblivion.esm",
        profile: byroredux_core::character::CharacterRulesProfile::OBLIVION,
        attributes: byroredux_core::character::AttributeSet::TES_CLASSIC,
        derived_rows: None,
        authors_actor_values: false,
    },
];

/// Assert one game's rosters + derived-row output keys against its own master.
fn assert_rosters_resolve(case: &RosterCase) {
    let Some(data) = data_dir(case.env, case.fallback) else {
        eprintln!("[{} CHARAL] skipping: game data unavailable", case.label);
        return;
    };
    let path = data.join(case.master);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let index = parse_esm(&bytes).unwrap_or_else(|e| panic!("parse {path:?}: {e:?}"));

    // The header→profile classification the rest of this asserts against.
    // Also the real-data half of the FO3-vs-FNV `HEDR < 1.0` split, which was
    // pinned by a synthetic-HEDR unit test on the FO3 side only.
    assert_eq!(
        index.character_rules,
        case.profile,
        "[{}] {} must classify as the {} character profile",
        case.label,
        case.master,
        case.profile.name()
    );

    if !case.authors_actor_values {
        assert!(
            index.actor_values.is_empty(),
            "[{}] {} was expected to author no AVIF records at all — if it now \
             does, its rosters became falsifiable and this case should assert \
             them like the others",
            case.label,
            case.master
        );
        return;
    }

    assert!(
        !index.actor_values.is_empty(),
        "[{}] no AVIF records parsed — every assertion below would pass \
         vacuously",
        case.label
    );

    let mut unresolved: Vec<&str> = Vec::new();
    for attr in case.attributes.members() {
        if index.actor_value_form_id(attr.editor_id()).is_none() {
            unresolved.push(attr.editor_id());
        }
    }
    for skill in index.character_rules.skills().skills() {
        if index.actor_value_form_id(skill.editor_id).is_none() {
            unresolved.push(skill.editor_id);
        }
    }
    assert!(
        unresolved.is_empty(),
        "[{}] roster entries with no authored AVIF in {}: {:?} — a roster keyed \
         on a display name instead of the record's identity produces a green \
         builder test and an empty table",
        case.label,
        case.master,
        unresolved
    );

    let ruleset = index.character_rules.build_ruleset(
        |id| index.actor_value_form_id(id),
        |id| index.game_setting_float(id),
    );
    match case.derived_rows {
        Some(expected) => {
            let rows = ruleset
                .as_ref()
                .unwrap_or_else(|| {
                    panic!(
                        "[{}] profile {} must build a ruleset",
                        case.label,
                        case.profile.name()
                    )
                })
                .derived_row_len();
            assert_eq!(
                rows, expected,
                "[{}] {} derived rows built against the real AVIF set, expected \
                 {expected} — every row is resolve-or-skip on its OUTPUT editor \
                 id, so a shortfall means an output key this game does not author",
                case.label, rows
            );
        }
        None => assert!(
            ruleset.is_none(),
            "[{}] profile {} has no wired RulesetBuilder arm; if one landed, \
             give this case its expected derived_rows",
            case.label,
            case.profile.name()
        ),
    }
}

#[test]
#[ignore]
fn charal_rosters_and_derived_keys_resolve_on_every_shipped_master() {
    for case in ROSTER_CASES {
        assert_rosters_resolve(case);
    }
}

#[test]
#[ignore]
fn skyrim_default_water_promotes_underwater_tail() {
    let Some(data) = data_dir(
        "BYROREDUX_SKYRIMSE_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
    ) else {
        eprintln!("[Skyrim WATR] skipping: game data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).expect("read Skyrim.esm");
    let index = parse_esm(&bytes).expect("parse Skyrim.esm");
    let water = index.waters.get(&0x0000_0018).expect("Skyrim default WATR");
    assert!(matches!(water.raw_dnam.len(), 228 | 232));
    if water.raw_dnam.len() >= 232 {
        assert!((water.params.flowmap_scale - 1.0).abs() < 1e-6);
    } else {
        assert_eq!(water.params.flowmap_scale, 0.0);
    }
    assert!(water.params.underwater_fog_far > water.params.underwater_fog_near);
    assert!(water.params.underwater_fog_far >= 900.0);
    assert!((water.params.wave_amplitude - 0.1).abs() < 1e-6);
    assert!((water.params.noise_uv_scale_a - 1.0 / 1920.0).abs() < 1e-6);
    assert!((water.params.noise_uv_scale_b - 1.0 / 6703.0).abs() < 1e-6);
    assert!((water.params.noise_uv_scale_c - 1.0 / 488.0).abs() < 1e-6);
    for (actual, expected) in water
        .params
        .noise_amplitude_scales
        .into_iter()
        .zip([0.6957, 0.6304, 0.4746])
    {
        assert!((actual - expected).abs() < 1e-5);
    }
    for (actual, expected) in water
        .params
        .depth_weights
        .into_iter()
        .zip([0.9, 0.5, 0.1, 0.2])
    {
        assert!((actual - expected).abs() < 1e-5);
    }
    for (actual, expected) in water
        .params
        .effect_controls
        .into_iter()
        .zip([9.0, 500.0, 0.34, 3.2])
    {
        assert!((actual - expected).abs() < 1e-5);
    }
}

#[test]
#[ignore]
fn installed_masters_water_fields_are_finite_and_ordered() {
    // Keep this as a cross-generation invariant rather than asserting one
    // byte layout: WATR DATA/DNAM tails differ between Oblivion, FO3/FNV,
    // Skyrim SE, and FO4, but the translated render contract must remain
    // finite and usable for every shipped record.
    let masters = [
        (
            "BYROREDUX_OBL_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
            "Oblivion.esm",
            "Oblivion",
        ),
        (
            "BYROREDUX_FNV_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            "FalloutNV.esm",
            "FNV",
        ),
        (
            "BYROREDUX_FO3_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
            "Fallout3.esm",
            "FO3",
        ),
        (
            "BYROREDUX_SKYRIMSE_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
            "Skyrim.esm",
            "Skyrim SE",
        ),
        (
            "BYROREDUX_FO4_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
            "Fallout4.esm",
            "FO4",
        ),
        (
            "BYROREDUX_FO76_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout76/Data",
            "SeventySix.esm",
            "FO76",
        ),
        (
            "BYROREDUX_STARFIELD_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data",
            "Starfield.esm",
            "Starfield",
        ),
    ];

    let mut checked_games = 0;
    for (env_var, fallback, filename, label) in masters {
        let Some(data) = data_dir(env_var, fallback) else {
            eprintln!("[{label} WATR] skipping: game data unavailable");
            continue;
        };
        let bytes = std::fs::read(data.join(filename)).expect("read installed master");
        let index = parse_esm(&bytes).expect("parse installed master");
        assert!(
            !index.waters.is_empty(),
            "{label} master must contain WATR records"
        );

        let mut authored_absorption_records = 0;
        for water in index.waters.values() {
            let p = water.params;
            if p.absorption_coefficients.iter().any(|value| *value > 0.0) {
                authored_absorption_records += 1;
            }
            let scalars = [
                p.fog_near,
                p.fog_far,
                p.depth_amount,
                p.underwater_fog_near,
                p.underwater_fog_far,
                p.reflectivity,
                p.fresnel,
                p.wind_speed,
                p.wind_direction,
                p.wave_amplitude,
                p.wave_frequency,
                p.sun_specular_power,
                p.noise_uv_scale_a,
                p.noise_uv_scale_b,
                p.noise_uv_scale_c,
                p.flowmap_scale,
                p.roughness,
                p.silt_amount,
            ];
            assert!(
                p.shallow_color
                    .iter()
                    .chain(p.deep_color.iter())
                    .chain(p.underwater_color.iter())
                    .chain(p.reflection_color.iter())
                    .chain(scalars.iter())
                    .chain(p.noise_amplitude_scales.iter())
                    .chain(p.depth_weights.iter())
                    .chain(p.effect_controls.iter())
                    .chain(std::iter::once(&p.specular_magnitude))
                    .chain(std::iter::once(&p.underwater_fog_amount))
                    .chain(p.absorption_coefficients.iter())
                    .chain(p.silt_light_color.iter())
                    .chain(p.silt_dark_color.iter())
                    .all(|value| value.is_finite()),
                "{label} WATR {} has non-finite translated fields",
                water.editor_id
            );
            assert!(p.fog_far >= p.fog_near, "{label} WATR fog ramp is inverted");
            assert!(
                p.underwater_fog_far == 0.0 || p.underwater_fog_far >= p.underwater_fog_near,
                "{label} WATR underwater fog ramp is inverted"
            );
        }
        if label == "Starfield" {
            assert!(
                authored_absorption_records > 0,
                "Starfield WATR records must expose authored extinction coefficients"
            );
            let water_clear = index
                .waters
                .values()
                .find(|water| water.editor_id == "WaterClear")
                .expect("Starfield WaterClear WATR");
            let water_mud_brown = index
                .waters
                .values()
                .find(|water| water.editor_id == "WaterMudBrown")
                .expect("Starfield WaterMudBrown WATR");
            for (actual, expected) in water_clear
                .params
                .absorption_coefficients
                .into_iter()
                .zip([0.16558, 0.09624, 0.07627])
            {
                assert!((actual - expected).abs() < 1.0e-5);
            }
            for (actual, expected) in water_clear
                .params
                .concentration
                .into_iter()
                .zip([8.840, 6.594, 4.710, 0.5145])
            {
                assert!((actual - expected).abs() < 1.0e-4);
            }
            for (actual, expected) in water_mud_brown
                .params
                .concentration
                .into_iter()
                .zip([7.392, 15.580, 18.550, 1.0])
            {
                assert!((actual - expected).abs() < 1.0e-4);
            }
            assert_ne!(
                water_clear.params.concentration, water_mud_brown.params.concentration,
                "distinct vanilla concentration vectors must survive parsing"
            );
            assert!(
                water_clear.params.concentration[0] > 1.0
                    && water_mud_brown.params.concentration[2] > 1.0,
                "vanilla pigment concentrations must not be normalized at the parser boundary"
            );
            assert!((water_clear.params.depth_amount - 8.0).abs() < 1.0e-5);
            assert_eq!(water_clear.params.fog_near, 80.0);
            assert_eq!(water_clear.params.fog_far, 600.0);
        }
        if label == "Oblivion" {
            let citadel = index
                .waters
                .values()
                .find(|water| water.editor_id == "OblivionCitadelLavaPlane")
                .expect("OblivionCitadelLavaPlane WATR");
            let lava_test = index
                .waters
                .values()
                .find(|water| water.editor_id == "OblivionLavaTest01")
                .expect("OblivionLavaTest01 WATR");
            assert_eq!(citadel.material_name, "lava");
            assert_eq!(lava_test.material_name, "lava");
            assert_eq!(citadel.legacy_flags, Some(0x01));
            assert_eq!(citadel.legacy_damage, Some(5000));
            assert_eq!(lava_test.legacy_damage, Some(50));
            assert!(citadel.texture_path.is_empty());
            assert!(lava_test.texture_path.is_empty());
            assert!(!lava_test.diffuse_texture_path.is_empty());
        }
        checked_games += 1;
    }
    assert!(
        checked_games > 0,
        "at least one installed master is required"
    );
}

#[test]
#[ignore]
fn fo4_ruleset_uses_only_authored_avif_outputs() {
    let Some(data) = data_dir(
        "BYROREDUX_FO4_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
    ) else {
        eprintln!("[FO4 AVIF] skipping: game data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("Fallout4.esm")).expect("read Fallout4.esm");
    let index = parse_esm(&bytes).expect("parse Fallout4.esm");
    let ruleset = byroredux_core::character::fallout4_ruleset(|editor_id| {
        index.actor_value_form_id(editor_id)
    });
    assert_eq!(ruleset.derived_row_len(), 3);
    assert_eq!(index.actor_value_form_id("Health"), Some(0x0000_02D4));
    assert_eq!(index.actor_value_form_id("ActionPoints"), Some(0x0000_02D5));
    assert_eq!(index.actor_value_form_id("CarryWeight"), Some(0x0000_02DC));
    assert_eq!(index.actor_value_form_id("MeleeDamage"), None);
}

#[test]
#[ignore]
fn parse_rate_fnv_esm() {
    let Some(data) = data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    ) else {
        eprintln!("[FNV] skipping: BYROREDUX_FNV_DATA unset and fallback path missing");
        return;
    };
    let esm_path = data.join("FalloutNV.esm");
    let bytes = std::fs::read(&esm_path).expect("read FalloutNV.esm");
    let parse_start = std::time::Instant::now();
    let index = parse_esm(&bytes).expect("parse FalloutNV.esm");
    let parse_elapsed = parse_start.elapsed();
    // #527 — fused single-pass walker. Pre-fix audit baseline was
    // 1.21s release on a cold load (two full walks of the 70 MB
    // ESM); post-fix observed ~1.095s. The timing is diagnostic
    // only — too disk-cache-sensitive to assert against without
    // dedicated bench infra. The functional baselines below catch
    // any regression that would matter to consumers.
    eprintln!("[FNV] parse_esm wall={:?}", parse_elapsed);

    eprintln!(
        "[FNV] total={} | items={} containers={} LVLI={} LVLN={} NPCs={} \
         races={} classes={} factions={} globals={} game_settings={} \
         packages={} quests={} dialogues={} messages={} perks={} \
         spells={} magic_effects={} activators={} terminals={} form_lists={} \
         projectiles={} effect_shaders={} item_mods={} armor_addons={} body_parts={} \
         reputations={} explosions={} combat_styles={} idle_animations={} \
         impacts={} impact_data_sets={} recipes(COBJ)={} recipe_categories(RCCT)={} \
         recipe_records(RCPE)={} trees={}",
        index.total(),
        index.items.len(),
        index.containers.len(),
        index.leveled_items.len(),
        index.leveled_npcs.len(),
        index.npcs.len(),
        index.races.len(),
        index.classes.len(),
        index.factions.len(),
        index.globals.len(),
        index.game_settings.len(),
        index.packages.len(),
        index.quests.len(),
        index.dialogues.len(),
        index.messages.len(),
        index.perks.len(),
        index.spells.len(),
        index.magic_effects.len(),
        index.activators.len(),
        index.terminals.len(),
        index.form_lists.len(),
        index.projectiles.len(),
        index.effect_shaders.len(),
        index.item_mods.len(),
        index.armor_addons.len(),
        index.body_parts.len(),
        index.reputations.len(),
        index.explosions.len(),
        index.combat_styles.len(),
        index.idle_animations.len(),
        index.impacts.len(),
        index.impact_data_sets.len(),
        index.recipes.len(),
        index.recipe_categories.len(),
        index.recipe_records.len(),
        index.trees.len(),
    );

    // Primary M24 baseline assertion — the "13,684 structured records"
    // claim that the ROADMAP, CLAUDE.md, and the FNV audit all cite.
    // Covers every EsmIndex category (items, containers, LVLI/LVLN, NPCs,
    // races, classes, factions, globals, game_settings, weathers,
    // climates, scripts, supplementary records, cells + statics).
    assert!(
        index.total() >= FNV_TOTAL_FLOOR,
        "FNV total {} < M24 Phase 1 baseline {}",
        index.total(),
        FNV_TOTAL_FLOOR,
    );

    // Per-category floors — mirror the existing inline test at
    // records/mod.rs:525-574 so a single-category regression fails
    // loud even when the total stays above the overall floor.
    assert!(index.items.len() > 2500, "items={}", index.items.len());
    assert!(
        index.containers.len() > 2000,
        "containers={}",
        index.containers.len(),
    );
    assert!(
        index.leveled_items.len() > 2000,
        "LVLI={}",
        index.leveled_items.len(),
    );
    assert!(
        index.leveled_npcs.len() > 250,
        "LVLN={}",
        index.leveled_npcs.len(),
    );
    assert!(index.npcs.len() > 3000, "NPCs={}", index.npcs.len());
    assert!(
        index.factions.len() > 500,
        "factions={}",
        index.factions.len()
    );
    assert!(
        index.game_settings.len() > 500,
        "game_settings={}",
        index.game_settings.len(),
    );

    // Floors for the 7 categories added in #446/#447. Observed FNV
    // counts: packages=4163, quests=436, dialogues=18215, messages=1144,
    // perks=176, spells=270, magic_effects=289. Each floor sits a few
    // percent below.
    assert!(index.packages.len() > 4000, "PACK={}", index.packages.len());
    assert!(index.quests.len() > 400, "QUST={}", index.quests.len());
    assert!(
        index.dialogues.len() > 17_000,
        "DIAL={}",
        index.dialogues.len(),
    );
    assert!(index.messages.len() > 1000, "MESG={}", index.messages.len(),);
    assert!(index.perks.len() > 150, "PERK={}", index.perks.len());
    assert!(index.spells.len() > 250, "SPEL={}", index.spells.len());
    assert!(
        index.magic_effects.len() > 270,
        "MGEF={}",
        index.magic_effects.len(),
    );

    // ACTI / TERM floors (#521). Issue body estimated ≥1500/≥400;
    // reference run on vanilla FNV (no DLC) observes 1143/344 —
    // the audit's estimates included DLC content that isn't in a
    // fresh Steam install. Floors sit a few percent below the
    // observed vanilla numbers to absorb cell-group-skip edge cases
    // without masking a dispatch regression.
    assert!(
        index.activators.len() >= 1000,
        "ACTI={} (expected >= 1000; vanilla ships 1143)",
        index.activators.len(),
    );
    assert!(
        index.terminals.len() >= 300,
        "TERM={} (expected >= 300; vanilla ships 344)",
        index.terminals.len(),
    );

    // #630 / audit FNV-D2-02 regression guard: FLST FormID lists must
    // dispatch end-to-end. Pre-fix the entire top-level group fell
    // through to the catch-all skip and every `IsInList <flst>` perk
    // condition / Caravan deck lookup hit an empty map. Vanilla
    // FalloutNV.esm ships ~340 FLST records; floor at 250 absorbs the
    // BSA-vs-loose-files edge case without masking a dispatch
    // regression. At least one FLST must carry > 1 entry — an
    // EDID-only FLST with empty entries is the parse-side indicator
    // of a sub-record extraction regression.
    assert!(
        index.form_lists.len() >= 250,
        "FLST={} (expected >= 250; vanilla ships ~340)",
        index.form_lists.len(),
    );

    // SpeedTree Phase 1.1 / TREE record dispatch. Pre-fix TREE collapsed
    // into the generic MODL-only path, dropping ICON / SNAM / CNAM /
    // BNAM / PFIG silently. Vanilla FNV ships 3 TREE bases (Joshua tree,
    // creosote, dead tree); the floor at >= 1 absorbs DLC-only TREE
    // additions without masking a dispatch regression. Each must have a
    // non-empty model_path that ends in `.spt` — that's the SpeedTree
    // route the cell loader will eventually branch on.
    assert!(
        !index.trees.is_empty(),
        "TREE={} — every FNV exterior tree REFR points at a TREE base; \
         the dispatch must produce at least one entry",
        index.trees.len(),
    );
    let spt_trees = index
        .trees
        .values()
        .filter(|t| t.has_speedtree_binary())
        .count();
    assert_eq!(
        spt_trees,
        index.trees.len(),
        "every vanilla FNV TREE points at a `.spt` — found {}/{} routed \
         through the SpeedTree path",
        spt_trees,
        index.trees.len(),
    );
    let flst_with_entries = index
        .form_lists
        .values()
        .filter(|f| f.entries.len() > 1)
        .count();
    assert!(
        flst_with_entries >= 100,
        "FLSTs with >1 entry = {}/{} — pre-fix 0/0 because the group \
         was skipped; expected >= 100 after #630",
        flst_with_entries,
        index.form_lists.len(),
    );

    // #808 / audit FNV-D2-NEW-01 regression guard: 5 gameplay-critical
    // record types must dispatch end-to-end. Pre-fix each top-level
    // group fell through to the catch-all skip and the entire
    // category lookup returned an empty map. Floors below sit a few
    // percent under the observed vanilla counts so a dispatch
    // regression fails loud while ordinary content drift doesn't.
    //
    // Observed vanilla counts (FalloutNV.esm, no DLC, 2026-05-03):
    //   PROJ=95, EFSH=35, IMOD=50, ARMA=131, BPTD=49.
    // The audit body's "150-300 / 100 / 100-200 / 700+" estimates were
    // inflated against the FO3+FNV+DLC superset; vanilla FNV ships
    // smaller numbers. DLC content will push these up.
    assert!(
        index.projectiles.len() >= 80,
        "PROJ={} (expected >= 80; vanilla ships ~95)",
        index.projectiles.len(),
    );
    assert!(
        index.effect_shaders.len() >= 30,
        "EFSH={} (expected >= 30; vanilla ships ~35)",
        index.effect_shaders.len(),
    );
    assert!(
        index.item_mods.len() >= 40,
        "IMOD={} (expected >= 40; vanilla ships ~50)",
        index.item_mods.len(),
    );
    assert!(
        index.armor_addons.len() >= 110,
        "ARMA={} (expected >= 110; vanilla ships ~131)",
        index.armor_addons.len(),
    );
    assert!(
        index.body_parts.len() >= 40,
        "BPTD={} (expected >= 40; vanilla ships ~49)",
        index.body_parts.len(),
    );

    // At least one PROJ must have a parsed muzzle_speed > 0 — proves
    // the DATA sub-record decode fires, not just the EDID extraction.
    let projs_with_speed = index
        .projectiles
        .values()
        .filter(|p| p.muzzle_speed > 1.0)
        .count();
    assert!(
        projs_with_speed >= 60,
        "PROJ with muzzle_speed > 0 = {}/{}, expected >= 60 (DATA \
         decode regression)",
        projs_with_speed,
        index.projectiles.len(),
    );

    // At least one ARMA must have non-zero biped_flags — proves the
    // BMDT decode fires. ARMOs with zero biped flags exist (the all-
    // race-default ARMA from a few records) but most ARMAs have a
    // body region set.
    let arma_with_biped = index
        .armor_addons
        .values()
        .filter(|a| a.biped_flags != 0)
        .count();
    assert!(
        arma_with_biped >= 100,
        "ARMA with non-zero biped_flags = {}/{}, expected >= 100 \
         (BMDT decode regression)",
        arma_with_biped,
        index.armor_addons.len(),
    );

    // #809 / audit FNV-D2-NEW-02 regression guard: 7 supporting record
    // types must dispatch end-to-end. Pre-fix each fell through to the
    // catch-all skip.
    //
    // Observed vanilla counts (FalloutNV.esm, no DLC, 2026-05-03):
    //   REPU=13, EXPL=154, CSTY=84, IDLE=1597, IPCT=125, IPDS=60, COBJ=0.
    //
    // COBJ=0 is intentional — vanilla FNV's crafting system predates
    // the COBJ-driven recipe table (FO3 introduces the type but FNV
    // workbenches use script effects, not COBJ records). DLC content
    // (Honest Hearts, Old World Blues, Lonesome Road) adds some COBJs
    // but vanilla ships an empty group. Floor at 0 documents this.
    assert!(
        index.reputations.len() >= 10,
        "REPU={} (expected >= 10; vanilla ships ~13)",
        index.reputations.len(),
    );

    // #3325 — the FACT -> REPU edge. Before this landed, `index.reputations`
    // was an orphan map: 13 parsed records with nothing able to say which
    // faction moves which meter. The floor pins that the edge exists AND
    // that every binding lands inside `reputations` — a binding pointing at
    // a non-REPU FormID would mean the remap or the sub-record is wrong.
    let fact_reputation_bindings: Vec<u32> = index
        .factions
        .values()
        .filter_map(|faction| faction.reputation)
        .collect();
    assert!(
        fact_reputation_bindings.len() >= 46,
        "FACT WMI1 bindings={} (expected >= 46; vanilla ships 46)",
        fact_reputation_bindings.len(),
    );
    let unresolved: Vec<u32> = fact_reputation_bindings
        .iter()
        .copied()
        .filter(|form_id| !index.reputations.contains_key(form_id))
        .collect();
    assert!(
        unresolved.is_empty(),
        "every FACT WMI1 binding must resolve into index.reputations; \
         {} did not: {:08X?}",
        unresolved.len(),
        &unresolved[..unresolved.len().min(8)],
    );

    // #3324 — WEAP `VATS`. `ap_cost` was pinned to 0.0 for every FNV weapon
    // because the parser searched `DNAM`. The census found 245 of 261 WEAP
    // records carrying a `VATS` sub-record, 45 of them with a non-zero AP
    // cost at offset 12.
    let (vats_records, nonzero_ap) =
        index
            .items
            .values()
            .fold((0usize, 0usize), |acc, item| match &item.kind {
                byroredux_plugin::esm::records::ItemKind::Weapon { ap_cost, vats, .. } => (
                    acc.0 + usize::from(vats.is_some()),
                    acc.1 + usize::from(*ap_cost > 0.0),
                ),
                _ => acc,
            });
    assert!(
        vats_records >= 240,
        "WEAP with decoded VATS={vats_records} (expected >= 240; vanilla ships 245)",
    );
    assert!(
        nonzero_ap >= 40,
        "WEAP with non-zero VATS ap_cost={nonzero_ap} (expected >= 40; vanilla ships 45)",
    );
    assert!(
        index.explosions.len() >= 130,
        "EXPL={} (expected >= 130; vanilla ships ~154)",
        index.explosions.len(),
    );
    assert!(
        index.combat_styles.len() >= 70,
        "CSTY={} (expected >= 70; vanilla ships ~84)",
        index.combat_styles.len(),
    );
    assert!(
        index.idle_animations.len() >= 1400,
        "IDLE={} (expected >= 1400; vanilla ships ~1597)",
        index.idle_animations.len(),
    );
    assert!(
        index.impacts.len() >= 100,
        "IPCT={} (expected >= 100; vanilla ships ~125)",
        index.impacts.len(),
    );
    assert!(
        index.impact_data_sets.len() >= 50,
        "IPDS={} (expected >= 50; vanilla ships ~60)",
        index.impact_data_sets.len(),
    );
    // COBJ vanilla=0 — dispatch arm is in place; DLC content adds some.

    // #3040 — RCPE is FNV's actual live recipe format (COBJ's predecessor
    // on this game, not its successor). It's already summed into the
    // long-tail floor below via `recipe_records`, but pin a standalone
    // floor too so a regression in this specific record's dispatch doesn't
    // hide inside the 31-record sum — this is the record a `recipes=0`
    // reading of the COBJ counter could otherwise misread as "FNV authors
    // no recipes at all".
    assert!(
        index.recipe_records.len() >= 90,
        "RCPE={} (expected >= 90; vanilla ships ~106)",
        index.recipe_records.len(),
    );

    // At least one EXPL must have parsed damage > 0 — proves the DATA
    // sub-record decode fires.
    let expls_with_damage = index.explosions.values().filter(|e| e.damage > 0.0).count();
    assert!(
        expls_with_damage >= 100,
        "EXPL with damage > 0 = {}/{}, expected >= 100 (DATA decode \
         regression)",
        expls_with_damage,
        index.explosions.len(),
    );

    // At least one CSTY must have non-zero csty_flags — proves the
    // CSTD sub-record decode fires.
    let csty_with_flags = index
        .combat_styles
        .values()
        .filter(|c| c.csty_flags != 0)
        .count();
    assert!(
        csty_with_flags >= 50,
        "CSTY with non-zero flags = {}/{}, expected >= 50 (CSTD decode \
         regression)",
        csty_with_flags,
        index.combat_styles.len(),
    );

    // At least one IDLE must have a non-empty animation_path — proves
    // MODL extraction fires.
    let idle_with_path = index
        .idle_animations
        .values()
        .filter(|i| !i.animation_path.is_empty())
        .count();
    assert!(
        idle_with_path >= 1000,
        "IDLE with animation_path = {}/{}, expected >= 1000 (MODL \
         extraction regression)",
        idle_with_path,
        index.idle_animations.len(),
    );

    // #810 / audit FNV-D2-NEW-03 regression guard: the 31 long-tail
    // record types must dispatch end-to-end via `parse_minimal_esm_record`.
    // Pre-fix each fell through the catch-all skip. Granular per-record
    // floors aren't worth the test churn — when a real consumer arrives
    // and a record gains its own dedicated parser via the #808/#809
    // pattern, the per-record floor lands with that work. Instead pin
    // the SUM as a single anti-regression guard: vanilla FNV ships 5000+
    // records across the long tail (1000+ SOUN alone), so a count below
    // 1000 means the dispatch arms aren't firing.
    let long_tail_total: usize = index.audio_locations.len()
        + index.animation_objects.len()
        + index.acoustic_spaces.len()
        + index.camera_shots.len()
        + index.camera_paths.len()
        + index.default_objects.len()
        + index.menu_icons.len()
        + index.media_sets.len()
        + index.music_types.len()
        + index.sounds.len()
        + index.voice_types.len()
        + index.ammo_effects.len()
        + index.debris.len()
        + index.grasses.len()
        + index.imagespace_modifiers.len()
        + index.load_screens.len()
        + index.load_screen_types.len()
        + index.placeable_waters.len()
        + index.ragdolls.len()
        + index.dehydration_stages.len()
        + index.hunger_stages.len()
        + index.radiation_stages.len()
        + index.sleep_deprivation_stages.len()
        + index.caravan_cards.len()
        + index.caravan_decks.len()
        + index.challenges.len()
        + index.poker_chips.len()
        + index.caravan_money.len()
        + index.casinos.len()
        + index.recipe_categories.len()
        + index.recipe_records.len();
    assert!(
        long_tail_total >= 1000,
        "long-tail total = {} (expected >= 1000; vanilla FNV ships ~5500 \
         across the 31 record types — most of that is SOUN). A count \
         this low means the dispatch arms aren't firing.",
        long_tail_total,
    );

    // SOUN is the largest single contributor (~1100 vanilla); pin a
    // stand-alone floor so a SOUN-specific dispatch regression fails
    // loud independently of the other 30 records.
    assert!(
        index.sounds.len() >= 800,
        "SOUN={} (expected >= 800; vanilla ships ~1100)",
        index.sounds.len(),
    );

    eprintln!(
        "[FNV] long-tail total = {} | sounds={} idle={} grasses={} debris={}",
        long_tail_total,
        index.sounds.len(),
        index.idle_animations.len(),
        index.grasses.len(),
        index.debris.len(),
    );

    // #533 / audit M33-01 regression guard: at least one FNV weather must
    // have a non-zero NAM0 sky colour. Pre-fix the `>= 240 B` gate dropped
    // ~12/63 FNV weathers silently (those using the 160-B stride). Weather
    // count floor: FNV ships ≥50 WTHRs and at least the common-case ones
    // (e.g. NVWastelandClear*) must parse.
    assert!(
        index.weathers.len() >= 50,
        "FNV weathers={} (expected >= 50)",
        index.weathers.len(),
    );
    let nonzero_nam0 = index
        .weathers
        .values()
        .filter(|w| {
            let c = w.sky_colors[0][1]; // SKY_UPPER / TOD_DAY
            c.r != 0 || c.g != 0 || c.b != 0
        })
        .count();
    assert!(
        nonzero_nam0 >= 40,
        "FNV non-zero-NAM0 weathers={}/{}, expected >= 40",
        nonzero_nam0,
        index.weathers.len(),
    );

    // #534 / audit M33-02 regression guard: cloud texture sub-records
    // live in DNAM/CNAM/ANAM/BNAM (not 00TX-03TX). Pre-fix the parser
    // populated zero cloud textures across every WTHR in every shipped
    // master. FNV weathers near-universally ship DNAM (layer 0) per the
    // FourCC histogram (63/63 in vanilla).
    let with_layer_0 = index
        .weathers
        .values()
        .filter(|w| {
            w.cloud_textures[0]
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_some()
        })
        .count();
    assert!(
        with_layer_0 >= 50,
        "FNV weathers with cloud layer 0 = {}/{} — pre-fix 0/63; \
         expected >= 50 after #534",
        with_layer_0,
        index.weathers.len(),
    );

    // #536 / audit M33-04 regression guard: FNV FNAM fog parsing.
    // Pre-fix every FNV weather defaulted to `fog_day_far = 10000.0`
    // because the FNAM arm body was empty (comment claimed "fallback
    // when HNAM is absent" but FNV has no HNAM). Count weathers with
    // any non-default fog field as proof the body now fires.
    let with_nondefault_fog = index
        .weathers
        .values()
        .filter(|w| {
            (w.fog_day_far - 10000.0).abs() > 0.1
                || w.fog_day_near != 0.0
                || (w.fog_night_far - 10000.0).abs() > 0.1
                || w.fog_night_near != 0.0
        })
        .count();
    assert!(
        with_nondefault_fog >= 50,
        "FNV weathers with non-default fog = {}/{} — pre-fix 0/63; \
         expected >= 50 after #536",
        with_nondefault_fog,
        index.weathers.len(),
    );

    // #538 regression guard: classification at DATA byte 11. Find the
    // canonical `NVWastelandClear` and confirm its classification flag
    // is `WTHR_PLEASANT`. Pre-fix the parser read byte 13 (padding) and
    // returned `0x00` for this record.
    let clear = index
        .weathers
        .values()
        .find(|w| w.editor_id == "NVWastelandClear")
        .expect("NVWastelandClear should parse");
    assert_eq!(
        clear.classification,
        byroredux_plugin::esm::records::weather::WTHR_PLEASANT,
        "NVWastelandClear should classify as PLEASANT; got 0x{:02X}",
        clear.classification,
    );

    // #1538 regression guard: SCOL (static collections) must parse for FNV.
    // The `is_fo4_plus` gate wrongly treated SCOL as FO4-only and skipped
    // the whole GRUP, dropping all 98 FalloutNV.esm SCOL bases — 1084 REFRs
    // (road segments, guardrails, debris LOD clusters) then mis-resolved to
    // nothing. SCOL is a Gamebryo-Fallout record (FO3 54, FNV 98); the gate
    // now admits Fallout3NV. Exact count pins the parse, not a floor.
    assert_eq!(
        index.cells.scols.len(),
        98,
        "FNV must parse exactly 98 SCOL bases (pre-#1538 the is_fo4_plus \
         gate skipped the whole GRUP, leaving 0); got {}",
        index.cells.scols.len(),
    );
}

#[test]
#[ignore]
fn parse_rate_fo3_esm() {
    let Some(data) = data_dir(
        "BYROREDUX_FO3_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
    ) else {
        eprintln!("[FO3] skipping: BYROREDUX_FO3_DATA unset and fallback path missing");
        return;
    };
    let esm_path = data.join("Fallout3.esm");
    let bytes = std::fs::read(&esm_path).expect("read Fallout3.esm");
    let index = parse_esm(&bytes).expect("parse Fallout3.esm");
    eprintln!(
        "[FO3] total={} | items={} containers={} LVLI={} LVLN={} LVLC={} \
         NPCs={} creatures={} factions={} globals={} game_settings={} \
         scripts={} trees={}",
        index.total(),
        index.items.len(),
        index.containers.len(),
        index.leveled_items.len(),
        index.leveled_npcs.len(),
        index.leveled_creatures.len(),
        index.npcs.len(),
        index.creatures.len(),
        index.factions.len(),
        index.globals.len(),
        index.game_settings.len(),
        index.scripts.len(),
        index.trees.len(),
    );

    // `index.total()` sums the ~95 typed category maps (index.rs::total),
    // a subset of the file's structured records — NOT the raw record count.
    // Observed 2026-04 on the GOTY master: 31,101; FO3_TOTAL_FLOOR (30,000)
    // sits just below it to absorb patch drift without masking regressions.
    // (The stale "18,007 records" from AUDIT_FO3_2026-04-19 predates the
    // #446/#447 category additions — see the FO3_TOTAL_FLOOR const doc.)
    // Distinct from the *file* baseline re-verified 2026-05-26: 44,657 total
    // = 37,459 structured + 7,198 NAVM.
    assert!(
        index.total() >= FO3_TOTAL_FLOOR,
        "FO3 total {} < audit baseline {}",
        index.total(),
        FO3_TOTAL_FLOOR,
    );

    // FO3-specific record categories — CREA + LVLC + SCPT resolve
    // regressions around the FO3 audit fixes (#442, #443, #448).
    assert!(
        index.creatures.len() >= 50,
        "CREA={} — FO3 bestiary must parse per #442",
        index.creatures.len(),
    );
    // LVLC floor reflects observed FO3.esm count (60 vanilla, GOTY
    // patch revision). The audit's "FO3 uses LVLC for most enemies"
    // characterization was off — FO3 actually leans on LVLN like FNV
    // with a small LVLC tail. Keep the floor low to absorb DLC patches
    // without masking a full regression.
    assert!(
        index.leveled_creatures.len() >= 40,
        "LVLC={} — FO3 enemy spawn tables must parse per #448",
        index.leveled_creatures.len(),
    );
    assert!(
        index.scripts.len() >= 500,
        "SCPT={} — pre-Papyrus bytecode records must parse per #443",
        index.scripts.len(),
    );

    // #533 / audit M33-01: FO3 NAM0 is 160 B (not 240). Pre-fix the parser
    // silently dropped every FO3 weather → black sky dome on every
    // exterior. Assert the fix by requiring most weathers (vanilla ships
    // 27 WTHRs; some are stubs like DefaultWeather with zero bytes on
    // disk) to parse to at least one non-zero RGB channel in SKY_UPPER.
    assert!(
        index.weathers.len() >= 20,
        "FO3 weathers={} (expected >= 20)",
        index.weathers.len(),
    );
    let nonzero_nam0 = index
        .weathers
        .values()
        .filter(|w| {
            let c = w.sky_colors[0][1]; // SKY_UPPER / TOD_DAY
            c.r != 0 || c.g != 0 || c.b != 0
        })
        .count();
    assert!(
        nonzero_nam0 >= 15,
        "FO3 non-zero-NAM0 weathers={}/{} — expected >= 15 after #533 fix; \
         pre-fix every weather dropped NAM0 silently",
        nonzero_nam0,
        index.weathers.len(),
    );

    // #534 / audit M33-02: FO3 ships 27 WTHRs, every one has DNAM.
    let with_layer_0 = index
        .weathers
        .values()
        .filter(|w| {
            w.cloud_textures[0]
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_some()
        })
        .count();
    assert!(
        with_layer_0 >= 20,
        "FO3 weathers with cloud layer 0 = {}/{} — expected >= 20 after #534",
        with_layer_0,
        index.weathers.len(),
    );

    // #536 / audit M33-04: FO3 FNAM fog.
    let with_nondefault_fog = index
        .weathers
        .values()
        .filter(|w| {
            (w.fog_day_far - 10000.0).abs() > 0.1
                || w.fog_day_near != 0.0
                || (w.fog_night_far - 10000.0).abs() > 0.1
                || w.fog_night_near != 0.0
        })
        .count();
    assert!(
        with_nondefault_fog >= 15,
        "FO3 weathers with non-default fog = {}/{} — expected >= 15 after #536",
        with_nondefault_fog,
        index.weathers.len(),
    );

    // SpeedTree Phase 1.1 / TREE record dispatch — same shape as the
    // FNV assertion. Vanilla FO3 ships 9 TREE bases (DC swamp foliage
    // + a handful of dead trees). Every one points at a `.spt`.
    assert!(
        !index.trees.is_empty(),
        "FO3 TREE={} — DC swamp / wasteland trees must dispatch",
        index.trees.len(),
    );
    let spt_trees = index
        .trees
        .values()
        .filter(|t| t.has_speedtree_binary())
        .count();
    assert_eq!(
        spt_trees,
        index.trees.len(),
        "every vanilla FO3 TREE points at a `.spt` — found {}/{} routed \
         through the SpeedTree path",
        spt_trees,
        index.trees.len(),
    );
}

/// Oblivion: the 160-byte NAM0 stride target of #533. Minimal parse
/// harness (no per-category floors — that lives in future Oblivion
/// dispatch work) just verifies every NAM0 is read. Observed vanilla
/// 2026-04: 19 CLMTs, 37 WTHRs.
#[test]
#[ignore]
fn parse_rate_oblivion_esm() {
    let Some(data) = data_dir(
        "BYROREDUX_OBL_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    ) else {
        eprintln!("[OBL] skipping: BYROREDUX_OBL_DATA unset and fallback path missing");
        return;
    };
    let esm_path = data.join("Oblivion.esm");
    let bytes = std::fs::read(&esm_path).expect("read Oblivion.esm");
    let index = parse_esm(&bytes).expect("parse Oblivion.esm");

    eprintln!(
        "[OBL] total={} | weathers={} climates={} trees={}",
        index.total(),
        index.weathers.len(),
        index.climates.len(),
        index.trees.len(),
    );

    // #2909 — nested temporary CELL records without XCLC must not replace
    // the type-1 world's structurally persistent CELL.
    let market_persistent = index
        .cells
        .worldspace_persistent_cells
        .get("icmarketdistrict")
        .expect("ICMarketDistrict persistent CELL");
    assert_eq!(market_persistent.form_id, 0x0002_C12B);
    assert_eq!(
        market_persistent.references.len(),
        58,
        "ICMarketDistrict persistent reference set was clobbered",
    );

    // #533 / audit M33-01: Oblivion NAM0 is 160 B. Same gate failure as
    // FO3 pre-fix — every WTHR silently dropped. Assertion mirrors the
    // FO3 one.
    assert!(
        index.weathers.len() >= 30,
        "OBL weathers={} (expected >= 30)",
        index.weathers.len(),
    );
    let nonzero_nam0 = index
        .weathers
        .values()
        .filter(|w| {
            let c = w.sky_colors[0][1]; // SKY_UPPER / TOD_DAY
            c.r != 0 || c.g != 0 || c.b != 0
        })
        .count();
    assert!(
        nonzero_nam0 >= 25,
        "OBL non-zero-NAM0 weathers={}/{} — expected >= 25 after #533 fix",
        nonzero_nam0,
        index.weathers.len(),
    );

    // #534 / audit M33-02: Oblivion ships 2 cloud layers (DNAM + CNAM).
    // Histogram: DNAM on 35/37 WTHRs.
    let with_layer_0 = index
        .weathers
        .values()
        .filter(|w| {
            w.cloud_textures[0]
                .as_deref()
                .filter(|s| !s.is_empty())
                .is_some()
        })
        .count();
    assert!(
        with_layer_0 >= 25,
        "OBL weathers with cloud layer 0 = {}/{} — expected >= 25 after #534",
        with_layer_0,
        index.weathers.len(),
    );

    // #536 / audit M33-04: Oblivion FNAM is 16 B and carries fog (HNAM
    // is 56 B of *different* lighting-model fields — see #537). Pre-fix
    // the HNAM arm gated on `>= 16` and silently overwrote FNAM's
    // correct fog values with HNAM's first-4-f32 lighting parameters,
    // saturating every Oblivion exterior to `fog_far ≈ 4.0`.
    let with_nondefault_fog = index
        .weathers
        .values()
        .filter(|w| {
            (w.fog_day_far - 10000.0).abs() > 0.1
                || w.fog_day_near != 0.0
                || (w.fog_night_far - 10000.0).abs() > 0.1
                || w.fog_night_near != 0.0
        })
        .count();
    assert!(
        with_nondefault_fog >= 25,
        "OBL weathers with non-default fog = {}/{} — expected >= 25 after #536",
        with_nondefault_fog,
        index.weathers.len(),
    );
    // Sanity bound: no Oblivion weather should come back with
    // `fog_far < 100` (that was the HNAM-clobber footprint).
    let tiny_fog = index
        .weathers
        .values()
        .filter(|w| w.fog_day_far > 0.0 && w.fog_day_far < 100.0)
        .count();
    assert_eq!(
        tiny_fog, 0,
        "OBL weathers with absurd fog_day_far < 100 = {} — \
         pre-fix HNAM clobbered fog_far to ~4.0. Should be 0 after #536.",
        tiny_fog,
    );

    // #538: Oblivion is the cleanest evidence — its vanilla WTHRs span
    // all four flag values. Pin one of each against byte 11.
    use byroredux_plugin::esm::records::weather::{
        WTHR_CLOUDY, WTHR_PLEASANT, WTHR_RAINY, WTHR_SNOW,
    };
    for (edid, expected) in &[
        ("Clear", WTHR_PLEASANT),
        ("Cloudy", WTHR_CLOUDY),
        ("Rain", WTHR_RAINY),
        ("Snow", WTHR_SNOW),
    ] {
        let w = index
            .weathers
            .values()
            .find(|w| w.editor_id == *edid)
            .unwrap_or_else(|| panic!("OBL weather '{}' should parse", edid));
        assert_eq!(
            w.classification, *expected,
            "OBL '{}' classification = 0x{:02X}; expected 0x{:02X}",
            edid, w.classification, *expected,
        );
    }

    // SpeedTree Phase 1.1 / TREE record dispatch — Oblivion is the
    // densest forest content in the lineage (vanilla Cyrodiil ships
    // 142 TREE bases for the various oak / pine / birch / etc.
    // species). The floor at >= 100 absorbs DLC trims without
    // masking a regression. Every one points at a `.spt`.
    assert!(
        index.trees.len() >= 100,
        "OBL TREE={} (expected >= 100; vanilla ships 142) — Cyrodiil \
         forests rely entirely on the TREE dispatch",
        index.trees.len(),
    );
    let spt_trees = index
        .trees
        .values()
        .filter(|t| t.has_speedtree_binary())
        .count();
    assert_eq!(
        spt_trees,
        index.trees.len(),
        "every vanilla Oblivion TREE points at a `.spt` — found {}/{} \
         routed through the SpeedTree path",
        spt_trees,
        index.trees.len(),
    );
    // Sanity: at least one TREE carries CNAM (canopy params). Pre-fix
    // CNAM was silently dropped alongside ICON/SNAM/BNAM/PFIG.
    let with_cnam = index
        .trees
        .values()
        .filter(|t| !t.canopy_params.is_empty())
        .count();
    assert!(
        with_cnam >= 100,
        "OBL TREE with CNAM = {}/{} — pre-#TREE every CNAM dropped silently",
        with_cnam,
        index.trees.len(),
    );

    // #966 / OBL-D3-NEW-02 — Oblivion-unique base records that fell
    // through the catch-all skip pre-fix. Floors below vanilla counts
    // so DLC trims / patches don't fail the test, but high enough to
    // catch a dispatch-arm regression.
    eprintln!(
        "[OBL] birthsigns={} clothing={} apparatuses={} sigil_stones={} soul_gems={}",
        index.birthsigns.len(),
        index.clothing.len(),
        index.apparatuses.len(),
        index.sigil_stones.len(),
        index.soul_gems.len(),
    );
    assert!(
        index.birthsigns.len() >= 13,
        "OBL BSGN = {} (expected >= 13 — vanilla ships exactly 13)",
        index.birthsigns.len(),
    );
    assert!(
        index.clothing.len() >= 100,
        "OBL CLOT = {} (expected >= 100 — vanilla ~150)",
        index.clothing.len(),
    );
    assert!(
        index.apparatuses.len() >= 4,
        "OBL APPA = {} (expected >= 4 — vanilla ships 4 tools)",
        index.apparatuses.len(),
    );
    assert!(
        index.sigil_stones.len() >= 10,
        "OBL SGST = {} (expected >= 10)",
        index.sigil_stones.len(),
    );
    assert!(
        index.soul_gems.len() >= 10,
        "OBL SLGM = {} (expected >= 10)",
        index.soul_gems.len(),
    );
    // Every SLGM must surface SLCP soul_capacity > 0 — the audit
    // originally mis-named the field as "DATA byte 0" but the
    // authoritative source is the SLCP sub-record. A zero capacity
    // means the parser silently dropped SLCP again.
    let with_capacity = index
        .soul_gems
        .values()
        .filter(|s| s.soul_capacity > 0)
        .count();
    assert!(
        with_capacity * 2 >= index.soul_gems.len(),
        "at least half of OBL SLGMs should carry SLCP, got {}/{}",
        with_capacity,
        index.soul_gems.len(),
    );
    // Sanity: soul magnitude enums fit in 0..=5.
    for s in index.soul_gems.values() {
        assert!(
            s.soul_capacity <= 5 && s.current_soul <= 5,
            "SLGM '{}' soul enum out of range: capacity={} current={}",
            s.editor_id,
            s.soul_capacity,
            s.current_soul,
        );
    }
}

/// FO4: vanilla `Fallout4.esm` parse-rate harness. Mirrors the FNV /
/// FO3 patterns. Floors sit a few percent below 2026-05-04 observed
/// counts to absorb patch drift without masking dispatch regressions.
///
/// Closes #819 / FO4-D4-NEW-07 — was missing while FNV / FO3 / Oblivion
/// each had one. Floors specifically lock in the 5 FO4-architecture
/// categories that #817 added to `EsmIndex::categories()`
/// (texture_sets / scols / packins / movables / material_swaps).
#[test]
#[ignore]
fn parse_rate_fo4_esm() {
    let Some(data) = data_dir(
        "BYROREDUX_FO4_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
    ) else {
        eprintln!("[FO4] skipping: BYROREDUX_FO4_DATA unset and fallback path missing");
        return;
    };
    let esm_path = data.join("Fallout4.esm");
    let bytes = std::fs::read(&esm_path).expect("read Fallout4.esm");
    let parse_start = std::time::Instant::now();
    let index = parse_esm(&bytes).expect("parse Fallout4.esm");
    let parse_elapsed = parse_start.elapsed();
    eprintln!("[FO4] parse_esm wall={:?}", parse_elapsed);

    let default_water = index
        .waters
        .get(&0x0000_0018)
        .expect("Fallout4.esm default WATR 0x18");
    eprintln!(
        "[FO4] WATR 0x18 {} DNAM={} params={:?}",
        default_water.editor_id,
        default_water.raw_dnam.len(),
        default_water.params,
    );
    assert_eq!(default_water.editor_id, "ExtOceanWater");
    assert_eq!(default_water.raw_dnam.len(), 201);
    assert!((default_water.params.shallow_color[0] - 45.0 / 255.0).abs() < 1e-6);
    assert!((default_water.params.deep_color[2] - 57.0 / 255.0).abs() < 1e-6);
    assert!((default_water.params.underwater_color[0] - 80.0 / 255.0).abs() < 1e-6);
    assert!((default_water.params.wave_amplitude - 0.1).abs() < 1e-6);
    assert!((default_water.params.wave_frequency - 0.85).abs() < 1e-6);
    assert!((default_water.params.noise_uv_scale_a - 0.0001).abs() < 1e-7);
    assert!((default_water.params.reflectivity - 0.2935).abs() < 1e-6);
    assert!((default_water.params.fresnel - 0.058).abs() < 1e-6);
    assert_eq!(default_water.params.fog_near, 80.0);
    assert_eq!(default_water.params.fog_far, 600.0);

    let scol_placements: usize = index
        .cells
        .scols
        .values()
        .map(|s| s.parts.iter().map(|p| p.placements.len()).sum::<usize>())
        .sum();
    let junk_items = index
        .items
        .values()
        .filter(|item| matches!(item.kind, byroredux_plugin::esm::records::ItemKind::Junk))
        .count();
    let loose_mod_items = index
        .items
        .values()
        .filter(|item| {
            matches!(
                item.kind,
                byroredux_plugin::esm::records::ItemKind::Mod { .. }
            )
        })
        .count();
    let decoded_weapons = index.items.values().filter_map(|item| match &item.kind {
        byroredux_plugin::esm::records::ItemKind::Weapon { damage, .. } => Some(*damage),
        _ => None,
    });
    let (weapon_count, nonzero_weapon_damage) = decoded_weapons
        .fold((0usize, 0usize), |(count, nonzero), damage| {
            (count + 1, nonzero + usize::from(damage > 0))
        });
    let (armor_count, armor_with_weight, armor_with_rating) = index.items.values().fold(
        (0usize, 0usize, 0usize),
        |(count, weighted, rated), item| match &item.kind {
            byroredux_plugin::esm::records::ItemKind::Armor {
                armor_rating_x100, ..
            } => (
                count + 1,
                weighted + usize::from(item.common.weight > 0.0),
                rated + usize::from(*armor_rating_x100 > 0),
            ),
            _ => (count, weighted, rated),
        },
    );
    let (ammo_count, ammo_with_value, ammo_with_weight) = index.items.values().fold(
        (0usize, 0usize, 0usize),
        |(count, valued, weighted), item| match item.kind {
            byroredux_plugin::esm::records::ItemKind::Ammo { .. } => (
                count + 1,
                valued + usize::from(item.common.value > 0),
                weighted + usize::from(item.common.weight > 0.0),
            ),
            _ => (count, valued, weighted),
        },
    );
    let (book_count, books_with_value) =
        index
            .items
            .values()
            .fold((0usize, 0usize), |(count, valued), item| match item.kind {
                byroredux_plugin::esm::records::ItemKind::Book { .. } => {
                    (count + 1, valued + usize::from(item.common.value > 0))
                }
                _ => (count, valued),
            });

    eprintln!(
        "[FO4] total={} game={:?} | cells={} statics={} scols={} \
         (placements={}) packins={} movables={} material_swaps={} \
         texture_sets={} items={} containers={} LVLI={} LVLN={} NPCs={} \
         races={} classes={} factions={} globals={} game_settings={} \
         weathers={} climates={} trees={} OMOD-links={} junk={} mods={}",
        index.total(),
        index.game,
        index.cells.cells.len(),
        index.cells.statics.len(),
        index.cells.scols.len(),
        scol_placements,
        index.cells.packins.len(),
        index.cells.movables.len(),
        index.cells.material_swaps.len(),
        index.cells.texture_sets.len(),
        index.items.len(),
        index.containers.len(),
        index.leveled_items.len(),
        index.leveled_npcs.len(),
        index.npcs.len(),
        index.races.len(),
        index.classes.len(),
        index.factions.len(),
        index.globals.len(),
        index.game_settings.len(),
        index.weathers.len(),
        index.climates.len(),
        index.trees.len(),
        index.object_mod_loose_items.len(),
        junk_items,
        loose_mod_items,
    );

    // HEDR → GameKind dispatch. Pre-#439 the FO4 master would
    // misclassify as Fallout3NV; this guard keeps that fixed.
    assert_eq!(
        index.game,
        GameKind::Fallout4,
        "FO4 ESM classified as {:?}, expected Fallout4",
        index.game,
    );
    assert!(
        index.object_mod_loose_items.len() >= 2_300,
        "OMOD loose-item relations={} (expected >= 2300; vanilla yields 2409)",
        index.object_mod_loose_items.len(),
    );
    assert!(
        junk_items >= 600,
        "FO4 Junk={} (expected >= 600; vanilla yields 620 component-bearing MISC records)",
        junk_items,
    );
    assert!(
        loose_mod_items >= 1_200,
        "FO4 Mods={} (expected >= 1200; vanilla yields 1283 OMOD-linked loose MISC records)",
        loose_mod_items,
    );
    assert!(
        weapon_count >= 250 && nonzero_weapon_damage > 0,
        "FO4 WEAP decode regressed: {weapon_count} records, \
         {nonzero_weapon_damage} with authored damage",
    );
    assert!(
        armor_count >= 680 && armor_with_weight > 0 && armor_with_rating > 0,
        "FO4 ARMO decode regressed: {armor_count} records, {armor_with_weight} \
         with weight, {armor_with_rating} with rating",
    );
    assert!(
        ammo_count >= 55 && ammo_with_value > 0 && ammo_with_weight > 0,
        "FO4 AMMO decode regressed: {ammo_count} records, {ammo_with_value} \
         with value, {ammo_with_weight} with weight",
    );
    assert!(
        book_count >= 320 && books_with_value > 0,
        "FO4 BOOK decode regressed: {book_count} records, \
         {books_with_value} with authored value",
    );

    // Primary baseline. With #817 categories landed, observed 2026-05-04
    // is 76,468 records.
    assert!(
        index.total() >= FO4_TOTAL_FLOOR,
        "FO4 total {} < baseline {}",
        index.total(),
        FO4_TOTAL_FLOOR,
    );

    // FO4-architecture categories — #817 made these visible to
    // category_breakdown(). A regression that empties any of them
    // (e.g. `parse_scol_group` rewrite that drops the insert) must
    // fail loud here. Live counts: scols=2617, packins=872,
    // material_swaps=2537, texture_sets=379, movables=0 (vanilla).
    assert!(
        index.cells.scols.len() >= 2500,
        "SCOL={} (expected >= 2500; vanilla ships 2617) — \
         dispatch / parse regression",
        index.cells.scols.len(),
    );
    assert!(
        index.cells.packins.len() >= 850,
        "PKIN={} (expected >= 850; vanilla ships 872)",
        index.cells.packins.len(),
    );
    assert!(
        index.cells.material_swaps.len() >= 2400,
        "MSWP={} (expected >= 2400; vanilla ships 2537)",
        index.cells.material_swaps.len(),
    );
    assert!(
        index.cells.texture_sets.len() >= 376,
        "TXST={} (expected >= 376; vanilla ships 379) — \
         DODT + DNAM now parsed (#813/#814); 3 remaining below ceiling \
         are records with no parseable sub-records in vanilla Fallout4.esm",
        index.cells.texture_sets.len(),
    );
    // MOVS: vanilla ships 0; pin to 0 to catch a future spurious
    // population (DLC-only or mod-content additions can lift this
    // floor when those harnesses arrive).
    assert_eq!(
        index.cells.movables.len(),
        0,
        "MOVS={} (vanilla Fallout4.esm ships 0; non-zero indicates \
         a DLC was loaded — bump the floor when that's expected)",
        index.cells.movables.len(),
    );

    // SCOL placement decode regression guard (#405). 2617 SCOL
    // records expand to 40,330 ONAM-anchored placements on vanilla.
    // A regression in ScolPlacement::from_bytes that returns None
    // unconditionally would drop placement count to 0 while
    // record count stays at 2617.
    assert!(
        scol_placements >= 38_000,
        "SCOL placements = {} (expected >= 38_000; vanilla yields \
         40_330 across 2617 records). #405 ONAM/DATA decode \
         regression suspected.",
        scol_placements,
    );

    // Cell + STAT floors — the FO4 cell loader pipeline depends on these
    // populating before SCOL placements can resolve. #2911 recovered 231
    // legitimate interiors without EDID, lifting the vanilla count from
    // 964 to 1195.
    assert!(
        index.cells.cells.len() >= 1_190,
        "FO4 cells={} (expected >= 1190; vanilla ships 1195)",
        index.cells.cells.len(),
    );
    assert!(
        index.cells.statics.len() >= 30_000,
        "FO4 statics={} (expected >= 30_000; vanilla ships 31_989)",
        index.cells.statics.len(),
    );

    // Per-category floors mirroring the FNV / FO3 harness shape.
    // Observed vanilla: items=4076, containers=471, LVLI=2098,
    // LVLN=228, NPCs=3015, factions=699, globals=1346,
    // game_settings=2039, weathers=71, climates=7, races=45,
    // classes=31.
    assert!(index.items.len() >= 3800, "items={}", index.items.len());
    assert!(
        index.containers.len() >= 450,
        "containers={}",
        index.containers.len(),
    );
    assert!(
        index.leveled_items.len() >= 1900,
        "LVLI={}",
        index.leveled_items.len(),
    );
    assert!(
        index.leveled_npcs.len() >= 200,
        "LVLN={}",
        index.leveled_npcs.len(),
    );
    assert!(index.npcs.len() >= 2800, "NPCs={}", index.npcs.len());
    assert!(index.races.len() >= 40, "races={}", index.races.len());
    assert!(index.classes.len() >= 25, "classes={}", index.classes.len(),);
    assert!(
        index.factions.len() >= 660,
        "factions={}",
        index.factions.len(),
    );
    assert!(
        index.globals.len() >= 1200,
        "globals={}",
        index.globals.len(),
    );
    assert!(
        index.game_settings.len() >= 1900,
        "game_settings={}",
        index.game_settings.len(),
    );
    assert!(
        index.weathers.len() >= 60,
        "weathers={}",
        index.weathers.len(),
    );
    assert!(
        index.climates.len() >= 6,
        "climates={}",
        index.climates.len(),
    );
}

/// #967 / OBL-D3-NEW-03 — real-Oblivion RACE coverage. Pins the
/// audit's requested invariant: every vanilla race must surface a
/// non-zero `base_height` (the 1.0 default leaves through DATA
/// short-reads — pre-#967 we never wrote anything to it) AND at
/// least one race must surface non-default voice forms via VNAM.
///
/// `#[ignore]`-gated by Oblivion install (mirrors `parse_rate_oblivion_esm`).
#[test]
#[ignore]
fn race_oblivion_data_and_subs_against_vanilla() {
    let Some(data) = data_dir(
        "BYROREDUX_OBL_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    ) else {
        eprintln!("[OBL/RACE] skip: data dir missing");
        return;
    };
    let bytes = std::fs::read(data.join("Oblivion.esm")).expect("read Oblivion.esm");
    let index = parse_esm(&bytes).expect("parse Oblivion.esm");

    assert!(
        index.races.len() >= 15,
        "OBL races={} (vanilla ships at least 15 races)",
        index.races.len(),
    );

    // DATA: every race must surface base_height in the documented
    // 0.5..2.0 range. The 1.0 default is a legitimate authoring
    // value (Imperial / Breton ship 1.0 deliberately), so we can't
    // just check `!= 1.0`. The sanity gate catches NaN / garbage
    // f32 reads without false-negatives on 1.0-author races.
    let sane_heights = index
        .races
        .values()
        .filter(|r| {
            (0.5..=2.0).contains(&r.base_height.0) && (0.5..=2.0).contains(&r.base_height.1)
        })
        .count();
    assert_eq!(
        sane_heights,
        index.races.len(),
        "OBL races with sane base_height={}/{} (NaN / garbage from DATA?)",
        sane_heights,
        index.races.len(),
    );
    // At least one race must author a non-1.0 height — proves the
    // DATA read actually consumed disk bytes and didn't fall through
    // to defaults across the board. Vanilla beast races ship 1.04.
    let non_default_height = index
        .races
        .values()
        .filter(|r| r.base_height.0 != 1.0 || r.base_height.1 != 1.0)
        .count();
    assert!(
        non_default_height >= 5,
        "OBL races with non-default base_height={}/{} \
         (DATA parse never wrote anything?)",
        non_default_height,
        index.races.len(),
    );

    // VNAM / DNAM / ATTR floors — vanilla Oblivion authors these on
    // a SUBSET of races (not every race). Empirical run 2026-05-18:
    // 15 races total / 5 with VNAM / ? with DNAM / ? with ATTR.
    // Each floor is "at least one" so a future regression that
    // silently dropped all of these would fail; the upper bound
    // floats with authoring choices.
    let with_voices = index
        .races
        .values()
        .filter(|r| r.voice_forms.is_some())
        .count();
    assert!(
        with_voices >= 1,
        "OBL races with VNAM voice_forms={}/{} (expected at least 1)",
        with_voices,
        index.races.len(),
    );

    let with_hair = index
        .races
        .values()
        .filter(|r| r.default_hair.is_some())
        .count();
    assert!(
        with_hair >= 1,
        "OBL races with DNAM default_hair={}/{} (expected at least 1)",
        with_hair,
        index.races.len(),
    );

    let with_attr = index
        .races
        .values()
        .filter(|r| r.base_attributes.is_some())
        .count();
    assert!(
        with_attr >= 1,
        "OBL races with ATTR={}/{} (expected at least 1)",
        with_attr,
        index.races.len(),
    );

    eprintln!(
        "[OBL/RACE] races={} | sane_heights={} non_default_heights={} \
         voices={} hairs={} attrs={}",
        index.races.len(),
        sane_heights,
        non_default_height,
        with_voices,
        with_hair,
        with_attr,
    );
}

/// #968 / OBL-D3-NEW-04 — real-Oblivion CLAS coverage. Pins the
/// audit's regression assertion: vanilla "Knight" class must surface
/// `primary_attributes = Some((Strength, Personality))`,
/// `specialization = Some(0 /* Combat */)`, and `major_skills.len() == 7`.
///
/// `#[ignore]`-gated by Oblivion install.
#[test]
#[ignore]
fn clas_oblivion_knight_against_vanilla() {
    let Some(data) = data_dir(
        "BYROREDUX_OBL_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    ) else {
        eprintln!("[OBL/CLAS] skip: data dir missing");
        return;
    };
    let bytes = std::fs::read(data.join("Oblivion.esm")).expect("read Oblivion.esm");
    let index = parse_esm(&bytes).expect("parse Oblivion.esm");

    // Vanilla CLAS count: 111 in Oblivion.esm (empirical 2026-05-18).
    assert!(
        index.classes.len() >= 100,
        "OBL classes={} (expected >= 100)",
        index.classes.len(),
    );

    let knight = index
        .classes
        .values()
        .find(|c| c.editor_id == "Knight")
        .expect("vanilla Oblivion.esm must include the 'Knight' CLAS");

    // Strength = 0, Personality = 6 per Oblivion's attribute enum
    // (0..7 = Str/Int/Wil/Agi/Spd/End/Per/Luck).
    assert_eq!(
        knight.primary_attributes,
        Some((0, 6)),
        "Knight.primary_attributes = (Strength, Personality)",
    );
    assert_eq!(
        knight.specialization,
        Some(0),
        "Knight.specialization = 0 (Combat)",
    );
    // Audit asserted major_skills.len() == 7; empirical decode of
    // the 52-byte DATA confirms 7 (vs the audit prose's claim of 14).
    assert_eq!(knight.major_skills.len(), 7);
    // Vanilla majors: Block / Illusion / HeavyArmor / Blunt / Blade /
    // Speechcraft / HandToHand (SkillIndex values).
    assert_eq!(
        knight.major_skills,
        vec![0x0F, 0x17, 0x12, 0x10, 0x0E, 0x20, 0x11],
        "Knight.major_skills = [Block, Illusion, HeavyArmor, Blunt, \
         Blade, Speechcraft, HandToHand]",
    );
    // Playable flag.
    assert_eq!(knight.flags_oblivion, Some(0x01));

    // Sanity gate: every Oblivion class should surface primary
    // attributes + 7 majors. Pre-#968 the FNV arm ran for Oblivion
    // and produced garbage `attribute_weights` + nonsense `tag_skills`.
    let with_primaries = index
        .classes
        .values()
        .filter(|c| c.primary_attributes.is_some() && c.major_skills.len() == 7)
        .count();
    assert_eq!(
        with_primaries,
        index.classes.len(),
        "OBL CLAS with primary_attributes + 7 majors = {}/{} \
         (DATA parse failed on some?)",
        with_primaries,
        index.classes.len(),
    );

    eprintln!(
        "[OBL/CLAS] classes={} | Knight ok | with_primaries={}",
        index.classes.len(),
        with_primaries,
    );
}

// ── #1181 / FO4-D4-004 — unconditional FO4-architecture fixture ─────────
//
// `parse_rate_fo4_esm` above is `#[ignore]`-gated because it needs the
// real Fallout4.esm. CI without game data skips it, so the
// five-map regression net (`texture_sets` / `scols` / `packins` /
// `movables` / `material_swaps` floors in `EsmIndex::categories()`)
// only fires on opt-in. A refactor that silently empties one of those
// maps would not surface in default CI.
//
// This test builds a synthetic Fallout4-shape ESM in-memory — a TES4
// header with HEDR version 1.0 (the FO4 dispatch band per
// `reader.rs::GameKind::from_header`) followed by minimal SCOL / PKIN /
// TXST / MSWP records. After `parse_esm` it asserts each typed map has
// at least one entry. MOVS is omitted because vanilla Fallout4.esm
// ships zero MOVS records — the dispatch arm is exercised by the
// `parse_rate_fo4_esm` ignored-test (which still pins MOVS == 0).
//
// See audit `docs/audits/AUDIT_FO4_2026-05-18.md` D4-004 + #819 (real-
// data harness) + #817 (five-map exposure).

/// Build a 24-byte-header record (`typ`, `form_id`, sub-record list).
/// Mirrors the helper in `crates/plugin/src/esm/records/tests.rs`
/// (private to the unit-test cfg); duplicated here so this integration
/// test stays self-contained.
fn fixture_build_record(typ: &[u8; 4], form_id: u32, subs: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    for (st, data) in subs {
        sub_data.extend_from_slice(*st);
        sub_data.extend_from_slice(&(data.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(data);
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(typ);
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // timestamp + VC + unknown
    buf.extend_from_slice(&sub_data);
    buf
}

/// Wrap a record payload in a top-level GRUP (`label`, group_type = 0).
fn fixture_wrap_top_group(label: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let total = 24 + payload.len();
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GRUP");
    buf.extend_from_slice(&(total as u32).to_le_bytes());
    buf.extend_from_slice(label);
    buf.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top-level)
    buf.extend_from_slice(&[0u8; 8]); // timestamp + VC
    buf.extend_from_slice(payload);
    buf
}

/// Build a TES4 file header with HEDR version 1.0 — the FO4 dispatch
/// band per `reader.rs::GameKind::from_header` (`0.98..=1.04` →
/// `Fallout4`).
fn fixture_build_fo4_tes4() -> Vec<u8> {
    let mut hedr = Vec::new();
    hedr.extend_from_slice(b"HEDR");
    hedr.extend_from_slice(&12u16.to_le_bytes());
    hedr.extend_from_slice(&1.0f32.to_le_bytes()); // FO4 version
    hedr.extend_from_slice(&4u32.to_le_bytes()); // record_count (informational)
    hedr.extend_from_slice(&0u32.to_le_bytes()); // next_object_id

    let mut buf = Vec::new();
    buf.extend_from_slice(b"TES4");
    buf.extend_from_slice(&(hedr.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&0u32.to_le_bytes()); // form_id
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&hedr);
    buf
}

#[test]
fn parse_fo4_architecture_fixture_populates_typed_maps() {
    // SCOL: empty subs still inserts (parse_scol_group has no record-
    // contents condition on the insert). Minimal EDID makes the fixture
    // realistic + survives the StaticObject `editor_id.is_empty()` gate.
    let scol = fixture_build_record(b"SCOL", 0x0010_0001, &[(b"EDID", b"TestScol\0")]);
    let scol_group = fixture_wrap_top_group(b"SCOL", &scol);

    // PKIN: same shape — EDID + the unconditional `packins.insert`.
    let pkin = fixture_build_record(b"PKIN", 0x0020_0001, &[(b"EDID", b"TestPkin\0")]);
    let pkin_group = fixture_wrap_top_group(b"PKIN", &pkin);

    // TXST: needs at least one TX00..TX07 or MNAM so the parsed
    // `TextureSet` differs from `default()` — the walker's insert gate
    // at `cell/support.rs:290` skips records that produce a default-
    // valued set.
    let txst = fixture_build_record(
        b"TXST",
        0x0030_0001,
        &[
            (b"EDID", b"TestTxst\0"),
            (b"TX00", b"textures/test/diffuse.dds\0"),
        ],
    );
    let txst_group = fixture_wrap_top_group(b"TXST", &txst);

    // MSWP: unconditional insert in `parse_mswp_group`.
    let mswp = fixture_build_record(b"MSWP", 0x0040_0001, &[(b"EDID", b"TestMswp\0")]);
    let mswp_group = fixture_wrap_top_group(b"MSWP", &mswp);

    // Assemble the synthetic ESM: TES4 header + the four top-level
    // GRUPs in any order. Walker dispatches by GRUP label so order is
    // free — pick the same as vanilla (TXST → SCOL → PKIN → MSWP) for
    // readability.
    let mut esm = fixture_build_fo4_tes4();
    esm.extend_from_slice(&txst_group);
    esm.extend_from_slice(&scol_group);
    esm.extend_from_slice(&pkin_group);
    esm.extend_from_slice(&mswp_group);

    let index = parse_esm(&esm).expect("parse synthetic FO4 fixture");

    // HEDR → GameKind: 1.0 falls in the (0.98..=1.04) FO4 band.
    assert_eq!(
        index.game,
        GameKind::Fallout4,
        "synthetic HEDR=1.0 must classify as Fallout4 (got {:?})",
        index.game,
    );

    // Five-map regression net floors (the actual #1181 contract).
    // MOVS is intentionally omitted — vanilla ships zero MOVS records;
    // the dispatch arm coverage lives in `parse_rate_fo4_esm`'s
    // `assert_eq!(... movables.len(), 0)` pin.
    assert!(
        !index.cells.scols.is_empty(),
        "SCOL dispatch arm dropped — `scols` map empty after parsing a \
         synthetic SCOL record (#1181 / FO4-D4-004 net)",
    );
    assert!(
        !index.cells.packins.is_empty(),
        "PKIN dispatch arm dropped — `packins` map empty after parsing a \
         synthetic PKIN record (#1181 / FO4-D4-004 net)",
    );
    assert!(
        !index.cells.texture_sets.is_empty(),
        "TXST dispatch arm dropped — `texture_sets` map empty after parsing \
         a synthetic TXST record with TX00 populated (#1181 / FO4-D4-004 net)",
    );
    assert!(
        !index.cells.material_swaps.is_empty(),
        "MSWP dispatch arm dropped — `material_swaps` map empty after parsing \
         a synthetic MSWP record (#1181 / FO4-D4-004 net)",
    );

    // Spot-check the form-IDs landed at the right keys — guards against
    // a future refactor that inserts everything under the wrong map
    // (e.g. SCOL → packins) which would still satisfy the non-empty
    // floors above.
    assert!(
        index.cells.scols.contains_key(&0x0010_0001),
        "SCOL form-id 0x00100001 not present in scols map",
    );
    assert!(
        index.cells.packins.contains_key(&0x0020_0001),
        "PKIN form-id 0x00200001 not present in packins map",
    );
    assert!(
        index.cells.texture_sets.contains_key(&0x0030_0001),
        "TXST form-id 0x00300001 not present in texture_sets map",
    );
    assert!(
        index.cells.material_swaps.contains_key(&0x0040_0001),
        "MSWP form-id 0x00400001 not present in material_swaps map",
    );
}

/// One-off diagnostic for the misplaced-saloon-wall investigation
/// (2026-05-26). Walks `GSProspectorSaloonInterior` REFRs against
/// FalloutNV.esm and emits a TSV-ish dump sorted by spatial
/// position so we can correlate to the in-game render.
///
/// Columns: refr_form, base_form, base_mesh, pos_x, pos_y, pos_z,
///          rot_x_deg, rot_y_deg, rot_z_deg, scale
///
/// Run: `BYROREDUX_FNV_DATA=... cargo test -p byroredux-plugin
///       --release --test parse_real_esm -- --ignored
///       dump_prospector_saloon_refrs --nocapture`
#[test]
#[ignore]
fn dump_prospector_saloon_refrs() {
    let Some(data) = data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    ) else {
        eprintln!("[dump] skipping: BYROREDUX_FNV_DATA unset and fallback path missing");
        return;
    };
    let bytes = std::fs::read(data.join("FalloutNV.esm")).expect("read FalloutNV.esm");
    let index = parse_esm(&bytes).expect("parse FalloutNV.esm");

    let key = "gsprospectorsaloonInterior".to_ascii_lowercase();
    let Some(cell) = index.cells.cells.get(&key) else {
        eprintln!(
            "[dump] cell '{key}' not found; got {} interior cells",
            index.cells.cells.len()
        );
        return;
    };

    let mut rows: Vec<_> = cell
        .references
        .iter()
        .map(|r| {
            let mesh = index
                .cells
                .statics
                .get(&r.base_form_id)
                .map(|s| s.model_path.clone())
                .unwrap_or_else(|| String::from("<no base>"));
            (r, mesh)
        })
        .collect();
    // Sort by (mesh-name asc, then position-X) so duplicate base meshes group together.
    rows.sort_by(|a, b| {
        let m = a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase());
        if m == std::cmp::Ordering::Equal {
            a.0.position[0]
                .partial_cmp(&b.0.position[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            m
        }
    });

    eprintln!(
        "[dump] GSProspectorSaloonInterior REFRs: {}\n\
         refr_form\tbase_form\tpos_x\tpos_y\tpos_z\trx_deg\try_deg\trz_deg\tscale\tmesh",
        rows.len()
    );
    let rad2deg = 180.0 / std::f32::consts::PI;
    for (r, mesh) in &rows {
        eprintln!(
            "{:08X}\t{:08X}\t{:>8.1}\t{:>8.1}\t{:>8.1}\t{:>+7.1}\t{:>+7.1}\t{:>+7.1}\t{:.2}\t{}",
            r.form_id,
            r.base_form_id,
            r.position[0],
            r.position[1],
            r.position[2],
            r.rotation[0] * rad2deg,
            r.rotation[1] * rad2deg,
            r.rotation[2] * rad2deg,
            r.scale,
            mesh
        );
    }

    // Tally multi-axis REFRs (those whose rotation has TWO or more
    // non-trivial Euler components — these are the ones that would
    // expose XYZ vs ZYX product divergence post the 2026-05-26 fix).
    let mut multi_axis = 0usize;
    let mut only_z = 0usize;
    let eps = 0.01_f32.to_radians();
    for (r, _) in &rows {
        let nx = r.rotation[0].abs() > eps;
        let ny = r.rotation[1].abs() > eps;
        let nz = r.rotation[2].abs() > eps;
        match (nx, ny, nz) {
            (false, false, _) => only_z += 1,
            (true, _, _) | (_, true, _) => multi_axis += 1,
        }
    }
    eprintln!(
        "[dump] rotation profile: {} multi-axis (rx or ry non-zero), {} z-only / identity",
        multi_axis, only_z
    );

    // Regression assertions (#1320 / TH6-NEW-01): this was a print-only
    // diagnostic that passed vacuously. Pin the invariants that must hold for
    // any valid FNV parse — a populated interior that resolved by key has
    // REFRs, and at least one must link to a base mesh (the base-form join is
    // what the dump's mesh column exercises). Exact counts / rotation-profile
    // bands are intentionally left as printed diagnostics: they need a
    // measured baseline, not a guessed literal.
    assert!(
        !rows.is_empty(),
        "GSProspectorSaloonInterior resolved but produced zero REFRs — parse regression"
    );
    assert!(
        rows.iter().any(|(_, mesh)| mesh != "<no base>"),
        "no REFR in GSProspectorSaloonInterior resolved to a base mesh — \
         base-form linkage regression"
    );
}

/// PLDT (package location) real-data regression. FNV package editor IDs
/// conventionally embed their own radius in the name (e.g.
/// `DefaultSandboxEditorLocation512`) — a strong independent check of the
/// PLDT byte layout against
/// <https://tes5edit.github.io/fopdoc/FalloutNV/Records/PACK.html>, spot-
/// verified 2026-07-14 against every `DefaultSandbox*` package in vanilla
/// FalloutNV.esm (all matched; e.g. `…LinkedMarker1024` → radius 1024,
/// `…CurrentLocation256` → radius 256).
#[test]
#[ignore]
fn parse_rate_fnv_pack_pldt_location() {
    let Some(data) = data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    ) else {
        eprintln!("[FNV] skipping: BYROREDUX_FNV_DATA unset and fallback path missing");
        return;
    };
    let bytes = std::fs::read(data.join("FalloutNV.esm")).expect("read FalloutNV.esm");
    let index = parse_esm(&bytes).expect("parse FalloutNV.esm");

    // Observed 2026-07-14: 3804 / 4163 packages carry a PLDT. Floor sits a
    // few percent below to absorb DLC drift without masking a dispatch
    // regression that would silently drop the sub-record.
    let with_location = index
        .packages
        .values()
        .filter(|p| p.location.is_some())
        .count();
    assert!(
        with_location > 3500,
        "PACK.location populated on {with_location} / {} packages, expected > 3500",
        index.packages.len(),
    );

    // Location Type is a documented 0..=7 enum; any other value means the
    // PLDT cursor has drifted (e.g. reading a different sub-record's bytes
    // as PLDT).
    for p in index.packages.values() {
        if let Some(loc) = &p.location {
            assert!(
                loc.location_type <= 7,
                "{} location_type={} out of the documented 0..=7 range",
                p.editor_id,
                loc.location_type,
            );
            assert!(
                loc.radius >= 0,
                "{} radius={} is negative",
                p.editor_id,
                loc.radius,
            );
        }
    }

    // Name-encoded radius cross-check: most `DefaultSandbox*Location<N>*`
    // packages' PLDT radius equals the `<N>` in their own editor ID — a
    // vanilla-authored convention, not a hard guarantee (e.g.
    // `DefaultSandboxEditorLocation500` genuinely carries radius 1000 in
    // FalloutNV.esm, a CK-authoring inconsistency, not a decode bug — every
    // other sampled package matches exactly). Assert a majority match
    // rather than 100%, so this stays a decode-corruption tripwire without
    // being fragile to individual authoring quirks.
    let name_radius_re = |name: &str| -> Option<i32> {
        let digits: String = name
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    };
    let mut checked = 0;
    let mut mismatched = Vec::new();
    for p in index.packages.values() {
        if !p.editor_id.starts_with("DefaultSandbox") {
            continue;
        }
        let Some(loc) = &p.location else { continue };
        let Some(expected) = name_radius_re(&p.editor_id) else {
            continue; // e.g. "DefaultSandboxEditorLocation" with no digits
        };
        checked += 1;
        if loc.radius != expected {
            mismatched.push(format!(
                "{} (decoded {}, name says {})",
                p.editor_id, loc.radius, expected
            ));
        }
    }
    assert!(
        checked > 20,
        "expected to cross-check > 20 DefaultSandbox* packages, only checked {checked}",
    );
    assert!(
        mismatched.len() * 10 <= checked, // allow up to 10% authoring-quirk noise
        "{}/{checked} DefaultSandbox* packages' PLDT radius disagrees with their \
         name-encoded radius (>10% — looks like a decode regression, not authoring \
         noise): {mismatched:?}",
        mismatched.len(),
    );
}

/// #3109 / WATR-ARB-06 — the signal that caught #3104: a decoder folding a
/// *constant* into a per-water control is invisible to synthetic fixtures
/// (which pin one record) but obvious across a shipped population.
/// `normal_magnitude` read `DNAM[92]`, the Displacement Starting Size, which is
/// `0.05` on 34/34 vanilla Skyrim records — so every Skyrim water type shared
/// one normal tilt while its authored amplitudes spanned 0.0725..1.0.
///
/// The invariant: a per-water scalar is either **untouched** (every record at
/// the struct default, meaning nothing was decoded) or **genuinely per-record**
/// (two or more distinct values). Uniform at a *non-default* value is the
/// failure signature — it means an offset was decoded and it was the wrong one.
///
/// `#[ignore]`-gated like its siblings: needs installed game data.
#[test]
#[ignore]
fn installed_masters_per_water_scalars_are_not_decoded_constants() {
    // Same master list as `installed_masters_water_fields_are_finite_and_ordered`.
    let masters = [
        (
            "BYROREDUX_OBL_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
            "Oblivion.esm",
            "Oblivion",
        ),
        (
            "BYROREDUX_FNV_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            "FalloutNV.esm",
            "FNV",
        ),
        (
            "BYROREDUX_FO3_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
            "Fallout3.esm",
            "FO3",
        ),
        (
            "BYROREDUX_SKYRIMSE_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
            "Skyrim.esm",
            "Skyrim SE",
        ),
        (
            "BYROREDUX_FO4_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
            "Fallout4.esm",
            "FO4",
        ),
        (
            "BYROREDUX_FO76_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout76/Data",
            "SeventySix.esm",
            "FO76",
        ),
        (
            "BYROREDUX_STARFIELD_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data",
            "Starfield.esm",
            "Starfield",
        ),
    ];

    // Struct defaults — "nothing was decoded into this slot".
    const NORMAL_MAGNITUDE_SENTINEL: f32 = 1.0;
    const NOISE_AMPLITUDE_SENTINEL: f32 = 0.0;
    // Below this, a population is too small for uniformity to mean anything.
    const MIN_POPULATION: usize = 8;

    let distinct = |values: &[f32]| -> usize {
        let mut keys: Vec<i64> = values.iter().map(|v| (*v as f64 * 1.0e6) as i64).collect();
        keys.sort_unstable();
        keys.dedup();
        keys.len()
    };

    let mut checked_games = 0;
    for (env_var, fallback, filename, label) in masters {
        let Some(data) = data_dir(env_var, fallback) else {
            eprintln!("[{label} WATR] skipping: game data unavailable");
            continue;
        };
        let bytes = std::fs::read(data.join(filename)).expect("read installed master");
        let index = parse_esm(&bytes).expect("parse installed master");
        if index.waters.len() < MIN_POPULATION {
            eprintln!(
                "[{label} WATR] skipping: {} records is below the meaningfulness floor",
                index.waters.len()
            );
            continue;
        }
        checked_games += 1;

        for (field, sentinel, values) in [
            (
                "normal_magnitude",
                NORMAL_MAGNITUDE_SENTINEL,
                index
                    .waters
                    .values()
                    .map(|w| w.params.normal_magnitude)
                    .collect::<Vec<_>>(),
            ),
            (
                "noise_amplitude_scales[0]",
                NOISE_AMPLITUDE_SENTINEL,
                index
                    .waters
                    .values()
                    .map(|w| w.params.noise_amplitude_scales[0])
                    .collect::<Vec<_>>(),
            ),
        ] {
            let n = distinct(&values);
            if n > 1 {
                continue; // genuinely per-record
            }
            let only = values[0];
            assert!(
                (only - sentinel).abs() < 1.0e-6,
                "[{label}] {field} is uniform at {only} across all {} WATR records, and that \
                 is not the {sentinel} struct default — a decoder folded a CONSTANT into a \
                 per-water control. This is the #3104 signature: `normal_magnitude` read the \
                 Displacement Starting Size (0.05 on 34/34 Skyrim records) and collapsed every \
                 water type onto one normal tilt. Either the offset is wrong, or the field is \
                 not per-record and should stay at its sentinel (#3109).",
                values.len(),
            );
        }
    }

    assert!(
        checked_games > 0,
        "no installed master was available — this guard proved nothing. Install a supported \
         game or point BYROREDUX_*_DATA at one."
    );
    eprintln!("[WATR] per-water scalar guard checked {checked_games} game(s)");
}

/// #3107 / WATR-ARB-04 — the 196/184-byte `DNAM` is the MAJORITY FO3/FNV
/// carrier (42/53 FO3, 70/78 FNV), and it used to run `decode_dnam_pre_fo4`
/// alone, which reads bytes 0..52 and returns. Everything past the prefix — the
/// rain and displacement simulators, the three noise layers, the fog amounts,
/// the underwater fog pair, the noise UV scales and amplitudes and the specular
/// tail — was left at canonical defaults on 79% of FO3 and 90% of FNV water.
///
/// Pinned against shipped bytes rather than the decoder's own output: the
/// assertion is that the tail actually *lands* on the majority of the real
/// population, so a regression that reverts the dispatch arm fails here even
/// though every synthetic fixture still passes.
///
/// `#[ignore]`-gated like its siblings: needs installed game data.
#[test]
#[ignore]
fn installed_fallout_masters_decode_the_dnam_visual_tail() {
    let masters = [
        (
            "BYROREDUX_FNV_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            "FalloutNV.esm",
            "FNV",
        ),
        (
            "BYROREDUX_FO3_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
            "Fallout3.esm",
            "FO3",
        ),
    ];

    let mut checked_games = 0;
    for (env_var, fallback, filename, label) in masters {
        let Some(data) = data_dir(env_var, fallback) else {
            eprintln!("[{label} WATR] skipping: game data unavailable");
            continue;
        };
        let bytes = std::fs::read(data.join(filename)).expect("read installed master");
        let index = parse_esm(&bytes).expect("parse installed master");
        assert!(
            !index.waters.is_empty(),
            "{label} must contain WATR records"
        );
        checked_games += 1;

        let total = index.waters.len();
        // The displacement simulator lives at 76..92, well past the 52-byte
        // prefix, and vanilla authors it on essentially every record (the GECK
        // default tuple `0.4 0.6 0.985 10.0 0.05`). A record still sitting on
        // the all-zero default never had its tail read.
        let with_tail = index
            .waters
            .values()
            .filter(|w| w.params.displacement != [0.0; 3])
            .count();
        // Underwater fog far at 148 is the deepest field the tail reads.
        let with_deep_tail = index
            .waters
            .values()
            .filter(|w| w.params.underwater_fog_far > 1.0)
            .count();

        eprintln!(
            "[{label} WATR] {total} records: {with_tail} with a decoded displacement block, \
             {with_deep_tail} reaching the underwater fog pair at 144/148"
        );
        assert!(
            with_tail * 2 > total,
            "[{label}] only {with_tail} of {total} WATR records decoded the displacement \
             simulator at 76..92. The majority DNAM carrier is stopping at the 52-byte \
             prefix again (#3107)."
        );
        assert!(
            with_deep_tail * 2 > total,
            "[{label}] only {with_deep_tail} of {total} WATR records reached the underwater \
             fog pair at 144/148 — the tail is being truncated short of its end (#3107)."
        );
    }

    assert!(
        checked_games > 0,
        "neither Fallout master was available — this guard proved nothing."
    );
}

/// #3205 / FO3-2026-08-20-D3-02 — the long-`DATA` carrier's 0..16 head is
/// wind and wave controls, not the opaque prefix its doc-comment used to
/// claim.
///
/// `decode_data_fo3nv` skipped offsets 0..16 "because xEdit calls them
/// opaque" and substituted `wind_direction`/`wind_speed` from noise layer 1
/// and `wave_amplitude`/`wave_frequency` from the displacement simulator's
/// force/velocity at 76/80 — so the two FO3 carriers disagreed about where
/// four canonical fields live. The head is now read exactly as
/// `decode_dnam_pre_fo4` reads it.
///
/// Pinned against **shipped bytes**, not the decoder's own output, and
/// deliberately over the whole population rather than one fixture: the
/// substituted values were plausible, which is why this survived.
///
/// The census this reproduces (all 53 FO3 / 78 FNV `WATR` visual payloads):
/// offset 4 is `90.0` on 53/53, and offsets 0/8/12 hold the GECK water
/// default tuple `0.1 / 0.5 / 1.0` on 46 of 53. The displacement simulator's
/// `0.4 / 0.6` — what the old arm reported for amp/freq — appears on no
/// record at all once the head is read.
#[test]
#[ignore]
fn installed_fallout_masters_read_the_watr_head_not_the_simulator_tail() {
    // The authored `(wave_amplitude, wave_frequency, wind_speed)` tuples in
    // vanilla FO3/FNV. Every record is one of these; none is the displacement
    // simulator's force/velocity pair.
    const AUTHORED: &[(f32, f32, f32)] = &[
        (0.5, 1.0, 0.1),  // GECK default water
        (0.2, 0.25, 3.0), // the "open water" tuple (DefaultWater, Potomac, …)
        (0.5, 0.5, 2.0),  // one FO3 / two FNV records
    ];
    const DISPLACEMENT_FORCE_VELOCITY: (f32, f32) = (0.4, 0.6);

    let masters = [
        (
            "BYROREDUX_FO3_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
            "Fallout3.esm",
            "FO3",
        ),
        (
            "BYROREDUX_FNV_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            "FalloutNV.esm",
            "FNV",
        ),
    ];

    let mut checked_games = 0;
    for (env_var, fallback, filename, label) in masters {
        let Some(data) = data_dir(env_var, fallback) else {
            eprintln!("[{label} WATR head] skipping: game data unavailable");
            continue;
        };
        let bytes = std::fs::read(data.join(filename)).expect("read installed master");
        let index = parse_esm(&bytes).expect("parse installed master");
        checked_games += 1;

        let long_data = index
            .waters
            .values()
            .filter(|w| w.raw_data.len() >= 100)
            .count();
        assert!(
            long_data > 0,
            "[{label}] no long-`DATA` WATR records — this guard would prove nothing"
        );

        for water in index.waters.values() {
            let p = &water.params;
            // Offset 4, degrees on the wire. Unanimous in vanilla; the old arm
            // sourced this from `noise_wind_directions[0]` on the long-`DATA`
            // records, which is a different field entirely.
            assert!(
                (p.wind_direction.to_degrees() - 90.0).abs() < 1e-3,
                "[{label}] {} wind_direction {:.3}° — offset 4 is 90.0 on every \
                 vanilla record; a non-90 reading means the head is being \
                 sourced from a noise layer again (#3205)",
                water.editor_id,
                p.wind_direction.to_degrees(),
            );
            assert!(
                (p.wave_amplitude - DISPLACEMENT_FORCE_VELOCITY.0).abs() > 1e-6
                    || (p.wave_frequency - DISPLACEMENT_FORCE_VELOCITY.1).abs() > 1e-6,
                "[{label}] {} reports the displacement simulator's force/velocity \
                 ({:.3}/{:.3}) as its wave amplitude/frequency — the long-`DATA` \
                 arm is reading 76/80 instead of the authored head at 8/12 (#3205)",
                water.editor_id,
                p.wave_amplitude,
                p.wave_frequency,
            );
            assert!(
                AUTHORED.iter().any(|&(amp, freq, speed)| {
                    (p.wave_amplitude - amp).abs() < 1e-6
                        && (p.wave_frequency - freq).abs() < 1e-6
                        && (p.wind_speed - speed).abs() < 1e-6
                }),
                "[{label}] {} head tuple ({:.3}, {:.3}, {:.3}) is none of the \
                 authored vanilla tuples {AUTHORED:?}",
                water.editor_id,
                p.wave_amplitude,
                p.wave_frequency,
                p.wind_speed,
            );
        }

        // The named pin the finding asks for: a long-`DATA` record reporting
        // the GECK default tuple. `PuddleWaterSmall01` (0x0002_4A50) ships the
        // 186-byte `DATA` on both masters.
        let puddle = index
            .waters
            .get(&0x0002_4A50)
            .expect("PuddleWaterSmall01 (0x24A50)");
        assert!(
            puddle.raw_data.len() >= 100 && puddle.raw_dnam.is_empty(),
            "[{label}] PuddleWaterSmall01 must be the long-`DATA` carrier, got \
             data={} dnam={}",
            puddle.raw_data.len(),
            puddle.raw_dnam.len(),
        );
        assert!((puddle.params.wave_amplitude - 0.5).abs() < 1e-6);
        assert!((puddle.params.wave_frequency - 1.0).abs() < 1e-6);
        assert!((puddle.params.wind_speed - 0.1).abs() < 1e-6);

        // Cross-carrier agreement, which is what the divergence actually cost:
        // `DefaultWater` (0x18) ships the 186-byte `DATA` on FO3 and the
        // 196-byte `DNAM` on FNV, and both must decode the same head.
        let default_water = index.waters.get(&0x0000_0018).expect("DefaultWater (0x18)");
        assert!((default_water.params.wave_amplitude - 0.2).abs() < 1e-6);
        assert!((default_water.params.wave_frequency - 0.25).abs() < 1e-6);
        assert!((default_water.params.wind_speed - 3.0).abs() < 1e-6);

        eprintln!(
            "[{label} WATR head] {} records ({long_data} long-`DATA`): every one \
             reports a 90° wind direction and an authored wave tuple",
            index.waters.len(),
        );
    }

    assert!(
        checked_games > 0,
        "neither Fallout master was available — this guard proved nothing."
    );
}

/// #3285 — FNV-sourced coverage for the shared `expand_leveled_inner`.
///
/// `#3217` narrowed `multi_pick` from `flags & (0x02 | 0x04)` to
/// `flags & 0x04`, so `LVLF` bit 1 ("calculate for each item in count") no
/// longer expands every eligible tier. Its justification and both of its tests
/// are entirely Skyrim-sourced ("over-equipped 1,491 vanilla Skyrim NPCs"), but
/// `expand_leveled_inner` is game-agnostic and FO3/FNV `LVLI` uses the same
/// `LVLD`/`LVLF`/`LVLO` layout — so the change silently altered FNV output too,
/// with no FNV fixture anywhere to notice.
///
/// This is a **characterisation** test, not a correctness claim. The true
/// FO3/FNV mechanic is RNG-driven (roll N times against a caller-supplied
/// count this codebase does not thread through), so neither the old nor the new
/// deterministic approximation models it exactly, and this audit found no
/// GECK-level documentation settling which is closer. Pinning current behaviour
/// on real records is what is actually available: it makes the blast radius
/// visible and makes any future change to this shared function announce its
/// effect on FNV instead of landing unobserved.
#[test]
#[ignore]
fn fnv_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master() {
    let Some(data) = data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    ) else {
        eprintln!("[FNV/LVLI] skipping: game data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("FalloutNV.esm")).expect("read FalloutNV.esm");
    let index = parse_esm(&bytes).expect("parse FalloutNV.esm");

    // The affected population: bit 0x02 set, bit 0x04 clear, entries spanning
    // more than one distinct level. Exactly the records whose selection output
    // #3217 changed from "every eligible tier" to "the single highest".
    let affected: Vec<&byroredux_plugin::esm::records::LeveledList> = index
        .leveled_items
        .values()
        .filter(|l| l.flags & 0x02 != 0 && l.flags & 0x04 == 0)
        .filter(|l| {
            let mut levels: Vec<u16> = l.entries.iter().map(|e| e.level).collect();
            levels.sort_unstable();
            levels.dedup();
            levels.len() > 1
        })
        .collect();

    eprintln!(
        "[FNV/LVLI] {} total LVLI, {} in the #3217-affected set",
        index.leveled_items.len(),
        affected.len()
    );

    assert!(
        index.leveled_items.len() >= 2_700,
        "FNV LVLI count collapsed to {} — the parser, not this fix, regressed",
        index.leveled_items.len()
    );
    assert!(
        affected.len() >= 200,
        "the #3217-affected FNV set shrank to {} records; if that is intended, \
         re-derive the blast radius before re-pinning",
        affected.len()
    );

    // A named representative: a Legion armour bundle whose entries sit at two
    // levels. Under the pre-#3217 code an actor at level >= 9 received both
    // tiers at once; under current code it receives only the level-9 tier.
    let recruit_prime = index
        .leveled_items
        .values()
        .find(|l| l.editor_id == "LeveledLegionArmorRecruitPrime")
        .expect("LeveledLegionArmorRecruitPrime LVLI present in FalloutNV.esm");
    assert_eq!(
        recruit_prime.flags & 0x04,
        0,
        "the representative must stay a bit-0x02-only record for this pin to mean anything"
    );

    let mut out = Vec::new();
    byroredux_plugin::equip::expand_leveled_form_id(recruit_prime.form_id, 20, &index, &mut out);
    eprintln!(
        "[FNV/LVLI] {} (flags={:#04x}) at level 20 -> {:?}",
        recruit_prime.editor_id, recruit_prime.flags, out
    );
    assert_eq!(
        out.len(),
        1,
        "a bit-0x02-only tier ladder must resolve to exactly one item on FNV, \
         same as on Skyrim (#3217); got {out:?}"
    );
}

/// #3365 — the Skyrim counterpart of
/// `fnv_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master`.
///
/// #3217 narrowed `multi_pick` from `flags & (0x02 | 0x04)` to `flags & 0x04`,
/// and its entire justification is Skyrim-sourced ("1,491 vanilla Skyrim NPCs
/// over-equipped"). Yet all three of its own tests are synthetic fixtures, and
/// the only real-data pin that existed was the FNV one above — added by #3285
/// as a side-effect characterisation. FNV is not a proxy: it ships 2,700+ LVLI
/// with a different flag mix, against Skyrim's 3,075.
///
/// Measured on the shipped `Skyrim.esm`:
/// ```text
/// LVLI total = 3075
/// flags histogram {0:553, 1:62, 2:239, 3:1855, 4:280, 8:5, 9:1, 10:39, 11:41}
/// #3217-affected (0x02 set, 0x04 clear, multi-level) = 935
/// Use-All (0x04) = 280
/// ```
///
/// Like the FNV pin this is a **characterisation** test: it asserts the
/// population is still there and that the representative still single-picks, so
/// a future change to the shared `expand_leveled_inner` announces its effect on
/// Skyrim instead of landing unobserved.
#[test]
#[ignore]
fn skyrim_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master() {
    let Some(data) = data_dir(
        "BYROREDUX_SKYRIMSE_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
    ) else {
        eprintln!("[Skyrim/LVLI] skipping: game data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).expect("read Skyrim.esm");
    let index = parse_esm(&bytes).expect("parse Skyrim.esm");

    let affected: Vec<&byroredux_plugin::esm::records::LeveledList> = index
        .leveled_items
        .values()
        .filter(|l| l.flags & 0x02 != 0 && l.flags & 0x04 == 0)
        .filter(|l| {
            let mut levels: Vec<u16> = l.entries.iter().map(|e| e.level).collect();
            levels.sort_unstable();
            levels.dedup();
            levels.len() > 1
        })
        .collect();
    let use_all = index
        .leveled_items
        .values()
        .filter(|l| l.flags & 0x04 != 0)
        .count();

    eprintln!(
        "[Skyrim/LVLI] {} total LVLI, {} in the #3217-affected set, {} Use-All",
        index.leveled_items.len(),
        affected.len(),
        use_all,
    );

    assert!(
        index.leveled_items.len() >= 3_000,
        "Skyrim LVLI count collapsed to {} — the parser, not this fix, regressed",
        index.leveled_items.len()
    );
    assert!(
        affected.len() >= 900,
        "the #3217-affected Skyrim set shrank to {} records (measured 935); if \
         that is intended, re-derive the blast radius before re-pinning",
        affected.len()
    );
    assert!(
        use_all >= 250,
        "the Use-All (0x04) set shrank to {use_all} (measured 280) — multi_pick \
         would then be unreachable and this pin vacuous"
    );

    // The named representative from #3217's own fixture doc (`equip.rs`'s
    // `expand_leveled_nested_tier_ladders_do_not_combinatorially_explode`):
    // dunIronbindBeemJa, whose outfit holds two `flags = 0x03` enchant ladders.
    let npc = index
        .npcs
        .values()
        .find(|n| n.editor_id.eq_ignore_ascii_case("dunIronbindBeemJa"))
        .expect("dunIronbindBeemJa NPC_ present in Skyrim.esm");
    let outfit_id = npc
        .default_outfit
        .expect("dunIronbindBeemJa carries a DOFT default outfit");
    let outfit = index
        .outfits
        .get(&outfit_id)
        .expect("the DOFT FormID resolves to an OTFT");

    // Two of the five slots are 0x03 tier ladders — the shape that multiplied
    // out pre-#3217 (18 tiers x 5 variants). Assert they are still ladders, so
    // the pin below cannot pass by the records having become trivial.
    let ladders: Vec<&byroredux_plugin::esm::records::LeveledList> = outfit
        .items
        .iter()
        .filter_map(|item| index.leveled_items.get(item))
        .collect();
    assert!(
        ladders.len() >= 2,
        "expected dunIronbindBeemJa's outfit to still contain leveled slots; got {}",
        ladders.len()
    );
    for ladder in &ladders {
        assert_eq!(
            ladder.flags & 0x04,
            0,
            "{} must stay a bit-0x02-only record for this pin to mean anything",
            ladder.editor_id
        );
        assert!(
            ladder.entries.len() >= 5,
            "{} collapsed to {} entries — a one-entry list single-picks trivially",
            ladder.editor_id,
            ladder.entries.len()
        );
    }

    // Every leveled slot must yield exactly one item, at every level: the whole
    // outfit expands to one item per OTFT slot, never a product of the ladders.
    for level in [1i16, 10, 20, 50] {
        for ladder in &ladders {
            let mut one = Vec::new();
            byroredux_plugin::equip::expand_leveled_form_id(
                ladder.form_id,
                level,
                &index,
                &mut one,
            );
            assert_eq!(
                one.len(),
                1,
                "{} at level {level} must single-pick (#3217); got {one:08X?}",
                ladder.editor_id
            );
        }

        let mut out = Vec::new();
        for item in &outfit.items {
            byroredux_plugin::equip::expand_leveled_form_id(*item, level, &index, &mut out);
        }
        eprintln!(
            "[Skyrim/LVLI] dunIronbindBeemJa outfit at level {level} -> {} items",
            out.len()
        );
        assert_eq!(
            out.len(),
            outfit.items.len(),
            "the outfit must expand to exactly one item per OTFT slot at level \
             {level}; got {out:08X?}"
        );
    }
}
