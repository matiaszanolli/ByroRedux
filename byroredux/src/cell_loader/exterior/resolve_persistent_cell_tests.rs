use super::resolve_persistent_cell;
use byroredux_plugin::esm::cell::{CellData, EsmCellIndex, WorldspaceRecord};

fn empty_cell(form_id: u32) -> CellData {
    CellData {
        form_id,
        ..Default::default()
    }
}

fn worldspace(form_id: u32, edid: &str, parent: Option<u32>) -> WorldspaceRecord {
    WorldspaceRecord {
        form_id,
        editor_id: edid.to_string(),
        parent_worldspace: parent,
        ..WorldspaceRecord::default()
    }
}

#[test]
fn a_worldspace_with_its_own_persistent_cell_resolves_directly() {
    let mut index = EsmCellIndex::default();
    index
        .worldspaces
        .insert("solo".into(), worldspace(0x10, "Solo", None));
    index
        .worldspace_persistent_cells
        .insert("solo".into(), empty_cell(0x999));

    let (owner, cell) = resolve_persistent_cell(&index, "solo").expect("direct hit");
    assert_eq!(owner, "Solo");
    assert_eq!(cell.form_id, 0x999);
}

#[test]
fn a_childless_worldspace_inherits_its_parents_persistent_cell() {
    let mut index = EsmCellIndex::default();
    index
        .worldspaces
        .insert("parentworld".into(), worldspace(0x10, "ParentWorld", None));
    index.worldspaces.insert(
        "childworld".into(),
        worldspace(0x20, "ChildWorld", Some(0x10)),
    );
    index
        .worldspace_persistent_cells
        .insert("parentworld".into(), empty_cell(0x999));
    // ChildWorld deliberately has no entry in worldspace_persistent_cells.

    let (owner, cell) =
        resolve_persistent_cell(&index, "childworld").expect("inherited via WNAM");
    assert_eq!(owner, "ParentWorld");
    assert_eq!(cell.form_id, 0x999);
}

#[test]
fn a_multi_hop_chain_walks_to_the_first_ancestor_that_has_one() {
    let mut index = EsmCellIndex::default();
    index
        .worldspaces
        .insert("grandparent".into(), worldspace(0x10, "Grandparent", None));
    index
        .worldspaces
        .insert("parent".into(), worldspace(0x20, "Parent", Some(0x10)));
    index
        .worldspaces
        .insert("child".into(), worldspace(0x30, "Child", Some(0x20)));
    index
        .worldspace_persistent_cells
        .insert("grandparent".into(), empty_cell(0x999));
    // Neither Parent nor Child authors its own persistent CELL.

    let (owner, cell) = resolve_persistent_cell(&index, "child").expect("two-hop inherit");
    assert_eq!(owner, "Grandparent");
    assert_eq!(cell.form_id, 0x999);
}

#[test]
fn no_persistent_cell_anywhere_in_the_chain_resolves_to_none() {
    let mut index = EsmCellIndex::default();
    index
        .worldspaces
        .insert("parentworld".into(), worldspace(0x10, "ParentWorld", None));
    index.worldspaces.insert(
        "childworld".into(),
        worldspace(0x20, "ChildWorld", Some(0x10)),
    );
    // Neither worldspace authors a persistent CELL anywhere.

    assert!(resolve_persistent_cell(&index, "childworld").is_none());
}

#[test]
fn a_wnam_cycle_terminates_instead_of_looping_forever() {
    let mut index = EsmCellIndex::default();
    // A points at B, B points at A — malformed/adversarial data.
    index
        .worldspaces
        .insert("a".into(), worldspace(0x10, "A", Some(0x20)));
    index
        .worldspaces
        .insert("b".into(), worldspace(0x20, "B", Some(0x10)));

    assert!(
        resolve_persistent_cell(&index, "a").is_none(),
        "a cycle with no persistent CELL anywhere must terminate cleanly, not hang"
    );
}

#[test]
fn an_unrelated_worldspace_missing_from_the_index_resolves_to_none() {
    let index = EsmCellIndex::default();
    assert!(resolve_persistent_cell(&index, "nonexistent").is_none());
}
