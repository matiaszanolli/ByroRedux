//! Real-data BSA reader regression tests (sibling of `ba2_real.rs`,
//! audit FO4-DIM2-05 / #587 SIBLING checklist item).
//!
//! The legacy BSA reader covers three vanilla format versions:
//!   * v103 — Oblivion / Morrowind compatibility
//!   * v104 — Fallout 3, Fallout New Vegas, Skyrim LE
//!   * v105 — Skyrim SE, Fallout 4 patches
//!
//! The pre-#587 unit tests cover synthetic v104 / v105 records but
//! never touch a vanilla archive. These tests close that gap by
//! exercising the version dispatch + extract paths against real game
//! files. v103 coverage was added under #690 — without it a
//! "simplification" that removes the `if version == 105 { 24 } else
//! { 16 }` folder-record branch (or breaks the `version >= 104`
//! embed-name gate, or routes v103 down a non-zlib codec path) would
//! slip past `cargo test -- --ignored` even on a fully-installed
//! dev box.
//!
//! Gated `#[ignore]` on `BYROREDUX_OBLIVION_DATA` /
//! `BYROREDUX_FNV_DATA` / `BYROREDUX_SKYRIMSE_DATA`; opt-in via:
//! ```sh
//! cargo test -p byroredux-bsa --test bsa_real -- --ignored
//! ```

use byroredux_bsa::BsaArchive;
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

/// #4660 — the data directory resolved but lacks an archive the test needs.
/// Under the strict lane that is a failure naming the file: a green skip here
/// is exactly the hole #3850 closed for the data directory itself.
#[track_caller]
fn require_archive(path: &std::path::Path) {
    if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
        panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file");
    }
    eprintln!("Skipping: {path:?} not found");
}

fn data_dir(env_var: &str, fallback: &str) -> Option<PathBuf> {
    if let Some(v) = std::env::var(env_var).ok().filter(|s| !s.is_empty()) {
        let p = PathBuf::from(&v);
        if p.is_dir() {
            return Some(p);
        }
        // #3850: an explicitly-set override is BINDING. Silently reading the
        // hardcoded dev-machine path instead would report results from a
        // different install than the operator named.
        panic!("{env_var} points to {v:?}, which is not a directory");
    }
    let p = PathBuf::from(fallback);
    if p.is_dir() {
        return Some(p);
    }
    require_game_data(env_var, &p);
    None
}

fn oblivion_data_dir() -> Option<PathBuf> {
    data_dir(
        "BYROREDUX_OBLIVION_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    )
}

fn fnv_data_dir() -> Option<PathBuf> {
    data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    )
}

fn skyrimse_data_dir() -> Option<PathBuf> {
    data_dir(
        "BYROREDUX_SKYRIMSE_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
    )
}

fn skyrimle_data_dir() -> Option<PathBuf> {
    // Not a Steam install: the reference copy lives in a Wine prefix (same
    // path as `Game::SkyrimLE.default_path()` in `crates/nif/tests/common`).
    data_dir(
        "BYROREDUX_SKYRIMLE_DATA",
        "/home/matias/Games/skyrim-original/drive_c/Program Files (x86)/The Elder Scrolls V Skyrim/Data",
    )
}

fn fo3_data_dir() -> Option<PathBuf> {
    data_dir(
        "BYROREDUX_FO3_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
    )
}

