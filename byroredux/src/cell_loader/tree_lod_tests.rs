//! #4913 — the Skyrim tree-LOD family: `.btt` / `.lst` parsing, the
//! billboard mesh and the ring's quad selection.

use super::*;

fn btt_bytes(groups: &[(u32, &[TreeLodInstance])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(groups.len() as u32).to_le_bytes());
    for (species, trees) in groups {
        out.extend_from_slice(&species.to_le_bytes());
        out.extend_from_slice(&(trees.len() as u32).to_le_bytes());
        for tree in *trees {
            for v in tree.position {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&tree.rotation.to_le_bytes());
            out.extend_from_slice(&tree.scale.to_le_bytes());
            out.extend_from_slice(&tree.ref_form_id.to_le_bytes());
            out.extend_from_slice(&[0u8; 8]);
        }
    }
    out
}

fn lst_bytes(entries: &[TreeLodSpecies]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in entries {
        out.extend_from_slice(&e.index.to_le_bytes());
        out.extend_from_slice(&e.width.to_le_bytes());
        out.extend_from_slice(&e.height.to_le_bytes());
        for v in e.uv {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    out
}

fn tree(species: u32, position: [f32; 3], rotation: f32, scale: f32) -> TreeLodInstance {
    TreeLodInstance {
        species,
        position,
        rotation,
        scale,
        ref_form_id: 0x1234,
    }
}

fn species(index: u32, width: f32, height: f32) -> TreeLodSpecies {
    TreeLodSpecies {
        index,
        width,
        height,
        uv: [0.25, 0.0, 0.5, 0.5],
    }
}

#[test]
fn btt_groups_parse_with_their_species_and_fields() {
    let a = [tree(9, [100.0, -200.0, 5.0], 1.5, 1.2), tree(9, [1.0, 2.0, 3.0], 0.0, 1.0)];
    let b = [tree(10, [7.0, 8.0, 9.0], 3.0, 0.5)];
    let parsed = parse_btt(&btt_bytes(&[(9, &a), (10, &b)])).unwrap();
    assert_eq!(parsed.trailing_bytes, 0);
    assert_eq!(parsed.trees.len(), 3);
    assert_eq!(parsed.trees[0], a[0]);
    assert_eq!(parsed.trees[1], a[1]);
    assert_eq!(parsed.trees[2], b[0]);
    assert_eq!(parsed.trees[2].species, 10);
}

/// The six Solstheim files carry bytes past their declared groups (#5186):
/// the declared trees still parse, and the excess is reported, not refused.
#[test]
fn btt_trailing_bytes_are_reported_not_fatal() {
    let a = [tree(1, [0.0; 3], 0.0, 1.0)];
    let mut bytes = btt_bytes(&[(1, &a)]);
    bytes.extend_from_slice(&[0xAB; 5]);
    let parsed = parse_btt(&bytes).unwrap();
    assert_eq!(parsed.trees.len(), 1);
    assert_eq!(parsed.trailing_bytes, 5);
}

/// Archive input is untrusted: a count that outruns the file is an error,
/// never an allocation or a read past the end.
#[test]
fn btt_and_lst_refuse_counts_that_outrun_the_file() {
    assert!(matches!(parse_btt(&[]), Err(TreeLodError::Truncated { .. })));
    assert!(matches!(parse_btt(&[1, 0, 0]), Err(TreeLodError::Truncated { .. })));
    // One group claiming u32::MAX trees in an 12-byte file.
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&7u32.to_le_bytes());
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        parse_btt(&bytes),
        Err(TreeLodError::CountExceedsData { count: u32::MAX, .. })
    ));
    // A group count with no group headers behind it.
    assert!(matches!(
        parse_btt(&5u32.to_le_bytes()),
        Err(TreeLodError::Truncated { .. })
    ));
    assert!(matches!(parse_tree_lst(&[]), Err(TreeLodError::Truncated { .. })));
    assert!(matches!(
        parse_tree_lst(&u32::MAX.to_le_bytes()),
        Err(TreeLodError::CountExceedsData { .. })
    ));
}

