//! nif_stats — walk a NIF source and report parse statistics.
//!
//! Usage:
//!   cargo run -p byroredux-nif --example nif_stats -- <path>
//!
//! `<path>` may be:
//!   - a single `.nif` file
//!   - a directory containing `.nif` files (recursed)
//!   - a `.bsa` archive (all internal `.nif` entries are extracted)
//!
//! The tool prints a summary: total files, successes, failures (with
//! a few examples), a histogram of block types seen in successful
//! parses, and a sorted list of error messages for the failing parses.
//!
//! Exit code is non-zero when parse success rate drops below 95%.

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MIN_SUCCESS_RATE: f64 = 0.95;

struct Stats {
    total: usize,
    ok: usize,
    block_histogram: BTreeMap<String, usize>,
    /// Grouped by the first line of the error message.
    failure_groups: BTreeMap<String, Vec<String>>,
}

impl Stats {
    fn new() -> Self {
        Self {
            total: 0,
            ok: 0,
            block_histogram: BTreeMap::new(),
            failure_groups: BTreeMap::new(),
        }
    }

    fn record_success(&mut self, block_names: impl Iterator<Item = String>) {
        self.total += 1;
        self.ok += 1;
        for name in block_names {
            *self.block_histogram.entry(name).or_insert(0) += 1;
        }
    }

    fn record_failure(&mut self, path: String, err: String) {
        self.total += 1;
        // Group errors by their first line — avoids per-file message noise.
        let group_key = err.lines().next().unwrap_or(&err).to_string();
        self.failure_groups.entry(group_key).or_default().push(path);
    }

    fn success_rate(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            self.ok as f64 / self.total as f64
        }
    }

    fn print(&self) {
        println!();
        println!("─── Parse stats ──────────────────────────────────────────────");
        println!("  total:    {:>6}", self.total);
        println!(
            "  ok:       {:>6}  ({:.2}%)",
            self.ok,
            self.success_rate() * 100.0
        );
        println!("  failures: {:>6}", self.total - self.ok);

        if !self.block_histogram.is_empty() {
            let mut sorted: Vec<(&String, &usize)> = self.block_histogram.iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(a.1));
            println!();
            println!("─── Block type histogram (top 20) ────────────────────────────");
            for (name, count) in sorted.iter().take(20) {
                println!("  {:>6}  {}", count, name);
            }
            println!("  ({} distinct block types)", sorted.len());
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
}

fn process_bytes(stats: &mut Stats, label: String, bytes: &[u8]) {
    match parse_nif(bytes) {
        Ok(scene) => {
            let names = scene.blocks.iter().map(|b| b.block_type_name().to_string());
            stats.record_success(names);
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

    let mut args = std::env::args().skip(1);
    let Some(path_arg) = args.next() else {
        eprintln!("usage: nif_stats <path>");
        eprintln!("  <path> may be a .nif file, a directory, or a .bsa archive");
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

    stats.print();

    if stats.total > 0 && stats.success_rate() < MIN_SUCCESS_RATE {
        eprintln!(
            "\nparse success rate {:.2}% is below the {:.0}% threshold",
            stats.success_rate() * 100.0,
            MIN_SUCCESS_RATE * 100.0
        );
        std::process::exit(1);
    }
}
