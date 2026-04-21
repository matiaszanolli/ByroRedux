//! Corpus integration test — parse every `.bgsm` / `.bgem` file out of
//! `Fallout4 - Materials.ba2` and assert a minimum success rate per
//! variant.
//!
//! Mirrors the `crates/nif/tests/parse_real_nifs.rs` shape:
//!   - `#[ignore]` by default because the archive lives in a Steam
//!     install, not in the repo.
//!   - `BYROREDUX_FO4_DATA` env var falls back to the canonical Steam
//!     path. Skip-gracefully when neither resolves.
//!   - Summary prints a failure-bucket histogram (first line of the
//!     error) with the top-5 example filenames per bucket so a
//!     regression lands loud and diagnosable.
//!   - BGSM and BGEM are separate subtests with independent budgets —
//!     different serializers, different code paths, same corpus.
//!
//! Run:
//! ```sh
//! cargo test -p byroredux-bgsm --test parse_all -- --ignored
//! ```
//!
//! Override the archive location:
//! ```sh
//! BYROREDUX_FO4_DATA=/path/to/Fallout4/Data \
//!   cargo test -p byroredux-bgsm --test parse_all -- --ignored
//! ```
//!
//! ## Coverage commitment
//!
//! The reference run lands at 100% clean across 6,616 BGSM + 283
//! BGEM files on vanilla FO4 Data (Steam install, no DLC). The issue
//! #491 plan opened at 0.95 to absorb any unseen version variants
//! 1–22, but since the first green run is already at 1.0 we keep the
//! threshold tight from the start — same gate shape as
//! `parse_real_nifs.rs::MIN_SUCCESS_RATE`. Any single-file regression
//! then fails loudly and the bucket histogram points straight at the
//! broken version. A future mod-content test that tolerates partial
//! coverage should define its own looser constant rather than
//! relaxing this one.
//!
//! See `docs/audits/AUDIT_FO4_2026-04-17.md` Dim 6 + issue #491.

use byroredux_bgsm::{parse, MaterialFile};
use byroredux_bsa::Ba2Archive;
use std::collections::HashMap;
use std::path::PathBuf;

/// Minimum per-variant success rate before the test fails.
/// Intentionally tight (1.0) so a single regressed file surfaces as
/// a failure. See module docs for rationale.
const MIN_SUCCESS_RATE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Variant {
    Bgsm,
    Bgem,
}

impl Variant {
    fn label(self) -> &'static str {
        match self {
            Variant::Bgsm => "BGSM",
            Variant::Bgem => "BGEM",
        }
    }
}

#[derive(Default)]
struct VariantStats {
    total: usize,
    clean: usize,
    /// Error-message first-line → (count, up-to-5 filenames).
    buckets: HashMap<String, (usize, Vec<String>)>,
}

impl VariantStats {
    fn record_ok(&mut self) {
        self.total += 1;
        self.clean += 1;
    }

    fn record_err(&mut self, path: &str, err: &str) {
        self.total += 1;
        let bucket_key = err.lines().next().unwrap_or(err).to_string();
        let entry = self.buckets.entry(bucket_key).or_insert_with(|| (0, Vec::new()));
        entry.0 += 1;
        if entry.1.len() < 5 {
            entry.1.push(path.to_string());
        }
    }

    fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.clean as f64 / self.total as f64
    }

    fn print_summary(&self, label: &str) {
        eprintln!(
            "[{label}] parsed {}/{} {label} files ({:.2}% clean)",
            self.clean,
            self.total,
            self.success_rate() * 100.0,
        );
        if self.buckets.is_empty() {
            return;
        }
        // Sort buckets by count descending so the worst regression
        // surfaces first in the diff.
        let mut buckets: Vec<(&String, &(usize, Vec<String>))> = self.buckets.iter().collect();
        buckets.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        for (msg, (count, examples)) in buckets.iter().take(10) {
            eprintln!("  [{count:>4}] {}", msg);
            for p in examples.iter().take(5) {
                eprintln!("         example: {}", p);
            }
        }
        if buckets.len() > 10 {
            eprintln!("  ... and {} more distinct error buckets", buckets.len() - 10);
        }
    }
}

