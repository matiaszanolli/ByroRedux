//! Regression tests for #791 — `CellRootIndex` populated by
//! `stamp_cell_root`, drained by `unload_cell`. Pre-#791 the unload
//! path scanned the entire `CellRoot` SparseSet to filter victims of
//! a single cell, scaling O(total resident entities). The index
//! makes lookup O(victims). The Vulkan-dependent half of `unload_cell`
//! can't be unit-tested in isolation, so these tests exercise just
//! the index data-structure contract.

use super::*;

/// Spawn `count` entities and return the (first, last) entity-id range
/// suitable for `stamp_cell_root`. `last` is exclusive — exactly the
/// snapshot semantics of `next_entity_id()` before/after a cell load.
fn spawn_range(world: &mut World, count: u32) -> (EntityId, EntityId) {
    let first = world.next_entity_id();
    for _ in 0..count {
        let _ = world.spawn();
    }
    (first, world.next_entity_id())
}

/// Five cells each spawn a non-overlapping entity range. After
/// `stamp_cell_root` runs for all five, every per-cell `Vec` in the
/// index contains exactly the entities that load produced (the
/// in-range IDs plus the trailing cell_root). Removing one cell's
/// entry leaves the other four bit-identical.
#[test]
fn stamp_cell_root_populates_cell_root_index_per_cell() {
    let mut world = World::new();
    world.insert_resource(CellRootIndex::new());

    // Five cells: each spawns 3 owned entities + a cell_root entity.
    // Production cells own ~1500 entities; 3 is enough to verify the
    // structural contract.
    let mut cells: Vec<(EntityId, Vec<EntityId>)> = Vec::new();
    for _ in 0..5 {
        let (first, last) = spawn_range(&mut world, 3);
        let cell_root = world.spawn();
        let owned: Vec<EntityId> = (first..last).chain(std::iter::once(cell_root)).collect();
        stamp_cell_root(&mut world, cell_root, first, last);
        cells.push((cell_root, owned));
    }

    {
        let idx = world.resource::<CellRootIndex>();
        assert_eq!(idx.map.len(), 5, "all five cells indexed");
        for (cell_root, want) in &cells {
            let mut got = idx.map.get(cell_root).cloned().expect("cell present");
            got.sort();
            let mut want_sorted = want.clone();
            want_sorted.sort();
            assert_eq!(got, want_sorted, "cell_root={cell_root}: index entry mismatch");
        }
    }

    // Drain one cell's entry — exactly what unload_cell does. Other
    // cells must not shift their entries.
    let target = cells[2].0;
    let drained = world
        .try_resource_mut::<CellRootIndex>()
        .and_then(|mut idx| idx.map.remove(&target))
        .expect("target cell was indexed");
    let mut drained_sorted = drained.clone();
    drained_sorted.sort();
    let mut want_drained = cells[2].1.clone();
    want_drained.sort();
    assert_eq!(drained_sorted, want_drained);

    // The other four cells' entries must be unchanged.
    let idx = world.resource::<CellRootIndex>();
    assert_eq!(idx.map.len(), 4, "draining one cell removes exactly one entry");
    for (cell_root, want) in &cells {
        if *cell_root == target {
            continue;
        }
        let mut got = idx.map.get(cell_root).cloned().expect("cell present");
        got.sort();
        let mut want_sorted = want.clone();
        want_sorted.sort();
        assert_eq!(got, want_sorted, "cell_root={cell_root}: post-drain entry shifted");
    }
}

/// `stamp_cell_root` is defensive about the `CellRootIndex` resource
/// being absent (test fixtures that don't register it). The CellRoot
/// component stamp still runs; only the inverted-index population is
/// skipped. `unload_cell`'s drain path also short-circuits on
/// missing-resource, so the two stay in lockstep.
#[test]
fn stamp_cell_root_no_ops_index_when_resource_absent() {
    let mut world = World::new();
    // CellRootIndex deliberately NOT registered.
    let (first, last) = spawn_range(&mut world, 4);
    let cell_root = world.spawn();
    stamp_cell_root(&mut world, cell_root, first, last);

    // CellRoot components must still be stamped (the existing
    // semantics for the `CellRoot` SparseSet did not change).
    let q = world.query::<CellRoot>().expect("CellRoot storage present");
    let mut stamped: Vec<EntityId> = q.iter().map(|(eid, _)| eid).collect();
    stamped.sort();
    let mut want: Vec<EntityId> = (first..last).chain(std::iter::once(cell_root)).collect();
    want.sort();
    assert_eq!(stamped, want);

    // The index is absent — try_resource returns None, which is what
    // unload_cell's drain branch handles via and_then chain.
    assert!(world.try_resource::<CellRootIndex>().is_none());
}

/// An empty cell (load that spawns zero entities, only the cell_root
/// itself) still gets a single-element entry in the index. Pre-#791
/// the query path naturally handled this via the cell_root's own
/// CellRoot stamp; the index needs to mirror.
#[test]
fn empty_cell_still_creates_index_entry_for_cell_root() {
    let mut world = World::new();
    world.insert_resource(CellRootIndex::new());

    // Snapshot before/after where no entities are spawned in between.
    let first = world.next_entity_id();
    let last = first; // first == last: empty range
    let cell_root = world.spawn();
    stamp_cell_root(&mut world, cell_root, first, last);

    let idx = world.resource::<CellRootIndex>();
    let entry = idx.map.get(&cell_root).expect("empty cell still gets an entry");
    assert_eq!(entry, &vec![cell_root], "only the cell_root itself");
}
