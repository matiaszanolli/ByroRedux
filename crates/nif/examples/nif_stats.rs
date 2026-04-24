//! nif_stats — walk a NIF source and report parse statistics.
//!
//! Usage:
//!   cargo run -p byroredux-nif --example nif_stats -- <path> [flags]
//!
//! `<path>` may be:
//!   - a single `.nif` file
//!   - a directory containing `.nif` files (recursed)
//!   - a `.bsa` archive (all internal `.nif` entries are extracted)
//!
//! Output flags:
//!   `--tsv`           Emit machine-readable per-type histogram on stdout
//!                     (`<type>\t<parsed>\t<unknown>`). Suppresses the
//!                     human-readable summary. Used as the source of
//!                     truth by the per-block-baseline regression test.
//!   `--unknown-only`  In the human-readable summary, only show types
//!                     where `unknown > 0` — i.e. types that the
//!                     dispatch table claims to know but that landed on
//!                     the recovery path on at least one instance.
//!                     Highlights regressions; suppresses the bulk of
//!                     fully-parsed type rows.
//!
//! Per-block-type histogram (R3 — `parsed` vs `unknown`):
//!   Each block in the scene is attributed to its **header-advertised**
//!   type name, not its parsed Rust type. When dispatch succeeds, that
//!   block contributes to `parsed`. When it falls into the `NiUnknown`
//!   recovery path (under-consumed parser via `block_size` seek, runtime
//!   size cache, `oblivion_skip_sizes` hint, or dispatch-table miss),
//!   it contributes to `unknown`. A type with `parsed=N>0, unknown=M>0`
//!   is the regression signal R3 cares about: dispatch can parse this
//!   type, but at least one instance in the corpus failed.
//!
//! Exit code is non-zero when parse success rate drops below 100% (the
//! vanilla-content commitment per ROADMAP). Override with
//! `NIF_STATS_MIN_SUCCESS_RATE=<0.0..=1.0>` for modded content where
//! partial coverage is expected.

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::blocks::NiUnknown;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Default success rate gate. All 7 supported games ship at 100%
/// (ROADMAP "Full-archive parse rates: ALL 7 games at 100%") — any drop
/// is a vanilla regression. Override via `NIF_STATS_MIN_SUCCESS_RATE`
/// env var when running against modded or unknown content.
///
/// See issue #487 for the gate-tightening rationale.
const DEFAULT_MIN_SUCCESS_RATE: f64 = 1.0;

