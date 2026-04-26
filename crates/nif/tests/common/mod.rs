//! Shared helpers for integration tests that consume real game content.
//!
//! Game data can't be committed to the repo, so tests driven by this module
//! resolve asset paths from environment variables and skip gracefully when
//! the data isn't available. The CI machine has no game data, so all tests
//! that depend on real NIFs are `#[ignore]` by default and opted into with
//! `cargo test -- --ignored`.
//!
//! ## Environment variables
//!
//! Each variable points at a game's `Data/` directory (or equivalent):
//!
//! | Variable                   | Game              |
//! |----------------------------|-------------------|
//! | `BYROREDUX_OBLIVION_DATA`  | Oblivion          |
//! | `BYROREDUX_FO3_DATA`       | Fallout 3         |
//! | `BYROREDUX_FNV_DATA`       | Fallout New Vegas |
//! | `BYROREDUX_SKYRIMSE_DATA`  | Skyrim SE         |
//!
//! If a variable is unset, the helper falls back to the canonical Steam
//! install path on this machine. Override the variable to point elsewhere.
//!
//! ## Example
//!
//! ```no_run
//! mod common;
//! use common::{Game, game_data_dir};
//!
//! #[test]
//! #[ignore]
//! fn walk_fnv_meshes() {
//!     let Some(data) = game_data_dir(Game::FalloutNV) else { return; };
//!     // ...
//! }
//! ```

#![allow(dead_code)] // Not every helper is used by every test file.

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::blocks::{NiObject, NiUnknown};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Games we have real test content for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Oblivion,
    Fallout3,
    FalloutNV,
    SkyrimSE,
    Fallout4,
    Fallout76,
    Starfield,
}

/// Which archive format the game's mesh archive uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Bsa,
    Ba2,
}

impl Game {
    pub fn env_var(self) -> &'static str {
        match self {
            Game::Oblivion => "BYROREDUX_OBLIVION_DATA",
            Game::Fallout3 => "BYROREDUX_FO3_DATA",
            Game::FalloutNV => "BYROREDUX_FNV_DATA",
            Game::SkyrimSE => "BYROREDUX_SKYRIMSE_DATA",
            Game::Fallout4 => "BYROREDUX_FO4_DATA",
            Game::Fallout76 => "BYROREDUX_FO76_DATA",
            Game::Starfield => "BYROREDUX_STARFIELD_DATA",
        }
    }

    /// Canonical Steam install path on the reference development machine.
    /// Used only as a fallback when the env var is unset — never as a hard
    /// assumption. Tests still skip when neither source resolves.
    pub fn default_path(self) -> PathBuf {
        let base = "/mnt/data/SteamLibrary/steamapps/common";
        match self {
            Game::Oblivion => PathBuf::from(format!("{base}/Oblivion/Data")),
            Game::Fallout3 => PathBuf::from(format!("{base}/Fallout 3 goty/Data")),
            Game::FalloutNV => PathBuf::from(format!("{base}/Fallout New Vegas/Data")),
            Game::SkyrimSE => PathBuf::from(format!("{base}/Skyrim Special Edition/Data")),
            Game::Fallout4 => PathBuf::from(format!("{base}/Fallout 4/Data")),
            Game::Fallout76 => PathBuf::from(format!("{base}/Fallout76/Data")),
            Game::Starfield => PathBuf::from(format!("{base}/Starfield/Data")),
        }
    }

    /// Typical filename of the primary mesh archive.
    pub fn mesh_archive(self) -> &'static str {
        match self {
            Game::Oblivion => "Oblivion - Meshes.bsa",
            Game::Fallout3 => "Fallout - Meshes.bsa",
            Game::FalloutNV => "Fallout - Meshes.bsa",
            Game::SkyrimSE => "Skyrim - Meshes0.bsa",
            Game::Fallout4 => "Fallout4 - Meshes.ba2",
            Game::Fallout76 => "SeventySix - Meshes.ba2",
            Game::Starfield => "Starfield - Meshes01.ba2",
        }
    }

    pub fn archive_kind(self) -> ArchiveKind {
        match self {
            Game::Oblivion | Game::Fallout3 | Game::FalloutNV | Game::SkyrimSE => ArchiveKind::Bsa,
            Game::Fallout4 | Game::Fallout76 | Game::Starfield => ArchiveKind::Ba2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Game::Oblivion => "Oblivion",
            Game::Fallout3 => "Fallout 3",
            Game::FalloutNV => "Fallout New Vegas",
            Game::SkyrimSE => "Skyrim SE",
            Game::Fallout4 => "Fallout 4",
            Game::Fallout76 => "Fallout 76",
            Game::Starfield => "Starfield",
        }
    }
}