#[test]
fn lst_entries_round_trip() {
    let entries = [species(0, 566.4, 1521.5), species(1, 368.2, 892.7)];
    let parsed = parse_tree_lst(&lst_bytes(&entries)).unwrap();
    assert_eq!(parsed, entries);
    assert_eq!(parse_tree_lst(&lst_bytes(&[])).unwrap(), []);
}

/// Two crossed cards per tree, standing on the base, `width × scale` wide
/// and `height × scale` tall, Z-up converted to Y-up, atlas rect mapped with
/// the top edge on `v_min`.
#[test]
fn each_tree_is_two_crossed_cards_standing_on_its_base() {
    let list = [species(9, 100.0, 400.0)];
    let mesh = build_tree_lod_mesh(&[tree(9, [1000.0, -2000.0, 50.0], 0.0, 2.0)], &list);
    assert_eq!(mesh.skipped, 0);
    assert_eq!(mesh.vertices.len(), 8);
    assert_eq!(mesh.indices.len(), 12);
    assert!(mesh.indices.iter().all(|&i| (i as usize) < mesh.vertices.len()));

    // Base (1000, -2000, 50) Z-up → (1000, 50, 2000) Y-up.
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &mesh.vertices {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(v.position[axis]);
            hi[axis] = hi[axis].max(v.position[axis]);
        }
    }
    assert_eq!((lo[1], hi[1]), (50.0, 50.0 + 400.0 * 2.0), "base to scaled height");
    // Yaw 0: card A along world X (half-width 100), card B along Y-up Z.
    assert_eq!((lo[0], hi[0]), (900.0, 1100.0));
    assert_eq!((lo[2], hi[2]), (1900.0, 2100.0));

    // Bottom edge takes v_max, top edge v_min, across u_min..u_max.
    let bottoms: Vec<_> = mesh.vertices.iter().filter(|v| v.position[1] == 50.0).collect();
    let tops: Vec<_> = mesh.vertices.iter().filter(|v| v.position[1] != 50.0).collect();
    assert!(bottoms.iter().all(|v| v.uv[1] == 0.5));
    assert!(tops.iter().all(|v| v.uv[1] == 0.0));
    assert!(mesh.vertices.iter().all(|v| v.uv[0] == 0.25 || v.uv[0] == 0.5));
}

