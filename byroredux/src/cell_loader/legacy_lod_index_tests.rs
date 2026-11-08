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
use crate::cell_loader::{dlodsettings_path, parse_dlodsettings, DlodSettings};
use super::LegacyLodQuadIndex;
use byroredux_plugin::esm::reader::GameKind;

use byroredux_plugin::esm::test_paths::{self, fo3_data_dir};

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
        test_paths::require_or_skip(&format!("{meshes:?}"));
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
    let selected = select_authored_lod_quads(&quads, &ladder, player, None, &|_, _| false);
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
        test_paths::require_or_skip("the FO3 LOD archives");
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
        test_paths::require_or_skip(&format!("{meshes:?}"));
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

    // #5387 — the multi-residue siblings are leftovers from different
    // LOD generations whose footprints overlap; selection keeps the
    // MAJORITY lattice and must select its members, never draw the
    // overlapping minority.
    let ladder = LodBandLadder::for_object_game(GameKind::Fallout3NV).expect("FO3 object ladder");

    // The majority residue class of level 8.
    let mut counts: std::collections::HashMap<(i32, i32), usize> =
        std::collections::HashMap::new();
    for &(_, qx, qy) in level8.iter() {
        *counts.entry((qx.rem_euclid(8), qy.rem_euclid(8))).or_insert(0) += 1;
    }
    let (majority, _) = counts
        .iter()
        .max_by_key(|(residue, count)| (**count, std::cmp::Reverse(**residue)))
        .expect("washmontop level 8 is non-empty");
    let majority_quads: Vec<_> = level8
        .iter()
        .copied()
        .filter(|&(_, qx, qy)| (qx.rem_euclid(8), qy.rem_euclid(8)) == *majority)
        .collect();

    for &(l, qx, qy) in majority_quads.iter().take(3) {
        let player = (qx + l / 2, qy + l / 2);
        let selected = select_authored_lod_quads(&quads, &ladder, player, None, &|_, _| false);
        assert!(
            selected.contains(&(l, qx, qy)),
            "standing in majority-generation quad ({qx}, {qy}), that quad must be selected"
        );
        // #5387's real-data pin: no two selected same-level quads'
        // footprints overlap anywhere on washmontop.
        for level in [4i32, 8] {
            let same_level: Vec<_> =
                selected.iter().copied().filter(|&(l, _, _)| l == level).collect();
            for (i, &(l1, x1, y1)) in same_level.iter().enumerate() {
                for &(l2, x2, y2) in same_level.iter().skip(i + 1) {
                    let overlap = x1 < x2 + l2
                        && x2 < x1 + l1
                        && y1 < y2 + l2
                        && y2 < y1 + l1;
                    assert!(
                        !overlap,
                        "selected same-level quads ({l1},{x1},{y1}) and ({l2},{x2},{y2}) \
                         overlap — the leftover-generation prune missed them (#5387)"
                    );
                }
            }
        }
    }
}

// ── DLOD-BAND-003 / -004: the `.dlodsettings` descriptor ─────────────────────────────────────────────

fn descriptor_bytes(top: u32, sw: (i16, i16), ne: (i16, i16), lowest: u32) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&4u32.to_le_bytes());
    b.extend_from_slice(&top.to_le_bytes());
    b.extend_from_slice(&(4 * top).to_le_bytes());
    for v in [sw.0, sw.1, ne.0, ne.1] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&lowest.to_le_bytes());
    b
}

/// V8 -- the 24-byte layout, and nothing else, is a descriptor.
#[test]
fn dlodsettings_parses_the_24_byte_layout_and_rejects_everything_else() {
    let good = descriptor_bytes(32, (-40, -24), (60, 52), 4);
    assert_eq!(
        parse_dlodsettings(&good),
        Some(DlodSettings { top_level: 32, sw: (-40, -24), ne: (60, 52), lowest_level: 4 })
    );
    assert_eq!(dlodsettings_path("wasteland"), "lodsettings\\wasteland.dlodsettings");
    assert_eq!(parse_dlodsettings(&good[..23]), None, "truncated");
    let mut long = good.clone();
    long.push(0);
    assert_eq!(parse_dlodsettings(&long), None, "trailing bytes");
    let mut wrong_first = good.clone();
    wrong_first[0] = 5;
    assert_eq!(parse_dlodsettings(&wrong_first), None);
    let mut odd_extent = good.clone();
    odd_extent[8] = 9;
    assert!(parse_dlodsettings(&odd_extent).is_some(), "two vanilla descriptors' third word is not 4 x the second");
    assert_eq!(parse_dlodsettings(&descriptor_bytes(0, (0, 0), (0, 0), 4)), None, "no top level");
}

