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

use byroredux_bgsm::{
    parse_bgem_diag, parse_bgsm_diag, Error, MaterialFile, ParseDiagnostics, NEWEST_KNOWN_VERSION,
};
use byroredux_bsa::Ba2Archive;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

    /// Exact file counts in the vanilla FO4 base Materials.ba2 archive
    /// (no DLC), measured by the corpus census.
    fn expected_count(self) -> usize {
        match self {
            Variant::Bgsm => 6_616,
            Variant::Bgem => 283,
        }
    }
}

/// `"BGSM"` / `"BGEM"` read as little-endian u32 (the crate keeps its own
/// constants `pub(crate)`).
const BGSM_MAGIC: u32 = u32::from_le_bytes(*b"BGSM");
const BGEM_MAGIC: u32 = u32::from_le_bytes(*b"BGEM");

/// `parse` with the #4664/#4672 diagnostics kept: dispatches on the magic
/// exactly like `byroredux_bgsm::parse`, but through `parse_bgsm_diag` /
/// `parse_bgem_diag` (#5011 — the sweeps called `parse()` and counted `Ok`
/// only, so a gating regression that reads the wrong fields but consumes
/// every byte, or leaves bytes over without erroring, stayed green).
fn parse_diag(bytes: &[u8]) -> (Result<MaterialFile, Error>, ParseDiagnostics) {
    let magic = bytes
        .get(..4)
        .map(|m| u32::from_le_bytes([m[0], m[1], m[2], m[3]]));
    match magic {
        Some(BGSM_MAGIC) => {
            let (r, d) = parse_bgsm_diag(bytes);
            (r.map(MaterialFile::Bgsm), d)
        }
        Some(BGEM_MAGIC) => {
            let (r, d) = parse_bgem_diag(bytes);
            (r.map(MaterialFile::Bgem), d)
        }
        // Too short or unknown magic: let `parse` produce the real error.
        _ => (
            byroredux_bgsm::parse(bytes),
            ParseDiagnostics::default(),
        ),
    }
}

/// The #4664 drift signals — leftover bytes or a version past the newest
/// layout the crate knows. Lossy strings (#4672) are tallied separately:
/// they are a content fact, not a layout one.
fn drift_description(d: &ParseDiagnostics) -> Option<String> {
    let mut parts = Vec::new();
    if d.unconsumed_bytes > 0 {
        parts.push(format!("{} unconsumed byte(s)", d.unconsumed_bytes));
    }
    if let Some(v) = d.version_over_ceiling {
        parts.push(format!("version {v} > {NEWEST_KNOWN_VERSION}"));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

#[derive(Default)]
struct VariantStats {
    total: usize,
    clean: usize,
    /// Error-message first-line → (count, up-to-5 filenames).
    buckets: HashMap<String, (usize, Vec<String>)>,
    /// Files that parsed `Ok` but tripped a #4664 drift signal (#5011).
    drift: Vec<(String, String)>,
    /// Files with at least one lossily-decoded string (#4672); reported,
    /// not gated.
    lossy_strings: usize,
}

impl VariantStats {
    fn record_ok(&mut self) {
        self.total += 1;
        self.clean += 1;
    }

    fn record_err(&mut self, path: &str, err: &str) {
        self.total += 1;
        let bucket_key = err.lines().next().unwrap_or(err).to_string();
        let entry = self
            .buckets
            .entry(bucket_key)
            .or_insert_with(|| (0, Vec::new()));
        entry.0 += 1;
        if entry.1.len() < 5 {
            entry.1.push(path.to_string());
        }
    }

    /// Record a successful parse's diagnostics (#5011).
    fn note_diagnostics(&mut self, path: &str, d: &ParseDiagnostics) {
        if let Some(what) = drift_description(d) {
            self.drift.push((path.to_string(), what));
        }
        if d.lossy_strings {
            self.lossy_strings += 1;
        }
    }

    fn print_drift(&self, label: &str) {
        eprintln!(
            "[{label}] {} file(s) with layout drift; {} with lossy strings",
            self.drift.len(),
            self.lossy_strings
        );
        for (path, what) in self.drift.iter().take(10) {
            eprintln!("  DRIFT {path}: {what}");
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
        buckets.sort_by_key(|bucket| std::cmp::Reverse(bucket.1 .0));
        for (msg, (count, examples)) in buckets.iter().take(10) {
            eprintln!("  [{count:>4}] {}", msg);
            for p in examples.iter().take(5) {
                eprintln!("         example: {}", p);
            }
        }
        if buckets.len() > 10 {
            eprintln!(
                "  ... and {} more distinct error buckets",
                buckets.len() - 10
            );
        }
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

fn fo4_data_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var("BYROREDUX_FO4_DATA")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let p = PathBuf::from(&v);
        if p.is_dir() {
            return Some(p);
        }
        // #3850: an explicitly-set override is BINDING — never silently
        // substitute the hardcoded dev-machine path.
        panic!("BYROREDUX_FO4_DATA points to {v:?}, which is not a directory");
    }
    let p = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data");
    if p.is_dir() {
        return Some(p);
    }
    require_game_data("BYROREDUX_FO4_DATA", &p);
    None
}

/// #4665 (PAR-D4-2026-09-21-03) — FO76's Data directory. Same binding
/// override contract as `fo4_data_dir`.
fn fo76_data_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var("BYROREDUX_FO76_DATA")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let p = PathBuf::from(&v);
        if p.is_dir() {
            return Some(p);
        }
        panic!("BYROREDUX_FO76_DATA points to {v:?}, which is not a directory");
    }
    let p = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout76/Data");
    if p.is_dir() {
        return Some(p);
    }
    require_game_data("BYROREDUX_FO76_DATA", &p);
    None
}

/// #4660 — the data directory resolved but lacks an archive the test needs.
/// Under the strict lane that is a failure naming the file, not a green skip.
#[track_caller]
fn require_archive(path: &std::path::Path) {
    if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
        panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file");
    }
    eprintln!("skipping: {path:?} not found");
}