/// Oblivion ships v103 BSAs (zlib compression, 16-byte folder
/// records, u32 offsets, no `embed_file_names` flag — the
/// `version >= 104` gate at `archive.rs:188` excludes v103). Open
/// the meshes archive, extract a NIF, assert it carries the Gamebryo
/// magic header. This test exists to close the v103 disk-coverage
/// gap surfaced by AUDIT_OBLIVION_2026-04-25 (#690): v103 was
/// empirically validated through `nif_stats` (8,032 NIFs) and the
/// `oblivion_extract` example, but neither runs under
/// `cargo test -p byroredux-bsa`.
#[test]
#[ignore = "needs Oblivion game data on disk"]
fn oblivion_meshes_bsa_v103_extracts_nif_with_gamebryo_magic() {
    let Some(data) = oblivion_data_dir() else {
        eprintln!("Skipping: BYROREDUX_OBLIVION_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Oblivion - Meshes.bsa");
    if !archive_path.is_file() {
        require_archive(&archive_path);
        return;
    }

    let archive = BsaArchive::open(&archive_path).expect("open Oblivion - Meshes.bsa");
    assert_eq!(archive.version(), 103, "Oblivion - Meshes.bsa must be v103");
    assert!(
        archive.file_count() > 1000,
        "Oblivion meshes BSA ships ~20k entries; got {}",
        archive.file_count()
    );

    let entry = archive
        .list_files()
        .into_iter()
        .find(|p| p.ends_with(".nif"))
        .map(|s| s.to_string())
        .expect("Oblivion meshes BSA must ship at least one NIF");
    let bytes = archive
        .extract(&entry)
        .unwrap_or_else(|e| panic!("extract '{entry}' failed: {e}"));
    assert!(
        bytes.len() >= 20,
        "NIF '{entry}' decompressed to {} bytes — too small",
        bytes.len()
    );
    assert_eq!(
        &bytes[..4],
        b"Game",
        "extracted '{entry}' lacks Gamebryo magic; got {:?}",
        &bytes[..4]
    );
}

/// #4259 (OB-D2-01) — permanent brute-force extraction sweep for v103,
/// mirroring `skyrimse_meshes_bsa_v105_brute_force_extract_zero_errors`
/// below but across every vanilla + DLC archive rather than one, matching
/// what the ad hoc audit sweep actually measured. A live ad hoc sweep
/// during the 2026-09-11 audit found 147,629 files / 0 errors across all
/// 17 vanilla + DLC Oblivion archives; that sweep was never committed, so
/// a future regression in v103's zlib codec path, 16-byte folder-record
/// handling, or the Xbox-archive-flag gate would slip past
/// `cargo test -- --ignored` even on a fully-installed dev box.
#[test]
#[ignore = "needs Oblivion game data on disk"]
fn oblivion_all_bsas_v103_brute_force_extract_zero_errors() {
    let Some(data) = oblivion_data_dir() else {
        eprintln!("Skipping: BYROREDUX_OBLIVION_DATA not set and default path missing");
        return;
    };

    // Every vanilla + official DLC v103 archive Oblivion ships (17 total,
    // matching the audit's own measured count).
    const VANILLA_AND_DLC_ARCHIVES: &[&str] = &[
        "Oblivion - Meshes.bsa",
        "Oblivion - Misc.bsa",
        "Oblivion - Sounds.bsa",
        "Oblivion - Textures - Compressed.bsa",
        "Oblivion - Voices1.bsa",
        "Oblivion - Voices2.bsa",
        "DLCBattlehornCastle.bsa",
        "DLCFrostcrag.bsa",
        "DLCHorseArmor.bsa",
        "DLCOrrery.bsa",
        "DLCShiveringIsles - Meshes.bsa",
        "DLCShiveringIsles - Sounds.bsa",
        "DLCShiveringIsles - Textures.bsa",
        "DLCShiveringIsles - Voices.bsa",
        "DLCThievesDen.bsa",
        "DLCVileLair.bsa",
        "Knights.bsa",
    ];

    let mut archives_opened = 0usize;
    let mut total_files: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut errors: Vec<(String, String, String)> = Vec::new();

    'archives: for &name in VANILLA_AND_DLC_ARCHIVES {
        let archive_path = data.join(name);
        if !archive_path.is_file() {
            require_archive(&archive_path);
            continue;
        }
        let archive = BsaArchive::open(&archive_path).unwrap_or_else(|e| panic!("open {name}: {e}"));
        assert_eq!(archive.version(), 103, "{name} must be v103");
        archives_opened += 1;

        for path in archive.list_files() {
            match archive.extract(path) {
                Ok(bytes) => {
                    total_bytes += bytes.len() as u64;
                    total_files += 1;
                }
                Err(e) => {
                    errors.push((name.to_string(), path.to_string(), e.to_string()));
                    if errors.len() >= 16 {
                        break 'archives;
                    }
                }
            }
        }
    }

    assert!(
        archives_opened > 0,
        "no vanilla/DLC Oblivion v103 archives found under {data:?} — check the install"
    );
    eprintln!(
        "Oblivion v103 brute-force extract: {archives_opened} archives, {total_files} files, \
         {:.1} MB total, {} errors",
        total_bytes as f64 / 1_048_576.0,
        errors.len(),
    );
    if !errors.is_empty() {
        for (archive, path, err) in &errors {
            eprintln!("  ERR  [{archive}] {path}: {err}");
        }
        panic!(
            "Oblivion v103 brute-force extract sweep produced {} errors (must be 0)",
            errors.len()
        );
    }
    if archives_opened == VANILLA_AND_DLC_ARCHIVES.len() {
        assert_eq!(
            total_files, 147_629,
            "Oblivion vanilla + DLC BSA listing census changed"
        );
    }
}

