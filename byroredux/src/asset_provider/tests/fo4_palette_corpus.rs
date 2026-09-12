//! Real-data corpus census for the FO4 lit-palette (grayscale-to-palette)
//! shader path — #3928 (FO4-2026-09-05b-D9-01).
//!
//! `#3897`/`#3898` added synthetic unit tests for the NIF flag capture, the
//! BGSM enable-bit merge, and the flag packer — all three pass, but none of
//! them observes a colour or even a real authored value: nothing in the
//! repository walks actual FO4 material archives to confirm the palette
//! path is exercised by real content at all, or to pin how much of it there
//! is so a future translation change that collapses the parameter shows up
//! as a count moving. `byroredux/tests/golden_frames.rs` renders the cube
//! demo, which has no BGSM and no LUT, so it cannot see this branch either.
//!
//! This is the "cheapest useful gate" the issue asks for: a corpus census,
//! not a screenshot. It walks `Fallout4 - Materials.ba2`'s `.bgsm` files,
//! counts how many enable the palette (`grayscale_to_palette_color`), and
//! reports the distinct `grayscale_to_palette_scale` values and shared LUT
//! (`greyscale_texture`) paths in play. Gated `#[ignore]` on real FO4 game
//! data (same convention as `archive_precedence.rs` / `precombined.rs`);
//! `cargo test -p byroredux --bin byroredux fo4_palette -- --ignored`.
//!
//! The exact expected counts are deliberately NOT hardcoded here — this
//! session had no FO4 install to measure a baseline against (per this
//! project's no-guessing policy, a number this test can't verify itself
//! isn't asserted). The test instead asserts the structural invariant that
//! matters most ("the palette path is exercised by real content, not
//! accidentally dead") and prints the exact counts so a maintainer who runs
//! it once can fill in a tighter pin (`assert_eq!(palette_enabled, N)`)
//! against their own installed corpus.

use byroredux_bgsm::MaterialFile;

fn fo4_data_dir() -> std::path::PathBuf {
    // #3850: an explicitly-set override is BINDING — never silently
    // substitute the hardcoded dev-machine path for a typo'd/missing one.
    if let Some(v) = std::env::var_os("BYROREDUX_FO4_DATA").filter(|s| !s.is_empty()) {
        let p = std::path::PathBuf::from(v);
        assert!(
            p.is_dir(),
            "BYROREDUX_FO4_DATA points to {p:?}, which is not a directory"
        );
        return p;
    }
    std::path::PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data")
}

#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn lit_palette_path_is_exercised_by_real_fo4_materials() {
    let data = fo4_data_dir();
    let materials_path = data.join("Fallout4 - Materials.ba2");
    if !materials_path.is_file() {
        eprintln!(
            "Skipping: {materials_path:?} not found (BYROREDUX_FO4_DATA not set \
             and default path missing)"
        );
        return;
    }
    let archive = match byroredux_bsa::Ba2Archive::open(&materials_path) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Skipping: open {materials_path:?}: {e}");
            return;
        }
    };

    let bgsm_paths: Vec<&str> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".bgsm"))
        .collect();
    assert!(
        !bgsm_paths.is_empty(),
        "Fallout4 - Materials.ba2 must contain at least one .bgsm — an empty \
         list means the archive open/list path itself regressed, not that \
         the palette census has nothing to count"
    );

    let mut scanned = 0usize;
    let mut parse_failures = 0usize;
    let mut palette_enabled = 0usize;
    let mut distinct_scales: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut distinct_luts: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for path in &bgsm_paths {
        let Ok(bytes) = archive.extract(path) else {
            continue;
        };
        scanned += 1;
        let bgsm = match byroredux_bgsm::parse(&bytes) {
            Ok(MaterialFile::Bgsm(m)) => m,
            Ok(MaterialFile::Bgem(_)) => continue, // shouldn't happen for a .bgsm path
            Err(_) => {
                parse_failures += 1;
                continue;
            }
        };
        if bgsm.base.grayscale_to_palette_color {
            palette_enabled += 1;
            // Bucket the scale by its bit pattern (not the float itself) so
            // the set is well-ordered and NaN-safe — this is a census, not
            // a numeric comparison.
            distinct_scales.insert(bgsm.grayscale_to_palette_scale.to_bits());
            if !bgsm.greyscale_texture.is_empty() {
                distinct_luts.insert(bgsm.greyscale_texture.to_ascii_lowercase());
            }
        }
    }

    eprintln!(
        "FO4 lit-palette census: {scanned} .bgsm files scanned ({parse_failures} parse \
         failures), {palette_enabled} palette-enabled, {} distinct scale values over {} \
         distinct LUT textures",
        distinct_scales.len(),
        distinct_luts.len(),
    );

    // The one assertion this session can responsibly make without a real
    // corpus to calibrate against: the path is exercised by REAL content,
    // not accidentally dead. A maintainer running this against an actual
    // FO4 install should tighten this to `assert_eq!(palette_enabled, N)`
    // once a baseline is measured — see the module doc.
    assert!(
        palette_enabled > 0,
        "expected at least one real FO4 .bgsm with grayscale_to_palette_color \
         enabled (the MedTekResearch01 bench cell's hi-tech-panel materials, \
         e.g. hittechmetalpanel_01lgrad.dds, are exactly this content per \
         #3928) — zero means either the corpus lacks the expected content or \
         the palette translation itself silently went dead"
    );
}
