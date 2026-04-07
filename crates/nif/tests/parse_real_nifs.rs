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
