//! Per-game NIF parse-rate integration tests.
//!
//! These tests walk a real game's mesh archive, parse every `.nif`, and
//! assert that at least `MIN_SUCCESS_RATE` of them parse without error.
//! They are `#[ignore]`d by default because they require game data and
//! run for several seconds. Opt in with:
//!
//! ```sh
//! cargo test -p byroredux-nif --test parse_real_nifs -- --ignored
//! ```
//!
//! Point the `BYROREDUX_*_DATA` env vars at your `Data/` directories if
//! your install path differs from the defaults in `common::Game::default_path`.

mod common;

use common::{open_mesh_archive, parse_all_nifs_in_archive, Game};

/// Acceptance threshold per N23.10 + ROADMAP. Gates on the
/// **recoverable** rate (clean + NiUnknown-recovered + truncated) so a
/// hard parse failure on any vanilla NIF is a regression. Every
/// supported game currently hits 100% recoverable — the recovery paths
/// (block_size seek, runtime size cache, `oblivion_skip_sizes` hint,
/// dispatch-level unknown-type fallback) absorb under-consuming parser
/// bugs by substituting `NiUnknown` placeholders and continuing.
///
/// The `clean` rate (fully parsed, no placeholders) is printed by
/// `ParseStats::print_summary` as a secondary metric. Pre-#568 it
/// masqueraded as the gate metric — the record_success path silently
/// absorbed `NiUnknown` recoveries, so Skyrim's ~55% placeholder rate
/// (from bhkRigidBody and friends) reported as "100% clean". Driving
/// `clean` upward is open work tracked on the individual parser-bug
/// issues (e.g. #546). This gate stays at recoverable so hard-failure
/// regressions still fail loud and clear.
///
/// If a future mod-content test tolerates partial coverage, define a
/// separate `MIN_SUCCESS_RATE_MOD` and use it there rather than
/// loosening the vanilla gate. See issue #487.
const MIN_RECOVERABLE_RATE: f64 = 1.0;

fn run_game(game: Game, limit: Option<usize>) {
    let Some(archive) = open_mesh_archive(game) else {
        return; // Skip if game data not available — common::open_mesh_archive prints the reason.
    };

    eprintln!(
        "[{}] opened {} ({} files)",
        game.label(),
        game.mesh_archive(),
        archive.file_count()
    );

    let stats = parse_all_nifs_in_archive(&archive, limit);
    stats.print_summary(game.label());

    assert!(
        stats.total > 0,
        "[{}] expected at least one NIF in archive",
        game.label()
    );
    assert!(
        stats.recoverable_rate() >= MIN_RECOVERABLE_RATE,
        "[{}] parse recoverable rate {:.2}% is below the {:.0}% threshold ({} hard failures)",
        game.label(),
        stats.recoverable_rate() * 100.0,
        MIN_RECOVERABLE_RATE * 100.0,
        stats.failures.len()
    );
}

#[test]
#[ignore]
fn parse_rate_fallout_nv() {
    run_game(Game::FalloutNV, None);
}

#[test]
#[ignore]
fn parse_rate_fallout_3() {
    run_game(Game::Fallout3, None);
}

#[test]
#[ignore]
fn parse_rate_skyrim_se() {
    run_game(Game::SkyrimSE, None);
}

#[test]
#[ignore]
fn parse_rate_oblivion() {
    // Oblivion BSA v103 uses zlib compression (handled in
    // `crates/bsa/src/archive.rs:470-475`). Previous "decompression not
    // yet implemented" comment was stale after M26+.
    run_game(Game::Oblivion, None);
}

#[test]
#[ignore]
fn parse_rate_fallout_4() {
    run_game(Game::Fallout4, None);
}

#[test]
#[ignore]
fn parse_rate_fallout_76() {
    run_game(Game::Fallout76, None);
}

#[test]
#[ignore]
fn parse_rate_starfield() {
    // Starfield meshes use BA2 v2 GNRL with the 32-byte header extension.
    // Texture archives (BA2 v3 DX10) use a different chunk layout that's
    // not yet supported and is tracked separately.
    run_game(Game::Starfield, None);
}

