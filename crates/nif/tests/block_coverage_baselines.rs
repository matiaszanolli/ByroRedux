//! Parse-block coverage regression pin (#1332 / NIF-2026-05-29-04).
//!
//! A **distinct surface** from the two existing baseline harnesses:
//!   * `translation_completeness.rs` measures whether parsed blocks
//!     *translate* to a canonical Material — it never looks at block
//!     counts, so a truncated file that produces no Material anyway is
//!     invisible to it.
//!   * `per_block_baselines.rs` pins per-type `NiUnknown` growth against
//!     a checked-in histogram — a *regression* signal, not an absolute
//!     coverage gate.
//!
//! Neither asserts **block-count parity**. That is the gap the Oblivion
//! `bhkConvexSweepShape` / `bhkMeshShape` cascade (F-01 / #1331's audit
//! sibling) slipped through: a sizeless block with no dispatch arm drops
//! the rest of the file, the file-level recoverable-rate stays 100%, and
//! only an ad-hoc probe caught it. This pin closes that:
//!
//!   * **Oblivion (sizeless): block-count parity, regression-gated.**
//!     A sizeless truncation drops the rest of the file
//!     (`scene.len() < header.num_blocks`). The ideal is zero truncation,
//!     but vanilla Oblivion still has a tail of files whose undispatched
//!     sizeless blocks truncate (beyond the F-01 `bhkConvexSweepShape` /
//!     `bhkMeshShape` pair, which *is* fixed — those three named files
//!     parse whole now). Fixing the remaining tail means writing new
//!     block parsers — out of scope for a coverage-pin. So, like
//!     `per_block_baselines`, the gate is **no-new-truncation**: the set
//!     of truncating files is checked in, improvements are silent, and a
//!     *new* truncating file (e.g. an F-01-class dispatch-arm regression)
//!     fails the test. The baseline file is the visible, tracked record
//!     of the remaining gap — the opposite of the silent loss this pin
//!     exists to prevent.
//!   * **Sized games (FO3+): NiUnknown-rate ceiling.** `block_size` lets
//!     the loop seek past a busted block and drop a `NiUnknown`
//!     placeholder, so the count stays whole; the regression signal is
//!     the *rate* of those placeholders. Pinned against a checked-in
//!     per-game baseline (improvement is silent; growth fails).
//!
//! ## Why opt-in
//!
//! Same constraint as `parse_real_nifs.rs` / `per_block_baselines.rs`:
//! CI machines have no game data, so every test is `#[ignore]`d and skips
//! cleanly when the archive can't be opened.
//!
//! Capture the sized-game baselines once on a machine that has the data:
//!
//! ```sh
//! BYROREDUX_REGEN_BASELINES=1 \
//!   cargo test -p byroredux-nif --test block_coverage_baselines -- --ignored --nocapture
//! ```
//!
//! That writes `crates/nif/tests/data/block_coverage_baselines/<game>.tsv`
//! for every sized game whose data is available. Check those files in.

mod common;

use common::{open_all_mesh_archives, open_optional_mesh_archives, Game};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn regen_mode() -> bool {
    std::env::var("BYROREDUX_REGEN_BASELINES")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

fn baselines_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("block_coverage_baselines")
}

// ── Oblivion: exact block-count parity (sizeless) ─────────────────────