fn min_success_rate() -> f64 {
    std::env::var("NIF_STATS_MIN_SUCCESS_RATE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|r| (0.0..=1.0).contains(r))
        .unwrap_or(DEFAULT_MIN_SUCCESS_RATE)
}

/// Per-block-type counts. `parsed` is dispatch-table success; `unknown`
/// is the count of blocks that landed on the `NiUnknown` recovery path
/// while still advertising this type in the header.
#[derive(Debug, Default, Clone, Copy)]
struct BlockCounts {
    parsed: usize,
    unknown: usize,
}

impl BlockCounts {
    fn total(&self) -> usize {
        self.parsed + self.unknown
    }
}

struct Stats {
    total: usize,
    /// Parses that returned Ok with a complete scene graph.
    clean: usize,
    /// Parses that returned Ok but with `scene.truncated == true`.
    /// Tracked separately from `clean` because they represent silent
    /// data loss (one or more blocks dropped). See #393.
    truncated: usize,
    /// Sum of `dropped_block_count` across every truncated scene —
    /// gives a rough "blocks lost" telemetry figure.
    dropped_blocks: usize,
    /// Examples of truncated files (path, dropped count), capped for
    /// the summary output.
    truncated_examples: Vec<(String, usize)>,
    /// Per-header-type histogram with `parsed` vs `unknown` split.
    block_histogram: BTreeMap<String, BlockCounts>,
    /// Grouped by the first line of the error message.
    failure_groups: BTreeMap<String, Vec<String>>,
}

impl Stats {
    fn new() -> Self {
        Self {
            total: 0,
            clean: 0,
            truncated: 0,
            dropped_blocks: 0,
            truncated_examples: Vec::new(),
            block_histogram: BTreeMap::new(),
            failure_groups: BTreeMap::new(),
        }
    }

    /// Walk a parsed scene and accumulate per-header-type counts. A
    /// block that downcasts to `NiUnknown` contributes to `unknown[its
    /// preserved type_name]`; otherwise it contributes to
    /// `parsed[block_type_name()]`. Centralised here so success and
    /// truncated paths share identical attribution logic.
    fn record_blocks<'a>(
        &mut self,
        blocks: impl Iterator<Item = &'a Box<dyn byroredux_nif::blocks::NiObject>>,
    ) {
        for block in blocks {
            let entry = match block.as_any().downcast_ref::<NiUnknown>() {
                Some(unknown) => {
                    let key = unknown.type_name.as_ref();
                    let bucket = self.block_histogram.entry(key.to_string()).or_default();
                    bucket.unknown += 1;
                    continue;
                }
                None => self
                    .block_histogram
                    .entry(block.block_type_name().to_string())
                    .or_default(),
            };
            entry.parsed += 1;
        }
    }

    fn record_success<'a>(
        &mut self,
        blocks: impl Iterator<Item = &'a Box<dyn byroredux_nif::blocks::NiObject>>,
    ) {
        self.total += 1;
        self.clean += 1;
        self.record_blocks(blocks);
    }

    /// A truncated scene still contributes block histogram data (the
    /// partial parse is useful for telemetry), but does NOT count
    /// toward the clean-parse rate used by the exit-code gate. See #393.
    fn record_truncated<'a>(
        &mut self,
        path: String,
        dropped: usize,
        blocks: impl Iterator<Item = &'a Box<dyn byroredux_nif::blocks::NiObject>>,
    ) {
        self.total += 1;
        self.truncated += 1;
        self.dropped_blocks += dropped;
        if self.truncated_examples.len() < 20 {
            self.truncated_examples.push((path, dropped));
        }
        self.record_blocks(blocks);
    }

    fn record_failure(&mut self, path: String, err: String) {
        self.total += 1;
        // Group errors by their first line — avoids per-file message noise.
        let group_key = err.lines().next().unwrap_or(&err).to_string();
        self.failure_groups.entry(group_key).or_default().push(path);
    }

    /// Clean-parse rate — the exit-code gate metric. Truncated files
    /// count as failures because they represent silent data loss.
    fn success_rate(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            self.clean as f64 / self.total as f64
        }
    }

    /// Sum of `unknown` counts across every type. This is the
    /// per-block-type recovery surface, distinct from the file-level
    /// clean/truncated/failed counters above.
    fn total_unknown_blocks(&self) -> usize {
        self.block_histogram.values().map(|c| c.unknown).sum()
    }

    /// Number of types where dispatch succeeded for some instances but
    /// the recovery path absorbed others. Most direct R3 signal.
    fn types_with_partial_unknown(&self) -> usize {
        self.block_histogram
            .values()
            .filter(|c| c.parsed > 0 && c.unknown > 0)
            .count()
    }

    fn print(&self, unknown_only: bool) {
        let failures = self.total - self.clean - self.truncated;
        println!();
        println!("─── Parse stats ──────────────────────────────────────────────");
        println!("  total:     {:>6}", self.total);
        println!(
            "  clean:     {:>6}  ({:.2}%)",
            self.clean,
            self.success_rate() * 100.0
        );
        println!(
            "  truncated: {:>6}  ({} blocks dropped)",
            self.truncated, self.dropped_blocks
        );
        println!("  failures:  {:>6}", failures);
        println!(
            "  recovered: {:>6}  ({} types with partial unknown)",
            self.total_unknown_blocks(),
            self.types_with_partial_unknown()
        );

        if !self.block_histogram.is_empty() {
            // Sort by total descending for the human-readable summary;
            // ties broken by parsed-only descending so well-behaved
            // types group together. The TSV mode keeps the BTreeMap's
            // alphabetical order — that's stable across runs and
            // diff-friendly for the baseline regression test.
            let mut sorted: Vec<(&String, &BlockCounts)> =
                self.block_histogram.iter().collect();
            sorted.sort_by(|a, b| {
                b.1.total()
                    .cmp(&a.1.total())
                    .then_with(|| b.1.parsed.cmp(&a.1.parsed))
            });

            // Always print the regression-signal block first: types
            // where dispatch sometimes succeeds and sometimes falls
            // into the recovery path.
            let partial: Vec<&(&String, &BlockCounts)> = sorted
                .iter()
                .filter(|(_, c)| c.parsed > 0 && c.unknown > 0)
                .collect();
            if !partial.is_empty() {
                println!();
                println!(
                    "─── Types with partial unknown (regression signals) ───────────"
                );
                println!("  {:>8} {:>8}  {}", "parsed", "unknown", "type");
                for (name, counts) in &partial {
                    println!(
                        "  {:>8} {:>8}  {}",
                        counts.parsed, counts.unknown, name
                    );
                }
            }

            // Pure-unknown types: dispatch table doesn't know them at
            // all. Not regressions — usually new types or legacy
            // edge-cases — but useful telemetry for parser priorities.
            let pure_unknown: Vec<&(&String, &BlockCounts)> = sorted
                .iter()
                .filter(|(_, c)| c.parsed == 0 && c.unknown > 0)
                .collect();
            if !pure_unknown.is_empty() {
                println!();
                println!("─── Unparsed types (no dispatch entry) ────────────────────────");
                println!("  {:>8}  {}", "unknown", "type");
                for (name, counts) in pure_unknown.iter().take(20) {
                    println!("  {:>8}  {}", counts.unknown, name);
                }
                if pure_unknown.len() > 20 {
                    println!("  ... and {} more pure-unknown types", pure_unknown.len() - 20);
                }
            }

            if !unknown_only {
                println!();
                println!("─── Block type histogram (top 20 by total) ────────────────────");
                println!(
                    "  {:>8} {:>8}  {}",
                    "parsed", "unknown", "type"
                );
                for (name, counts) in sorted.iter().take(20) {
                    println!(
                        "  {:>8} {:>8}  {}",
                        counts.parsed, counts.unknown, name
                    );
                }
                println!("  ({} distinct block types)", sorted.len());
            }
        }

        if !self.truncated_examples.is_empty() {
            println!();
            println!("─── Truncated scenes (sample) ─────────────────────────────────");
            for (path, dropped) in &self.truncated_examples {
                println!("  dropped {} blocks  {}", dropped, path);
            }
            if self.truncated > self.truncated_examples.len() {
                println!(
                    "  ... and {} more truncated",
                    self.truncated - self.truncated_examples.len()
                );
            }
        }

        if !self.failure_groups.is_empty() {
            println!();
            println!("─── Failure groups ────────────────────────────────────────────");
            let mut groups: Vec<(&String, &Vec<String>)> = self.failure_groups.iter().collect();
            groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
            for (msg, paths) in groups {
                println!("  ({} files) {}", paths.len(), msg);
                for p in paths.iter().take(3) {
                    println!("    - {}", p);
                }
                if paths.len() > 3 {
                    println!("    ... and {} more", paths.len() - 3);
                }
            }
        }
    }

    /// Emit `<type>\t<parsed>\t<unknown>` per line in the BTreeMap's
    /// alphabetical order. Stable across runs — used by the per-block
    /// baseline regression test as the comparison source. Header line
    /// (`# nif_stats per-block histogram, total=N files`) makes
    /// hand-inspection of checked-in baselines easier.
    fn print_tsv(&self) {
        println!(
            "# nif_stats per-block histogram\ttotal={}\tclean={}\ttruncated={}",
            self.total, self.clean, self.truncated
        );
        for (name, counts) in &self.block_histogram {
            println!("{}\t{}\t{}", name, counts.parsed, counts.unknown);
        }
    }
}

