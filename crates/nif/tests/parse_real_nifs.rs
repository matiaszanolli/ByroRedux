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

/// Acceptance threshold per N23.10. A game is considered "supported" when
/// at least this fraction of its mesh NIFs parse without error.
const MIN_SUCCESS_RATE: f64 = 0.95;

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
        stats.success_rate() >= MIN_SUCCESS_RATE,
        "[{}] parse success rate {:.2}% is below the {:.0}% threshold ({} failures)",
        game.label(),
        stats.success_rate() * 100.0,
        MIN_SUCCESS_RATE * 100.0,
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
    // Oblivion BSA v103 decompression is not yet implemented, so this test
    // will skip cleanly (archive opens but extract fails). Once decompression
    // lands, the threshold applies.
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
                stats.success_rate() >= MIN_SUCCESS_RATE,
                "[{} smoke] parse success rate {:.2}% below threshold",
                game.label(),
                stats.success_rate() * 100.0,
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