/// DLOD-BAND-004 -- the SW cell is the lattice origin only where every authored quad sits on it.
#[test]
fn a_descriptor_tiles_only_when_every_quad_is_on_its_lattice() {
    let d = DlodSettings { top_level: 32, sw: (-40, -24), ne: (0, 0), lowest_level: 4 };
    // Level 8 on (-40, -24): x -40, -32; y -24, -16 are all multiples of 8 away from the origin.
    assert!(d.tiles(&[(8, -40, -24), (8, -32, -16), (4, -36, -20), (32, -40, -24)]));
    assert!(!d.tiles(&[(8, -40, -24), (8, -36, -24)]), "a leftover quad half a level off");
    assert!(!d.tiles(&[]), "no quads, no claim");
    // The index combines both families.
    let mut index = LegacyLodQuadIndex::default();
    index.objects.entry("ws".into()).or_default().insert((8, -40, -24));
    index.terrain.entry("ws".into()).or_default().insert((4, -36, -20));
    assert_eq!(index.lattice_origin("ws", &d), Some((-40, -24)));
    index.terrain.entry("ws".into()).or_default().insert((8, -37, -24));
    assert_eq!(index.lattice_origin("ws", &d), None, "one off-lattice terrain quad takes the prune back");
    assert_eq!(index.lattice_origin("other", &d), None);
}

/// DLOD-BAND-004 -- with a tiling descriptor each level keeps the residue class its origin names; without one
/// (or off the lattice) the majority prune decides as before.
#[test]
fn the_descriptor_origin_picks_the_lattice_class_the_majority_would_not() {
    use super::lod_bands::select_authored_lod_quads_refining;
    let ladder = LodBandLadder::for_terrain_game(byroredux_plugin::esm::reader::GameKind::Fallout3NV).unwrap();
    // Two overlapping level-8 classes: (0,0) with one quad, (4,0) with two. The majority is (4,0).
    let quads = vec![(8, 0, 0), (8, 4, 16), (8, 12, 16)];
    let no_descriptor = select_authored_lod_quads_refining(&quads, &ladder, (0, 0), None, &|_, _| false, -1, None);
    assert!(no_descriptor.iter().all(|&(_, x, _)| x.rem_euclid(8) == 4), "the majority class wins: {no_descriptor:?}");
    let with_origin = select_authored_lod_quads_refining(&quads, &ladder, (0, 0), None, &|_, _| false, -1, Some((0, 0)));
    assert_eq!(with_origin, vec![(8, 0, 0)], "the descriptor's lattice names class (0,0)");
}

/// A7 / DLOD-BAND-004 -- over every vanilla FO3 and FNV descriptor, base game and DLC (53), the SW cell tiles
/// every authored `blocks\` and terrain quad for all 25 FNV worldspaces and 21 FO3 ones; the seven FO3
/// worldspaces named in the PRD (§9.1) have quads off it and keep the prune. Run with:
///   cargo test -p byroredux --bin byroredux dlodsettings_origin -- --ignored
#[test]
#[ignore = "needs the vanilla Fallout 3 and New Vegas archives on disk"]
fn dlodsettings_origin_tiles_every_authored_quad() {
    use byroredux_plugin::esm::test_paths;
    use std::collections::BTreeMap;
    let mut tiling: BTreeMap<String, bool> = BTreeMap::new();
    for (title, dir) in [("FO3", test_paths::fo3_data_dir()), ("FNV", test_paths::fnv_data_dir())] {
        let Ok(read) = std::fs::read_dir(&dir) else {
            test_paths::require_or_skip(&format!("{title} install"));
            return;
        };
        let mut quads: BTreeMap<String, Vec<(i32, i32, i32)>> = BTreeMap::new();
        let mut descriptors: Vec<(String, DlodSettings)> = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa")) {
                continue;
            }
            let archive = byroredux_bsa::BsaArchive::open(path.to_str().unwrap()).expect("open archive");
            for file in archive.list_files().into_iter().map(str::to_owned).collect::<Vec<_>>() {
                if let Some((_, ws, level, x, y)) = super::legacy_lod_index::parse_legacy_lod_quad_name(&file) {
                    quads.entry(ws).or_default().push((level, x, y));
                } else if let Some(ws) = file
                    .to_ascii_lowercase()
                    .strip_prefix("lodsettings\\")
                    .and_then(|r| r.strip_suffix(".dlodsettings"))
                {
                    let desc = parse_dlodsettings(&archive.extract(&file).unwrap()).unwrap_or_else(|| panic!("{file} is not a descriptor"));
                    descriptors.push((ws.to_owned(), desc));
                }
            }
        }
        for (ws, desc) in descriptors {
            let all = quads.get(&ws).map(Vec::as_slice).unwrap_or(&[]);
            tiling.insert(format!("{title}:{ws}"), desc.tiles(all));
        }
    }
    let off: Vec<&str> = tiling.iter().filter(|(_, tiles)| !**tiles).map(|(k, _)| k.as_str()).collect();
    eprintln!("{} descriptors; off the SW lattice: {off:?}", tiling.len());
    assert_eq!(tiling.len(), 53, "28 FO3 + 25 FNV descriptors, base game and DLC");
    assert_eq!(tiling.keys().filter(|k| k.starts_with("FNV:")).count(), 25);
    assert!(tiling.iter().filter(|(k, _)| k.starts_with("FNV:")).all(|(_, tiles)| *tiles), "all 25 FNV descriptors tile");
    assert_eq!(
        off,
        [
            "FO3:dcworld01",
            "FO3:dcworld06",
            "FO3:dcworld12",
            "FO3:dcworld17",
            "FO3:dlc02baileyscrossroads",
            "FO3:paradisefalls",
            "FO3:washmontop"
        ]
    );
}
