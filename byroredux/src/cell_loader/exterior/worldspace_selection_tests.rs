use super::{select_worldspace_key, summarize_foreground_cells, PREFERRED_WORLDSPACES};
use std::collections::HashMap;

fn cells(coords: &[(i32, i32)]) -> HashMap<(i32, i32), ()> {
    coords.iter().copied().map(|coord| (coord, ())).collect()
}

/// #5226 — MegatonWorld and Wasteland both contain (-1,-7): the
/// explicit override picks MegatonWorld (case-insensitively), where
/// the no-override heuristic would have to break the tie itself.
#[test]
fn a_matching_wrld_override_wins_over_the_grid_chain() {
    let worldspaces = HashMap::from([
        ("MegatonWorld".to_string(), cells(&[(-1, -7)])),
        ("Wasteland".to_string(), cells(&[(-1, -7)])),
    ]);
    assert_eq!(
        select_worldspace_key(&worldspaces, -1, -7, 0, Some("megatonworld"))
            .expect("the override matches")
            .as_str(),
        "MegatonWorld"
    );
}

/// #5226 — an unmatched override is a CLI typo, not a hint to run the
/// grid heuristic: it errors naming the requested EDID and the loaded
/// set. Pre-fix `--wrld MegatonWrld --grid -1,-7` silently loaded
/// Wasteland's cell at the same coordinate.
#[test]
fn an_unmatched_wrld_override_errors_naming_the_loaded_set() {
    let worldspaces = HashMap::from([
        ("MegatonWorld".to_string(), cells(&[(-1, -7)])),
        ("Wasteland".to_string(), cells(&[(-1, -7)])),
    ]);
    let err = select_worldspace_key(&worldspaces, -1, -7, 0, Some("MegatonWrld"))
        .expect_err("a typo must not fall through to the grid heuristic");
    let message = format!("{err:#}");
    assert!(message.contains("MegatonWrld"), "{message}");
    assert!(
        message.contains("MegatonWorld") && message.contains("Wasteland"),
        "the available EDIDs must be listed: {message}"
    );
}

/// The no-override path keeps its own error when the plugin set
/// carries no exterior worldspaces at all.
#[test]
fn no_worldspaces_without_an_override_still_errors() {
    let worldspaces: HashMap<String, HashMap<(i32, i32), ()>> = HashMap::new();
    let err = select_worldspace_key(&worldspaces, 0, 0, 0, None)
        .expect_err("no worldspace is an error");
    assert!(
        format!("{err:#}").contains("No worldspace found"),
        "{err:#}"
    );
}

#[test]
fn preferred_containing_worldspace_wins_ambiguous_grid_deterministically() {
    for preferred in PREFERRED_WORLDSPACES {
        let names = ["smallworld", preferred, "largeworld"];
        for rotation in 0..names.len() {
            let mut worldspaces = HashMap::new();
            for offset in 0..names.len() {
                let name = names[(rotation + offset) % names.len()];
                let coords = if name == "largeworld" {
                    &[(0, 0), (1, 0), (2, 0)][..]
                } else {
                    &[(0, 0)][..]
                };
                worldspaces.insert(name.to_string(), cells(coords));
            }

            assert_eq!(
                select_worldspace_key(&worldspaces, 0, 0, 0, None)
                    .expect("a worldspace matches the grid")
                    .as_str(),
                preferred
            );
        }
    }
}

