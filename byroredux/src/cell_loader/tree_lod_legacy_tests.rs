use super::*;
use super::super::placement_lod::Placement;

fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Rgba8 {
    let mut image = Rgba8::new(width, height);
    for px in image.pixels.as_chunks_mut::<4>().0 {
        px.copy_from_slice(&rgba);
    }
    image
}

/// V2 -- the distant tree's texture is `textures\trees\billboards\<.spt stem>.dds`; only an `.spt` has one.
#[test]
fn billboard_texture_path_is_named_by_the_spt_stem() {
    assert_eq!(
        billboard_texture_path("Trees\\TreeOakLarge01.SPT").as_deref(),
        Some("textures\\trees\\billboards\\treeoaklarge01.dds")
    );
    assert_eq!(
        billboard_texture_path("trees/shrub.spt").as_deref(),
        Some("textures\\trees\\billboards\\shrub.dds")
    );
    assert_eq!(billboard_texture_path("trees\\tree.nif"), None, "Skyrim trees are NIFs");
    assert_eq!(billboard_texture_path(""), None);
    assert_eq!(billboard_texture_path("trees\\.spt"), None, "no stem");
}

/// The packer lays tiles out row-major, resamples each billboard into its tile, and reports each species'
/// rect in atlas coordinates with the tile's top edge as `v_min`.
#[test]
fn pack_billboards_tiles_resamples_and_reports_each_rect() {
    let sources = vec![
        BillboardSource { form_id: 0x10, width: 100.0, height: 200.0, image: solid(2, 2, [255, 0, 0, 255]) },
        BillboardSource { form_id: 0x20, width: 50.0, height: 80.0, image: solid(64, 32, [0, 255, 0, 128]) },
        BillboardSource { form_id: 0x30, width: 10.0, height: 10.0, image: solid(1, 1, [0, 0, 255, 255]) },
    ];
    let packed = pack_billboards(&sources).expect("three billboards pack");
    // ceil(sqrt(3)) = 2 columns, 2 rows of 128-texel tiles.
    assert_eq!((packed.width, packed.height), (256, 256));
    assert_eq!(packed.pixels.len(), 256 * 256 * 4);
    assert_eq!(packed.species.len(), 3);
    assert_eq!(packed.species[0].index, 0x10);
    assert_eq!(packed.species[0].uv, [0.0, 0.0, 0.5, 0.5]);
    assert_eq!(packed.species[1].uv, [0.5, 0.0, 1.0, 0.5]);
    assert_eq!(packed.species[2].uv, [0.0, 0.5, 0.5, 1.0]);
    assert_eq!((packed.species[1].width, packed.species[1].height), (50.0, 80.0));
    let px = |x: usize, y: usize| -> [u8; 4] {
        let at = (y * 256 + x) * 4;
        packed.pixels[at..at + 4].try_into().unwrap()
    };
    assert_eq!(px(64, 64), [255, 0, 0, 255], "tile 0 centre is the red billboard");
    assert_eq!(px(192, 64), [0, 255, 0, 128], "tile 1 keeps the source alpha");
    assert_eq!(px(64, 192), [0, 0, 255, 255], "tile 2 sits on the second row");
    assert_eq!(px(192, 192), [0, 0, 0, 0], "the unused fourth tile stays transparent");
}

#[test]
fn pack_billboards_declines_nothing_and_oversized_grids() {
    assert!(pack_billboards(&[]).is_none());
    let one = |id| BillboardSource { form_id: id, width: 1.0, height: 1.0, image: solid(1, 1, [1, 2, 3, 4]) };
    // 33 x 33 tiles of 128 texels would be 4224 > 4096.
    let many: Vec<_> = (0..33 * 33).map(one).collect();
    assert!(pack_billboards(&many).is_none());
    let fits: Vec<_> = (0..32 * 32).map(one).collect();
    assert!(pack_billboards(&fits).is_some());
}

/// DLOD-TREE-001 -- only the TREE groups of an Oblivion `.lod` become cards, and each placement keeps its
/// position, yaw and scale.
#[test]
fn oblivion_tree_groups_become_instances_and_stat_groups_do_not() {
    let placement = |x: f32, yaw: f32, scale: f32| Placement {
        position: [x, 2.0, 3.0],
        rotation: [0.0, 0.0, yaw],
        scale,
    };
    let groups = vec![
        PlacementGroup { base_form_id: 0xAA, placements: vec![placement(1.0, 0.5, 1.2), placement(4.0, 1.5, 0.8)] },
        PlacementGroup { base_form_id: 0xBB, placements: vec![placement(9.0, 0.0, 1.0)] },
        PlacementGroup { base_form_id: 0xAA, placements: vec![placement(7.0, 0.0, 1.0)] },
    ];
    let instances = oblivion_tree_instances(&groups, |form| form == 0xAA);
    assert_eq!(instances.len(), 3, "the 0xBB STAT group is not a tree");
    assert!(instances.iter().all(|i| i.species == 0xAA));
    assert_eq!(instances[0].position, [1.0, 2.0, 3.0]);
    assert_eq!((instances[1].rotation, instances[1].scale), (1.5, 0.8));
}

