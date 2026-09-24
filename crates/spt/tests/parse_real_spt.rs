//! Corpus-integration test for the `.spt` TLV walker.
//!
//! Mirrors `crates/nif/tests/parse_real_nifs.rs` and
//! `crates/plugin/tests/parse_real_esm.rs`: env-var gated, `#[ignore]`,
//! requires a vanilla BSA on disk. Asserts the SpeedTree
//! compatibility plan's Phase 1.3 acceptance gate — ≥ 95 % of FNV
//! `.spt` files reach the walker's out-of-range stop (or EOF) without
//! falling into an unknown-tag bail-out first. That stop is `TAG_MAX`,
//! not a geometry section: the TLV stream continues past it (#3808).
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
use byroredux_spt::parser::best_resync_shift;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct Stats {
    total_files: u32,
    parsed_with_entries: u32,
    /// Files whose parse hit `unknown_tags` non-empty — i.e. bailed
    /// out before reaching the out-of-range stop.
    files_with_unknown_tags: u32,
    /// Total entries decoded across the corpus (sanity bound).
    total_entries: u64,
}

impl Stats {
    fn coverage_rate(&self) -> f32 {
        if self.total_files == 0 {
            return 0.0;
        }
        let clean_files = self
            .total_files
            .saturating_sub(self.files_with_unknown_tags);
        clean_files as f32 / self.total_files as f32
    }
}

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

fn data_dir(env_var: &str, fallback: &str) -> Option<PathBuf> {
    if let Some(v) = std::env::var(env_var).ok().filter(|s| !s.is_empty()) {
        let p = PathBuf::from(&v);
        if p.exists() {
            return Some(p);
        }
        panic!("{env_var} points to {v:?}, which does not exist");
    }
    let p = PathBuf::from(fallback);
    if p.exists() {
        return Some(p);
    }
    require_game_data(env_var, &p);
    None
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
#[ignore = "needs FNV game data on disk"]
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
#[ignore = "needs FO3 game data on disk"]
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
    assert!(
        stats.total_files > 0,
        "FO3 corpus must contain at least one `.spt`"
    );
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
#[ignore = "needs Oblivion game data on disk"]
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

/// Lower edge of the tail tag bands #3808's dissection found past the
/// walker's stop in every corpus file (14 000 / 15 000 / 16 000 / 18 000 /
/// 19 000 / 20 000 / 21 000 / 22 000). Upper edge keeps float-bit words
/// (≥ 0x3F000000 ≈ 1.06e9) and other payload debris out.
const TAIL_TAG_MIN: u32 = 14_000;
const TAIL_TAG_MAX: u32 = 23_000;

/// Boundary-gate stats for one archive sweep (#4122).
#[derive(Debug, Default)]
struct BoundaryStats {
    total_files: u32,
    reached_eof: u32,
    /// Stops whose word sits in `[TAIL_TAG_MIN, TAIL_TAG_MAX]` — the
    /// walker consumed everything up to the tail's first tag.
    on_boundary: u32,
    /// Stops whose resync shift is 0 (`parser::best_resync_shift`).
    shift_zero: u32,
    /// `(path, stop word, shift)` for every file failing either check.
    violations: Vec<(String, u32, usize)>,
}

/// #4122 — the desync acceptance gate: the walker's stop must sit on the
/// true TLV boundary in every corpus file. Two properties per file:
///
/// 1. the stop word itself is a 14 000–22 000-band tail tag — the walker
///    consumed the parameter section (and the known-tag run that follows
///    it past `TAG_MAX`) up to the first tail tag, not into some
///    mis-sized payload;
/// 2. `best_resync_shift` == 0 — the stop is on the stream's 4-byte grid
///    (implied by 1, but asserted separately so a regression names which
///    property broke).
///
/// Pre-fix this gate failed on 73 of 159 files (the #3808 desync table:
/// 86/159 shift-0), with stop words like `0x3f80` (float bits), `768`
/// (a misaligned payload head) or `0`. The culprits were two dictionary
/// entries — `10002` (stride 1, now 32) and `13013` (7 bytes, now 4).
#[test]
#[ignore = "needs vanilla game data on disk"]
fn walker_stops_on_true_tlv_boundary() {
    let games = [
        (
            "FNV",
            "BYROREDUX_FNV_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            "Fallout - Meshes.bsa",
        ),
        (
            "FO3",
            "BYROREDUX_FO3_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data",
            "Fallout - Meshes.bsa",
        ),
        (
            "OBL",
            "BYROREDUX_OBL_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
            "Oblivion - Meshes.bsa",
        ),
        (
            "SI",
            "BYROREDUX_OBL_DATA",
            "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data",
            "DLCShiveringIsles - Meshes.bsa",
        ),
    ];

    let mut totals = BoundaryStats::default();
    for (label, env_var, fallback, bsa_name) in games {
        let Some(data) = data_dir(env_var, fallback) else {
            eprintln!("[{label}] skip: {env_var} unset and fallback missing");
            continue;
        };
        let archive = BsaArchive::open(data.join(bsa_name)).expect("open BSA");
        let spt_files: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".spt"))
            .map(|f| f.to_string())
            .collect();

        for path in &spt_files {
            let Ok(bytes) = archive.extract(path) else {
                continue;
            };
            let Ok(scene) = parse_spt(&bytes) else {
                continue;
            };
            totals.total_files += 1;
            if scene.reached_eof {
                totals.reached_eof += 1;
                continue;
            }
            let stop = scene.tail_offset;
            let shift = best_resync_shift(&bytes, stop).0;
            let word = if stop + 4 <= bytes.len() {
                Some(u32::from_le_bytes([
                    bytes[stop],
                    bytes[stop + 1],
                    bytes[stop + 2],
                    bytes[stop + 3],
                ]))
            } else {
                None
            };
            let on_boundary = word.is_some_and(|w| (TAIL_TAG_MIN..=TAIL_TAG_MAX).contains(&w));
            if on_boundary {
                totals.on_boundary += 1;
            }
            if shift == 0 {
                totals.shift_zero += 1;
            }
            if !on_boundary || shift != 0 {
                if totals.violations.len() < 12 {
                    totals.violations.push((path.clone(), word.unwrap_or(0), shift));
                }
            }
        }
        eprintln!(
            "[{label}] {} files | {} on boundary | {} shift-0 | {} eof",
            totals.total_files, totals.on_boundary, totals.shift_zero, totals.reached_eof,
        );
    }

    assert!(
        totals.total_files > 0,
        "no `.spt` corpus found — set BYROREDUX_FNV_DATA / _FO3_DATA / _OBL_DATA"
    );
    assert_eq!(
        totals.on_boundary,
        totals.total_files - totals.reached_eof,
        "{} of {} non-EOF stops landed off the true TLV boundary (stop word \
         outside [{}, {}]) — a dictionary entry mis-sizes its payload. \
         Samples (path / stop word / shift): {:?}",
        totals.total_files - totals.reached_eof - totals.on_boundary,
        totals.total_files,
        TAIL_TAG_MIN,
        TAIL_TAG_MAX,
        &totals.violations[..totals.violations.len().min(8)],
    );
    assert_eq!(
        totals.shift_zero,
        totals.total_files - totals.reached_eof,
        "{} of {} non-EOF stops needed a 1-3 byte resync — the walker \
         stopped inside a mis-sized payload. Samples: {:?}",
        totals.total_files - totals.reached_eof - totals.shift_zero,
        totals.total_files,
        &totals.violations[..totals.violations.len().min(8)],
    );
}
