//! #6477 — Oblivion's body NIFs author their underwear / groin cover as a second sub-mesh in the
//! skin's own `textures\characters\` folder. The import boundary must flag the skin meshes
//! (`ImportedMaterial::character_skin`) and not that cover, or the race-skin override repaints the
//! underwear. `#[ignore]`d: needs the Oblivion base meshes archive.
//!
//! ```sh
//! cargo test -p byroredux-nif --test oblivion_body_skin_flag -- --ignored
//! ```

mod common;

use byroredux_core::string::StringPool;
use common::Game;

const BODY_NIFS: &[&str] = &[
    r"meshes\characters\_male\upperbody.nif",
    r"meshes\characters\_male\lowerbody.nif",
    r"meshes\characters\_male\femaleupperbody.nif",
    r"meshes\characters\_male\femalelowerbody.nif",
    r"meshes\characters\_male\hand.nif",
];

#[test]
#[ignore = "needs Oblivion game data on disk"]
fn the_underwear_sub_meshes_of_the_vanilla_body_nifs_are_not_race_skin() {
    let Some(archive) = common::open_mesh_archive(Game::Oblivion) else {
        return;
    };
    let mut skin_meshes = 0usize;
    let mut cover_meshes = 0usize;
    for path in BODY_NIFS {
        let Ok(bytes) = archive.extract(path) else {
            eprintln!("skipping {path}: not in the archive");
            continue;
        };
        let scene = byroredux_nif::parse_nif(&bytes).expect("body NIF parses");
        let mut pool = StringPool::new();
        let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
        for mesh in &imported.meshes {
            let Some(sym) = mesh.material.textures.base_color else { continue };
            let texture = pool.resolve(sym).unwrap_or_default().to_owned();
            let stem = texture.rsplit('\\').next().unwrap_or(&texture);
            let is_cover = stem.starts_with("underwear") || stem.starts_with("groin");
            if is_cover {
                cover_meshes += 1;
                assert!(!mesh.material.character_skin, "{path}: the cover mesh '{texture}' must not be flagged as skin");
            } else if texture.starts_with("textures\\characters\\") {
                skin_meshes += 1;
                assert!(mesh.material.character_skin, "{path}: the skin mesh '{texture}' must be flagged");
            }
        }
    }
    assert!(skin_meshes > 0, "no skin meshes found — the probe read nothing");
    assert!(cover_meshes > 0, "no underwear / groin cover found — the premise (#6477) is not reproduced");
}