/// The Oblivion region→climate rung — vanilla Tamriel authors no WRLD
/// climate; its cells reach a climate through CELL XCLR → REGN CNAM.
/// The helper must take the first region that carries a resolvable
/// climate in XCLR order, probe the four orthogonal neighbours when
/// the exact center grid has no cell, and return None when no region
/// of any probed cell carries one.
#[test]
fn region_climate_follows_xclr_order_and_neighbour_probes() {
    use crate::env_translate::region_climate_for_center;
    use byroredux_plugin::esm::cell::CellData;
    use byroredux_plugin::esm::cell::EsmCellIndex;
    use byroredux_plugin::esm::records::RegnRecord;
    use std::collections::HashMap;

    let region = |climate: Option<u32>| RegnRecord {
        form_id: 0,
        editor_id: String::new(),
        worldspace_form: None,
        climate_form: climate,
        color: None,
        areas: Vec::new(),
        entries: Vec::new(),
    };
    let cell_with_regions = |regions: &[u32]| CellData {
        regions: regions.to_vec(),
        ..Default::default()
    };
    let mut index = EsmCellIndex::default();
    let mut tamriel = HashMap::new();
    // (0,0) carries two regions in XCLR order: the first is
    // climate-less, the second carries the climate — order must win.
    tamriel.insert((0, 0), cell_with_regions(&[0xAA00_0001, 0xAA00_0002]));
    // (1,0) — a neighbour whose single region carries a different
    // climate, only reachable when the center grid has no cell.
    tamriel.insert((1, 0), cell_with_regions(&[0xAA00_0003]));
    index.exterior_cells.insert("tamriel".into(), tamriel);

    let mut regions = HashMap::new();
    regions.insert(0xAA00_0001u32, region(None));
    regions.insert(0xAA00_0002u32, region(Some(0x00_01C52)));
    regions.insert(0xAA00_0003u32, region(Some(0x00_0688E2)));

    // Center cell present: first climate-bearing region in XCLR order.
    assert_eq!(
        region_climate_for_center(&index, "tamriel", 0, 0, &regions),
        Some(0x00_01C52),
        "the first region with a climate wins, XCLR order"
    );
    // No cell at the exact center (wilderness gap): the orthogonal
    // neighbour probes reach (1,0) and its region's climate answers.
    assert_eq!(
        region_climate_for_center(&index, "tamriel", 2, 0, &regions),
        Some(0x00_0688E2),
        "a missing center falls through to the (1,0) neighbour"
    );
    // Diagonal-only neighbours are NOT probed (orthogonal ring only):
    // (2,1)'s probes are (2,1),(3,1),(1,1),(2,2),(2,0) — none exist.
    assert_eq!(
        region_climate_for_center(&index, "tamriel", 2, 1, &regions),
        None,
        "diagonal-only neighbours are out of the probe ring — None, not a guess"
    );
    // Unknown worldspace or no climate anywhere: None.
    assert_eq!(region_climate_for_center(&index, "nowhere", 0, 0, &regions), None);
    let mut bare = HashMap::new();
    bare.insert((5, 5), cell_with_regions(&[0xAA00_0001]));
    index.exterior_cells.insert("bare".into(), bare);
    assert_eq!(
        region_climate_for_center(&index, "bare", 5, 5, &regions),
        None,
        "a region without CNAM contributes nothing"
    );
}

#[test]
fn lone_non_default_grid_match_still_beats_game_default() {
    let worldspaces = HashMap::from([
        ("wasteland".to_string(), cells(&[(0, 0)])),
        ("anchorage".to_string(), cells(&[(20, 20)])),
    ]);

    assert_eq!(
        select_worldspace_key(&worldspaces, 20, 20, 0, None)
            .expect("a worldspace matches the grid")
            .as_str(),
        "anchorage"
    );
}

#[test]
fn ambiguous_non_default_grid_match_uses_stable_most_cells_fallback() {
    let worldspaces = HashMap::from([
        ("smallworld".to_string(), cells(&[(0, 0)])),
        ("largeworld".to_string(), cells(&[(0, 0), (1, 0), (2, 0)])),
    ]);

    assert_eq!(
        select_worldspace_key(&worldspaces, 0, 0, 0, None)
            .expect("a worldspace matches the grid")
            .as_str(),
        "largeworld"
    );
}

#[test]
fn empty_foreground_reports_nearest_viable_cells_deterministically() {
    let cells = HashMap::from([
        ((0, 0), (false, 0, 0)),
        ((3, 0), (true, 0, 0)),
        ((0, 2), (false, 4, 0)),
        ((-2, 0), (false, 0, 1)),
        ((1, 1), (false, 0, 0)),
    ]);
    let readiness = summarize_foreground_cells(Some(&cells), (0, 0), 3, |signals| *signals);

    assert!(!readiness.is_content_backed());
    assert_eq!(
        readiness
            .nearest_viable
            .iter()
            .map(|cell| cell.coord)
            .collect::<Vec<_>>(),
        vec![(-2, 0), (0, 2), (3, 0)]
    );
}

#[test]
fn land_references_and_precombines_each_make_foreground_content_backed() {
    for signals in [(true, 0, 0), (false, 1, 0), (false, 0, 1)] {
        let cells = HashMap::from([((4, -7), signals)]);
        let readiness =
            summarize_foreground_cells(Some(&cells), (4, -7), 5, |signals| *signals);
        assert!(readiness.is_content_backed());
        assert!(readiness.nearest_viable.is_empty());
    }
}

#[test]
fn missing_foreground_is_distinct_from_empty_foreground() {
    let cells = HashMap::from([((1, 0), (true, 0, 0))]);
    let readiness = summarize_foreground_cells(Some(&cells), (0, 0), 5, |signals| *signals);

    assert!(readiness.requested.is_none());
    assert_eq!(readiness.nearest_viable[0].coord, (1, 0));
}