#[test]
fn rotation_turns_the_cross_in_plan() {
    let list = [species(1, 100.0, 100.0)];
    let half_turn = build_tree_lod_mesh(&[tree(1, [0.0; 3], std::f32::consts::FRAC_PI_4, 1.0)], &list);
    // At 45° the cards' ends reach 50·√2/2 ≈ 35.36 on each plan axis — never
    // the axis-aligned 50 a yaw-0 cross has.
    let max_x = half_turn.vertices.iter().map(|v| v.position[0].abs()).fold(0.0, f32::max);
    assert!((max_x - 50.0 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-3, "{max_x}");
}

/// A tree whose species the list lacks, or whose numbers are unusable, is
/// dropped — never drawn at a guessed size.
#[test]
fn unknown_species_and_unusable_numbers_are_skipped() {
    let list = [species(1, 100.0, 100.0), species(2, 0.0, 100.0)];
    let trees = [
        tree(1, [0.0; 3], 0.0, 1.0),
        tree(99, [0.0; 3], 0.0, 1.0),          // not in the list
        tree(2, [0.0; 3], 0.0, 1.0),           // zero width
        tree(1, [f32::NAN, 0.0, 0.0], 0.0, 1.0), // non-finite position
        tree(1, [0.0; 3], 0.0, f32::NAN),      // bad scale falls back to 1.0
    ];
    let mesh = build_tree_lod_mesh(&trees, &list);
    assert_eq!(mesh.skipped, 3);
    assert_eq!(mesh.vertices.len(), 2 * 8);
}

#[test]
fn quad_names_parse_in_any_case_and_separator() {
    assert_eq!(
        parse_tree_quad_name("meshes\\terrain\\tamriel\\trees\\tamriel.4.4.-12.btt"),
        Some(("tamriel".to_owned(), 4, -12))
    );
    assert_eq!(
        parse_tree_quad_name("Meshes/Terrain/DLC2SolstheimWorld/Trees/DLC2SolstheimWorld.4.-8.16.BTT"),
        Some(("dlc2solstheimworld".to_owned(), -8, 16))
    );
    // Not tree quads: the `.lst`, the object/terrain families, another level,
    // another worldspace's stem, trailing fields.
    for name in [
        "meshes\\terrain\\tamriel\\trees\\tamriel.lst",
        "meshes\\terrain\\tamriel\\objects\\tamriel.4.4.-12.bto",
        "meshes\\terrain\\tamriel\\tamriel.4.4.-12.btr",
        "meshes\\terrain\\tamriel\\trees\\tamriel.8.4.-12.btt",
        "meshes\\terrain\\tamriel\\trees\\other.4.4.-12.btt",
        "meshes\\terrain\\tamriel\\trees\\tamriel.4.4.-12.7.btt",
        "meshes\\terrain\\tamriel\\trees\\tamriel.4.4.btt",
    ] {
        assert_eq!(parse_tree_quad_name(name), None, "{name}");
    }
}

#[test]
fn quads_in_range_are_nearest_first_with_deterministic_ties() {
    // Player in cell (5, 5): quad (4, 4) holds it (distance 0).
    let all = [(12, 4), (4, 4), (-4, 4), (4, 12), (40, 40), (4, -4)];
    let quads = tree_quads_in_range(&all, (5, 5), 8);
    assert_eq!(quads[0], (4, 4), "the player's own quad first");
    assert!(!quads.contains(&(40, 40)), "distance 32 is out of range");
    for &(qx, qy) in &quads {
        assert!(quad_min_chebyshev(qx, qy, 4, (5, 5)) <= 8);
    }
    // (12,4), (-4,4), (4,12), (4,-4) all sit at distance 1..3; ties break on
    // (qy, qx), so the order never depends on input order.
    let mut reversed = all;
    reversed.reverse();
    assert_eq!(tree_quads_in_range(&reversed, (5, 5), 8), quads);
    assert_eq!(quads.len(), 5);
    // Distance is measured to the nearest cell of the quad: a quad exactly
    // `range` cells away is in, one cell further is out.
    assert_eq!(tree_quads_in_range(&[(8, 0)], (0, 0), 8), [(8, 0)]);
    assert!(tree_quads_in_range(&[(8, 0)], (0, 0), 7).is_empty());
}

#[test]
fn full_detail_cells_veto_the_quads_they_touch() {
    let resident: HashSet<(i32, i32)> = [(5, 6)].into_iter().collect();
    assert!(footprint_intersects_full_detail(4, 4, &resident));
    assert!(!footprint_intersects_full_detail(8, 4, &resident));
    assert!(!footprint_intersects_full_detail(0, 4, &resident));
}

/// The distance is `fTreeLoadDistance` (75000 BU at Ultra) in cells, and
/// only Skyrim has the tier.
#[test]
fn tree_ring_range_is_the_sourced_load_distance() {
    use byroredux_plugin::esm::reader::GameKind;
    assert_eq!(tree_load_cells(GameKind::Skyrim), Some(18));
    assert_eq!(tree_load_cells(GameKind::Fallout4), None);
    assert_eq!(tree_load_cells(GameKind::Fallout3NV), None);
    assert!(tree_lod_supported(GameKind::Skyrim));
    assert!(!tree_lod_supported(GameKind::Fallout4));
}

/// #4913 / #5186 — the real corpus: all 386 vanilla `.btt` parse, 380 to
/// exactly their length, every group's species is in its worldspace's
/// `.lst`, and every `.lst` entry is usable.
///
///   cargo test -p byroredux --bin byroredux tree_lod_corpus -- --ignored
#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn tree_lod_corpus_parses_and_resolves_every_species() {
    let data = std::env::var("BYROREDUX_SKYRIMSE_DATA")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data".into()
        });
    let path = data.join("Skyrim - Meshes1.bsa");
    if !path.is_file() {
        assert!(
            std::env::var("BYROREDUX_REQUIRE_GAME_DATA").map_or(true, |v| v == "0"),
            "BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file"
        );
        eprintln!("skipping: {path:?} not found");
        return;
    }
    let archive = byroredux_bsa::BsaArchive::open(&path).expect("open Skyrim - Meshes1.bsa");
    let files: Vec<String> = archive.list_files().into_iter().map(str::to_owned).collect();
    let (mut parsed, mut exact, mut trees, mut unresolved) = (0, 0, 0, 0);
    let mut lists: std::collections::HashMap<String, Vec<TreeLodSpecies>> = Default::default();
    for file in files.iter().filter(|f| f.to_ascii_lowercase().ends_with(".lst")) {
        let list = parse_tree_lst(&archive.extract(file).unwrap()).expect("every .lst parses");
        assert!(list.iter().all(|s| s.width > 0.0 && s.height > 0.0), "{file}");
        let ws = file.to_ascii_lowercase().split('\\').nth(2).unwrap().to_owned();
        lists.insert(ws, list);
    }
    assert_eq!(lists.len(), 9, "the nine vanilla worldspace species lists");
    for file in files.iter().filter(|f| f.to_ascii_lowercase().ends_with(".btt")) {
        let ws = file.to_ascii_lowercase().split('\\').nth(2).unwrap().to_owned();
        let btt = parse_btt(&archive.extract(file).unwrap())
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        parsed += 1;
        exact += usize::from(btt.trailing_bytes == 0);
        trees += btt.trees.len();
        let list = lists.get(&ws).unwrap_or_else(|| panic!("{file}: no .lst for {ws}"));
        unresolved += btt.trees.iter().filter(|t| !list.iter().any(|s| s.index == t.species)).count();
        let mesh = build_tree_lod_mesh(&btt.trees, list);
        assert_eq!(mesh.vertices.len(), (btt.trees.len() - mesh.skipped) * 8, "{file}");
    }
    eprintln!("tree LOD corpus: {parsed} .btt, {exact} exact, {trees} trees, {unresolved} unresolved");
    assert_eq!(parsed, 386);
    assert_eq!(exact, 380, "six Solstheim files carry trailing bytes (#5186)");
    assert_eq!(unresolved, 0, "every tree's species is listed in its worldspace's .lst");
}

