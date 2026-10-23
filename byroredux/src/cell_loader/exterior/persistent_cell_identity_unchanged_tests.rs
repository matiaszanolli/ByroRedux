use super::persistent_cell_identity_unchanged;
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

/// A child worldspace crossing back to its parent resolves to the
/// SAME persistent CELL — the case item C2 exists to catch.
#[test]
fn child_to_parent_crossing_is_unchanged_when_child_has_no_own_persistent_cell() {
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

    // The player was in ParentWorld (root's CellFormId == 0x999);
    // crossing into ChildWorld resolves to the same 0x999 via WNAM.
    assert!(persistent_cell_identity_unchanged(
        &index,
        "childworld",
        0x999,
    ));
}

/// Two sibling worldspaces sharing one ancestor's persistent CELL:
/// crossing between them is also unchanged.
#[test]
fn sibling_to_sibling_crossing_is_unchanged_via_shared_ancestor() {
    let mut index = EsmCellIndex::default();
    index
        .worldspaces
        .insert("parentworld".into(), worldspace(0x10, "ParentWorld", None));
    index
        .worldspaces
        .insert("siblinga".into(), worldspace(0x20, "SiblingA", Some(0x10)));
    index
        .worldspaces
        .insert("siblingb".into(), worldspace(0x30, "SiblingB", Some(0x10)));
    index
        .worldspace_persistent_cells
        .insert("parentworld".into(), empty_cell(0x999));

    assert!(persistent_cell_identity_unchanged(
        &index, "siblingb", 0x999,
    ));
}

/// A worldspace with its own distinct persistent CELL must NOT be
/// treated as unchanged — this is the ordinary case that must still
/// fully drain+rebuild.
#[test]
fn crossing_into_a_worldspace_with_its_own_persistent_cell_is_changed() {
    let mut index = EsmCellIndex::default();
    index
        .worldspaces
        .insert("worlda".into(), worldspace(0x10, "WorldA", None));
    index
        .worldspaces
        .insert("worldb".into(), worldspace(0x20, "WorldB", None));
    index
        .worldspace_persistent_cells
        .insert("worlda".into(), empty_cell(0x999));
    index
        .worldspace_persistent_cells
        .insert("worldb".into(), empty_cell(0x888));

    assert!(!persistent_cell_identity_unchanged(&index, "worldb", 0x999,));
}

/// No persistent CELL resolves at all for the destination — must not
/// be treated as unchanged (there is nothing to compare against, and
/// falsely reporting "unchanged" here would preserve a root that the
/// new worldspace has no home for).
#[test]
fn destination_with_no_resolvable_persistent_cell_is_changed() {
    let mut index = EsmCellIndex::default();
    index
        .worldspaces
        .insert("worlda".into(), worldspace(0x10, "WorldA", None));
    index
        .worldspaces
        .insert("worldb".into(), worldspace(0x20, "WorldB", None));
    index
        .worldspace_persistent_cells
        .insert("worlda".into(), empty_cell(0x999));
    // WorldB authors no persistent CELL and has no parent to inherit from.

    assert!(!persistent_cell_identity_unchanged(&index, "worldb", 0x999,));
}
