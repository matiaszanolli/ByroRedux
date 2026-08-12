//! M35 — numeric-sibling archive auto-load (Skyrim zero-based series).
//!
//! Extracted from the 2051-LOC `asset_provider/tests.rs` (#2411 / TD1-010)
//! at the topic-divider comments that file already carried. Contents
//! unchanged.

use super::super::*;

// ── M35 — numeric-sibling archive auto-load (Skyrim zero-based series) ──

/// FNV ships `Fallout - Textures.bsa` + `Fallout - Textures2.bsa`: a
/// no-trailing-digit primary offers `…2`..`…9`. Unchanged by the M35 fix.
#[test]
fn siblings_fnv_no_suffix_offers_2_through_9() {
    let s = numeric_sibling_paths("Fallout - Textures.bsa");
    assert_eq!(s.len(), 8);
    assert_eq!(s[0], "Fallout - Textures2.bsa");
    assert_eq!(s[7], "Fallout - Textures9.bsa");
    assert!(!s.iter().any(|p| p.ends_with("Textures1.bsa")));
}

/// Skyrim's zero-based series start (`…0`) must offer `…1`..`…9` — this is
/// the M35 fix that pulls in `Textures7.bsa` (object-LOD atlas + `.btr`
/// terrain diffuse) and `Meshes1.bsa` (`.btr`/`.bto`) from the `…0` name.
#[test]
fn siblings_skyrim_zero_start_offers_1_through_9() {
    let s = numeric_sibling_paths("Skyrim - Textures0.bsa");
    assert_eq!(s.len(), 9);
    assert_eq!(s[0], "Skyrim - Textures1.bsa");
    assert!(s.iter().any(|p| p == "Skyrim - Textures7.bsa"));
    assert_eq!(s[8], "Skyrim - Textures9.bsa");
    // Meshes0 → Meshes1 (the `.btr`/`.bto` archive) without an explicit arg.
    let m = numeric_sibling_paths("Skyrim - Meshes0.bsa");
    assert!(m.iter().any(|p| p == "Skyrim - Meshes1.bsa"));
}

/// A mid-series non-zero member (`…2`) auto-expands nothing — the user is
/// listing members explicitly; expanding would double-open every archive.
#[test]
fn siblings_mid_series_digit_offers_none() {
    assert!(numeric_sibling_paths("Skyrim - Textures3.bsa").is_empty());
    assert!(numeric_sibling_paths("Skyrim - Textures1.bsa").is_empty());
}

/// `…10` (a digit before the trailing `0`) is an explicit member, NOT a
/// series start — must not be mistaken for one and expanded to `…11`..`…19`.
#[test]
fn siblings_ten_suffix_is_not_a_series_start() {
    assert!(numeric_sibling_paths("Mod - Textures10.bsa").is_empty());
}

/// BA2 extension is handled the same way (FO4/Starfield naming).
#[test]
fn siblings_ba2_zero_start() {
    let s = numeric_sibling_paths("DLC - Textures0.ba2");
    assert_eq!(s[0], "DLC - Textures1.ba2");
    assert!(s.iter().all(|p| p.ends_with(".ba2")));
}

/// SF-D7-NEW-02 / #2106 — Starfield's two-digit zero-padded series start
/// (`…01`) must offer `…02`..`…09`, not fall into the mid-series
/// "don't expand" bucket (its last char `'1'` is a digit, same as `…2`).
/// Without this, `Starfield - Meshes01.ba2` (the project's own documented
/// launch command) silently never loads `Meshes02.ba2`.
#[test]
fn siblings_starfield_two_digit_zero_start_offers_02_through_09() {
    let s = numeric_sibling_paths("Starfield - Meshes01.ba2");
    assert_eq!(s.len(), 8);
    assert_eq!(s[0], "Starfield - Meshes02.ba2");
    assert_eq!(s[7], "Starfield - Meshes09.ba2");
    assert!(s.iter().all(|p| p.ends_with(".ba2")));
}

/// `…101` (a digit before the two-digit `01` tail) is an explicit 3-digit
/// member, NOT a two-digit series start — must not expand to `…102`..`…109`.
#[test]
fn siblings_three_digit_101_suffix_is_not_a_two_digit_series_start() {
    assert!(numeric_sibling_paths("Mod - Meshes101.bsa").is_empty());
}