/// Either a BSA or a BA2 archive, exposing a unified file-list / extract API
/// so test code doesn't need to branch on the format.
pub enum MeshArchive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl MeshArchive {
    pub fn file_count(&self) -> usize {
        match self {
            MeshArchive::Bsa(a) => a.file_count(),
            MeshArchive::Ba2(a) => a.file_count(),
        }
    }

    pub fn list_files(&self) -> Vec<String> {
        match self {
            MeshArchive::Bsa(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
            MeshArchive::Ba2(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn extract(&self, path: &str) -> std::io::Result<Vec<u8>> {
        match self {
            MeshArchive::Bsa(a) => a.extract(path),
            MeshArchive::Ba2(a) => a.extract(path),
        }
    }
}

/// Resolve the data directory for a game, falling back to the default Steam
/// path. Returns `None` and prints a skip notice if neither resolves.
pub fn game_data_dir(game: Game) -> Option<PathBuf> {
    if let Ok(val) = std::env::var(game.env_var()) {
        let path = PathBuf::from(val);
        if path.is_dir() {
            return Some(path);
        }
        eprintln!(
            "[{}] {} points to {:?} which is not a directory; falling back to default",
            game.label(),
            game.env_var(),
            path
        );
    }
    let default = game.default_path();
    if default.is_dir() {
        return Some(default);
    }
    eprintln!(
        "[{}] skipping: no data dir (set {} or install to {:?})",
        game.label(),
        game.env_var(),
        default
    );
    None
}

/// Open the primary mesh archive for a game if present. Picks BSA or BA2
/// based on the game's `archive_kind`.
pub fn open_mesh_archive(game: Game) -> Option<MeshArchive> {
    let data = game_data_dir(game)?;
    let archive_path = data.join(game.mesh_archive());
    if !archive_path.is_file() {
        eprintln!("[{}] skipping: {:?} not found", game.label(), archive_path);
        return None;
    }
    let result = match game.archive_kind() {
        ArchiveKind::Bsa => BsaArchive::open(&archive_path).map(MeshArchive::Bsa),
        ArchiveKind::Ba2 => Ba2Archive::open(&archive_path).map(MeshArchive::Ba2),
    };
    match result {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!(
                "[{}] skipping: failed to open {:?}: {}",
                game.label(),
                archive_path,
                e
            );
            None
        }
    }
}

/// Per-file parse outcome: a parse that returned `Err`, or a parse
/// that returned `Ok` but with a scene that aborted mid-file. See #393.
#[derive(Debug)]
pub enum ParseStatus {
    /// `parse_nif` returned `Ok` and the scene graph is complete.
    Clean { block_count: usize },
    /// `parse_nif` returned `Ok` but `scene.truncated == true` — one
    /// or more blocks were dropped because the parser bailed before
    /// reading every block. `dropped` counts the missing entries.
    /// Counts as a **failure** for the rate metric.
    Truncated { block_count: usize, dropped: usize },
    /// `parse_nif` returned `Err`.
    Failed(String),
}

#[derive(Debug)]
pub struct ParseOutcome {
    pub path: String,
    pub status: ParseStatus,
}

impl ParseOutcome {
    pub fn is_clean(&self) -> bool {
        matches!(self.status, ParseStatus::Clean { .. })
    }
}

/// Parse statistics across a batch of NIFs.
///
/// `success_rate()` is now the **clean-parse** rate — truncated scenes
/// count as failures. A truncated scene is a silent data loss (root
/// NiNode may be missing, descendants unreachable), not a recoverable
/// parse (#393). Secondary counters are surfaced by `print_summary`
/// for diagnostics.
#[derive(Debug, Default)]
pub struct ParseStats {
    pub total: usize,
    /// Clean parses (Ok + not truncated).
    pub clean: usize,
    /// Truncated scenes: parse returned Ok but dropped at least one
    /// block. Kept separately so consumers can track recovery rate.
    pub truncated: Vec<ParseOutcome>,
    /// Hard parse failures (parse_nif returned Err).
    pub failures: Vec<ParseOutcome>,
}

impl ParseStats {
    /// Clean-parse rate: clean / total. Treats truncated as failure
    /// because geometry is silently missing in that case.
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.clean as f64 / self.total as f64
    }

    /// Recoverable rate: (clean + truncated) / total. Used as a
    /// secondary metric — a NIF that truncates still yields some
    /// renderable blocks; losing zero files to hard parse errors is
    /// what this tracks.
    pub fn recoverable_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        (self.clean + self.truncated.len()) as f64 / self.total as f64
    }

    pub fn record(&mut self, outcome: ParseOutcome) {
        self.total += 1;
        match &outcome.status {
            ParseStatus::Clean { .. } => self.clean += 1,
            ParseStatus::Truncated { .. } => self.truncated.push(outcome),
            ParseStatus::Failed(_) => self.failures.push(outcome),
        }
    }

    /// Test helper: inject a synthetic outcome without touching real
    /// NIF data. Used by the metric-split regression tests in this
    /// module.
    #[cfg(test)]
    fn record_status(&mut self, path: &str, status: ParseStatus) {
        self.record(ParseOutcome {
            path: path.to_string(),
            status,
        });
    }

    pub fn print_summary(&self, label: &str) {
        let clean_rate = self.success_rate() * 100.0;
        let recoverable_rate = self.recoverable_rate() * 100.0;
        eprintln!(
            "[{label}] parsed {}/{} NIFs: clean {:.2}% ({} clean / {} truncated / {} failed), recoverable {:.2}%",
            self.clean + self.truncated.len(),
            self.total,
            clean_rate,
            self.clean,
            self.truncated.len(),
            self.failures.len(),
            recoverable_rate,
        );
        // Print up to 3 truncated examples — these hide silent data loss.
        for outcome in self.truncated.iter().take(3) {
            if let ParseStatus::Truncated {
                block_count,
                dropped,
            } = &outcome.status
            {
                eprintln!(
                    "  TRUNC {}: kept {} blocks, dropped {}",
                    outcome.path, block_count, dropped
                );
            }
        }
        if self.truncated.len() > 3 {
            eprintln!("  ... and {} more truncated", self.truncated.len() - 3);
        }
        // Print up to 5 hard failure examples.
        for outcome in self.failures.iter().take(5) {
            if let ParseStatus::Failed(e) = &outcome.status {
                eprintln!("  FAIL {}: {}", outcome.path, e);
            }
        }
        if self.failures.len() > 5 {
            eprintln!("  ... and {} more failures", self.failures.len() - 5);
        }
    }
}

/// Parse every NIF inside a mesh archive (BSA or BA2) and collect stats.
/// If `limit` is `Some(n)`, only the first `n` NIFs are parsed.
pub fn parse_all_nifs_in_archive(archive: &MeshArchive, limit: Option<usize>) -> ParseStats {
    let mut stats = ParseStats::default();
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .collect();

    let iter: Box<dyn Iterator<Item = &String>> = match limit {
        Some(n) => Box::new(files.iter().take(n)),
        None => Box::new(files.iter()),
    };

    for path in iter {
        let outcome = match archive.extract(path) {
            Err(e) => ParseOutcome {
                path: path.clone(),
                status: ParseStatus::Failed(format!("extract: {e}")),
            },
            Ok(bytes) => match byroredux_nif::parse_nif(&bytes) {
                Ok(scene) => {
                    // #568 — a non-zero `recovered_blocks` means at
                    // least one block was replaced with NiUnknown via
                    // the parse-loop recovery path (block_size seek,
                    // runtime size cache, oblivion_skip_sizes hint, or
                    // dispatch-level unknown-type fallback). Treat the
                    // NIF as non-clean so regressions like #546 turn
                    // the parse-rate gate red rather than hiding.
                    let status = if scene.truncated || scene.recovered_blocks > 0 {
                        ParseStatus::Truncated {
                            block_count: scene.len(),
                            dropped: scene.dropped_block_count + scene.recovered_blocks,
                        }
                    } else {
                        ParseStatus::Clean {
                            block_count: scene.len(),
                        }
                    };
                    ParseOutcome {
                        path: path.clone(),
                        status,
                    }
                }
                Err(e) => ParseOutcome {
                    path: path.clone(),
                    status: ParseStatus::Failed(format!("parse: {e}")),
                },
            },
        };
        stats.record(outcome);
    }

    stats
}

/// Parse every .nif under a directory (recursively) and collect stats.
pub fn parse_all_nifs_in_dir(root: &Path, limit: Option<usize>) -> ParseStats {
    let mut stats = ParseStats::default();
    let mut stack = vec![root.to_path_buf()];
    let mut count = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let lower = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if lower != "nif" {
                continue;
            }
            if let Some(n) = limit {
                if count >= n {
                    return stats;
                }
            }
            count += 1;
            let display = path.display().to_string();
            let outcome = match std::fs::read(&path) {
                Err(e) => ParseOutcome {
                    path: display,
                    status: ParseStatus::Failed(format!("read: {e}")),
                },
                Ok(bytes) => match byroredux_nif::parse_nif(&bytes) {
                    Ok(scene) => {
                        // #568 — see sibling site for full rationale.
                        let status = if scene.truncated || scene.recovered_blocks > 0 {
                            ParseStatus::Truncated {
                                block_count: scene.len(),
                                dropped: scene.dropped_block_count + scene.recovered_blocks,
                            }
                        } else {
                            ParseStatus::Clean {
                                block_count: scene.len(),
                            }
                        };
                        ParseOutcome {
                            path: display,
                            status,
                        }
                    }
                    Err(e) => ParseOutcome {
                        path: display,
                        status: ParseStatus::Failed(format!("parse: {e}")),
                    },
                },
            };
            stats.record(outcome);
        }
    }
    stats
}