/// A `.dtl` record names the placed REFR; its base (a STAT with a NIF tree model) groups the placements for
/// the instanced flat-mesh path, and an unresolved reference is dropped.
#[test]
fn dtl_trees_group_by_the_base_their_reference_resolves_to() {
    let tree = |refr: u32, x: f32, rotation: f32, scale: f32| DtlTree {
        tree_type: 3,
        position: [x, 2.0, 3.0],
        rotation,
        scale,
        ref_form_id: refr,
    };
    // REFRs 1 and 3 stand on STAT 0xA0, REFR 2 on 0xB0, REFR 9 is unresolved.
    let base_of = |refr: u32| match refr {
        1 | 3 => Some(0xA0),
        2 => Some(0xB0),
        _ => None,
    };
    let trees = vec![tree(1, 10.0, 0.5, 0.9), tree(2, 20.0, 1.0, 1.1), tree(9, 30.0, 0.0, 1.0), tree(3, 40.0, 2.0, 1.0)];
    let groups = dtl_placement_groups(&trees, base_of);
    assert_eq!(groups.iter().map(|g| g.base_form_id).collect::<Vec<_>>(), vec![0xA0, 0xB0], "ordered by base");
    assert_eq!(groups[0].placements.len(), 2, "REFRs 1 and 3 share the 0xA0 group");
    assert_eq!(groups[0].placements[0].position, [10.0, 2.0, 3.0]);
    assert_eq!(groups[0].placements[0].rotation, [0.0, 0.0, 0.5], "the dtl yaw is the Z euler");
    assert_eq!(groups[0].placements[1].scale, 1.0);
    assert_eq!(groups[1].placements.len(), 1);
    assert!(dtl_placement_groups(&trees, |_| None).is_empty());
}

/// V5 for Fallout 3 -- every wasteland tree species ships an authored `<model>_lod_flat.nif` beside its model;
/// that is the distant mesh, not a guessed card.
#[test]
fn lod_flat_nif_path_names_the_authored_distant_tree_mesh() {
    assert_eq!(
        lod_flat_nif_path("landscape\\trees\\TreeDead03.nif").as_deref(),
        Some("meshes\\landscape\\trees\\treedead03_lod_flat.nif")
    );
    assert_eq!(
        lod_flat_nif_path("Meshes/Landscape/Trees/treeWastelandEvergreen03.NIF").as_deref(),
        Some("meshes\\landscape\\trees\\treewastelandevergreen03_lod_flat.nif")
    );
    assert_eq!(lod_flat_nif_path("trees\\oak.spt"), None, "an .spt has no flat mesh");
}

#[test]
fn dtl_file_names_parse_with_negative_coordinates() {
    assert_eq!(
        parse_dtl_name("meshes\\landscape\\lod\\wasteland\\trees\\wasteland.level8.x-24.y16.dtl"),
        Some(("wasteland".into(), 8, -24, 16))
    );
    assert_eq!(
        parse_dtl_name("Meshes/Landscape/LOD/Wasteland/Trees/Wasteland.Level4.X0.Y-4.DTL"),
        Some(("wasteland".into(), 4, 0, -4))
    );
    assert_eq!(parse_dtl_name("meshes\\landscape\\lod\\wasteland\\trees\\treetypes.lst"), None);
    assert_eq!(parse_dtl_name("meshes\\landscape\\lod\\wasteland\\blocks\\wasteland.level4.x0.y0.dtl"), None);
    assert_eq!(parse_dtl_name("meshes\\landscape\\lod\\wasteland\\trees\\other.level4.x0.y0.dtl"), None);
}

/// A tree standing in the middle of cell `(cx, cy)`.
fn tree_in_cell(cx: i32, cy: i32) -> DtlTree {
    let centre = |c: i32| (c as f32 + 0.5) * 4096.0;
    DtlTree { tree_type: 0, position: [centre(cx), centre(cy), 0.0], rotation: 0.0, scale: 1.0, ref_form_id: 1 }
}

/// Every cell within Chebyshev `radius` of `(0, 0)`.
fn square(radius: i32) -> HashSet<(i32, i32)> {
    (-radius..=radius).flat_map(|x| (-radius..=radius).map(move |y| (x, y))).collect()
}

