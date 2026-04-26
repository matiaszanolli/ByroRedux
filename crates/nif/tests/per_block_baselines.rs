//! Per-block-type baseline regression test (R3).
//!
//! Closes the blind spot that `parse_real_nifs.rs` and `MIN_RECOVERABLE_RATE
//! = 1.0` leave open: a parser regression where a previously-handled
//! type starts under-consuming registers as a successful "recoverable"
//! parse — the loop seeks past the busted block via `block_size`, drops
//! an `NiUnknown` placeholder, and the file-level rate stays 100%. The
//! geometry is silently missing; the gate stays green.
//!
//! This test gates on the **per-header-type** parsed/unknown split.
//! Each game's mesh archive is walked, every NIF is parsed, and a
//! `PerBlockHistogram` is built keying on the header's advertised type
//! name. Result is compared against a checked-in baseline TSV; any
//! type whose `unknown` count grew (or `parsed` count shrank) is a
//! regression.
//!
//! ## Workflow
//!
//! Capture once per game (run on a machine that has the data):
//!
//! ```sh
//! BYROREDUX_REGEN_BASELINES=1 \
//!   cargo test -p byroredux-nif --test per_block_baselines -- --ignored --nocapture
//! ```
//!
//! That writes `crates/nif/tests/data/per_block_baselines/<game>.tsv`
//! for every game whose data is available. Check those files in.
//! Subsequent runs without the env var assert against the saved
//! baselines and fail loud on regression.
//!
//! ## Why opt-in
//!
//! Same constraint as `parse_real_nifs.rs`: CI machines have no game
//! data. Tests are `#[ignore]`d so `cargo test` stays green by default
//! and intentional invocations capture or validate.

mod common;

use common::{
    compare_histograms, open_mesh_archive, parse_archive_with_histogram, BaselineRegression, Game,
    PerBlockHistogram,
};
use std::path::PathBuf;

/// Directory holding one baseline TSV per supported game. Path is
/// resolved against `CARGO_MANIFEST_DIR` so the test works regardless
/// of `cwd`. Created on demand in regen mode.
fn baselines_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("per_block_baselines")
}

fn baseline_path(game: Game) -> PathBuf {
    baselines_dir().join(format!("{}.tsv", baseline_stem(game)))
}

/// Filesystem-friendly stem for a game's baseline. Lower-case, no
/// spaces — keeps the checked-in filenames diffable across platforms.
fn baseline_stem(game: Game) -> &'static str {
    match game {
        Game::Oblivion => "oblivion",
        Game::Fallout3 => "fallout_3",
        Game::FalloutNV => "fallout_nv",
        Game::SkyrimSE => "skyrim_se",
        Game::Fallout4 => "fallout_4",
        Game::Fallout76 => "fallout_76",
        Game::Starfield => "starfield",
    }
}

fn regen_mode() -> bool {
    std::env::var("BYROREDUX_REGEN_BASELINES")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// Drive the test for a single game. Skips silently when the archive
/// isn't available (consistent with `parse_real_nifs.rs`); writes a
/// new baseline in regen mode; otherwise asserts against the checked-in
/// baseline.
fn run_baseline(game: Game) {
    let Some(archive) = open_mesh_archive(game) else {
        return;
    };
    eprintln!(
        "[{}] opened {} ({} files)",
        game.label(),
        game.mesh_archive(),
        archive.file_count()
    );

    let (stats, hist) = parse_archive_with_histogram(&archive, None);
    stats.print_summary(game.label());
    eprintln!(
        "[{}] per-block histogram: {} distinct types, {} unknown blocks across {} types with partial unknown",
        game.label(),
        hist.counts.len(),
        hist.total_unknown(),
        hist.types_with_partial_unknown(),
    );

    let path = baseline_path(game);

    if regen_mode() {
        let dir = baselines_dir();
        std::fs::create_dir_all(&dir).expect("create baselines dir");
        let tsv = hist.to_tsv(stats.total);
        std::fs::write(&path, &tsv).expect("write baseline");
        eprintln!(
            "[{}] regen mode: wrote baseline to {} ({} bytes)",
            game.label(),
            path.display(),
            tsv.len()
        );
        return;
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => panic!(
            "[{}] no baseline at {} ({}); regenerate with \
             `BYROREDUX_REGEN_BASELINES=1 cargo test -p byroredux-nif \
             --test per_block_baselines -- --ignored`",
            game.label(),
            path.display(),
            e
        ),
    };
    let baseline = PerBlockHistogram::from_tsv(&text).unwrap_or_else(|e| {
        panic!(
            "[{}] corrupt baseline {}: {}",
            game.label(),
            path.display(),
            e
        )
    });

    let regressions = compare_histograms(&hist, &baseline);
    if regressions.is_empty() {
        eprintln!(
            "[{}] per-block baseline OK ({} types matched)",
            game.label(),
            baseline.counts.len()
        );
        return;
    }

    eprintln!(
        "[{}] {} per-block regression(s) vs {}:",
        game.label(),
        regressions.len(),
        path.display()
    );
    for reg in &regressions {
        match reg {
            BaselineRegression::UnknownGrew {
                type_name,
                baseline,
                current,
            } => eprintln!(
                "  UNKNOWN grew  {:>30}  {} -> {}  (parser regression?)",
                type_name, baseline, current
            ),
            BaselineRegression::ParsedShrank {
                type_name,
                baseline,
                current,
            } => eprintln!(
                "  PARSED shrank {:>30}  {} -> {}  (filter or dispatch loss?)",
                type_name, baseline, current
            ),
        }
    }
    panic!(
        "[{}] per-block-type histogram regressed against checked-in baseline. \
         Investigate the listed types; if the change is intentional, regenerate \
         with `BYROREDUX_REGEN_BASELINES=1`.",
        game.label()
    );
}

#[test]
#[ignore]
fn per_block_baseline_fallout_nv() {
    run_baseline(Game::FalloutNV);
}

#[test]
#[ignore]
fn per_block_baseline_fallout_3() {
    run_baseline(Game::Fallout3);
}

#[test]
#[ignore]
fn per_block_baseline_oblivion() {
    run_baseline(Game::Oblivion);
}

#[test]
#[ignore]
fn per_block_baseline_skyrim_se() {
    run_baseline(Game::SkyrimSE);
}

#[test]
#[ignore]
fn per_block_baseline_fallout_4() {
    run_baseline(Game::Fallout4);
}

#[test]
#[ignore]
fn per_block_baseline_fallout_76() {
    run_baseline(Game::Fallout76);
}

#[test]
#[ignore]
fn per_block_baseline_starfield() {
    run_baseline(Game::Starfield);
}
