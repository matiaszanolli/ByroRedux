//! FormId → Entity resolution scoped to whichever **ordinary** `CellRoot`
//! a caller names (EX-14/15 #2369 / EX-16 #2372 — see
//! `docs/engine/stream-boundary-state-continuity.md` §3).
//!
//! [`CellRootRefIndex`](crate::components::CellRootRefIndex) is the
//! resource; this module owns the build/query logic — same split as
//! `persistent_ref_index`, whose sibling this is. Both are thin wrappers
//! over the shared single-root build/rebuild walk in
//! [`super::form_id_root_index`].

use super::form_id_root_index;
use crate::components::CellRootRefIndex;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;

/// Resolve `form_id` (global load-order `u32` space — the same key space
/// `resolve_entity_by_global_form_id` resolves through) to its live
/// entity within `root`, rebuilding `index` first if it was last built
/// for a different root. `O(1)` after the (amortized) rebuild.
///
/// Callers are expected to use this for exactly one root at a time and
/// resolve everything they need from it before moving on to another root
/// — see [`CellRootRefIndex`]'s doc for why this isn't a proactively
/// maintained multi-root cache.
#[allow(dead_code)] // landed ahead of its consumer — see the module doc
pub(crate) fn resolve_cell_root_ref(
    world: &World,
    index: &mut CellRootRefIndex,
    root: EntityId,
    form_id: u32,
) -> Option<EntityId> {
    form_id_root_index::resolve(world, &mut index.map, &mut index.built_for, root, form_id)
}

/// Force a rebuild on the next [`resolve_cell_root_ref`] call regardless
/// of whether `root` has changed — for callers that know the root's
/// contents changed underneath an unchanged root entity (e.g. a
/// multi-frame budgeted spawn job reaching completion after the index was
/// already built against its in-progress state, mirroring
/// `persistent_ref_index::invalidate`'s own rationale).
#[allow(dead_code)] // landed ahead of its consumer — see the module doc
pub(crate) fn invalidate(index: &mut CellRootRefIndex) {
    index.built_for = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{CellRoot, FormIdComponent};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    fn intern(pool: &mut FormIdPool, plugin: u128, local: u32) -> byroredux_core::form_id::FormId {
        pool.intern(FormIdPair {
            plugin: PluginId(plugin),
            local: LocalFormId(local),
        })
    }

    #[test]
    fn resolves_a_form_id_to_its_entity_within_the_named_root() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = CellRootRefIndex::new();

        let cell_root = world.spawn();
        let refr = world.spawn();
        world.insert(refr, CellRoot(cell_root));
        let fid = {
            let mut pool = world.resource_mut::<FormIdPool>();
            intern(&mut pool, 1, 0x0001_2345)
        };
        world.insert(refr, FormIdComponent(fid));

        assert_eq!(
            resolve_cell_root_ref(&world, &mut index, cell_root, 0x0001_2345),
            Some(refr)
        );
        assert_eq!(index.built_for, Some(cell_root));
    }

    #[test]
    fn misses_cleanly_for_an_unknown_form_id() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = CellRootRefIndex::new();
        let cell_root = world.spawn();

        assert_eq!(
            resolve_cell_root_ref(&world, &mut index, cell_root, 0xDEAD_BEEF),
            None
        );
    }

    #[test]
    fn excludes_entities_owned_by_a_different_cell_root() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = CellRootRefIndex::new();

        let root_of_interest = world.spawn();
        let other_root = world.spawn();
        let other_refr = world.spawn();
        world.insert(other_refr, CellRoot(other_root));
        let fid = {
            let mut pool = world.resource_mut::<FormIdPool>();
            intern(&mut pool, 2, 0x0000_00AA)
        };
        world.insert(other_refr, FormIdComponent(fid));

        assert_eq!(
            resolve_cell_root_ref(&world, &mut index, root_of_interest, 0x0000_00AA),
            None
        );
    }

    #[test]
    fn rebuilds_when_the_named_root_changes() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = CellRootRefIndex::new();

        let root_a = world.spawn();
        let refr_a = world.spawn();
        world.insert(refr_a, CellRoot(root_a));
        let fid_a = {
            let mut pool = world.resource_mut::<FormIdPool>();
            intern(&mut pool, 3, 0x0000_0001)
        };
        world.insert(refr_a, FormIdComponent(fid_a));
        assert_eq!(
            resolve_cell_root_ref(&world, &mut index, root_a, 0x0000_0001),
            Some(refr_a)
        );

        // A tile despawns and respawns with a fresh root entity (routine
        // ordinary-cell churn, unlike the persistent-CELL case) — the
        // stale root_a→refr_a mapping must not leak into the new lookup.
        let root_b = world.spawn();
        assert_eq!(
            resolve_cell_root_ref(&world, &mut index, root_b, 0x0000_0001),
            None
        );
        assert_eq!(index.built_for, Some(root_b));
    }

    #[test]
    fn invalidate_forces_a_rebuild_even_with_the_same_root() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = CellRootRefIndex::new();
        let cell_root = world.spawn();

        assert_eq!(
            resolve_cell_root_ref(&world, &mut index, cell_root, 0x0000_0042),
            None
        );

        // An entity spawns into the same root after the first (empty)
        // query — a budgeted spawn job settling, mirroring
        // persistent_ref_index's identical test.
        let refr = world.spawn();
        world.insert(refr, CellRoot(cell_root));
        let fid = {
            let mut pool = world.resource_mut::<FormIdPool>();
            intern(&mut pool, 4, 0x0000_0042)
        };
        world.insert(refr, FormIdComponent(fid));

        assert_eq!(
            resolve_cell_root_ref(&world, &mut index, cell_root, 0x0000_0042),
            None
        );

        invalidate(&mut index);
        assert_eq!(
            resolve_cell_root_ref(&world, &mut index, cell_root, 0x0000_0042),
            Some(refr)
        );
    }
}
