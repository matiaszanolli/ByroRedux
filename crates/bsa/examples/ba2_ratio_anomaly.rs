//! ba2_ratio_anomaly — scan BA2 archives for GNRL records where
//! `packed_size > unpacked_size`.
//!
//! Investigation tool for #598 / FO4-DIM2-08. Prior audit observed some
//! GNRL records in `Fallout4 - Meshes.ba2` where `packed_size /
//! unpacked_size ≈ 3.0` — a ratio impossible for well-formed zlib
//! deflate (worst case on uncompressible input is ~0.1% overhead).
//! This tool walks every GNRL entry across one or more BA2 archives,
//! flags every anomaly, and groups by file extension so layout quirks
//! (e.g. tiny-file block padding) are easy to separate from genuine
//! parser misreads.
//!
//! Usage:
//!   cargo run -p byroredux-bsa --example ba2_ratio_anomaly -- <path...>
//!   cargo run -p byroredux-bsa --example ba2_ratio_anomaly -- --tsv <path...>
//!
//! `<path>` may be a single .ba2 file or a directory; directories are
//! searched (non-recursively) for .ba2 entries.
//!
//! Flags:
//!   `--tsv`   emit one row per anomaly: `<archive>\t<entry>\t<packed>\t<unpacked>`
//!             on stdout; suppresses the human-readable summary.

use byroredux_bsa::Ba2Archive;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct ExtensionStats {
    count: usize,
    total_packed: u64,
    total_unpacked: u64,
    /// Largest observed `packed / unpacked` ratio in this extension
    /// bucket. Sentinel `0.0` when the bucket is empty.
    max_ratio: f64,
    /// Sample of extreme entries — `(archive, entry, packed, unpacked)`
    /// kept for the human summary. Capped to keep output bounded.
    samples: Vec<(String, String, u32, u32)>,
}

#[derive(Default)]
struct Summary {
    /// Per-archive: total GNRL entries scanned + anomalies seen.
    per_archive: BTreeMap<String, (usize, usize)>,
    /// Per-extension: aggregate stats across all archives.
    per_extension: BTreeMap<String, ExtensionStats>,
    total_entries: usize,
    total_anomalies: usize,
}

fn extension_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "<no-ext>".to_string())
}

fn scan_archive(summary: &mut Summary, archive_path: &Path, tsv: bool) -> Result<(), String> {
    let archive = Ba2Archive::open(archive_path).map_err(|e| {
        format!(
            "open BA2 {}: {}",
            archive_path.display(),
            e
        )
    })?;
    let archive_label = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<unknown>")
        .to_string();

    let mut total = 0usize;
    let mut anomalies = 0usize;

    for (entry, packed, unpacked) in archive.iter_general_sizes() {
        total += 1;
        if packed <= unpacked {
            continue;
        }
        anomalies += 1;
        let ext = extension_of(entry);
        let bucket = summary.per_extension.entry(ext).or_default();
        bucket.count += 1;
        bucket.total_packed += packed as u64;
        bucket.total_unpacked += unpacked as u64;
        let ratio = if unpacked == 0 {
            f64::INFINITY
        } else {
            packed as f64 / unpacked as f64
        };
        if ratio > bucket.max_ratio {
            bucket.max_ratio = ratio;
        }
        if bucket.samples.len() < 10 {
            bucket
                .samples
                .push((archive_label.clone(), entry.to_string(), packed, unpacked));
        }
        if tsv {
            println!("{}\t{}\t{}\t{}", archive_label, entry, packed, unpacked);
        }
    }

    summary
        .per_archive
        .insert(archive_label, (total, anomalies));
    summary.total_entries += total;
    summary.total_anomalies += anomalies;
    Ok(())
}