/// #5903 — the remaining v104/v105 BSA generations need the same full
/// archive walk and extraction coverage as Oblivion's v103 corpus.
#[test]
#[ignore = "needs FO3, FNV and Skyrim SE game data on disk"]
fn fo3_fnv_and_skyrimse_bsa_corpus_census() {
    for (label, data, expected_archives, expected_entries) in [
        ("FO3", fo3_data_dir(), 16usize, 159_155u64),
        ("FNV", fnv_data_dir(), 21usize, 182_177u64),
        ("Skyrim SE", skyrimse_data_dir(), 23usize, 172_961u64),
    ] {
        let Some(data) = data else {
            eprintln!("[{label}] data unavailable; skipping");
            continue;
        };
        let mut paths: Vec<_> = std::fs::read_dir(&data)
            .unwrap_or_else(|e| panic!("[{label}] read {data:?}: {e}"))
            .map(|entry| entry.unwrap_or_else(|e| panic!("[{label}] directory entry: {e}")).path())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("bsa"))
            })
            .collect();
        paths.sort();
        let mut entries = 0u64;
        let mut extracted = 0u64;
        let mut errors = Vec::new();
        for path in &paths {
            let archive = BsaArchive::open(path)
                .unwrap_or_else(|e| panic!("[{label}] open {path:?}: {e}"));
            let files = archive.list_files();
            assert_eq!(archive.file_count(), files.len());
            entries += archive.file_count() as u64;
            for file in files {
                match archive.extract(file) {
                    Ok(_) => extracted += 1,
                    Err(error) => {
                        errors.push((path.clone(), file.to_string(), error.to_string()));
                        if errors.len() >= 16 {
                            break;
                        }
                    }
                }
            }
            if errors.len() >= 16 {
                break;
            }
        }
        eprintln!(
            "[{label}] {} BSA archives, {entries} entries, {extracted} extracted, {} errors",
            paths.len(), errors.len()
        );
        assert_eq!(paths.len(), expected_archives, "[{label}] BSA archive census changed");
        assert_eq!(entries, expected_entries, "[{label}] BSA file census changed");
        assert_eq!(extracted, entries, "[{label}] not every BSA file extracted: {errors:?}");
    }
}

/// #5903 — Skyrim LE is optional because most machines only install SE.
#[test]
#[ignore = "needs Skyrim LE game data on disk; set BYROREDUX_SKYRIMLE_DATA"]
fn skyrimle_bsa_corpus_full_extract_zero_errors() {
    let Some(data) = skyrimle_data_dir() else {
        eprintln!("[Skyrim LE] BYROREDUX_SKYRIMLE_DATA not set and default path missing");
        return;
    };
    let mut paths: Vec<_> = std::fs::read_dir(&data)
        .unwrap_or_else(|e| panic!("[Skyrim LE] read {data:?}: {e}"))
        .map(|entry| entry.unwrap_or_else(|e| panic!("[Skyrim LE] directory entry: {e}")).path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("bsa"))
        })
        .collect();
    paths.sort();
    let mut entries = 0u64;
    let mut extracted = 0u64;
    let mut errors = Vec::new();
    for path in &paths {
        let archive = BsaArchive::open(path)
            .unwrap_or_else(|e| panic!("[Skyrim LE] open {path:?}: {e}"));
        let files = archive.list_files();
        assert_eq!(archive.file_count(), files.len());
        entries += archive.file_count() as u64;
        for file in files {
            match archive.extract(file) {
                Ok(_) => extracted += 1,
                Err(error) => {
                    errors.push((path.clone(), file.to_string(), error.to_string()));
                    if errors.len() >= 16 {
                        break;
                    }
                }
            }
        }
        if errors.len() >= 16 {
            break;
        }
    }
    eprintln!(
        "[Skyrim LE] {} BSA archives, {entries} entries, {extracted} extracted, {} errors",
        paths.len(), errors.len()
    );
    assert!(!paths.is_empty(), "Skyrim LE Data contains no BSA archives");
    assert!(entries > 0, "Skyrim LE BSA corpus contains no files");
    assert_eq!(extracted, entries, "not every Skyrim LE BSA file extracted: {errors:?}");
}

