//! #3637 — last-listed archive wins on a mesh/texture path collision.
//!
//! `count_shadowed_entries` (the shadow-count diagnostic) is exercised
//! directly against two synthetic in-memory-sized archives; the actual
//! extract-precedence fix needs real overlapping BA2 content to prove
//! end-to-end (a synthetic single-entry archive can't reproduce "the same
//! path, different bytes, in two real archives" without reimplementing the
//! BA2 writer), so that half is `#[ignore]`-gated against the installed FO4
//! `Data/` — same convention as `facegen_texture_fallback.rs`. Run with:
//!
//!   cargo test -p byroredux --bin byroredux archive_precedence -- --ignored

use super::super::*;

fn fo4_data_dir() -> std::path::PathBuf {
    std::env::var_os("BYROREDUX_FO4_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data")
        })
}

/// The exact real-world shape #3637 measured: `Fallout4 - MeshesExtra.ba2`
/// (base game) and a DLC's `- Main.ba2` both carry the same precombine
/// `_oc.nif` path with different bytes — DLC precombine re-bakes silently
/// shadowed by their base-game namesake because the natural invocation
/// lists `--bsa` in `--master` order (base, then DLC).
///
/// Finds any such colliding path at run time (rather than hardcoding a
/// specific hash) — the assertion is about precedence, not about which
/// particular object collides, and this way the test survives if the
/// installed game files ever change.
#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn later_listed_bsa_wins_a_mesh_path_collision() {
    let data = fo4_data_dir();
    let base_path = data.join("Fallout4 - MeshesExtra.ba2");
    let dlc_path = data.join("DLCCoast - Main.ba2");
    if !base_path.is_file() || !dlc_path.is_file() {
        return;
    }

    let base = byroredux_bsa::Ba2Archive::open(&base_path).expect("open base MeshesExtra.ba2");
    let dlc = byroredux_bsa::Ba2Archive::open(&dlc_path).expect("open DLCCoast - Main.ba2");

    let dlc_files = dlc.list_files();
    let Some(&collision) = dlc_files.iter().find(|&&f| {
        f.ends_with("_oc.nif") && base.contains(f) && base.extract(f).ok() != dlc.extract(f).ok()
    }) else {
        // The corpus this was measured against (2026-08-30) had 1,681 such
        // collisions; if a game update ever removes every last one, skip
        // rather than fail — this test proves precedence, not corpus shape.
        return;
    };

    // Natural invocation order: base game first, DLC after — matching
    // every documented FO4 command line in this repo (`--master` order).
    let provider = build_texture_provider(&[
        "--bsa".to_owned(),
        base_path.to_string_lossy().into_owned(),
        "--bsa".to_owned(),
        dlc_path.to_string_lossy().into_owned(),
    ]);

    let resolved = provider
        .extract_mesh(collision)
        .expect("collision path must resolve from the combined archive chain");
    assert_eq!(
        resolved,
        dlc.extract(collision).unwrap(),
        "the later-listed archive (the DLC) must win — the base game's byte-\
         identical-in-name precombine must not silently shadow the DLC re-bake"
    );
}

/// The shadow-count diagnostic itself, isolated from the extract-precedence
/// behaviour above: opening a second archive that re-lists paths the first
/// already carries must report a nonzero shadow count.
#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn count_shadowed_entries_reports_the_collision() {
    let data = fo4_data_dir();
    let base_path = data.join("Fallout4 - MeshesExtra.ba2");
    let dlc_path = data.join("DLCCoast - Main.ba2");
    if !base_path.is_file() || !dlc_path.is_file() {
        return;
    }

    let base = Archive::open(&base_path.to_string_lossy()).expect("open base MeshesExtra.ba2");
    let dlc = Archive::open(&dlc_path.to_string_lossy()).expect("open DLCCoast - Main.ba2");

    let shadowed = count_shadowed_entries(&dlc, std::slice::from_ref(&base));
    assert!(
        shadowed > 0,
        "DLCCoast - Main.ba2 shares hundreds of precombine paths with the \
         base MeshesExtra.ba2 (#3637's own measurement: 633 for this pair) \
         — the diagnostic must see them"
    );
}