fn process_bytes(stats: &mut Stats, label: String, bytes: &[u8]) {
    match parse_nif(bytes) {
        Ok(scene) => {
            // #568 — a non-zero `recovered_blocks` means at least one
            // block fell into the NiUnknown recovery path (parser
            // misalignment like #546, or an unknown dispatch type).
            // Route through `record_truncated` so the clean-parse gate
            // doesn't hide these. `dropped_block_count` is authoritative
            // for genuine truncations; we add `recovered_blocks` to it
            // so the telemetry line reports every lost / placeholdered
            // block in one figure.
            let non_clean_blocks = scene.dropped_block_count + scene.recovered_blocks;
            if scene.truncated || scene.recovered_blocks > 0 {
                stats.record_truncated(label, non_clean_blocks, scene.blocks.iter());
            } else {
                stats.record_success(scene.blocks.iter());
            }
        }
        Err(e) => {
            stats.record_failure(label, e.to_string());
        }
    }
}

fn process_file(stats: &mut Stats, path: &Path) {
    match std::fs::read(path) {
        Err(e) => {
            stats.record_failure(path.display().to_string(), format!("read: {e}"));
        }
        Ok(bytes) => {
            process_bytes(stats, path.display().to_string(), &bytes);
        }
    }
}

fn process_dir(stats: &mut Stats, root: &Path) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("skipping unreadable directory: {:?}", dir);
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let is_nif = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("nif"))
                .unwrap_or(false);
            if is_nif {
                process_file(stats, &path);
            }
        }
    }
}