/// Per-block-type counts. `parsed` is dispatch-table success; `unknown`
/// is the count of blocks that landed on the `NiUnknown` recovery path
/// while still advertising this type in the header.
///
/// The R3 regression signal is `parsed > 0 && unknown > 0` for the same
/// type, or any growth in `unknown` against a checked-in baseline.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BlockCounts {
    pub parsed: usize,
    pub unknown: usize,
}

/// Per-header-type histogram across a corpus of parsed scenes.
///
/// Attribution rule: every block in the scene is keyed by its
/// **header-advertised type name** (i.e. the value the writer put in
/// the block-type table). When that block downcasts to `NiUnknown` it
/// contributes to `unknown[type_name]`; when it dispatched to a real
/// parser it contributes to `parsed[block_type_name()]`. Both share a
/// key, so a regressed parser that starts under-consuming a previously
/// well-handled type shows up as `parsed: N->N-k, unknown: 0->k` for
/// the same key — exactly the signal the per-block baseline test
/// gates on.
#[derive(Debug, Default, Clone)]
pub struct PerBlockHistogram {
    pub counts: BTreeMap<String, BlockCounts>,
}

impl PerBlockHistogram {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_scene_blocks<'a>(&mut self, blocks: impl Iterator<Item = &'a Box<dyn NiObject>>) {
        for block in blocks {
            if let Some(unknown) = block.as_any().downcast_ref::<NiUnknown>() {
                self.counts
                    .entry(unknown.type_name.as_ref().to_string())
                    .or_default()
                    .unknown += 1;
            } else {
                self.counts
                    .entry(block.block_type_name().to_string())
                    .or_default()
                    .parsed += 1;
            }
        }
    }

    /// Total number of blocks that landed in the `NiUnknown` recovery
    /// path across every type. Useful one-line health metric.
    pub fn total_unknown(&self) -> usize {
        self.counts.values().map(|c| c.unknown).sum()
    }

    /// Number of types where dispatch sometimes succeeded and sometimes
    /// fell into the recovery path. Most direct R3 signal.
    pub fn types_with_partial_unknown(&self) -> usize {
        self.counts
            .values()
            .filter(|c| c.parsed > 0 && c.unknown > 0)
            .count()
    }

    /// Serialise to the canonical TSV format consumed by the baseline
    /// regression test. Stable across runs (BTreeMap is sorted by key).
    /// First line is a `# header total=N` line so checked-in baselines
    /// stay hand-readable.
    pub fn to_tsv(&self, total_files: usize) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "# nif_stats per-block histogram\ttotal={}\n",
            total_files
        ));
        for (name, counts) in &self.counts {
            out.push_str(&format!(
                "{}\t{}\t{}\n",
                name, counts.parsed, counts.unknown
            ));
        }
        out
    }

    /// Parse a TSV produced by [`Self::to_tsv`]. Lines beginning with
    /// `#` are header/metadata and skipped. Malformed rows produce an
    /// `Err` so a corrupt baseline file fails loud rather than silently
    /// passing the regression gate.
    pub fn from_tsv(text: &str) -> Result<Self, String> {
        let mut hist = Self::new();
        for (lineno, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split('\t');
            let name = parts
                .next()
                .ok_or_else(|| format!("line {}: missing type name", lineno + 1))?;
            let parsed: usize = parts
                .next()
                .ok_or_else(|| format!("line {}: missing parsed count", lineno + 1))?
                .parse()
                .map_err(|e| format!("line {}: parsed count: {}", lineno + 1, e))?;
            let unknown: usize = parts
                .next()
                .ok_or_else(|| format!("line {}: missing unknown count", lineno + 1))?
                .parse()
                .map_err(|e| format!("line {}: unknown count: {}", lineno + 1, e))?;
            hist.counts
                .insert(name.to_string(), BlockCounts { parsed, unknown });
        }
        Ok(hist)
    }
}