/// Smoke subset — runs the first 50 NIFs from each available game in one
/// test so `cargo test -- --ignored` gives a fast signal without waiting
/// for the full per-game sweep. Useful during parser refactors.
#[test]
#[ignore]
fn parse_rate_smoke_all_games() {
    for game in [
        Game::FalloutNV,
        Game::Fallout3,
        Game::SkyrimSE,
        Game::Oblivion,
        Game::Fallout4,
        Game::Fallout76,
        Game::Starfield,
    ] {
        let Some(archive) = open_mesh_archive(game) else {
            continue;
        };
        let stats = parse_all_nifs_in_archive(&archive, Some(50));
        stats.print_summary(&format!("{} (smoke)", game.label()));
        if stats.total > 0 {
            assert!(
                stats.recoverable_rate() >= MIN_RECOVERABLE_RATE,
                "[{} smoke] parse recoverable rate {:.2}% below threshold",
                game.label(),
                stats.recoverable_rate() * 100.0,
            );
        }
    }
}

/// #401 — particle emitters must surface from real game content. Walks
/// the first up-to-200 NIFs in any candidate folder (`fire`, `smoke`,
/// `fx`, etc.) of an available reference archive, parses each, and
/// asserts that at least one produces an [`ImportedParticleEmitterFlat`].
/// Pre-fix the importer dropped every NiPSysBlock and every torch
/// rendered as an invisible node — this test would have caught it.
///
/// Robust to archive layout differences across games and mods: tries
/// FNV, Fallout 3, Oblivion, Skyrim SE in turn until one of them has
/// the expected folders. Fails only if every available archive yields
/// zero emitters across the full sweep, which would mean the importer
/// regressed (not that the archive layout drifted).
#[test]
#[ignore]
fn real_archive_torch_meshes_surface_particle_emitters() {
    use byroredux_nif::import::import_nif_particle_emitters;

    let candidate_folders = ["fire", "fx", "smoke", "fxsmoke", "magic", "effects"];
    let games_to_try = [
        Game::FalloutNV,
        Game::Fallout3,
        Game::Oblivion,
        Game::SkyrimSE,
    ];

    let mut tried_any_archive = false;
    for game in games_to_try {
        let Some(archive) = open_mesh_archive(game) else {
            continue;
        };
        tried_any_archive = true;
        let all_files = archive.list_files();
        let mut total_emitters = 0usize;
        let mut paths_with_emitters: Vec<String> = Vec::new();
        // Walk up to 200 candidate NIFs per game so the test stays
        // fast (a few seconds) but has enough samples to find at least
        // one emitter in any reasonable mesh archive.
        let candidates: Vec<&String> = all_files
            .iter()
            .filter(|f| {
                let lower = f.to_ascii_lowercase();
                lower.ends_with(".nif")
                    && candidate_folders.iter().any(|c| lower.contains(c))
            })
            .take(200)
            .collect();

        for path in &candidates {
            let bytes = match archive.extract(path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let scene = match byroredux_nif::parse_nif(&bytes) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let emitters = import_nif_particle_emitters(&scene);
            if !emitters.is_empty() {
                total_emitters += emitters.len();
                if paths_with_emitters.len() < 5 {
                    paths_with_emitters.push((*path).clone());
                }
            }
        }

        if total_emitters > 0 {
            eprintln!(
                "[{}] {} emitters across {} meshes (sampled {} NIFs from candidate folders)",
                game.label(),
                total_emitters,
                paths_with_emitters.len(),
                candidates.len(),
            );
            for p in &paths_with_emitters {
                eprintln!("  example: {}", p);
            }
            return; // pass on the first game that yields any emitters
        }
        eprintln!(
            "[{}] no emitters in {} candidate NIFs — trying next game",
            game.label(),
            candidates.len(),
        );
    }

    if !tried_any_archive {
        eprintln!("no reference game data available — skipping (set BYROREDUX_*_DATA env vars)");
        return;
    }
    panic!(
        "no particle emitters surfaced from any reference archive — \
         the importer regressed (the audit's invisible-torch failure \
         mode is back)"
    );
}