/// #6636 / V4 (DLOD-TREE-008) — what a `.btt` record's `ref_form_id` names, against `Skyrim.esm`: whether it is the
/// FormID of a placed TREE reference, whether that reference's cell holds the baked position, and whether the
/// quad covers that cell. This decides the key per-tree retirement can use (the REFR FormID, or the cell).
///
///   cargo test -p byroredux --bin byroredux tree_lod_ref_form_id_census -- --ignored --nocapture
#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn tree_lod_ref_form_id_census() {
    use byroredux_plugin::esm::{parse_esm, test_paths};
    use std::collections::HashMap;

    let dir = test_paths::skyrim_se_data_dir();
    let (Ok(bytes), Ok(archive)) = (
        std::fs::read(dir.join("Skyrim.esm")),
        byroredux_bsa::BsaArchive::open(dir.join("Skyrim - Meshes1.bsa").to_str().unwrap()),
    ) else {
        test_paths::require_or_skip("Skyrim.esm / Skyrim - Meshes1.bsa");
        return;
    };
    let index = parse_esm(&bytes).expect("parse Skyrim.esm");
    // REFR FormID -> (worldspace, cell, base, position)
    let mut refs: HashMap<u32, (&str, (i32, i32), u32, [f32; 3])> = HashMap::new();
    for (ws, cells) in &index.cells.exterior_cells {
        for (cell, data) in cells {
            for r in &data.references {
                refs.insert(r.form_id, (ws.as_str(), *cell, r.base_form_id, r.position));
            }
        }
    }
    // References held by a worldspace's persistent cell are not in any grid cell.
    let persistent: std::collections::HashSet<u32> = index
        .cells
        .worldspace_persistent_cells
        .values()
        .flat_map(|cell| cell.references.iter().map(|r| r.form_id))
        .collect();
    let (mut total, mut resolved, mut tree_base, mut cell_matches, mut quad_covers, mut near, mut zero) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let (mut dlc_unresolved, mut other_ws, mut persistent_refs) = (0usize, 0usize, 0usize);
    let mut distinct: std::collections::HashSet<u32> = Default::default();
    for file in archive.list_files() {
        let Some((ws, qx, qy)) = parse_tree_quad_name(file) else { continue };
        let btt = parse_btt(&archive.extract(file).unwrap()).unwrap();
        for t in &btt.trees {
            total += 1;
            distinct.insert(t.ref_form_id);
            zero += usize::from(t.ref_form_id == 0);
            let Some(&(ref_ws, cell, base, pos)) = refs.get(&t.ref_form_id) else {
                // Not in Skyrim.esm: a DLC reference (load-order byte 01+), which this census does not load.
                dlc_unresolved += usize::from(t.ref_form_id >> 24 != 0);
                persistent_refs += usize::from(persistent.contains(&t.ref_form_id));
                continue;
            };
            if ref_ws != ws {
                other_ws += 1;
                if other_ws <= 3 {
                    eprintln!("  ref {:08X} in `{ref_ws}`, .btt worldspace `{ws}`", t.ref_form_id);
                }
            }
            resolved += usize::from(ref_ws == ws);
            tree_base += usize::from(index.trees.contains_key(&base));
            let baked_cell = ((t.position[0] / 4096.0).floor() as i32, (t.position[1] / 4096.0).floor() as i32);
            cell_matches += usize::from(cell == baked_cell);
            quad_covers += usize::from((qx..qx + 4).contains(&cell.0) && (qy..qy + 4).contains(&cell.1));
            let d = ((pos[0] - t.position[0]).powi(2) + (pos[1] - t.position[1]).powi(2)).sqrt();
            near += usize::from(d < 1.0);
        }
    }
    eprintln!(
        "Skyrim .btt: {total} trees ({} distinct ref ids, {zero} zero); resolve to a Skyrim.esm REFR in their worldspace: {resolved}; \
         REFR base is a TREE: {tree_base}; REFR's cell == baked position's cell: {cell_matches}; quad covers the REFR's cell: {quad_covers}; \
         baked position within 1 unit of the REFR's: {near}; unresolved with a DLC load-order byte: {dlc_unresolved}; resolve in another worldspace: {other_ws}; unresolved but held by a persistent cell: {persistent_refs}",
        distinct.len()
    );
    // V4: a record's `ref_form_id` IS the FormID of the placed TREE reference it bakes: every one that resolves
    // lies at the baked position, in the baked position's cell (one reference sits on the edge of its quad), and
    // every one that does not resolve is a DLC reference this census does not load (1 709), a reference held by a
    // worldspace's persistent cell (3), or absent from `Skyrim.esm`'s parsed references with a base-game load-order
    // byte (83, cause not examined: possibly deleted or superseded references).
    assert_eq!((total, distinct.len(), zero), (43_333, 43_333, 0));
    assert_eq!((resolved, tree_base, cell_matches, quad_covers, near), (41_538, 41_538, 41_538, 41_537, 41_538));
    assert_eq!((dlc_unresolved, persistent_refs, total - resolved - dlc_unresolved - persistent_refs), (1_709, 3, 83));
}

