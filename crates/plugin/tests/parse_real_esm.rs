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

/// FNV: 13,684 structured records baseline per M24 Phase 1 (ROADMAP).
/// Observed 2026-04 patch revision: exactly 13,684 across all
/// `EsmIndex::total()` categories.
const FNV_TOTAL_FLOOR: usize = 13_684;

/// FO3: 18,007 records baseline per the FO3 audit (AUDIT_FO3_2026-04-19).
/// Margin kept below observed so DLC patches stay green.
const FO3_TOTAL_FLOOR: usize = 18_000;

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
         races={} classes={} factions={} globals={} game_settings={}",
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
}