/// Oblivion is sizeless — an undispatched block can't be skipped, so it
/// truncates the rest of the file (`scene.len() < header.num_blocks`).
/// Gate on **no new truncation** against a checked-in set of the files
/// that currently truncate: improvements are silent, a newly-truncating
/// file fails loud. An F-01-class regression (dispatch arm removed for a
/// sizeless block — `handscythe01.nif` / `oar01.nif` /
/// `ungrdltraphingedoor.nif` parse whole now) re-truncates files that
/// aren't in the baseline → red.
///
/// #3712 (NIF-2026-08-30-D3-01) widened this from the base
/// `Oblivion - Meshes.bsa` alone to also walk
/// [`common::open_optional_mesh_archives`]'s present-only DLC tier — the
/// eight vanilla DLC archives (1,580 NIFs, 16.4% of the full corpus) were
/// previously covered by no test in the repository, on the one game where
/// an undispatched block truncates the rest of the scene instead of
/// being absorbed into an `NiUnknown` placeholder.
///
/// `open_optional_mesh_archives`'s own doc warns that tier is "only safe
/// for rate-based gates — never for the count-keyed baseline harnesses",
/// which is true of a *single monolithic* count comparison (an absent
/// optional archive would make `parsed` drop against a baseline captured
/// with it present, exactly the "trading a partial gate for no gate"
/// failure `mesh_archives`'s own doc warns about). This gate stays safe
/// under that constraint by keying `parsed` and the truncating-file set
/// **per archive** rather than as one global sum: an archive absent from
/// the current run (e.g. a base-game-only CI host) is simply never
/// compared, neither contributing to nor requiring anything from the
/// baseline, while a present archive is checked against its own
/// independently-tracked baseline slice — unlike Skyrim SE's Creation
/// Club tier (#3369), Oblivion's DLC content is static and vanilla
/// (GOTY/Deluxe own it or don't; it never rotates), so per-archive counts
/// captured once stay reproducible for as long as that install exists.
#[test]
#[ignore = "needs Oblivion game data on disk"]
fn oblivion_block_count_parity() {
    let Some(mut archives) = open_all_mesh_archives(Game::Oblivion) else {
        return;
    };
    archives.extend(open_optional_mesh_archives(Game::Oblivion));

    // Per-archive parsed count, plus a truncating-file set keyed
    // `<archive>::<path>` so two archives can't collide on a shared
    // relative NIF path.
    let mut archive_parsed: BTreeMap<String, usize> = BTreeMap::new();
    let mut truncating: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_parsed = 0usize;

    for (name, archive) in &archives {
        let files: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
            .collect();

        let mut parsed = 0usize;
        for path in &files {
            let Ok(bytes) = archive.extract(path) else {
                continue;
            };
            // A hard parse error is a different failure mode (covered by
            // `parse_real_nifs.rs`); this pin is specifically about
            // *silent* block loss on an otherwise-Ok parse.
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            parsed += 1;
            if scene.truncated || scene.dropped_block_count > 0 {
                truncating.insert(format!("{name}::{path}"), scene.dropped_block_count);
            }
        }
        eprintln!("[Oblivion] {name}: {parsed}/{} NIFs parsed", files.len());
        total_parsed += parsed;
        archive_parsed.insert((*name).to_string(), parsed);
    }

    eprintln!(
        "[Oblivion] block-count parity across {} archive(s): {}/{} NIFs whole, {} truncating",
        archives.len(),
        total_parsed - truncating.len(),
        total_parsed,
        truncating.len()
    );

    let path = baseline_path("oblivion_truncations");
    if regen_mode() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("create baselines dir");
        // #3082 — `parsed` gets its own plain `key\tvalue` line (matching
        // `unknown_blocks\t{}` in the sized-game baselines below) so it
        // survives the `#`-comment filter on read-back below, instead of
        // living only in the human-readable header comment where the
        // parser could never see it. #3712 — now one `archive_parsed`
        // line per archive rather than a single aggregate `parsed` line,
        // so a future run missing an optional archive can skip that
        // archive's comparison instead of failing on a lower total.
        let mut body = format!(
            "# Oblivion sizeless-truncation baseline\ttruncating={}\tparsed={}\n",
            truncating.len(),
            total_parsed,
        );
        for (name, count) in &archive_parsed {
            body.push_str(&format!("archive_parsed\t{name}\t{count}\n"));
        }
        for (key, dropped) in &truncating {
            body.push_str(&format!("{key}\t{dropped}\n"));
        }
        std::fs::write(&path, &body).expect("write baseline");
        eprintln!("[Oblivion] regen mode: wrote {}", path.display());
        return;
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => panic!(
            "[Oblivion] no baseline at {} ({}); regenerate with \
             `BYROREDUX_REGEN_BASELINES=1 cargo test -p byroredux-nif \
             --test block_coverage_baselines -- --ignored`",
            path.display(),
            e
        ),
    };
    // #3712 — per-archive baseline counts, keyed by archive name. An
    // archive present in `archives` today with no matching baseline line
    // means the checked-in baseline is stale (regenerated on a host
    // missing an archive this run has) — panic rather than silently
    // treating it as a 0-count pass, since that would hide every future
    // regression on that archive.
    let baseline_archive_parsed: std::collections::HashMap<String, usize> = text
        .lines()
        .filter_map(|l| l.strip_prefix("archive_parsed\t"))
        .filter_map(|rest| {
            let (name, count) = rest.split_once('\t')?;
            count.trim().parse().ok().map(|c| (name.to_string(), c))
        })
        .collect();
    let baseline: BTreeSet<String> = text
        .lines()
        .filter(|l| {
            !l.trim().is_empty() && !l.starts_with('#') && !l.starts_with("archive_parsed\t")
        })
        .filter_map(|l| l.split('\t').next().map(str::to_string))
        .collect();

    let new_truncations: Vec<&String> = truncating
        .keys()
        .filter(|p| !baseline.contains(*p))
        .collect();

    if !new_truncations.is_empty() {
        for p in new_truncations.iter().take(20) {
            eprintln!("  NEW TRUNCATION {p}: dropped {}", truncating[*p]);
        }
        panic!(
            "[Oblivion] {} file(s) newly lose blocks to sizeless truncation — \
             not in the checked-in baseline. A sizeless block lost its dispatch \
             arm (F-01-class regression). If intentional, regenerate with \
             `BYROREDUX_REGEN_BASELINES=1`.",
            new_truncations.len()
        );
    }
    // #3082 — the other direction: a hard-parse-failure regression that
    // doesn't touch the truncating set at all. `parsed` counts every NIF
    // `parse_nif` didn't hard-error on, truncated or not, so a decrease
    // here means files that used to parse (even truncated) now fail
    // outright — a distinct, previously-invisible failure mode from
    // "newly truncating". An increase (previously-failing files now
    // parsing) is a silent improvement, same policy as every other gate
    // in this file. #3712 — checked per archive present this run; an
    // optional archive absent today is simply skipped, not treated as a
    // drop to zero.
    for (name, &parsed) in &archive_parsed {
        let Some(&baseline_parsed) = baseline_archive_parsed.get(name) else {
            panic!(
                "[Oblivion] '{name}' has no `archive_parsed` baseline line — the \
                 checked-in baseline is stale for this archive; regenerate with \
                 `BYROREDUX_REGEN_BASELINES=1`.",
            );
        };
        assert!(
            parsed >= baseline_parsed,
            "[Oblivion] '{name}' parsed NIF count dropped {} -> {} — {} file(s) that used \
             to parse (truncated or not) now hard-fail. If intentional, regenerate with \
             `BYROREDUX_REGEN_BASELINES=1`.",
            baseline_parsed,
            parsed,
            baseline_parsed.saturating_sub(parsed),
        );
    }
    eprintln!(
        "[Oblivion] no new truncation ({} known, all in baseline); {} archive(s) checked, \
         all parsed >= baseline",
        truncating.len(),
        archive_parsed.len(),
    );
}