fn open_archive_at(data: &Path, name: &str) -> Option<Ba2Archive> {
    let archive_path = data.join(name);
    if !archive_path.is_file() {
        require_archive(&archive_path);
        return None;
    }
    // #4660 — a present archive that fails to open is a reader regression,
    // exactly what this harness exists to catch; it used to read as a skip.
    let archive = Ba2Archive::open(&archive_path)
        .unwrap_or_else(|e| panic!("failed to open {archive_path:?}: {e}"));
    Some(archive)
}

fn open_materials_archive() -> Option<Ba2Archive> {
    open_archive_at(&fo4_data_dir()?, "Fallout4 - Materials.ba2")
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
        let bytes = match archive.extract(path) {
            Ok(b) => b,
            Err(e) => {
                stats.record_err(path, &format!("extract: {e}"));
                continue;
            }
        };
        let (parsed, diag) = parse_diag(&bytes);
        match parsed {
            Ok(MaterialFile::Bgsm(_)) if expect == Variant::Bgsm => {
                stats.record_ok();
                stats.note_diagnostics(path, &diag);
            }
            Ok(MaterialFile::Bgem(_)) if expect == Variant::Bgem => {
                stats.record_ok();
                stats.note_diagnostics(path, &diag);
            }
            Ok(other) => {
                let got = match other {
                    MaterialFile::Bgsm(_) => "BGSM",
                    MaterialFile::Bgem(_) => "BGEM",
                };
                stats.record_err(
                    path,
                    &format!(
                        "variant mismatch: file extension implies {} but parser returned {}",
                        expect.label(),
                        got
                    ),
                );
            }
            Err(e) => stats.record_err(path, &format!("parse: {e}")),
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
    stats.print_drift(expect.label());

    // #5011 / #4664 — vanilla is measured zero-noise (0/36,888 files with
    // leftover bytes; only v2 and v22 in use), so any drift is a gating
    // regression that still returns `Ok`.
    assert!(
        stats.drift.is_empty(),
        "[{}] {} file(s) parsed but left bytes unconsumed or exceeded the \
         known version ceiling: {:?}",
        expect.label(),
        stats.drift.len(),
        &stats.drift[..stats.drift.len().min(5)]
    );
    assert_eq!(
        stats.total,
        expect.expected_count(),
        "[{}] unexpected number of {} files in the vanilla Materials archive",
        expect.label(),
        expect.label(),
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
#[ignore = "needs FO4 game data on disk"]
fn parse_rate_fo4_bgsm_corpus() {
    run_variant(Variant::Bgsm);
}

#[test]
#[ignore = "needs FO4 game data on disk"]
fn parse_rate_fo4_bgem_corpus() {
    run_variant(Variant::Bgem);
}

/// #4665 (PAR-D4-2026-09-21-03) — the FO76 sweep. Vanilla FO76 uses v22
/// exclusively (25,888 BGSM + 4,101 BGEM measured 2026-09-21). Two files
/// in the corpus are NOT the binary format at all:
/// `materials\atx\setdressing\atx_plushie_mr.fuzzy_valentinesday\*.bgsm`
/// are Material-Editor JSON text — allowed as a counted, named bucket
/// (`json_not_binary`) rather than either a failure or a silent skip.
/// Whether FO76's own runtime reads those is unverified; this test only
/// pins that every REAL binary file parses.
#[test]
#[ignore = "needs FO76 game data on disk"]
fn parse_rate_fo76_materials_corpus() {
    let Some(data) = fo76_data_dir() else {
        return;
    };
    let Some(archive) = open_archive_at(&data, "SeventySix - Materials.ba2") else {
        return;
    };

    #[derive(Default)]
    struct Stats {
        total: usize,
        clean: usize,
        json_text: usize,
        json_examples: Vec<String>,
        failures: Vec<(String, String)>,
        /// #5011 — parsed `Ok` but tripped a #4664 drift signal.
        drift: Vec<(String, String)>,
    }
    let mut stats = Stats::default();
    for path in archive.list_files() {
        let lower = path.to_ascii_lowercase();
        if !(lower.ends_with(".bgsm") || lower.ends_with(".bgem")) {
            continue;
        }
        stats.total += 1;
        let bytes = match archive.extract(path) {
            Ok(b) => b,
            Err(e) => {
                stats.failures.push((path.to_string(), format!("extract: {e}")));
                continue;
            }
        };
        // JSON allowlist: Material-Editor exports are UTF text starting
        // with `{`; the binary magic is 4 raw ASCII bytes, never `{`.
        if bytes.first() == Some(&b'{') {
            stats.json_text += 1;
            if stats.json_examples.len() < 5 {
                stats.json_examples.push(path.to_string());
            }
            continue;
        }
        let (parsed, diag) = parse_diag(&bytes);
        if parsed.is_ok() {
            stats.clean += 1;
            if let Some(what) = drift_description(&diag) {
                stats.drift.push((path.to_string(), what));
            }
        } else {
            stats.failures.push((path.to_string(), "parse failed".to_string()));
        }
    }

    eprintln!(
        "[FO76] parsed {}/{} material files clean; {} JSON-text (allowed); failures: {}",
        stats.clean,
        stats.total,
        stats.json_text,
        stats.failures.len(),
    );
    for (path, err) in stats.failures.iter().take(10) {
        eprintln!("  FAIL {path}: {err}");
    }
    assert_eq!(
        stats.total, 29_991,
        "unexpected number of material files in SeventySix - Materials.ba2"
    );
    assert_eq!(
        stats.json_text, 2,
        "the FO76 Material-Editor JSON allowlist changed"
    );
    // The audit measured 29,989/29,991 OK with exactly 2 known JSON files;
    // both exceptions are accounted for above, so the remainder must be
    // spotless.
    for (path, what) in stats.drift.iter().take(10) {
        eprintln!("  DRIFT {path}: {what}");
    }
    assert!(
        stats.drift.is_empty(),
        "{} FO76 material file(s) parsed but drifted (unconsumed bytes / version \
         over the ceiling): {:?}",
        stats.drift.len(),
        &stats.drift[..stats.drift.len().min(5)]
    );
    assert!(
        stats.failures.is_empty(),
        "{} FO76 material file(s) failed to parse: {:?}",
        stats.failures.len(),
        &stats.failures[..stats.failures.len().min(5)]
    );
    assert!(
        stats.clean + stats.json_text == stats.total,
        "every file must be clean-parse or accounted JSON"
    );
}

/// #5011 — the gate's own predicate, runnable without game data: clean and
/// lossy-only parses are not drift; leftover bytes and an over-ceiling
/// version each are.
#[test]
fn drift_predicate_flags_only_layout_signals() {
    assert!(drift_description(&ParseDiagnostics::default()).is_none());
    assert!(drift_description(&ParseDiagnostics {
        lossy_strings: true,
        ..Default::default()
    })
    .is_none());
    assert!(drift_description(&ParseDiagnostics {
        unconsumed_bytes: 1,
        ..Default::default()
    })
    .is_some());
    assert!(drift_description(&ParseDiagnostics {
        version_over_ceiling: Some(NEWEST_KNOWN_VERSION + 1),
        ..Default::default()
    })
    .is_some());
}