fn print_summary(summary: &Summary) {
    println!();
    println!("─── BA2 packed/unpacked ratio anomaly scan ───────────────────");
    println!(
        "  scanned {} GNRL entries; {} have packed > unpacked",
        summary.total_entries, summary.total_anomalies
    );

    if summary.total_anomalies == 0 {
        println!("  no anomalies — every GNRL entry has packed_size <= unpacked_size");
        return;
    }

    println!();
    println!("─── Per-archive ──────────────────────────────────────────────");
    println!(
        "  {:>6} {:>10}  {}",
        "anomal", "scanned", "archive"
    );
    for (archive, (total, anomalies)) in &summary.per_archive {
        println!("  {:>6} {:>10}  {}", anomalies, total, archive);
    }

    println!();
    println!("─── Per-extension ────────────────────────────────────────────");
    println!(
        "  {:>6} {:>10} {:>10} {:>8}  {}",
        "count", "Σpacked", "Σunpkd", "max %",
        "ext"
    );
    let mut by_ext: Vec<(&String, &ExtensionStats)> = summary.per_extension.iter().collect();
    by_ext.sort_by(|a, b| b.1.count.cmp(&a.1.count));
    for (ext, stats) in &by_ext {
        println!(
            "  {:>6} {:>10} {:>10} {:>8.1}  .{}",
            stats.count,
            stats.total_packed,
            stats.total_unpacked,
            stats.max_ratio * 100.0,
            ext,
        );
    }

    println!();
    println!("─── Sample anomalies (per extension, up to 10) ──────────────");
    for (ext, stats) in &by_ext {
        if stats.samples.is_empty() {
            continue;
        }
        println!("  .{} ({} total)", ext, stats.count);
        for (archive, entry, packed, unpacked) in &stats.samples {
            let ratio = if *unpacked == 0 {
                f64::INFINITY
            } else {
                *packed as f64 / *unpacked as f64
            };
            println!(
                "    {:>6} / {:>6} ({:.2}×)  {}/{}",
                packed, unpacked, ratio, archive, entry,
            );
        }
    }

    println!();
    println!("─── Diagnosis hint ───────────────────────────────────────────");
    println!(
        "  Genuine deflate cannot inflate compressible data >0.1%, BUT\n  \
         the zlib STREAM FRAMING (2-byte CMF/FLG header + ~5-byte\n  \
         minimal stored block + 4-byte ADLER32 checksum ≈ 11-12 bytes\n  \
         floor) dominates on tiny entries. A 4-byte payload zlib-\n  \
         encodes to ~12 bytes — ratio 3.0× trivially.\n  \
\n  \
         Vanilla `Fallout4 - Meshes.ba2` (#598 baseline): 28/42 426\n  \
         GNRL entries (0.066%) trip the anomaly check, all tiny\n  \
         (4-35 B unpacked, 12-41 B packed) `.txt` / `.lod` / `.lst` /\n  \
         `.ssf` files — every ratio explained by zlib framing.\n  \
         BENIGN; no parser bug.\n  \
\n  \
         Anomalies that are NOT explained by tiny-file framing\n  \
         (large files, mixed extensions, ratios that grow with\n  \
         payload size) would be the genuine-bug signal."
    );
}

fn collect_ba2s(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    if !path.is_dir() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("ba2"))
                    .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

fn main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .try_init();

    let mut paths: Vec<String> = Vec::new();
    let mut tsv = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--tsv" => tsv = true,
            "-h" | "--help" => {
                eprintln!(
                    "usage: ba2_ratio_anomaly [--tsv] <path...>\n  \
                     <path>  .ba2 file or directory containing .ba2 files\n  \
                     --tsv   emit `archive\\tentry\\tpacked\\tunpacked` per anomaly"
                );
                std::process::exit(0);
            }
            other if other.starts_with("--") => {
                eprintln!("unknown flag: {}", other);
                std::process::exit(2);
            }
            other => paths.push(other.to_string()),
        }
    }

    if paths.is_empty() {
        eprintln!("usage: ba2_ratio_anomaly [--tsv] <path...>");
        std::process::exit(2);
    }

    let mut summary = Summary::default();
    let mut had_error = false;

    for path_arg in &paths {
        let path = PathBuf::from(path_arg);
        let archives = collect_ba2s(&path);
        if archives.is_empty() {
            eprintln!(
                "skipping {}: not a .ba2 file or a directory containing .ba2 files",
                path.display()
            );
            had_error = true;
            continue;
        }
        for archive_path in archives {
            if !tsv {
                eprintln!("scanning {}", archive_path.display());
            }
            if let Err(e) = scan_archive(&mut summary, &archive_path, tsv) {
                eprintln!("error: {}", e);
                had_error = true;
            }
        }
    }

    if !tsv {
        print_summary(&summary);
    }

    if had_error {
        std::process::exit(1);
    }
}
