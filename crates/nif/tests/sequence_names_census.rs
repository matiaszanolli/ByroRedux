//! Census of the `NiControllerSequence` names inside vanilla meshes — what an ObScript
//! `PlayGroup Forward` (an AnimGroup index) has to be matched against. M47.3.
//!
//! `#[ignore]` because it needs vanilla game data; run with
//! `cargo test -p byroredux-nif --test sequence_names_census -- --ignored --nocapture`.

mod common;

use std::collections::BTreeMap;

use common::{open_mesh_archive, Game};

use byroredux_nif::anim::import_kf;
use byroredux_nif::parse_nif;

fn census(game: Game) -> BTreeMap<String, usize> {
    let Some(archive) = open_mesh_archive(game) else {
        return BTreeMap::new();
    };
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    for path in archive.list_files() {
        if !byroredux_nif::corpus::is_nif_entry(&path) || path.contains("characters") || path.contains("creatures") {
            continue;
        }
        let Ok(bytes) = archive.extract(&path) else { continue };
        let Ok(scene) = parse_nif(&bytes) else { continue };
        for clip in import_kf(&scene) {
            *names.entry(clip.name.to_ascii_lowercase()).or_default() += 1;
        }
    }
    names
}

/// The AnimGroup names an ObScript `PlayGroup` is matched against must exist in the object meshes'
/// `NiControllerSequence`s, in numbers well above a stray hit. Measured 2026-11-06 over the
/// non-character, non-creature meshes: FNV 119 distinct names (forward 217, specialidle 156, open and
/// close 148, backward 145, idle 136), Oblivion 82 (forward 130, idle 96, open 86, close 84, backward
/// 66). Floors sit near 60 % of those, so the vanilla data cannot drift under them but a regression
/// that stops `import_kf` finding sequences fails (#6402).
#[test]
#[ignore = "needs vanilla game data"]
fn object_sequence_names() {
    let floors: [(Game, &[(&str, usize)]); 2] = [
        (
            Game::FalloutNV,
            &[("forward", 130), ("backward", 85), ("idle", 80), ("open", 85), ("close", 85), ("specialidle", 90)],
        ),
        (Game::Oblivion, &[("forward", 75), ("backward", 40), ("idle", 55), ("open", 50), ("close", 50)]),
    ];
    for (game, wanted) in floors {
        let names = census(game);
        if names.is_empty() {
            // `census` returns empty when the archive is absent; the test is `#[ignore]`d for exactly that.
            continue;
        }
        for &(name, floor) in wanted {
            let count = names.get(name).copied().unwrap_or(0);
            assert!(count >= floor, "{game:?}: sequence '{name}' appears {count} times, floor {floor}");
        }
    }
}
