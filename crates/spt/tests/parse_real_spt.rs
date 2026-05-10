//! Corpus-integration test for the `.spt` TLV walker.
//!
//! Mirrors `crates/nif/tests/parse_real_nifs.rs` and
//! `crates/plugin/tests/parse_real_esm.rs`: env-var gated, `#[ignore]`,
//! requires a vanilla BSA on disk. Asserts the SpeedTree
//! compatibility plan's Phase 1.3 acceptance gate — ≥ 95 % of FNV
//! `.spt` files reach the geometry tail without falling into an
//! unknown-tag bail-out before then.
//!
//! ## Usage
//!
//! ```bash
//! BYROREDUX_FNV_DATA="/path/to/Fallout New Vegas/Data" \
//!     cargo test -p byroredux-spt --release \
//!     --test parse_real_spt -- --ignored --nocapture
//! ```
//!
//! Same env-var convention as the other real-corpus tests
//! (`BYROREDUX_FNV_DATA` / `_FO3_DATA` / `_OBL_DATA`).

use byroredux_bsa::BsaArchive;
use byroredux_spt::parse_spt;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct Stats {
    total_files: u32,
    parsed_with_entries: u32,
    /// Files whose parse hit `unknown_tags` non-empty — i.e. bailed
    /// out before reaching the geometry tail.
    files_with_unknown_tags: u32,
    /// Total entries decoded across the corpus (sanity bound).
    total_entries: u64,
}

impl Stats {
    fn coverage_rate(&self) -> f32 {
        if self.total_files == 0 {
            return 0.0;
        }
        let clean_files = self.total_files.saturating_sub(self.files_with_unknown_tags);
        clean_files as f32 / self.total_files as f32
    }
}

fn data_dir(env_var: &str, fallback: &str) -> Option<PathBuf> {
    if let Ok(v) = std::env::var(env_var) {
        let p = PathBuf::from(v);
        if p.exists() {
            return Some(p);
        }
    }
    let p = PathBuf::from(fallback);
    p.exists().then_some(p)
}

fn sweep_archive(bsa_path: &Path, label: &str) -> Stats {
    let archive = BsaArchive::open(bsa_path).expect("open BSA");
    let spt_files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|f| f.to_ascii_lowercase().ends_with(".spt"))
        .map(|f| f.to_string())
        .collect();

    let mut stats = Stats::default();
    let mut unknown_samples: Vec<(String, u32, usize)> = Vec::new();

    for path in &spt_files {
        let bytes = match archive.extract(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        stats.total_files += 1;
        match parse_spt(&bytes) {
            Ok(scene) => {
                if !scene.entries.is_empty() {
                    stats.parsed_with_entries += 1;
                }
                stats.total_entries += scene.entries.len() as u64;
                if !scene.unknown_tags.is_empty() {
                    stats.files_with_unknown_tags += 1;
                    if unknown_samples.len() < 8 {
                        let (tag, off) = scene.unknown_tags[0];
                        unknown_samples.push((path.clone(), tag, off));
                    }
                }
            }
            Err(e) => {
                eprintln!("[{}] parse_spt failed on {}: {}", label, path, e);
                stats.files_with_unknown_tags += 1;
            }
        }
    }

    eprintln!(
        "[{}] {} files | {} with entries | {} hit unknown tag | {} entries total | {:.2} % coverage",
        label,
        stats.total_files,
        stats.parsed_with_entries,
        stats.files_with_unknown_tags,
        stats.total_entries,
        stats.coverage_rate() * 100.0,
    );
    if !unknown_samples.is_empty() {
        eprintln!("  unknown-tag samples (path / tag / offset):");
        for (p, t, o) in &unknown_samples {
            eprintln!("    {} | tag={} (0x{:04x}) at offset {}", p, t, t, o);
        }
    }
    stats
}

/// Acceptance gate for Phase 1.3: ≥ 95 % of FNV `.spt` files clear
/// the parameter section without an unknown-tag bail-out.
#[test]
#[ignore]
fn parse_rate_fnv_spt() {
    let Some(data) = data_dir(
        "BYROREDUX_FNV_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
    ) else {
        eprintln!("[FNV] skip: BYROREDUX_FNV_DATA unset and fallback missing");
        return;
    };
    let bsa = data.join("Fallout - Meshes.bsa");
    let stats = sweep_archive(&bsa, "FNV");
    assert!(
        stats.total_files > 0,
        "FNV corpus must contain at least one `.spt` (`Fallout - Meshes.bsa` ships ~10)"
    );
    assert!(
        stats.coverage_rate() >= 0.95,
        "FNV `.spt` parser coverage = {:.1}% (expected ≥ 95 %); \
         {} of {} files bailed on an unknown tag",
        stats.coverage_rate() * 100.0,
        stats.files_with_unknown_tags,
        stats.total_files,
    );
}

/// FO3: same gate as FNV — should be at parity since both share the
/// `__IdvSpt_02_` magic + observed tag dictionary.
#[test]
#[ignore]
fn parse_rate_fo3_spt() {
    let Some(data) = data_dir(
        "BYROREDUX_FO3_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
    ) else {
        eprintln!("[FO3] skip: BYROREDUX_FO3_DATA unset and fallback missing");
        return;
    };
    let bsa = data.join("Fallout - Meshes.bsa");
    let stats = sweep_archive(&bsa, "FO3");
    assert!(stats.total_files > 0, "FO3 corpus must contain at least one `.spt`");
    assert!(
        stats.coverage_rate() >= 0.95,
        "FO3 `.spt` parser coverage = {:.1}% (expected ≥ 95 %)",
        stats.coverage_rate() * 100.0,
    );
}

/// Oblivion: largest corpus (113 vanilla files); same gate. Tighter
/// floor matters because Cyrodiil exteriors lean entirely on TREE
/// REFRs for forest content.
#[test]
#[ignore]
fn parse_rate_oblivion_spt() {
    let Some(data) = data_dir(
        "BYROREDUX_OBL_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
    ) else {
        eprintln!("[OBL] skip: BYROREDUX_OBL_DATA unset and fallback missing");
        return;
    };
    let bsa = data.join("Oblivion - Meshes.bsa");
    let stats = sweep_archive(&bsa, "OBL");
    assert!(
        stats.total_files >= 100,
        "Oblivion corpus must contain ≥ 100 `.spt` files (vanilla ships 113)"
    );
    assert!(
        stats.coverage_rate() >= 0.95,
        "Oblivion `.spt` parser coverage = {:.1}% (expected ≥ 95 %)",
        stats.coverage_rate() * 100.0,
    );
}