/// One regression-rule violation between current and baseline counts.
#[derive(Debug)]
pub enum BaselineRegression {
    /// `unknown` count went up — a parser that used to handle this
    /// type started under-consuming. The strongest R3 signal.
    UnknownGrew {
        type_name: String,
        baseline: usize,
        current: usize,
    },
    /// `parsed` count went down — fewer instances are dispatching to
    /// the real parser. Could mean the type is being filtered out, the
    /// archive content shifted, or a regression that drops blocks
    /// before classification.
    ParsedShrank {
        type_name: String,
        baseline: usize,
        current: usize,
    },
}

/// Compare a freshly-built histogram against a baseline. Improvements
/// (current `unknown` < baseline, or current `parsed` > baseline) are
/// silent — regenerate the baseline when fixing a parser. Returns one
/// entry per regressed type. New types absent from the baseline are
/// always accepted (they can only add coverage).
pub fn compare_histograms(
    current: &PerBlockHistogram,
    baseline: &PerBlockHistogram,
) -> Vec<BaselineRegression> {
    let mut regressions = Vec::new();
    for (name, base_counts) in &baseline.counts {
        let cur_counts = current.counts.get(name).copied().unwrap_or_default();
        if cur_counts.unknown > base_counts.unknown {
            regressions.push(BaselineRegression::UnknownGrew {
                type_name: name.clone(),
                baseline: base_counts.unknown,
                current: cur_counts.unknown,
            });
        }
        if cur_counts.parsed < base_counts.parsed {
            regressions.push(BaselineRegression::ParsedShrank {
                type_name: name.clone(),
                baseline: base_counts.parsed,
                current: cur_counts.parsed,
            });
        }
    }
    regressions
}