// ── Sized games: NiUnknown-rate ceiling ───────────────────────────────

/// Aggregate parse-block coverage for one sized game.
struct Coverage {
    total_blocks: usize,
    unknown_blocks: usize,
}

impl Coverage {
    fn rate(&self) -> f64 {
        if self.total_blocks == 0 {
            0.0
        } else {
            self.unknown_blocks as f64 / self.total_blocks as f64
        }
    }
}

fn measure_coverage(game: Game) -> Option<Coverage> {
    // #2334 — sample every vanilla mesh archive, matching
    // `per_block_baselines.rs`. The two harnesses must walk the same
    // corpus or their totals describe different things.
    let archives = open_all_mesh_archives(game)?;
    let mut hist = common::PerBlockHistogram::new();
    for (_name, archive) in &archives {
        let (_stats, archive_hist) = common::parse_archive_with_histogram(archive, None);
        hist.merge(&archive_hist);
    }
    let total_blocks: usize = hist.counts.values().map(|c| c.parsed + c.unknown).sum();
    Some(Coverage {
        total_blocks,
        unknown_blocks: hist.total_unknown(),
    })
}

/// Baseline file: two `key\tvalue` lines (`total_blocks`, `unknown_blocks`)
/// plus a `#` header. Hand-readable and diffable.
fn baseline_path(stem: &str) -> PathBuf {
    baselines_dir().join(format!("{stem}.tsv"))
}

