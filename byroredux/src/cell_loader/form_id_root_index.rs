//! Shared single-root FormId → Entity resolution guts, factored out once a
//! second scope needed the exact same shape `persistent_ref_index` already
//! had: [`PersistentRefIndex`](crate::components::PersistentRefIndex)
//! (scoped to the worldspace's persistent CELL, EX-09/#2370) and
//! [`CellRootRefIndex`](crate::components::CellRootRefIndex) (scoped to
//! whichever *ordinary* `CellRoot` a caller names, EX-14/15/EX-16 —
//! see `docs/engine/stream-boundary-state-continuity.md` §3) are both
//! "one root, rebuilt on change, `O(1)` after that" caches over the same
//! `FormIdComponent`/`CellRoot` data. Two distinct resource types still
//! exist on purpose (see each struct's own doc) — a single shared slot
//! would thrash between a persistent-CELL lookup and an ordinary-root
//! lookup made on the same tick — but the build/query logic underneath
//! them doesn't need to exist twice.

use byroredux_core::ecs::components::{CellRoot, FormIdComponent};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::form_id::FormIdPool;
use std::collections::HashMap;

/// Resolve `form_id` (global load-order `u32` space) to its live entity
/// within `root`, rebuilding `map`/`built_for` first if they were last
/// built for a different root. `O(1)` after the (amortized) rebuild.
pub(super) fn resolve(
    world: &World,
    map: &mut HashMap<u32, EntityId>,
    built_for: &mut Option<EntityId>,
    root: EntityId,
    form_id: u32,
) -> Option<EntityId> {
    if *built_for != Some(root) {
        rebuild(world, map, built_for, root);
    }
    map.get(&form_id).copied()
}

fn rebuild(
    world: &World,
    map: &mut HashMap<u32, EntityId>,
    built_for: &mut Option<EntityId>,
    root: EntityId,
) {
    map.clear();
    *built_for = Some(root);
    let Some(q) = world.query::<FormIdComponent>() else {
        return;
    };
    let Some(pool) = world.try_resource::<FormIdPool>() else {
        return;
    };
    for (entity, fid) in q.iter() {
        let owned_by_root = world.get::<CellRoot>(entity).is_some_and(|r| r.0 == root);
        if !owned_by_root {
            continue;
        }
        if let Some(pair) = pool.resolve(fid.0) {
            map.insert(pair.local.0, entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

    fn intern(pool: &mut FormIdPool, plugin: u128, local: u32) -> byroredux_core::form_id::FormId {
        pool.intern(FormIdPair {
            plugin: PluginId(plugin),
            local: LocalFormId(local),
        })
    }

    #[test]
    fn resolves_and_rebuilds_on_root_change() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let mut map = HashMap::new();
        let mut built_for = None;

        let root_a = world.spawn();
        let refr_a = world.spawn();
        world.insert(refr_a, CellRoot(root_a));
        let fid_a = {
            let mut pool = world.resource_mut::<FormIdPool>();
            intern(&mut pool, 1, 0x0001)
        };
        world.insert(refr_a, FormIdComponent(fid_a));

        assert_eq!(
            resolve(&world, &mut map, &mut built_for, root_a, 0x0001),
            Some(refr_a)
        );
        assert_eq!(built_for, Some(root_a));

        // Different root — stale mapping must not leak across the rebuild.
        let root_b = world.spawn();
        assert_eq!(
            resolve(&world, &mut map, &mut built_for, root_b, 0x0001),
            None
        );
        assert_eq!(built_for, Some(root_b));
    }
}