/// Walk every NIF in a mesh archive, building both `ParseStats` and a
/// `PerBlockHistogram` in one pass. Returns the pair so the
/// per-block-baseline test can also surface file-level health
/// (clean / truncated / failed) for context when a regression is
/// reported.
pub fn parse_archive_with_histogram(
    archive: &MeshArchive,
    limit: Option<usize>,
) -> (ParseStats, PerBlockHistogram) {
    let mut stats = ParseStats::default();
    let mut hist = PerBlockHistogram::new();
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .collect();

    let iter: Box<dyn Iterator<Item = &String>> = match limit {
        Some(n) => Box::new(files.iter().take(n)),
        None => Box::new(files.iter()),
    };

    for path in iter {
        let outcome = match archive.extract(path) {
            Err(e) => ParseOutcome {
                path: path.clone(),
                status: ParseStatus::Failed(format!("extract: {e}")),
            },
            Ok(bytes) => match byroredux_nif::parse_nif(&bytes) {
                Ok(scene) => {
                    hist.record_scene_blocks(scene.blocks.iter());
                    let status = if scene.truncated || scene.recovered_blocks > 0 {
                        ParseStatus::Truncated {
                            block_count: scene.len(),
                            dropped: scene.dropped_block_count + scene.recovered_blocks,
                        }
                    } else {
                        ParseStatus::Clean {
                            block_count: scene.len(),
                        }
                    };
                    ParseOutcome {
                        path: path.clone(),
                        status,
                    }
                }
                Err(e) => ParseOutcome {
                    path: path.clone(),
                    status: ParseStatus::Failed(format!("parse: {e}")),
                },
            },
        };
        stats.record(outcome);
    }

    (stats, hist)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression #393: truncated scenes must NOT count toward the
    // clean-parse rate. They are silent data loss (one or more blocks
    // dropped) and their prior classification as "success" hid a ~9%
    // real failure rate on Oblivion content.

    #[test]
    fn clean_rate_counts_truncated_as_failure() {
        let mut stats = ParseStats::default();
        stats.record_status("a.nif", ParseStatus::Clean { block_count: 10 });
        stats.record_status("b.nif", ParseStatus::Clean { block_count: 12 });
        stats.record_status(
            "c.nif",
            ParseStatus::Truncated {
                block_count: 3,
                dropped: 37,
            },
        );
        stats.record_status("d.nif", ParseStatus::Failed("parse: oops".into()));

        assert_eq!(stats.total, 4);
        assert_eq!(stats.clean, 2);
        assert_eq!(stats.truncated.len(), 1);
        assert_eq!(stats.failures.len(), 1);
        // clean / total = 2/4 = 0.5
        assert!((stats.success_rate() - 0.5).abs() < 1e-6);
        // recoverable = (clean + truncated) / total = 3/4 = 0.75
        assert!((stats.recoverable_rate() - 0.75).abs() < 1e-6);
    }

    #[test]
    fn empty_stats_report_perfect_rates() {
        let stats = ParseStats::default();
        assert_eq!(stats.success_rate(), 1.0);
        assert_eq!(stats.recoverable_rate(), 1.0);
    }

    #[test]
    fn all_truncated_gives_zero_clean_one_hundred_recoverable() {
        let mut stats = ParseStats::default();
        for i in 0..10 {
            stats.record_status(
                &format!("t{i}.nif"),
                ParseStatus::Truncated {
                    block_count: 0,
                    dropped: 1,
                },
            );
        }
        assert_eq!(stats.success_rate(), 0.0);
        assert_eq!(stats.recoverable_rate(), 1.0);
    }

    fn hist_of(pairs: &[(&str, usize, usize)]) -> PerBlockHistogram {
        let mut h = PerBlockHistogram::new();
        for (name, parsed, unknown) in pairs {
            h.counts.insert(
                (*name).to_string(),
                BlockCounts {
                    parsed: *parsed,
                    unknown: *unknown,
                },
            );
        }
        h
    }

    #[test]
    fn histogram_tsv_roundtrips() {
        let h = hist_of(&[("NiNode", 100, 0), ("NiTransformData", 40, 2)]);
        let tsv = h.to_tsv(7);
        let parsed = PerBlockHistogram::from_tsv(&tsv).expect("roundtrip");
        assert_eq!(parsed.counts, h.counts);
    }

    #[test]
    fn histogram_tsv_skips_comments_and_blanks() {
        let tsv = "# header total=3\n\nNiNode\t10\t0\n";
        let parsed = PerBlockHistogram::from_tsv(tsv).expect("parse");
        assert_eq!(parsed.counts.len(), 1);
        assert_eq!(
            parsed.counts.get("NiNode").copied().unwrap(),
            BlockCounts {
                parsed: 10,
                unknown: 0
            }
        );
    }

    #[test]
    fn histogram_tsv_rejects_malformed_rows() {
        // Missing the unknown column must fail loud — a corrupt
        // baseline must not silently pass the regression gate.
        let tsv = "NiNode\t10\n";
        assert!(PerBlockHistogram::from_tsv(tsv).is_err());
    }

    // R3 regression-detection rules. Improvements (current better than
    // baseline) are silent — the baseline is regenerated when fixing a
    // parser. Regressions are surfaced both ways: unknown growth and
    // parsed shrinkage, since either points at a parser bug.

    #[test]
    fn unknown_growth_is_a_regression() {
        let baseline = hist_of(&[("NiTransformData", 40623, 0)]);
        let current = hist_of(&[("NiTransformData", 40615, 8)]);
        let regs = compare_histograms(&current, &baseline);
        assert_eq!(regs.len(), 2);
        assert!(matches!(
            regs[0],
            BaselineRegression::UnknownGrew { ref type_name, baseline: 0, current: 8 }
                if type_name == "NiTransformData"
        ));
        assert!(matches!(
            regs[1],
            BaselineRegression::ParsedShrank { ref type_name, baseline: 40623, current: 40615 }
                if type_name == "NiTransformData"
        ));
    }

    #[test]
    fn improvement_is_silent() {
        let baseline = hist_of(&[("bhkRigidBody", 0, 5000)]);
        let current = hist_of(&[("bhkRigidBody", 5000, 0)]);
        assert!(compare_histograms(&current, &baseline).is_empty());
    }

    #[test]
    fn types_new_in_current_are_silent() {
        let baseline = hist_of(&[("NiNode", 10, 0)]);
        let current = hist_of(&[("NiNode", 10, 0), ("NiNewBlock", 3, 0)]);
        assert!(compare_histograms(&current, &baseline).is_empty());
    }

    #[test]
    fn type_disappearing_from_current_is_a_regression() {
        // Baseline says NiTransformData parsed 100; current parses 0.
        // current.counts has no entry for NiTransformData → defaults to
        // (0, 0), which compares as ParsedShrank.
        let baseline = hist_of(&[("NiTransformData", 100, 0)]);
        let current = hist_of(&[]);
        let regs = compare_histograms(&current, &baseline);
        assert_eq!(regs.len(), 1);
        assert!(matches!(
            regs[0],
            BaselineRegression::ParsedShrank { ref type_name, baseline: 100, current: 0 }
                if type_name == "NiTransformData"
        ));
    }
}
