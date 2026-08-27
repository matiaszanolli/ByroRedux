//! Per-block-type baseline regression test (R3).
//!
//! Closes the blind spot that `parse_real_nifs.rs` and `MIN_RECOVERABLE_RATE
//! = 1.0` leave open: a parser regression where a previously-handled
//! type starts under-consuming registers as a successful "recoverable"
//! parse — the loop seeks past the busted block via `block_size`, drops
//! an `NiUnknown` placeholder, and the file-level rate stays 100%. The
//! geometry is silently missing; the gate stays green.
//!
//! This test gates on the per-type parsed/unknown split. Every vanilla
//! mesh-bearing archive for the game is walked (`Game::mesh_archives`,
//! #2334 — not just the primary one), every NIF is parsed, and the
//! per-archive `PerBlockHistogram`s are merged. The result is compared
//! against a checked-in baseline TSV; any type whose `unknown` count grew
//! (or `parsed` count shrank) is a regression.
//!
//! ## How the rows are keyed
//!
//! **Every row keys on the wire type name** — the string the file's own
//! header advertises for that block — for parsed and unknown blocks alike.
//! So `bhkSPCollisionObject`, `bhkBlendCollisionObject`, `bhkRigidBodyT`,
//! `BSSegmentedTriShape` and every other aliased type each get a standing
//! row of their own, at their real corpus count.
//!
//! Before #3326 parsed blocks keyed on the **struct's**
//! `block_type_name()`, and this section argued that aliasing "still gates
//! them, in both directions". It did not. Many dispatch arms deliberately
//! parse several wire types into one struct and keep the discriminator in a
//! *field* (`BhkRigidBody.is_t`, `BhkCollisionObject.is_blend`,
//! `BsRangeNode.kind`, `NiPSysBlock.original_type`,
//! `NiTriShape::parse_segmented`), so a re-route of wire type X from struct
//! A to struct B where `A::block_type_name() == B::block_type_name()` moved
//! **nothing** in the TSV. On FNV that blind spot covered 142,649 blocks —
//! 21.5% of the corpus — and included the precise shapes of #146
//! (`BSSegmentedTriShape` reverting to plain `NiTriShape::parse`) and #2316
//! (`bhkRigidBodyT` losing `is_t`, silently identity-collapsing the CInfo
//! transform on 28% of FNV rigid bodies). Both have synthetic-fixture unit
//! tests, so the risk was mitigated — but this gate contributed nothing to
//! catching them, which is not what the docstring above claimed.
//!
//! Two pre-#3326 baseline rows were provably not wire types at all:
//! `NiSingleInterpController` is `abstract="true"` in nif.xml (no such block
//! can exist on disk; the FNV count was `BSMaterialEmittanceMultController`
//! 471 + `BSRefractionStrengthController` 87 + `BSFrustumFOVController` 56 =
//! exactly the 614 it carried), and `NiPSysBlock` is a parser-internal
//! catch-all name that appears nowhere in nif.xml.
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
    compare_histograms, open_all_mesh_archives, parse_archive_with_histogram, BaselineRegression,
    Game, PerBlockHistogram,
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
    // #2334 — every vanilla mesh-bearing archive, not just the primary
    // one. FO3's DLC-only collision types (`bhkSPCollisionObject`,
    // `bhkBlendCollisionObject`, `bhkConvexTransformShape`) were
    // unreachable from this gate while it sampled `Fallout - Meshes.bsa`
    // alone.
    let Some(archives) = open_all_mesh_archives(game) else {
        return;
    };

    let mut hist = PerBlockHistogram::new();
    let mut total_files = 0usize;
    for (name, archive) in &archives {
        eprintln!(
            "[{}] opened {} ({} files)",
            game.label(),
            name,
            archive.file_count()
        );
        let (stats, archive_hist) = parse_archive_with_histogram(archive, None);
        stats.print_summary(&format!("{} / {}", game.label(), name));
        total_files += stats.total;
        hist.merge(&archive_hist);
    }

    eprintln!(
        "[{}] per-block histogram over {} archive(s), {} NIFs: {} distinct types, {} unknown blocks across {} types with partial unknown",
        game.label(),
        archives.len(),
        total_files,
        hist.counts.len(),
        hist.total_unknown(),
        hist.types_with_partial_unknown(),
    );

    let path = baseline_path(game);

    if regen_mode() {
        let dir = baselines_dir();
        std::fs::create_dir_all(&dir).expect("create baselines dir");
        let tsv = hist.to_tsv(total_files);
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

/// #2354 / SF-D8-03 — the NIFAL particle slice
/// (`extract_emitter_params`/`extract_emitter_rate` in
/// `crates/nif/src/import/walk/mod.rs`) is structurally unreachable on
/// Starfield: the 31,058-file Meshes01 corpus histogram (this test's
/// sibling, `per_block_baseline_starfield`, captured it) contains zero
/// `NiPSys*`/`NiParticleSystem*` blocks — Starfield authors particle
/// systems entirely outside the NIF container. Unlike the sibling test
/// above, this one needs no game data (it reads the already-checked-in
/// baseline TSV) and is NOT `#[ignore]`d, so a future format discovery
/// that starts shipping particle blocks in Starfield content flips this
/// test red on every `cargo test`, instead of the NIFAL particle
/// regression suite (#1411/#1434/#1445/#1771/#1775, all
/// Oblivion/FO3/FNV/Skyrim-driven) silently saying nothing about
/// Starfield coverage. See `docs/engine/nifal.md`'s Particles section.
#[test]
fn starfield_corpus_has_no_particle_blocks() {
    let path = baseline_path(Game::Starfield);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read checked-in baseline {}: {e}", path.display()));
    let hist = PerBlockHistogram::from_tsv(&text)
        .unwrap_or_else(|e| panic!("parse checked-in baseline {}: {e}", path.display()));
    let particle_types: Vec<&str> = hist
        .counts
        .keys()
        .map(String::as_str)
        .filter(|name| name.contains("PSys") || name.contains("Particle"))
        .collect();
    assert!(
        particle_types.is_empty(),
        "Starfield corpus baseline now contains particle block type(s) {particle_types:?} — \
         the NIFAL particle slice is reachable on Starfield after all. Update \
         `docs/engine/nifal.md`'s \"Starfield: particle slice N/A\" note and wire up \
         Starfield particle-emitter translation instead of treating this as a bug in \
         the test.",
    );
}
