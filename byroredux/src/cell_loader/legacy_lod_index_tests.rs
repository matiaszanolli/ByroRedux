//! #5222 — real-data coverage for the Fallout-legacy authored-LOD-quad
//! index and the footprint selection that replaced the `(0,0)`-anchored
//! lattice. The synthetic selection tests live beside
//! [`super::lod_bands::select_authored_lod_quads`]; this file proves the
//! whole scan → index → select chain against the real FO3 archives, on the
//! exact worldspaces the audit censused (`dcworld03`, `washmontop`).
//!
//! `#[ignore]`-gated like every other real-game-data test. Run with:
//!
//!   cargo test -p byroredux --bin byroredux legacy_lod -- --ignored

use super::lod_bands::{quad_min_chebyshev, select_authored_lod_quads, LodBandLadder};
use super::LegacyLodQuadIndex;
use byroredux_plugin::esm::reader::GameKind;

fn fo3_data_dir() -> std::path::PathBuf {
    if let Some(v) = std::env::var_os("BYROREDUX_FO3_DATA").filter(|s| !s.is_empty()) {
        let p = std::path::PathBuf::from(v);
        assert!(p.is_dir(), "BYROREDUX_FO3_DATA points to {p:?}, which is not a directory");
        return p;
    }
    std::path::PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data")
}

/// The audit's headline population, on real data: `dcworld03`'s level-8
/// quads sit on the (0, 6) residue — unreachable from any origin the old
/// `quad_origin` derivation could produce — and every one of the #3502
/// level-8-only worldspaces' quads were never requested. The authored
/// selection must draw them near the worldspace centre (the centroid of
/// the authored quads; no ESM load needed for that proxy), and it must
/// never emit a quad the archive does not author.
#[test]
#[ignore = "needs FO3 game data on disk"]
fn dcworld03_level8_quads_select_near_the_worldspace_centre() {
    let data = fo3_data_dir();
    let meshes = data.join("Fallout - Meshes.bsa");
    if !meshes.is_file() {
        eprintln!("[FO3/LOD] skipping: {meshes:?} not found");
        return;
    }
    let provider = crate::asset_provider::build_texture_provider(&[
        "--bsa".to_owned(),
        meshes.to_string_lossy().into_owned(),
    ]);
    let index = LegacyLodQuadIndex::scan(&provider);

    let quads = index.objects_for("dcworld03");
    assert!(!quads.is_empty(), "dcworld03 authored object quads must index");

    // Off-lattice proof (census: L8 residue (0, 6)) — the reason the
    // origin-deriving selection could never reach them.
    let level8: Vec<_> = quads.iter().copied().filter(|&(l, _, _)| l == 8).collect();
    assert!(!level8.is_empty(), "dcworld03 authors level-8 quads");
    assert!(
        level8
            .iter()
            .any(|&(_, qx, qy)| (qx.rem_euclid(8), qy.rem_euclid(8)) != (0, 0)),
        "every dcworld03 level-8 quad is (0,0)-aligned? then this test no \
         longer pins the #5222 defect class"
    );

    // Player at the worldspace centre proxy: the centroid of the authored
    // quads' centres.
    let n = quads.len() as i64;
    let (sx, sy) = quads
        .iter()
        .fold((0i64, 0i64), |(ax, ay), &(l, qx, qy)| (ax + qx as i64 + l as i64 / 2, ay + qy as i64 + l as i64 / 2));
    let player = ((sx / n) as i32, (sy / n) as i32);
    let ladder = LodBandLadder::for_object_game(GameKind::Fallout3NV).expect("FO3 object ladder");
    let selected = select_authored_lod_quads(&quads, &ladder, player, None);
    assert!(
        !selected.is_empty(),
        "the authored selection must draw something at the worldspace centre"
    );
    assert!(
        selected.iter().any(|&(l, qx, qy)| l == 8
            && quad_min_chebyshev(qx, qy, 8, player) < 16),
        "a level-8 quad must be selected inside the old 16-cell hollow — \
         the #3502/#5222 population (dcworld03 has no level-4 siblings for \
         its level-8 middle band)"
    );
    for q in &selected {
        assert!(quads.contains(q), "selected {q:?} is not an authored quad");
    }
}

/// The terrain half of the index (the issue's SIBLING check): the authored
/// DDS quads come out of the TEXTURES archive's name table through the
/// same scan, and `dcworld03`'s are off-lattice exactly like its blocks.
#[test]
#[ignore = "needs FO3 game data on disk"]
fn dcworld03_terrain_diffuse_quads_index_off_lattice() {
    let data = fo3_data_dir();
    let meshes = data.join("Fallout - Meshes.bsa");
    let textures = data.join("Fallout - Textures.bsa");
    if !meshes.is_file() || !textures.is_file() {
        eprintln!("[FO3/LOD] skipping: archives not found");
        return;
    }
    let provider = crate::asset_provider::build_texture_provider(&[
        "--bsa".to_owned(),
        meshes.to_string_lossy().into_owned(),
        "--textures-bsa".to_owned(),
        textures.to_string_lossy().into_owned(),
    ]);
    let index = LegacyLodQuadIndex::scan(&provider);

    let terrain = index.terrain_for("dcworld03");
    assert!(
        !terrain.is_empty(),
        "dcworld03 authored terrain DDS quads must index (census: 80 across \
         level 4/8)"
    );
    assert!(
        terrain
            .iter()
            .any(|&(l, qx, qy)| (qx.rem_euclid(l), qy.rem_euclid(l)) != (0, 0)),
        "the census puts dcworld03's terrain quads off the (0,0) lattice — \
         the population the old DDS lookup never resolved"
    );
}

/// `washmontop` is the audit's irregular case: level-8 quads on THREE
/// different y residues — no single lattice, origin-derived or otherwise,
/// can enumerate them. The index does.
#[test]
#[ignore = "needs FO3 game data on disk"]
fn washmontop_level8_spans_multiple_residues_and_all_select() {
    let data = fo3_data_dir();
    let meshes = data.join("Fallout - Meshes.bsa");
    if !meshes.is_file() {
        eprintln!("[FO3/LOD] skipping: {meshes:?} not found");
        return;
    }
    let provider = crate::asset_provider::build_texture_provider(&[
        "--bsa".to_owned(),
        meshes.to_string_lossy().into_owned(),
    ]);
    let index = LegacyLodQuadIndex::scan(&provider);

    let quads = index.objects_for("washmontop");
    assert!(!quads.is_empty(), "washmontop authored object quads must index");
    let level8: Vec<_> = quads.iter().copied().filter(|&(l, _, _)| l == 8).collect();
    let residues: std::collections::HashSet<(i32, i32)> = level8
        .iter()
        .map(|&(_, qx, qy)| (qx.rem_euclid(8), qy.rem_euclid(8)))
        .collect();
    assert!(
        residues.len() >= 2,
        "census: washmontop level 8 spans y residues 2/4/7 — found {residues:?}"
    );

    // Select from one residue's quad: the multi-residue siblings are
    // independent quads, and the selection must not assume one lattice.
    let ladder = LodBandLadder::for_object_game(GameKind::Fallout3NV).expect("FO3 object ladder");
    for &(l, qx, qy) in level8.iter().take(3) {
        let player = (qx + l / 2, qy + l / 2);
        let selected = select_authored_lod_quads(&quads, &ladder, player, None);
        assert!(
            selected.contains(&(l, qx, qy)),
            "standing in quad ({qx}, {qy}), that quad must be selected"
        );
    }
}
