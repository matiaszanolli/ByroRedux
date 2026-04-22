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
        eprintln!(
            "{env_var} points to {v:?} which is not a directory; falling back to default"
        );
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
    let index = parse_esm(&bytes).expect("parse FalloutNV.esm");

    eprintln!(
        "[FNV] total={} | items={} containers={} LVLI={} LVLN={} NPCs={} \
         races={} classes={} factions={} globals={} game_settings={} \
         packages={} quests={} dialogues={} messages={} perks={} \
         spells={} magic_effects={} activators={} terminals={}",
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
    assert!(index.factions.len() > 500, "factions={}", index.factions.len());
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
    assert!(
        index.messages.len() > 1000,
        "MESG={}",
        index.messages.len(),
    );
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
        .filter(|w| w.cloud_textures[0].as_deref().filter(|s| !s.is_empty()).is_some())
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
         scripts={}",
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
    );

    // Primary baseline from AUDIT_FO3_2026-04-19.md — 18,007 records
    // observed on the GOTY master; FO3_TOTAL_FLOOR sits slightly below
    // to absorb future patch drift without masking regressions.
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
        .filter(|w| w.cloud_textures[0].as_deref().filter(|s| !s.is_empty()).is_some())
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
        "[OBL] total={} | weathers={} climates={}",
        index.total(),
        index.weathers.len(),
        index.climates.len(),
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
        .filter(|w| w.cloud_textures[0].as_deref().filter(|s| !s.is_empty()).is_some())
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
}