fn write_baseline(path: &std::path::Path, cov: &Coverage) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("create baselines dir");
    let body = format!(
        "# block coverage baseline (NiUnknown ceiling)\n\
         total_blocks\t{}\nunknown_blocks\t{}\n",
        cov.total_blocks, cov.unknown_blocks
    );
    std::fs::write(path, body).expect("write baseline");
}

fn read_baseline_unknown(text: &str) -> usize {
    text.lines()
        .find_map(|l| l.strip_prefix("unknown_blocks\t"))
        .and_then(|v| v.trim().parse().ok())
        .expect("baseline missing `unknown_blocks` line")
}

/// Shared driver for the sized-game ceiling tests.
fn run_unknown_ceiling(game: Game, stem: &str) {
    let Some(cov) = measure_coverage(game) else {
        return; // no data on this host — skip, like the sibling harnesses.
    };
    eprintln!(
        "[{}] {} blocks, {} NiUnknown ({:.4}% recovery rate)",
        game.label(),
        cov.total_blocks,
        cov.unknown_blocks,
        cov.rate() * 100.0
    );

    let path = baseline_path(stem);
    if regen_mode() {
        write_baseline(&path, &cov);
        eprintln!("[{}] regen mode: wrote {}", game.label(), path.display());
        return;
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => panic!(
            "[{}] no baseline at {} ({}); regenerate with \
             `BYROREDUX_REGEN_BASELINES=1 cargo test -p byroredux-nif \
             --test block_coverage_baselines -- --ignored`",
            game.label(),
            path.display(),
            e
        ),
    };
    let baseline_unknown = read_baseline_unknown(&text);

    // Vanilla archives are fixed content, so the counts are deterministic:
    // a parser change can only lower the unknown count (improvement) or
    // raise it (regression). Improvement is silent; growth fails.
    assert!(
        cov.unknown_blocks <= baseline_unknown,
        "[{}] NiUnknown recovery count grew {} -> {} ({} blocks total). A block \
         type that used to dispatch now lands on the NiUnknown recovery path. \
         Investigate via `per_block_baselines`; if intentional, regenerate with \
         `BYROREDUX_REGEN_BASELINES=1`.",
        game.label(),
        baseline_unknown,
        cov.unknown_blocks,
        cov.total_blocks,
    );
    eprintln!(
        "[{}] NiUnknown ceiling OK ({} <= {})",
        game.label(),
        cov.unknown_blocks,
        baseline_unknown
    );
}

#[test]
#[ignore = "needs FO3 game data on disk"]
fn unknown_ceiling_fallout_3() {
    run_unknown_ceiling(Game::Fallout3, "fallout_3");
}

#[test]
#[ignore = "needs FNV game data on disk"]
fn unknown_ceiling_fallout_nv() {
    run_unknown_ceiling(Game::FalloutNV, "fallout_nv");
}

#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn unknown_ceiling_skyrim_se() {
    run_unknown_ceiling(Game::SkyrimSE, "skyrim_se");
}

#[test]
#[ignore = "needs FO4 game data on disk"]
fn unknown_ceiling_fallout_4() {
    run_unknown_ceiling(Game::Fallout4, "fallout_4");
}

#[test]
#[ignore = "needs FO76 game data on disk"]
fn unknown_ceiling_fallout_76() {
    run_unknown_ceiling(Game::Fallout76, "fallout_76");
}

#[test]
#[ignore = "needs Starfield game data on disk"]
fn unknown_ceiling_starfield() {
    run_unknown_ceiling(Game::Starfield, "starfield");
}
