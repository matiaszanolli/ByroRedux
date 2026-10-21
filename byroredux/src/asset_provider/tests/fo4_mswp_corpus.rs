//! Real-data pin for the MSWP source compare — #5755
//! (FO4-2026-10-12-D4-01).
//!
//! Every synthetic MSWP test used to author one literal on both sides, so
//! none could see that shipped data never shares a spelling: `BNAM` is bare
//! (relative to `Data\Materials\`) while a NIF names its shader material
//! `Materials\…` or by the absolute build path. The verbatim compare
//! matched 0 of 7,132 resolvable unbaked `XMSP` placements in
//! `Fallout4.esm`. This pin takes both strings from the game files: the
//! `PaintingGeneric_CanvasSwap` swaps from `Fallout4.esm` and the shader
//! names in `PictureFrame05Dest.nif`. Gated `#[ignore]` on FO4 data;
//! `cargo test -p byroredux --bin byroredux fo4_mswp -- --ignored`.

use super::fo4_palette_corpus::fo4_data_dir;
use crate::cell_loader::{mswp_source_matches, same_material_path};

#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn fo4_mswp_canvas_swap_matches_the_picture_frame_nif() {
    let data = fo4_data_dir();
    let esm_path = data.join("Fallout4.esm");
    let meshes_path = data.join("Fallout4 - Meshes.ba2");
    if !esm_path.is_file() || !meshes_path.is_file() {
        eprintln!("Skipping: {esm_path:?} or {meshes_path:?} not found");
        return;
    }

    let esm = std::fs::read(&esm_path).expect("read Fallout4.esm");
    let index = byroredux_plugin::esm::parse_esm(&esm).expect("parse Fallout4.esm");
    let canvas_swaps: Vec<_> = index
        .cells
        .material_swaps
        .values()
        .flat_map(|record| record.swaps.iter())
        .filter(|entry| {
            entry
                .source
                .to_ascii_lowercase()
                .ends_with("paintinggeneric_canvasswap.bgsm")
        })
        .collect();
    assert!(
        !canvas_swaps.is_empty(),
        "Fallout4.esm must author PaintingGeneric_CanvasSwap MSWP entries"
    );

    let archive = byroredux_bsa::Ba2Archive::open(&meshes_path).expect("open meshes BA2");
    let nif_path = archive
        .list_files()
        .into_iter()
        .find(|p| p.to_ascii_lowercase().ends_with("pictureframe05dest.nif"))
        .expect("Fallout4 - Meshes.ba2 must ship PictureFrame05Dest.nif")
        .to_string();
    let bytes = archive
        .extract(&nif_path)
        .expect("extract PictureFrame05Dest.nif");
    let nif = byroredux_nif::parse_nif(&bytes).expect("parse PictureFrame05Dest.nif");
    let mut pool = byroredux_core::string::StringPool::new();
    let meshes = byroredux_nif::import::import_nif(&nif, &mut pool);
    let shader_names: Vec<String> = meshes
        .iter()
        .filter_map(|mesh| mesh.material.material_path)
        .filter_map(|sym| pool.resolve(sym).map(str::to_owned))
        .collect();
    let canvas = shader_names
        .iter()
        .find(|name| name.ends_with("paintinggeneric_canvasswap.bgsm"))
        .unwrap_or_else(|| panic!("no CanvasSwap shader among {shader_names:?}"));

    eprintln!(
        "MSWP pin: NIF names {canvas:?}; {} CanvasSwap entries, first BNAM {:?}",
        canvas_swaps.len(),
        canvas_swaps[0].source
    );
    assert!(
        canvas_swaps
            .iter()
            .all(|entry| !entry.source.eq_ignore_ascii_case(canvas)),
        "the two sides now share a spelling; this pin no longer exercises #5755"
    );
    assert!(
        canvas_swaps
            .iter()
            .all(|entry| mswp_source_matches(&entry.source, canvas)),
        "every CanvasSwap BNAM must match the NIF's own shader name"
    );
    assert!(
        canvas_swaps
            .iter()
            .any(|entry| !entry.target.is_empty() && !same_material_path(&entry.target, canvas)),
        "at least one CanvasSwap entry must change the frame's material"
    );
}