// ── DLOD-TREE-004: Fallout 3 `.dtl` / `treetypes.lst` ────────────────────────────────────────────────

fn dtl_record(x: f32, y: f32, z: f32, rotation: f32, scale: f32, refr: u32, chunk: u32) -> Vec<u8> {
    let mut r = Vec::new();
    for v in [x, y, z, rotation, scale] {
        r.extend_from_slice(&v.to_le_bytes());
    }
    r.extend_from_slice(&refr.to_le_bytes());
    r.extend_from_slice(&chunk.to_le_bytes());
    r.extend_from_slice(&0xFFFF_FF00u32.to_le_bytes());
    r
}

/// V3 -- a `.dtl` is a chunk count, then per chunk a tree type, a record count and 32-byte records; the
/// headers are trusted and bytes after the last declared chunk are ignored (38 vanilla files carry them).
#[test]
fn parse_dtl_reads_chunks_and_ignores_undeclared_trailing_bytes() {
    let mut bytes = 2u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&3u32.to_le_bytes()); // type 3
    bytes.extend_from_slice(&2u32.to_le_bytes()); // two records
    bytes.extend(dtl_record(1.0, 2.0, 3.0, 9.0, 0.5, 0x1000, 0));
    bytes.extend(dtl_record(4.0, 5.0, 6.0, 8.0, 1.0, 0x1001, 0));
    bytes.extend_from_slice(&5u32.to_le_bytes()); // type 5
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend(dtl_record(7.0, 8.0, 9.0, 7.0, 2.0, 0x1002, 1));
    bytes.extend_from_slice(&[0xAB; 13]); // undeclared trailing bytes
    let trees = parse_dtl(&bytes).expect("a well-formed .dtl parses");
    assert_eq!(trees.len(), 3);
    assert_eq!((trees[0].tree_type, trees[0].position, trees[0].scale, trees[0].ref_form_id), (3, [1.0, 2.0, 3.0], 0.5, 0x1000));
    assert_eq!(trees[1].tree_type, 3);
    assert_eq!((trees[2].tree_type, trees[2].ref_form_id, trees[2].rotation), (5, 0x1002, 7.0));
}

