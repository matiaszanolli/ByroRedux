//! Real-data smoke test against vanilla `materialsbeta.cdb` from
//! `Starfield - Materials.ba2`. `#[ignore]`-gated by Starfield install.
//!
//! Run with:
//! ```
//! BYROREDUX_STARFIELD_DATA="/path/to/Starfield/Data" \
//!     cargo test -p byroredux-sfmaterial --test real_cdb -- --ignored --nocapture
//! ```
//!
//! The vanilla base-game `materialsbeta.cdb` contains ~1.44M instances. This
//! test deliberately uses the nested streaming validator, so only the schema
//! and string table stay live. Do not change it back to
//! `ComponentDatabaseFile::parse`: materialising the full generic value tree
//! has measured at ~9.19 GB peak RSS (#4274 / SF-D3-2026-09-11-03).

use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::{ComponentDatabaseFile, ParseLimits};
use std::path::PathBuf;

/// #3850 — the strict lane for real-data tests.
///
/// `BYROREDUX_REQUIRE_GAME_DATA=1` turns an absent corpus into a hard
/// failure instead of a silent libtest `ok`. Without it the `--ignored`
/// lane — the only lane these `#[ignore]`d tests ever execute in —
/// records a pass for a test that never touched a byte of game data, so
/// a green run is not evidence of anything. This is the Rust-side
/// counterpart of the shell gates' "explicit SKIP with exit code 77,
/// never a pass" rule (`docs/smoke-tests/README.md`, #3003).
#[track_caller]
fn require_game_data(env_var: &str, tried: &std::path::Path) {
    if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
        panic!(
            "BYROREDUX_REQUIRE_GAME_DATA is set, but no game data was found: \
             {env_var} is unset (or names a non-directory) and the default \
             {tried:?} is not a directory"
        );
    }
}

fn data_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var("BYROREDUX_STARFIELD_DATA")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let p = PathBuf::from(&v);
        if p.exists() {
            return Some(p);
        }
        // #3850: an explicitly-set override is BINDING — never silently
        // substitute the hardcoded dev-machine path.
        panic!("BYROREDUX_STARFIELD_DATA points to {v:?}, which is not a directory");
    }
    let p = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Starfield/Data");
    if p.exists() {
        return Some(p);
    }
    require_game_data("BYROREDUX_STARFIELD_DATA", &p);
    None
}

#[test]
#[ignore = "needs Starfield game data on disk"]
fn parse_vanilla_materialsbeta_cdb() {
    let Some(data) = data_dir() else {
        return;
    };
    let ba2 = Ba2Archive::open(data.join("Starfield - Materials.ba2")).expect("open materials BA2");
    let bytes = ba2
        .extract("materials\\materialsbeta.cdb")
        .expect("extract cdb");
    eprintln!("[sfmaterial] extracted {} bytes", bytes.len());
    eprintln!(
        "[sfmaterial] validating with ParseLimits::unlimited() — nested \
         values are consumed without materialising the 1.44M-instance tree"
    );

    let info =
        ComponentDatabaseFile::validate_instances_with_limits(&bytes, ParseLimits::unlimited())
            .expect("validate CDB");
    eprintln!(
        "[sfmaterial] streamed: {} classes / {} top-level values",
        info.class_count, info.value_count
    );

    // #4665 (PAR-D4-2026-09-21-03) — the counts are pinned EXACTLY (the
    // audit's measured 97 classes / 1,438,780 values), not floored: the
    // CDB is versioned content shipped with the game, so a change is a
    // format event worth failing on, not noise to absorb.
    assert_eq!(info.class_count, 97, "vanilla CDB class count");
    assert_eq!(info.value_count, 1_438_780, "vanilla CDB top-level values");
}