fn dtl_files(quads: &[(i32, i32, i32)]) -> Vec<(i32, i32, i32, String)> {
    quads.iter().map(|&(level, x, y)| (level, x, y, String::new())).collect()
}

/// A resident unit that drew one entity (or nothing, as a sentinel), built without `holes`.
fn unit(drew: bool, holes: &[(i32, i32)]) -> LegacyTreeUnit {
    let block = if drew {
        let mut block = ObjectLodBlock::empty(0);
        block.entities.push(1);
        LegacyTreeBlock::Cards(block)
    } else {
        LegacyTreeBlock::sentinel()
    };
    LegacyTreeUnit { block, holes: holes.to_vec() }
}

/// #7431 / DLOD-TREE-008 -- a `.dtl` file that straddles the resident boundary is still wanted, built without
/// its resident cells; only a wholly resident one is dropped. A level-8 quad covers 64 cells.
#[test]
fn a_straddling_dtl_quad_is_wanted_without_its_resident_cells() {
    // Resident: cells -4..=4. The level-8 quad at (0, 0) covers 0..=7; the level-4 quad at (0, 0) is inside.
    let resident = square(4);
    let files = dtl_files(&[(8, 0, 0), (4, 0, 0), (8, 8, 0)]);
    let wanted = wanted_dtl_quads(&files, (0, 0), 10, 5, &resident);
    assert_eq!(wanted[&(8, 0, 0)].len(), 25, "cells 0..=4 in both axes are left out");
    assert!(wanted[&(8, 0, 0)].is_sorted(), "the tree filter binary-searches the list");
    assert_eq!(wanted.get(&(4, 0, 0)), None, "all 16 cells are resident");
    assert_eq!(wanted[&(8, 8, 0)], Vec::<(i32, i32)>::new(), "nothing resident in it");
    assert_eq!(wanted.len(), 2);
}

/// DLOD-TREE-007 / -008 -- the trees in the left-out cells are dropped and every other tree is kept, on both
/// sides of the origin.
#[test]
fn only_the_trees_standing_in_left_out_cells_are_dropped() {
    let trees = [tree_in_cell(4, 4), tree_in_cell(5, 4), tree_in_cell(4, 5), tree_in_cell(-3, -2)];
    let mut holes = vec![(4, 4), (-3, -2)];
    holes.sort_unstable();
    assert_eq!(drop_dtl_trees_in_cells(&trees, &holes), [trees[1], trees[2]]);
    assert_eq!(drop_dtl_trees_in_cells(&trees, &[]), trees, "no resident cell, nothing dropped");
    // A cell's low edge belongs to it, its high edge to the next one.
    let mut edge = tree_in_cell(0, 0);
    edge.position = [4096.0, -4096.0, 0.0];
    assert!(drop_dtl_trees_in_cells(&[edge], &[(1, -1)]).is_empty());
}

/// DLOD-BAND-007 -- a reach that ends inside the full-detail radius wants nothing, even though a level-8 file
/// within reach extends past the resident cells and would draw trees beyond the reach.
#[test]
fn a_dtl_ring_inside_the_full_detail_radius_wants_nothing() {
    // The default radius: 12 loaded, full-detail radius 13, FO3 reach 10. The quad covers 8..=15.
    let (resident, files) = (square(12), dtl_files(&[(8, 8, 0)]));
    assert!(wanted_dtl_quads(&files, (0, 0), 10, 13, &resident).is_empty());
    // The A5 pose: radius 3, full-detail radius 4.
    assert_eq!(wanted_dtl_quads(&files, (0, 0), 10, 4, &square(3)).len(), 1);
}

/// #7431 / DLOD-GATE-003 -- a resident cell inside a drawn unit counts as an overlap unless the unit was built
/// without it. Sentinels draw nothing and never overlap.
#[test]
fn a_legacy_unit_overlaps_only_the_resident_cells_it_still_draws() {
    let mut ring = LegacyTreeRing::default();
    ring.blocks.insert((8, 0, 0), unit(true, &[(0, 0)]));
    ring.blocks.insert((8, 8, 0), unit(false, &[]));
    assert_eq!(ring.full_detail_overlaps(&[(0, 0), (9, 1)]), 0);
    assert_eq!(ring.full_detail_overlaps(&[(0, 0), (1, 0), (7, 7), (8, 8)]), 2, "(1, 0) and (7, 7)");
}

