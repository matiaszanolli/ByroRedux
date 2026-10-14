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
