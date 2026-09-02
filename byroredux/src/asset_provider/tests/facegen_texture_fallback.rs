//! #3555 — end-to-end coverage for the FaceGen-tool-export-path texture
//! fallback against the real Oblivion archives. The pure detection/basename
//! logic is unit-tested in `material_path.rs`
//! (`is_facegen_tool_path_*`); this proves the whole
//! `TextureProvider::extract`/`has_texture` path actually recovers the
//! real file, not just the predicate.
//!
//! `#[ignore]`-gated to match the convention every other real-game-data
//! test in the tree follows (`animation.rs`'s `skyrim_cart_idle_…`,
//! `crates/bsa/src/archive/tests.rs`). Run with:
//!
//!   cargo test -p byroredux --bin byroredux facegen_tool_path -- --ignored

use super::super::*;

fn oblivion_data_dir() -> std::path::PathBuf {
    std::env::var_os("BYROREDUX_OBLIVION_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data")
        })
}

/// The exact defect: `earshuman.nif` (and its `highelf`/`woodelf` siblings)
/// author `facegen\ears\human\EarsHuman.dds`, which does not exist anywhere
/// in the shipped archives — the real file is
/// `textures\characters\imperial\earshuman.dds`. Before the #3555 fix this
/// missed outright; the fallback must now find it by basename.
#[test]
#[ignore = "needs Oblivion game data on disk"]
fn facegen_tool_path_resolves_via_basename_fallback() {
    let textures_bsa = oblivion_data_dir().join("Oblivion - Textures - Compressed.bsa");
    if !textures_bsa.is_file() {
        return;
    }
    let provider = build_texture_provider(&[
        "--textures-bsa".to_owned(),
        textures_bsa.to_string_lossy().into_owned(),
    ]);

    assert!(
        provider.has_texture("facegen\\ears\\human\\EarsHuman.dds"),
        "the base-color diffuse must resolve through the basename fallback"
    );
    assert!(
        provider.extract("facegen\\ears\\human\\EarsHuman.dds").is_some(),
        "extract must return the real archive bytes, not just report presence"
    );

    // The load-time `_n.dds` normal-map sibling convention (#1303) derives
    // its candidate from the same authored diffuse path, so it must recover
    // too — this is the second miss the issue's evidence table reports.
    assert!(
        provider.has_texture("facegen\\ears\\human\\EarsHuman_n.dds"),
        "the derived normal-map sibling must also resolve through the fallback"
    );
}

/// A completely unrelated missing texture (no `facegen\` segment at all)
/// must still miss cleanly — the fallback is additive, not a general
/// "search everywhere" net that would mask real misses.
#[test]
#[ignore = "needs Oblivion game data on disk"]
fn unrelated_missing_texture_is_not_masked_by_the_fallback() {
    let textures_bsa = oblivion_data_dir().join("Oblivion - Textures - Compressed.bsa");
    if !textures_bsa.is_file() {
        return;
    }
    let provider = build_texture_provider(&[
        "--textures-bsa".to_owned(),
        textures_bsa.to_string_lossy().into_owned(),
    ]);

    assert!(
        !provider.has_texture("textures\\does_not_exist\\nowhere.dds"),
        "a genuinely absent, non-facegen path must still report missing"
    );
    assert!(provider
        .extract("textures\\does_not_exist\\nowhere.dds")
        .is_none());
}
