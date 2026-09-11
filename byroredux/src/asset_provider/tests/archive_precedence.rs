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
    // #3850: an explicitly-set override is BINDING. Returning it unchecked
    // meant a typo'd or DLC-stripped path surfaced much later as a failure
    // against a directory the operator never named.
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

/// #3917 — the sound pool obeys the same last-wins rule as meshes/textures.
///
/// `[profiles.fnv]` lists `["Fallout - Sound.bsa", "Update.bsa"]` with a
/// comment stating the patch goes last "under last-wins", but
/// `SoundArchiveProvider::extract` iterated forward and served the base
/// archive. Measured on the shipped FNV archives: the two overlap on exactly
/// one key, and the copies differ in size (267 774 B vs 210 534 B), so this
/// is a real difference in delivered audio rather than a tie.
///
/// `#[ignore]`-gated against installed FNV data, same convention as the mesh
/// sibling above. Run with:
///
///   cargo test -p byroredux --bin byroredux archive_precedence -- --ignored
#[test]
#[ignore = "requires installed Fallout New Vegas Data/"]
fn later_listed_sound_bsa_wins_a_collision() {
    let data = fnv_data_dir();
    let base = data.join("Fallout - Sound.bsa");
    let patch = data.join("Update.bsa");
    for path in [&base, &patch] {
        assert!(path.is_file(), "missing {path:?}");
    }

    const KEY: &str = r"sound\fx\wpn\minigun\wpn_minigun_spin_lpm.wav";

    let from_base = byroredux_bsa::BsaArchive::open(base.to_str().unwrap())
        .expect("open base sound archive")
        .extract(KEY)
        .expect("the collision key must exist in Fallout - Sound.bsa");
    let from_patch = byroredux_bsa::BsaArchive::open(patch.to_str().unwrap())
        .expect("open Update.bsa")
        .extract(KEY)
        .expect("the collision key must exist in Update.bsa");
    assert_ne!(
        from_base.len(),
        from_patch.len(),
        "the two copies must differ, or this test cannot tell the orders apart"
    );

    let args: Vec<String> = vec![
        "--sounds-bsa".into(),
        base.to_string_lossy().into_owned(),
        "--sounds-bsa".into(),
        patch.to_string_lossy().into_owned(),
    ];
    let provider = build_sound_archive_provider(&args);
    let served = provider
        .extract(KEY)
        .expect("provider must resolve the key");
    assert_eq!(
        served.len(),
        from_patch.len(),
        "the LAST-listed sound archive must win: got {} bytes (base is {}, patch is {})",
        served.len(),
        from_base.len(),
        from_patch.len()
    );
}

fn fnv_data_dir() -> std::path::PathBuf {
    if let Some(v) = std::env::var_os("BYROREDUX_FNV_DATA").filter(|s| !s.is_empty()) {
        let p = std::path::PathBuf::from(v);
        assert!(
            p.is_dir(),
            "BYROREDUX_FNV_DATA points to {p:?}, which is not a directory"
        );
        return p;
    }
    std::path::PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data")
}

// ── Archive precedence across the provider pools (#3917) ────────────────

/// Providers whose archive search order is deliberately NOT last-wins, each
/// with the issue that decided it. Anything else must iterate `.rev()`.
///
/// `script.rs` — `--scripts-bsa` carries a mod principal and override
/// archives are listed FIRST, the inverse of mod-manager load order
/// (#1743 / SCR-D7-03). `ScriptProvider` states this in its own doc, which
/// is the difference between a decision and an oversight.
const FIRST_WINS_BY_DESIGN: &[(&str, &str)] = &[("script.rs", "#1743")];

/// Every content provider resolves a collision the same way: last-listed
/// archive wins, matching Bethesda load order (#3637, `3562401b`).
///
/// This is a directory walk, not a hand-listed set of files, because a
/// hardcoded list cannot see the provider nobody wrote down — which is
/// exactly how the audio provider sat first-wins through #3637's sweep and
/// two follow-ups. #3917: by the time it was caught, `[profiles.fnv]`
/// already listed two sound archives with a comment asserting last-wins,
/// and the one key they overlap on really was served from the unpatched
/// archive.
#[test]
fn every_content_provider_resolves_collisions_last_wins() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/asset_provider");

    // Needle split so this file's own text cannot satisfy the scan — the
    // first attempt composed it as `"for archive in " + ""`, which still
    // left the whole literal in the source and made the test find itself.
    let needle = format!("{}{}", "for archive ", "in ");

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("asset_provider/ must be readable") {
            let path = entry.expect("readable entry").path();
            // `tests/` holds no providers; scanning it would only find this
            // file's own prose.
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut checked = 0usize;
    let walked = files.len();
    for path in &files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 file name")
            .to_string();
        let source = std::fs::read_to_string(path).expect("readable source");

        for (offset, _) in source.match_indices(&needle) {
            let line_end = source[offset..]
                .find('\n')
                .map_or(source.len(), |i| offset + i);
            let line = &source[offset..line_end];
            if let Some((_, issue)) = FIRST_WINS_BY_DESIGN.iter().find(|(f, _)| *f == name) {
                assert!(
                    source.contains(issue),
                    "{name} is exempt from last-wins but no longer cites {issue}. An \
                     exemption without its rationale written down is indistinguishable \
                     from an oversight — which is the whole of #3917."
                );
                continue;
            }
            assert!(
                line.contains(".rev()"),
                "{name}: `{}` iterates archives forward. Content providers resolve \
                 collisions last-wins (#3637); a forward walk serves the FIRST \
                 archive, i.e. the unpatched one. If this provider genuinely wants \
                 first-wins, add it to FIRST_WINS_BY_DESIGN with the issue that \
                 decided it, the way script.rs does.",
                line.trim()
            );
            checked += 1;
        }
    }

    assert!(
        walked >= 6,
        "expected to walk at least the six provider modules, saw {walked}"
    );
    assert!(
        checked >= 7,
        "expected at least seven last-wins archive walks (texture x4, material x2, \
         audio x1), found {checked} — the scan stopped matching, so it is no \
         longer guarding anything"
    );
}