fn process_bsa(stats: &mut Stats, path: &Path) -> Result<(), String> {
    let archive = BsaArchive::open(path).map_err(|e| format!("open BSA: {e}"))?;
    eprintln!("opened {} ({} files)", path.display(), archive.file_count());
    let nif_files: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("  → {} .nif entries", nif_files.len());
    for (i, nif_path) in nif_files.iter().enumerate() {
        if i > 0 && i.is_multiple_of(500) {
            eprintln!("  progress: {}/{}", i, nif_files.len());
        }
        match archive.extract(nif_path) {
            Ok(bytes) => process_bytes(stats, nif_path.clone(), &bytes),
            Err(e) => stats.record_failure(nif_path.clone(), format!("extract: {e}")),
        }
    }
    Ok(())
}

fn process_ba2(stats: &mut Stats, path: &Path) -> Result<(), String> {
    let archive = Ba2Archive::open(path).map_err(|e| format!("open BA2: {e}"))?;
    eprintln!(
        "opened {} (BA2 v{} {:?}, {} files)",
        path.display(),
        archive.version(),
        archive.variant(),
        archive.file_count()
    );
    let nif_files: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("  → {} .nif entries", nif_files.len());
    for (i, nif_path) in nif_files.iter().enumerate() {
        if i > 0 && i.is_multiple_of(500) {
            eprintln!("  progress: {}/{}", i, nif_files.len());
        }
        match archive.extract(nif_path) {
            Ok(bytes) => process_bytes(stats, nif_path.clone(), &bytes),
            Err(e) => stats.record_failure(nif_path.clone(), format!("extract: {e}")),
        }
    }
    Ok(())
}

fn main() {
    // Optional env_logger init so --verbose parse messages surface.
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .try_init();

    let mut path_arg: Option<String> = None;
    let mut tsv = false;
    let mut unknown_only = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--tsv" => tsv = true,
            "--unknown-only" => unknown_only = true,
            "-h" | "--help" => {
                eprintln!("usage: nif_stats <path> [--tsv] [--unknown-only]");
                eprintln!("  <path>          .nif file, directory, .bsa, or .ba2");
                eprintln!("  --tsv           emit machine-readable per-type histogram");
                eprintln!("  --unknown-only  human summary: skip fully-parsed types");
                std::process::exit(0);
            }
            other if other.starts_with("--") => {
                eprintln!("unknown flag: {}", other);
                std::process::exit(2);
            }
            other => {
                if path_arg.is_some() {
                    eprintln!("unexpected positional argument: {}", other);
                    std::process::exit(2);
                }
                path_arg = Some(other.to_string());
            }
        }
    }
    let Some(path_arg) = path_arg else {
        eprintln!("usage: nif_stats <path> [--tsv] [--unknown-only]");
        eprintln!("  <path> may be a .nif file, a directory, a .bsa, or a .ba2 archive");
        std::process::exit(2);
    };
    let path = PathBuf::from(path_arg);

    let mut stats = Stats::new();

    if !path.exists() {
        eprintln!("path does not exist: {:?}", path);
        std::process::exit(2);
    }

    if path.is_file() {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "bsa" => {
                if let Err(e) = process_bsa(&mut stats, &path) {
                    eprintln!("error: {}", e);
                    std::process::exit(2);
                }
            }
            "ba2" => {
                if let Err(e) = process_ba2(&mut stats, &path) {
                    eprintln!("error: {}", e);
                    std::process::exit(2);
                }
            }
            _ => process_file(&mut stats, &path),
        }
    } else if path.is_dir() {
        process_dir(&mut stats, &path);
    }

    if tsv {
        stats.print_tsv();
    } else {
        stats.print(unknown_only);
    }

    let threshold = min_success_rate();
    if stats.total > 0 && stats.success_rate() < threshold {
        eprintln!(
            "\nparse success rate {:.2}% is below the {:.2}% threshold",
            stats.success_rate() * 100.0,
            threshold * 100.0
        );
        std::process::exit(1);
    }
}
