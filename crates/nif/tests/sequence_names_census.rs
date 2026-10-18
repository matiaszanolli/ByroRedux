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
        if !path.ends_with(".nif") || path.contains("characters") || path.contains("creatures") {
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

#[test]
#[ignore = "needs vanilla game data"]
fn object_sequence_names() {
    for game in [Game::FalloutNV, Game::Oblivion] {
        let names = census(game);
        let mut rows: Vec<_> = names.into_iter().collect();
        rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        eprintln!("== {game:?}: {} distinct sequence names", rows.len());
        for (name, n) in rows.iter().take(25) {
            eprintln!("  {n:>5}  {name}");
        }
    }
}