/// FNV ships v104 BSAs (zlib compression, 16-byte folder records, u32
/// offsets). Open the meshes archive, extract a NIF, assert it carries
/// the Gamebryo magic header.
#[test]
#[ignore = "needs FNV game data on disk"]
fn fnv_meshes_bsa_v104_extracts_nif_with_gamebryo_magic() {
    let Some(data) = fnv_data_dir() else {
        eprintln!("Skipping: BYROREDUX_FNV_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Fallout - Meshes.bsa");
    if !archive_path.is_file() {
        require_archive(&archive_path);
        return;
    }

    let archive = BsaArchive::open(&archive_path).expect("open FNV Fallout - Meshes.bsa");
    assert_eq!(
        archive.version(),
        104,
        "FNV Fallout - Meshes.bsa must be v104"
    );
    assert!(
        archive.file_count() > 1000,
        "FNV meshes BSA ships ~30k entries; got {}",
        archive.file_count()
    );

    let entry = archive
        .list_files()
        .into_iter()
        .find(|p| p.ends_with(".nif"))
        .map(|s| s.to_string())
        .expect("FNV meshes BSA must ship at least one NIF");
    let bytes = archive
        .extract(&entry)
        .unwrap_or_else(|e| panic!("extract '{entry}' failed: {e}"));
    assert!(
        bytes.len() >= 20,
        "NIF '{entry}' decompressed to {} bytes — too small",
        bytes.len()
    );
    assert_eq!(
        &bytes[..4],
        b"Game",
        "extracted '{entry}' lacks Gamebryo magic; got {:?}",
        &bytes[..4]
    );
}

/// Skyrim SE ships v105 BSAs (LZ4 compression, 24-byte folder records,
/// u64 offsets). The version-dispatch path takes a different branch
/// than v104 — exercising both in CI guards against a regression
/// flipping bytes in either direction.
#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn skyrimse_meshes_bsa_v105_extracts_nif_with_gamebryo_magic() {
    let Some(data) = skyrimse_data_dir() else {
        eprintln!("Skipping: BYROREDUX_SKYRIMSE_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Skyrim - Meshes0.bsa");
    if !archive_path.is_file() {
        require_archive(&archive_path);
        return;
    }

    let archive = BsaArchive::open(&archive_path).expect("open Skyrim - Meshes0.bsa");
    assert_eq!(
        archive.version(),
        105,
        "Skyrim SE Skyrim - Meshes0.bsa must be v105"
    );
    assert!(
        archive.file_count() > 1000,
        "Skyrim SE meshes BSA ships ~50k entries; got {}",
        archive.file_count()
    );

    let entry = archive
        .list_files()
        .into_iter()
        .find(|p| p.ends_with(".nif"))
        .map(|s| s.to_string())
        .expect("Skyrim SE meshes BSA must ship at least one NIF");
    let bytes = archive
        .extract(&entry)
        .unwrap_or_else(|e| panic!("extract '{entry}' failed: {e}"));
    assert!(
        bytes.len() >= 20,
        "NIF '{entry}' decompressed to {} bytes — too small",
        bytes.len()
    );
    assert_eq!(
        &bytes[..4],
        b"Game",
        "extracted '{entry}' lacks Gamebryo magic; got {:?}",
        &bytes[..4]
    );
}

/// Brute-force regression sweep on Skyrim SE — the LZ4 codec path
/// has the most failure modes (block format vs frame format,
/// dictionary lookup, `read_block` vs `read_lz4_frame`). Exercising
/// every NIF entry in `Skyrim - Meshes0.bsa` and asserting zero
/// errors guards against codec regressions sneaking in via dependency
/// updates.
#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn skyrimse_meshes_bsa_v105_brute_force_extract_zero_errors() {
    let Some(data) = skyrimse_data_dir() else {
        eprintln!("Skipping: BYROREDUX_SKYRIMSE_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Skyrim - Meshes0.bsa");
    if !archive_path.is_file() {
        require_archive(&archive_path);
        return;
    }

    let archive = BsaArchive::open(&archive_path).expect("open Skyrim - Meshes0.bsa");
    let entries: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    assert!(
        entries.len() > 5000,
        "Skyrim SE Meshes0.bsa ships many NIFs; got {}",
        entries.len()
    );

    let mut errors: Vec<(String, String)> = Vec::new();
    let mut total_bytes: u64 = 0;
    for path in &entries {
        match archive.extract(path) {
            Ok(bytes) => total_bytes += bytes.len() as u64,
            Err(e) => {
                errors.push((path.clone(), e.to_string()));
                if errors.len() >= 16 {
                    break;
                }
            }
        }
    }

    eprintln!(
        "Skyrim SE brute-force extract: {} NIFs, {:.1} MB total, {} errors",
        entries.len(),
        total_bytes as f64 / 1_048_576.0,
        errors.len(),
    );
    if !errors.is_empty() {
        for (path, err) in &errors {
            eprintln!("  ERR  {path}: {err}");
        }
        panic!(
            "Skyrim SE Meshes0.bsa extract sweep produced {} errors (must be 0)",
            errors.len()
        );
    }
}

/// `declared_size` is what the streaming worker budgets decoded input
/// with before it extracts, so it must equal what `extract` then returns.
/// One archive per BSA version: v103 zlib with per-file compression
/// toggles, v104 (FNV, embedded names on some archives), v105 LZ4 frame.
#[test]
#[ignore = "needs Oblivion, FNV and Skyrim SE game data on disk"]
fn declared_size_matches_extract_across_bsa_versions() {
    let cases = [
        (oblivion_data_dir(), "Oblivion - Meshes.bsa"),
        (fnv_data_dir(), "Fallout - Meshes.bsa"),
        (skyrimse_data_dir(), "Skyrim - Meshes0.bsa"),
    ];
    for (dir, name) in cases {
        let Some(dir) = dir else {
            eprintln!("Skipping: no game data for {name}");
            continue;
        };
        let path = dir.join(name);
        if !path.is_file() {
            require_archive(&path);
            continue;
        }
        let archive = BsaArchive::open(&path).unwrap_or_else(|e| panic!("open {name}: {e}"));
        let mut mismatches = Vec::new();
        let files = archive.list_files();
        for file in &files {
            let declared = archive
                .declared_size(file)
                .unwrap_or_else(|e| panic!("{name}: declared_size({file}): {e}"));
            let extracted = archive
                .extract(file)
                .unwrap_or_else(|e| panic!("{name}: extract({file}): {e}"))
                .len();
            if declared != extracted {
                mismatches.push((file.to_string(), declared, extracted));
            }
        }
        eprintln!(
            "{name}: {} files, {} mismatches",
            files.len(),
            mismatches.len()
        );
        assert!(
            mismatches.is_empty(),
            "{name}: declared != extracted for {} files, first: {:?}",
            mismatches.len(),
            &mismatches[..mismatches.len().min(5)]
        );
    }
}

/// #5367 Phase V probe/floor — FNV's voice archive opens and the
/// Phase-G greeting line's authored voice file resolves under the
/// documented convention `sound\voice\<plugin>\<voice-type
/// EDID>\<INFO formid 8-hex>_<response>.ogg`. Doc Mitchell's greeting
/// INFO is 0x00107222 (`dt1-dialogue-layers.sh` leg 1's line); his
/// voice type is `MaleUniqueDocMitchell` (VTCK — the folder-name
/// convention this test pins). Uncompressed v104 (archive flag
/// measured: `compressed=0`).
#[test]
#[ignore = "needs FNV game data on disk"]
fn fnv_voices_bsa_resolves_greeting_line_voice() {
    let Some(data) = fnv_data_dir() else {
        require_game_data(
            "BYROREDUX_FNV_DATA",
            std::path::Path::new("/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data"),
        );
        eprintln!("[FNV voices] skipping: game data unavailable");
        return;
    };
    let archive = BsaArchive::open(data.join("Fallout - Voices1.bsa"))
        .unwrap_or_else(|e| panic!("open Voices1.bsa failed: {e}"));
    assert!(
        archive.file_count() >= 100_000,
        "FNV voice archive scale (measured 105 517): {}",
        archive.file_count()
    );
    let bytes = archive
        .extract("sound\\voice\\falloutnv.esm\\maleuniquedocmitchell\\vcg01_greeting_00107222_1.ogg")
        .unwrap_or_else(|e| panic!("greeting voice extract failed: {e}"));
    assert_eq!(&bytes[..4], b"OggS", "an Ogg Vorbis stream");
}

/// #5367 Phase V classification — Skyrim's voice container (#5464). Both
/// `Skyrim - Voices_en0.bsa` (v105, SE) and LE's `Skyrim - Voices.bsa`
/// hold `.fuz` files with one layout: magic `FUZE`, `u32` version,
/// `u32` lip size, then the raw `.lip` bytes (not a RIFF), then the audio
/// — a `RIFF` of form type `XWMA`. The codec is xWMA (WMA v2 / WMA Pro in
/// RIFF), not XMA2 (the Xbox 360 codec, which no PC file carries), so the
/// V2 voice runtime needs an xWMA decoder. The audit census measured
/// 3,000 / 3,000 sampled files on SE and on LE matching this layout.
/// Phase V's runtime is the Ogg path (FO3/FNV); this test exists so the
/// V2 scope statement cites bytes, not folklore.
#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn skyrim_fuz_container_shape() {
    let Some(data) = skyrimse_data_dir() else {
        require_game_data(
            "BYROREDUX_SKYRIMSE_DATA",
            std::path::Path::new(
                "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
            ),
        );
        eprintln!("[Skyrim fuz] skipping: game data unavailable");
        return;
    };
    let archive = BsaArchive::open(data.join("Skyrim - Voices_en0.bsa"))
        .unwrap_or_else(|e| panic!("open Voices_en0.bsa failed: {e}"));
    let fuzzes: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.ends_with(".fuz"))
        .take(64)
        .map(str::to_owned)
        .collect();
    assert!(!fuzzes.is_empty(), "the archive holds .fuz entries");
    for fuz in &fuzzes {
        let bytes = archive
            .extract(fuz)
            .unwrap_or_else(|e| panic!("extract {fuz} failed: {e}"));
        assert_eq!(&bytes[..4], b"FUZE", "{fuz}: the FUZ container magic");
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(version, 1, "{fuz}: FUZE version");
        let lip_size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        // The audio chunk starts right after the lip data: a RIFF of form
        // type XWMA (offset 8 of the RIFF header).
        let audio_at = 12 + lip_size;
        assert!(
            bytes.len() >= audio_at + 12,
            "{fuz}: {} bytes cannot hold lip_size {lip_size} + an audio header",
            bytes.len()
        );
        assert_eq!(
            &bytes[audio_at..audio_at + 4],
            b"RIFF",
            "{fuz}: audio RIFF at 12 + lip_size ({audio_at})"
        );
        assert_eq!(
            &bytes[audio_at + 8..audio_at + 12],
            b"XWMA",
            "{fuz}: the audio codec is xWMA, not XMA2"
        );
    }
    eprintln!(
        "[fuz] {} sampled .fuz files: FUZE v1 + raw lip + RIFF/XWMA at 12 + lip_size",
        fuzzes.len()
    );
}

/// #6517 — every game's data-dir helper must go through `data_dir`, so each gets
/// the dev-machine fallback and the strict lane's `require_game_data`. An
/// env-only helper silently reports `ok` for an absent install (the LE sweep
/// did, until this pin).
#[test]
fn every_game_data_dir_helper_routes_through_data_dir() {
    let src = include_str!("bsa_real.rs");
    let mut helpers = 0;
    // Built at runtime so this test's own text does not match.
    let signature = ["_data_dir() -> Option<", "PathBuf> {"].concat();
    for (at, _) in src.match_indices(&signature) {
        let body_start = at + src[at..].find('{').unwrap();
        let body_end = body_start + src[body_start..].find("\n}\n").unwrap();
        let body = &src[body_start..body_end];
        let name_start = src[..at].rfind("fn ").unwrap();
        assert!(
            body.contains("data_dir(\n") || body.contains("data_dir(\""),
            "`{}` does not route through `data_dir`: no fallback, and the strict lane \
             cannot fail an absent install",
            &src[name_start..at + "_data_dir".len()]
        );
        helpers += 1;
    }
    assert!(helpers >= 5, "found only {helpers} data-dir helpers — pattern drifted");
}