/// An untrusted count is checked against the bytes present, never allocated.
#[test]
fn parse_dtl_rejects_truncated_and_oversized_input() {
    assert!(matches!(parse_dtl(&[]), Err(TreeLodError::Truncated { .. })));
    let mut huge = 1u32.to_le_bytes().to_vec();
    huge.extend_from_slice(&1u32.to_le_bytes());
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(parse_dtl(&huge), Err(TreeLodError::CountExceedsData { .. })));
    // A chunk header cut short.
    let mut short = 1u32.to_le_bytes().to_vec();
    short.extend_from_slice(&1u32.to_le_bytes());
    assert!(matches!(parse_dtl(&short), Err(TreeLodError::Truncated { .. })));
    // Zero chunks is a valid, empty file.
    assert_eq!(parse_dtl(&0u32.to_le_bytes()).unwrap(), Vec::new());
}

/// V3 -- `treetypes.lst` is a count and 32-byte entries (an index and seven floats); the header count wins
/// over a longer body (`wasteland` declares 8 and holds 9).
#[test]
fn parse_treetypes_lst_trusts_the_header_count() {
    let mut bytes = 2u32.to_le_bytes().to_vec();
    for index in 0..3u32 {
        bytes.extend_from_slice(&index.to_le_bytes());
        for k in 0..7 {
            bytes.extend_from_slice(&((index * 10 + k) as f32).to_le_bytes());
        }
    }
    let list = parse_treetypes_lst(&bytes).expect("a well-formed lst parses");
    assert_eq!(list.len(), 2, "the third entry is past the declared count");
    assert_eq!(list[1].index, 1);
    assert_eq!(list[1].values, [10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0]);
    let mut over = 5u32.to_le_bytes().to_vec();
    over.extend_from_slice(&[0u8; 32]);
    assert!(matches!(parse_treetypes_lst(&over), Err(TreeLodError::CountExceedsData { .. })));
}

