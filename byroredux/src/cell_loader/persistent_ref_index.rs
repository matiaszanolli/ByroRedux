//! FormId → Entity resolution scoped to the active worldspace's
//! persistent CELL (#2370 / EX-09, "duplicate-persistent-ref guard").
//!
//! [`PersistentRefIndex`](crate::components::PersistentRefIndex) is the
//! resource; this module owns the build/query logic — same split as
//! `CellRootIndex` and `stamp_cell_root_range`
//! ([`super::load`]): the resource is a plain data holder in
//! `components.rs`, populated by a free function in the module that owns
//! the concept it indexes.
//!
//! This is the foundational identity mechanism EX-14/15's cross-worldspace
//! persistent-ref continuity and EX-16's actor/package migration both
//! need before they can be built — see
//! `docs/engine/exterior-readiness-plan.md`.

use crate::components::PersistentRefIndex;
use byroredux_core::ecs::components::{CellRoot, FormIdComponent};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::form_id::FormIdPool;

/// Resolve `form_id` (global load-order `u32` space — the same key space
/// `resolve_entity_by_global_form_id` resolves through) to its live
/// entity within `persistent_root`'s persistent CELL, rebuilding `index`
/// first if it was last built for a different root.
///
/// `O(1)` after the (amortized) rebuild, unlike `World::find_by_form_id`
/// / `resolve_entity_by_global_form_id`, which are a deliberate `O(n)`
/// scan over every `FormIdComponent` entity — fine for their occasional
/// callers, not fine for a per-REFR check during persistent-ref work.
///
/// # Known limitation
/// `PersistentCellApplyJob` (`cell_loader::exterior`) spawns its REFRs
/// across a *budgeted*, possibly multi-frame span. Calling this for a
/// `persistent_root` whose job has not yet reached
/// `PersistentCellApplyProgress::Complete` returns whatever subset had
/// spawned as of the last rebuild — not the job's eventual full set.
/// Callers are expected to query only once the worldspace's persistent
/// CELL has settled; this is documented, not type-enforced.
#[allow(dead_code)] // landed ahead of its consumer — see the module doc; EX-14/15 (#2369) and EX-16 (#2372) are the pending callers
pub(crate) fn resolve_persistent_ref(
    world: &World,
    index: &mut PersistentRefIndex,
    persistent_root: EntityId,
    form_id: u32,
) -> Option<EntityId> {
    if index.built_for != Some(persistent_root) {
        rebuild(world, index, persistent_root);
    }
    index.map.get(&form_id).copied()
}

/// Force a rebuild on the next [`resolve_persistent_ref`] call regardless
/// of whether `persistent_root` has changed — for callers that know the
/// persistent CELL's contents changed underneath an unchanged root (e.g.
/// a resumable `PersistentCellApplyJob` reaching `Complete` after the
/// index was already built against its in-progress state).
#[allow(dead_code)] // landed ahead of its consumer — see the module doc; EX-14/15 (#2369) and EX-16 (#2372) are the pending callers
pub(crate) fn invalidate(index: &mut PersistentRefIndex) {
    index.built_for = None;
}

fn rebuild(world: &World, index: &mut PersistentRefIndex, persistent_root: EntityId) {
    index.map.clear();
    index.built_for = Some(persistent_root);
    let Some(q) = world.query::<FormIdComponent>() else {
        return;
    };
    let Some(pool) = world.try_resource::<FormIdPool>() else {
        return;
    };
    for (entity, fid) in q.iter() {
        let owned_by_persistent_cell = world
            .get::<CellRoot>(entity)
            .is_some_and(|root| root.0 == persistent_root);
        if !owned_by_persistent_cell {
            continue;
        }
        if let Some(pair) = pool.resolve(fid.0) {
            index.map.insert(pair.local.0, entity);
        }
    }
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
    fn resolves_a_persistent_refs_form_id_to_its_entity() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = PersistentRefIndex::new();

        let cell_root = world.spawn();
        let refr = world.spawn();
        world.insert(refr, CellRoot(cell_root));
        let fid = {
            let mut pool = world.resource_mut::<FormIdPool>();
            intern(&mut pool, 1, 0x0001_2345)
        };
        world.insert(refr, FormIdComponent(fid));

        assert_eq!(
            resolve_persistent_ref(&world, &mut index, cell_root, 0x0001_2345),
            Some(refr)
        );
        assert_eq!(index.built_for, Some(cell_root));
    }

    #[test]
    fn misses_cleanly_for_an_unknown_form_id() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = PersistentRefIndex::new();
        let cell_root = world.spawn();

        assert_eq!(
            resolve_persistent_ref(&world, &mut index, cell_root, 0xDEAD_BEEF),
            None
        );
    }

    #[test]
    fn excludes_entities_owned_by_a_different_cell_root() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = PersistentRefIndex::new();

        let persistent_root = world.spawn();
        let ordinary_root = world.spawn();
        let ordinary_refr = world.spawn();
        world.insert(ordinary_refr, CellRoot(ordinary_root));
        let fid = {
            let mut pool = world.resource_mut::<FormIdPool>();
            intern(&mut pool, 2, 0x0000_00AA)
        };
        world.insert(ordinary_refr, FormIdComponent(fid));

        // Same form id, but the only entity carrying it belongs to a
        // different (ordinary, non-persistent) cell root — must not
        // resolve through the persistent index.
        assert_eq!(
            resolve_persistent_ref(&world, &mut index, persistent_root, 0x0000_00AA),
            None
        );
    }

    #[test]
    fn rebuilds_when_the_persistent_root_changes() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = PersistentRefIndex::new();

        let root_a = world.spawn();
        let refr_a = world.spawn();
        world.insert(refr_a, CellRoot(root_a));
        let fid_a = {
            let mut pool = world.resource_mut::<FormIdPool>();
            intern(&mut pool, 3, 0x0000_0001)
        };
        world.insert(refr_a, FormIdComponent(fid_a));
        assert_eq!(
            resolve_persistent_ref(&world, &mut index, root_a, 0x0000_0001),
            Some(refr_a)
        );

        // A worldspace crossing tears the old persistent root down and
        // allocates a fresh one for the destination — simulate that by
        // querying a different root id. The stale root_a→refr_a mapping
        // must not leak into the new lookup.
        let root_b = world.spawn();
        assert_eq!(
            resolve_persistent_ref(&world, &mut index, root_b, 0x0000_0001),
            None
        );
        assert_eq!(index.built_for, Some(root_b));
    }

    #[test]
    fn invalidate_forces_a_rebuild_even_with_the_same_root() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut index = PersistentRefIndex::new();
        let cell_root = world.spawn();

        assert_eq!(
            resolve_persistent_ref(&world, &mut index, cell_root, 0x0000_0042),
            None
        );

        // A REFR spawns into the same (still-resident) persistent cell
        // after the first query — the resumable-job case
        // `resolve_persistent_ref`'s doc caveat describes.
        let refr = world.spawn();
        world.insert(refr, CellRoot(cell_root));
        let fid = {
            let mut pool = world.resource_mut::<FormIdPool>();
            intern(&mut pool, 4, 0x0000_0042)
        };
        world.insert(refr, FormIdComponent(fid));

        // Without invalidation the cached (empty) build wins.
        assert_eq!(
            resolve_persistent_ref(&world, &mut index, cell_root, 0x0000_0042),
            None
        );

        invalidate(&mut index);
        assert_eq!(
            resolve_persistent_ref(&world, &mut index, cell_root, 0x0000_0042),
            Some(refr)
        );
    }
}