fn fo4_data_dir() -> Option<PathBuf> {
    if let Ok(val) = std::env::var("BYROREDUX_FO4_DATA") {
        let path = PathBuf::from(val);
        if path.is_dir() {
            return Some(path);
        }
        eprintln!(
            "BYROREDUX_FO4_DATA points to {:?} which is not a directory; trying default",
            path
        );
    }
    let default = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data");
    if default.is_dir() {
        return Some(default);
    }
    eprintln!(
        "skipping: no Fallout 4 Data directory found \
         (set BYROREDUX_FO4_DATA or install to {:?})",
        default
    );
    None
}

fn open_materials_archive() -> Option<Ba2Archive> {
    let data = fo4_data_dir()?;
    let archive_path = data.join("Fallout4 - Materials.ba2");
    if !archive_path.is_file() {
        eprintln!("skipping: {:?} not found", archive_path);
        return None;
    }
    match Ba2Archive::open(&archive_path) {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("skipping: failed to open {:?}: {}", archive_path, e);
            None
        }
    }
}

/// Walk the archive once, tally outcomes by variant. `expect` filters
/// to a single variant so BGSM and BGEM subtests stay independent.
/// Dispatch uses the file extension (archive paths are authoritative —
/// both variants share the same magic length so a stray rename is a
/// bigger problem than this test).
fn walk_variant(archive: &Ba2Archive, expect: Variant) -> VariantStats {
    let mut stats = VariantStats::default();
    let suffix = match expect {
        Variant::Bgsm => ".bgsm",
        Variant::Bgem => ".bgem",
    };
    for path in archive.list_files() {
        let lower = path.to_ascii_lowercase();
        if !lower.ends_with(suffix) {
            continue;
        }
        let bytes = match archive.extract(&path) {
            Ok(b) => b,
            Err(e) => {
                stats.record_err(&path, &format!("extract: {e}"));
                continue;
            }
        };
        match parse(&bytes) {
            Ok(MaterialFile::Bgsm(_)) if expect == Variant::Bgsm => stats.record_ok(),
            Ok(MaterialFile::Bgem(_)) if expect == Variant::Bgem => stats.record_ok(),
            Ok(other) => {
                let got = match other {
                    MaterialFile::Bgsm(_) => "BGSM",
                    MaterialFile::Bgem(_) => "BGEM",
                };
                stats.record_err(
                    &path,
                    &format!(
                        "variant mismatch: file extension implies {} but parser returned {}",
                        expect.label(),
                        got
                    ),
                );
            }
            Err(e) => stats.record_err(&path, &format!("parse: {e}")),
        }
    }
    stats
}

fn run_variant(expect: Variant) {
    let Some(archive) = open_materials_archive() else {
        return;
    };
    let stats = walk_variant(&archive, expect);
    stats.print_summary(expect.label());

    assert!(
        stats.total > 0,
        "[{}] expected at least one {} file in the Materials archive",
        expect.label(),
        expect.label()
    );
    assert!(
        stats.success_rate() >= MIN_SUCCESS_RATE,
        "[{}] parse success rate {:.2}% is below the {:.0}% threshold \
         ({} failures across {} files)",
        expect.label(),
        stats.success_rate() * 100.0,
        MIN_SUCCESS_RATE * 100.0,
        stats.total - stats.clean,
        stats.total,
    );
}

#[test]
#[ignore]
fn parse_rate_fo4_bgsm_corpus() {
    run_variant(Variant::Bgsm);
}

#[test]
#[ignore]
fn parse_rate_fo4_bgem_corpus() {
    run_variant(Variant::Bgem);
}
