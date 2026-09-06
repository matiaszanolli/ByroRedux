//! Real-data smoke test against vanilla `materialsbeta.cdb` from
//! `Starfield - Materials.ba2`. `#[ignore]`-gated by Starfield install.
//!
//! Run with:
//! ```
//! BYROREDUX_STARFIELD_DATA="/path/to/Starfield/Data" \
//!     cargo test -p byroredux-sfmaterial --test real_cdb -- --ignored --nocapture
//! ```

use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::ComponentDatabaseFile;
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

    let cdb = ComponentDatabaseFile::parse(&bytes).expect("parse cdb");
    eprintln!(
        "[sfmaterial] parsed: {} classes / {} instances",
        cdb.classes.len(),
        cdb.instances.len()
    );

    // Floor asserts — these should hold for any non-empty CDB.
    assert!(!cdb.classes.is_empty(), "vanilla CDB must declare classes");
    assert!(
        !cdb.instances.is_empty(),
        "vanilla CDB must contain instances"
    );

    // Spot-check that the first few class names look sensible (printable
    // ASCII) — a misaligned reader would print mojibake.
    for c in cdb.classes.iter().take(5) {
        let printable = c.name.chars().all(|ch| ch.is_ascii_graphic());
        assert!(printable, "class name not ASCII-printable: {:?}", c.name);
        eprintln!("  class[0..5] {} -> {}", c.type_id, c.name);
    }
}