/// A6 / DLOD-TREE-004 -- every vanilla `.dtl` and `treetypes.lst` in the FO3 base and DLC archives parses
/// and every chunk's tree type resolves in its worldspace's list. Run with:
///   cargo test -p byroredux --bin byroredux dtl_corpus -- --ignored
#[test]
#[ignore = "needs Fallout 3 game data on disk"]
fn dtl_corpus_parses_and_resolves_every_species() {
    use byroredux_plugin::esm::test_paths;
    let dir = test_paths::fo3_data_dir();
    let names = [
        "Fallout - Meshes.bsa",
        "Anchorage - Main.bsa",
        "BrokenSteel - Main.bsa",
        "PointLookout - Main.bsa",
        "ThePitt - Main.bsa",
    ];
    let mut lists: std::collections::HashMap<String, (usize, usize)> = Default::default(); // ws -> (entries, archive)
    let mut dtls: Vec<(String, String, Vec<u8>)> = Vec::new(); // (ws, file, bytes)
    let mut archives = Vec::new();
    for name in names {
        let path = dir.join(name);
        let Ok(archive) = byroredux_bsa::BsaArchive::open(path.to_str().unwrap()) else {
            test_paths::require_or_skip(name);
            return;
        };
        archives.push(archive);
    }
    for (a, archive) in archives.iter().enumerate() {
        for file in archive.list_files().into_iter().map(str::to_owned).collect::<Vec<_>>() {
            let lower = file.to_ascii_lowercase();
            let Some(rest) = lower.strip_prefix("meshes\\landscape\\lod\\") else { continue };
            let Some((ws, tail)) = rest.split_once("\\trees\\") else { continue };
            if tail == "treetypes.lst" {
                let list = parse_treetypes_lst(&archive.extract(&file).unwrap()).expect("every treetypes.lst parses");
                let slot = lists.entry(ws.to_owned()).or_insert((0, a));
                // A DLC list extends a worldspace's; keep the longest.
                slot.0 = slot.0.max(list.len());
            } else if tail.ends_with(".dtl") {
                dtls.push((ws.to_owned(), file.clone(), archive.extract(&file).unwrap()));
            }
        }
    }
    assert_eq!(dtls.len(), 108, "75 base + 33 DLC .dtl files (DLOD-GATE-007)");
    let mut trees = 0usize;
    for (ws, file, bytes) in &dtls {
        let parsed = parse_dtl(bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
        let (entries, _) = lists.get(ws).unwrap_or_else(|| panic!("{file}: no treetypes.lst for {ws}"));
        for tree in &parsed {
            assert!(
                (tree.tree_type as usize) < *entries,
                "{file}: tree type {} is not in {ws}'s {entries}-entry treetypes.lst",
                tree.tree_type
            );
        }
        trees += parsed.len();
    }
    assert!(trees > 0);
    eprintln!("{} .dtl files, {} worldspace lists, {} trees", dtls.len(), lists.len(), trees);

    // DLOD-TREE-006 -- the species a `.dtl` record names is its base FormID, which must be a TREE record the
    // plugin parsed (the billboard and its size live there).
    let esm = dir.join("Fallout3.esm");
    let Ok(esm_bytes) = std::fs::read(&esm) else {
        test_paths::require_or_skip("Fallout3.esm");
        return;
    };
    let index = byroredux_plugin::esm::parse_esm(&esm_bytes).expect("parse Fallout3.esm");
    let mut by_base: std::collections::BTreeMap<u32, usize> = Default::default();
    for (_, _, bytes) in &dtls {
        for tree in parse_dtl(bytes).unwrap() {
            *by_base.entry(tree.ref_form_id).or_default() += 1;
        }
    }
    // The record's FormID word is the placed REFR (32,016 distinct values over 32,275 trees), not a TREE
    // base, exactly as the Skyrim `.btt` carries `ref_form_id`. Its base is the TREE.
    let mut ref_base: std::collections::HashMap<u32, u32> = Default::default();
    for cells in index.cells.exterior_cells.values() {
        for data in cells.values() {
            for r in &data.references {
                ref_base.insert(r.form_id, r.base_form_id);
            }
        }
    }
    for cell in index.cells.worldspace_persistent_cells.values() {
        for r in &cell.references {
            ref_base.insert(r.form_id, r.base_form_id);
        }
    }
    // 27,469 of the 32,016 distinct references are placed references in `Fallout3.esm`; the rest are DLC
    // references this one-master census does not read.
    let any_ref = by_base.keys().filter(|r| ref_base.contains_key(r)).count();
    assert!(
        any_ref * 100 >= by_base.len() * 80,
        "{any_ref} of {} .dtl references are placed in Fallout3.esm",
        by_base.len()
    );
    // Their bases are STAT records with NIF tree models, not TREE records, and each wasteland species ships
    // an authored `_lod_flat.nif` (V5 for Fallout 3).
    let meshes = &archives[0];
    let flat_files: std::collections::HashSet<String> = meshes
        .list_files()
        .into_iter()
        .map(|f| f.to_ascii_lowercase())
        .filter(|f| f.ends_with("_lod_flat.nif"))
        .collect();
    let mut models: std::collections::BTreeMap<String, usize> = Default::default();
    for (refr, n) in &by_base {
        if let Some(base) = ref_base.get(refr) {
            if let Some(stat) = index.cells.statics.get(base) {
                *models.entry(stat.model_path.to_ascii_lowercase()).or_default() += n;
            }
        }
    }
    let tree_models: Vec<(&String, &usize)> = models.iter().filter(|(m, _)| m.starts_with("landscape\\trees\\")).collect();
    let with_flat: usize = tree_models
        .iter()
        .filter(|(m, _)| super::super::tree_lod_legacy::lod_flat_nif_path(m).is_some_and(|p| flat_files.contains(&p)))
        .map(|(_, n)| **n)
        .sum();
    let on_trees: usize = tree_models.iter().map(|(_, n)| **n).sum();
    eprintln!("{on_trees} dtl trees stand on landscape\\trees models; {with_flat} of them have a _lod_flat.nif");
    assert!(on_trees > 0 && with_flat * 100 >= on_trees * 95, "the authored flat mesh covers the .dtl trees");
    assert_eq!(
        index.trees.values().filter(|t| t.has_speedtree_binary()).count(),
        index.trees.values().filter(|t| t.billboard_size.is_some()).count(),
        "TREE records are `.spt` trees; none of the .dtl trees is one"
    );
}

/// DLOD-TREE-008 -- a quad that straddles the resident boundary draws the trees of its non-resident cells: a
/// tree is dropped only when the cell its position falls in is marked.
#[test]
fn drop_holed_trees_keeps_the_non_resident_cells_trees() {
    let tree = |x: f32, y: f32| TreeLodInstance { species: 1, position: [x, y, 0.0], rotation: 0.0, scale: 1.0, ref_form_id: 0 };
    // Quad SW cell (4, 8): cells x 4..=7, y 8..=11. Cell (4, 8) is bit 0, cell (5, 8) bit 1.
    let (x0, y0) = (4.0 * 4096.0, 8.0 * 4096.0);
    let trees = vec![tree(x0 + 10.0, y0 + 10.0), tree(x0 + 4096.0 + 10.0, y0 + 10.0), tree(x0 + 4096.0 * 3.0 + 10.0, y0 + 10.0)];
    let kept = drop_holed_trees(&trees, 4, 8, 0b11);
    assert_eq!(kept.len(), 1, "cells (4,8) and (5,8) are resident; only the (7,8) tree stays");
    assert_eq!(kept[0].position[0], x0 + 4096.0 * 3.0 + 10.0);
    assert_eq!(drop_holed_trees(&trees, 4, 8, 0).len(), 3, "no mask, no change");
}