/// #7431 / DLOD-GATE-005 -- the FO3 moat reads nonzero when an authored tree cell beside the resident set is
/// left uncovered, and 0 once the files over those cells resolve.
#[test]
fn the_fo3_tree_moat_reads_an_uncovered_authored_cell() {
    // The A5 pose. Every cell around the resident square is under one of four level-8 files.
    let quads = [(8, -8, -8), (8, 0, -8), (8, -8, 0), (8, 0, 0)];
    let resident = square(3);
    let mut ring = LegacyTreeRing { dtl_files: Some(dtl_files(&quads)), ..Default::default() };
    let moat = |ring: &LegacyTreeRing| ring.moat_cells(GameKind::Fallout3NV, &resident, (0, 0), 4);
    assert!(moat(&ring) > 0, "nothing resolved yet");
    for quad in quads {
        ring.blocks.insert(quad, unit(true, &[]));
    }
    assert_eq!(moat(&ring), 0);
    // Dropping one straddling file, as the whole-file veto did, reopens the gap.
    ring.blocks.remove(&(8, 0, 0));
    assert!(moat(&ring) > 0);
    // A sentinel is a resolved file with nothing left to draw.
    ring.blocks.insert((8, 0, 0), unit(false, &[]));
    assert_eq!(moat(&ring), 0);
}

/// DLOD-GATE-005 -- cells no `.dtl` file covers are not a gap (FNV authors none, DLOD-TREE-009), nor is
/// anything under a ring that wants nothing (DLOD-BAND-007) or has not scanned its files.
#[test]
fn the_fo3_tree_moat_is_zero_where_no_tree_is_wanted() {
    let resident = square(3);
    let moat = |files: Option<&[(i32, i32, i32)]>, full_detail_radius| {
        let ring = LegacyTreeRing { dtl_files: files.map(dtl_files), ..Default::default() };
        ring.moat_cells(GameKind::Fallout3NV, &resident, (0, 0), full_detail_radius)
    };
    assert_eq!(moat(Some(&[]), 4), 0, "no tree family");
    assert_eq!(moat(None, 4), 0, "not scanned yet");
    assert_eq!(moat(Some(&[(8, 0, 0)]), 13), 0, "the reach ends inside the full-detail radius");
    assert!(moat(Some(&[(8, 0, 0)]), 4) > 0);
    let skyrim = LegacyTreeRing::default().moat_cells(GameKind::Skyrim, &resident, (0, 0), 4);
    assert_eq!(skyrim, 0, "no legacy tree container");
}

/// #7431 / DLOD-GATE-005 -- Oblivion wants one unit per non-resident cell in reach: the moat reads nonzero
/// while a cell beside the resident set is unresolved.
#[test]
fn the_oblivion_tree_moat_reads_an_unresolved_cell() {
    let resident = square(2);
    let mut ring = LegacyTreeRing::default();
    let moat = |ring: &LegacyTreeRing| ring.moat_cells(GameKind::Oblivion, &resident, (0, 0), 2);
    // Cells within 4 of the resident square and within the 7-cell reach, less the square itself.
    assert_eq!(moat(&ring), 13 * 13 - 5 * 5);
    for (cx, cy) in placement_lod_cells_in_radius((0, 0), &resident, OBLIVION_TREE_RANGE_CELLS) {
        ring.blocks.insert((1, cx, cy), unit(false, &[]));
    }
    assert_eq!(moat(&ring), 0);
    ring.blocks.remove(&(1, 3, 0));
    assert_eq!(moat(&ring), 1);
}

/// DLOD-BAND-001 -- the reaches are the documented INI readings, and only the two games that author a
/// legacy tree container have one.
#[test]
fn legacy_tree_ranges_are_the_documented_ini_readings() {
    assert_eq!(OBLIVION_GRID_DISTANT_TREE_RANGE, 15, "uGridDistantTreeRange in Oblivion_default.ini");
    assert_eq!(OBLIVION_TREE_RANGE_CELLS, 7, "read as a diameter, like uGridsToLoad=5 (V1, owner's ruling)");
    assert_eq!(FALLOUT3_TREE_LOAD_BU, 40_000.0, "fTreeLoadDistance in the FO3 / FNV VeryHigh.ini");
    assert_eq!(legacy_tree_range_cells(GameKind::Oblivion), Some(7));
    assert_eq!(legacy_tree_range_cells(GameKind::Fallout3NV), Some(10), "40000 BU / 4096, rounded");
    for game in [GameKind::Skyrim, GameKind::Fallout4, GameKind::Fallout76, GameKind::Starfield] {
        assert_eq!(legacy_tree_family(game), None, "{game:?}");
        assert_eq!(legacy_tree_range_cells(game), None, "{game:?}");
    }
    assert_eq!(legacy_tree_family(GameKind::Oblivion), Some(LegacyTreeFamily::OblivionLod));
    assert_eq!(legacy_tree_family(GameKind::Fallout3NV), Some(LegacyTreeFamily::FalloutDtl));
}
