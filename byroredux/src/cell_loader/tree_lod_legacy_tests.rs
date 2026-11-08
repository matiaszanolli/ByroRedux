use super::*;
use super::super::placement_lod::Placement;

fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Rgba8 {
    let mut image = Rgba8::new(width, height);
    for px in image.pixels.chunks_exact_mut(4) {
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

/// DLOD-TREE-007 -- a `.dtl` quad touching a resident full-detail cell is not drawn.
#[test]
fn a_dtl_quad_with_a_resident_cell_is_vetoed() {
    let resident: HashSet<(i32, i32)> = [(5, 5)].into_iter().collect();
    assert!(footprint_has_resident_cell(4, 4, 4, &resident));
    assert!(!footprint_has_resident_cell(8, 4, 4, &resident));
    assert!(footprint_has_resident_cell(0, 0, 8, &resident), "a level-8 quad covers 0..=7");
}

/// DLOD-BAND-001 -- the reaches are the documented INI readings, and only the two games that author a
/// legacy tree container have one.
#[test]
fn legacy_tree_ranges_are_the_documented_ini_readings() {
    assert_eq!(OBLIVION_TREE_RANGE_CELLS, 15, "uGridDistantTreeRange in Oblivion_default.ini");
    assert_eq!(FALLOUT3_TREE_LOAD_BU, 40_000.0, "fTreeLoadDistance in the FO3 / FNV VeryHigh.ini");
    assert_eq!(legacy_tree_range_cells(GameKind::Oblivion), Some(15));
    assert_eq!(legacy_tree_range_cells(GameKind::Fallout3NV), Some(10), "40000 BU / 4096, rounded");
    for game in [GameKind::Skyrim, GameKind::Fallout4, GameKind::Fallout76, GameKind::Starfield] {
        assert_eq!(legacy_tree_family(game), None, "{game:?}");
        assert_eq!(legacy_tree_range_cells(game), None, "{game:?}");
    }
    assert_eq!(legacy_tree_family(GameKind::Oblivion), Some(LegacyTreeFamily::OblivionLod));
    assert_eq!(legacy_tree_family(GameKind::Fallout3NV), Some(LegacyTreeFamily::FalloutDtl));
}
