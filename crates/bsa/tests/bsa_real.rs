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
//! files. Gated `#[ignore]` on `BYROREDUX_FNV_DATA` /
//! `BYROREDUX_SKYRIMSE_DATA`; opt-in via:
//! ```sh
//! cargo test -p byroredux-bsa --test bsa_real -- --ignored
//! ```

use byroredux_bsa::BsaArchive;
use std::path::PathBuf;

fn data_dir(env_var: &str, fallback: &str) -> Option<PathBuf> {
    if let Ok(v) = std::env::var(env_var) {
        let p = PathBuf::from(&v);
        if p.is_dir() {
            return Some(p);
        }
        eprintln!("{env_var} points to {v:?} which is not a directory; falling back to default");
    }
    let p = PathBuf::from(fallback);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
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

/// FNV ships v104 BSAs (zlib compression, 16-byte folder records, u32
/// offsets). Open the meshes archive, extract a NIF, assert it carries
/// the Gamebryo magic header.
#[test]
#[ignore]
fn fnv_meshes_bsa_v104_extracts_nif_with_gamebryo_magic() {
    let Some(data) = fnv_data_dir() else {
        eprintln!("Skipping: BYROREDUX_FNV_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Fallout - Meshes.bsa");
    if !archive_path.is_file() {
        eprintln!("Skipping: {archive_path:?} not found");
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
#[ignore]
fn skyrimse_meshes_bsa_v105_extracts_nif_with_gamebryo_magic() {
    let Some(data) = skyrimse_data_dir() else {
        eprintln!("Skipping: BYROREDUX_SKYRIMSE_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Skyrim - Meshes0.bsa");
    if !archive_path.is_file() {
        eprintln!("Skipping: {archive_path:?} not found");
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
#[ignore]
fn skyrimse_meshes_bsa_v105_brute_force_extract_zero_errors() {
    let Some(data) = skyrimse_data_dir() else {
        eprintln!("Skipping: BYROREDUX_SKYRIMSE_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Skyrim - Meshes0.bsa");
    if !archive_path.is_file() {
        eprintln!("Skipping: {archive_path:?} not found");
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
