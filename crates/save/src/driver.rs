//! Save / restore drivers — the glue between a live [`World`] and a
//! [`Snapshot`].
//!
//! [`save_world`] needs only `&World` (it reads through `query` /
//! `try_resource`) and is safe to call from a Late-stage exclusive
//! system. [`restore_world`] needs `&mut World` — it clears every
//! storage and repopulates — so it must run off-frame, drained by the
//! binary between scheduler ticks.

use std::collections::BTreeMap;

use byroredux_core::ecs::world::World;
use byroredux_core::string::StringPool;

use crate::registry::SaveRegistry;
use crate::snapshot::Snapshot;
use crate::SaveError;

/// Capture the live world into a [`Snapshot`].
///
/// Walks every registered component column and saved resource, dumps the
/// `StringPool` in symbol order, and records `next_entity`. Empty
/// component columns and absent resources are omitted to keep the file
/// bounded by *live* state.
pub fn save_world(world: &World, registry: &SaveRegistry) -> Result<Snapshot, SaveError> {
    let strings = world
        .try_resource::<StringPool>()
        .map(|p| p.dump())
        .unwrap_or_default();

    let mut components = BTreeMap::new();
    for (name, save, _load) in registry.component_entries() {
        let value = save(world)?;
        // Skip empty columns — most registered types have no entities in
        // any given cell, and a `[]` per type bloats the file pointlessly.
        if value
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(!value.is_null())
        {
            components.insert(name.to_string(), value);
        }
    }

    let mut resources = BTreeMap::new();
    for (name, save, _load) in registry.resource_entries() {
        let value = save(world)?;
        if !value.is_null() {
            resources.insert(name.to_string(), value);
        }
    }

    Ok(Snapshot {
        next_entity: world.next_entity_id(),
        strings,
        components,
        resources,
    })
}

/// Replace the live world's entity population (and saved resources) with
/// the contents of `snapshot`.
///
/// Order matters:
/// 1. clear every storage (drop the old population),
/// 2. restore the `StringPool` so `FixedString` symbols resolve,
/// 3. set `next_entity` so original ids pass the `insert_batch` guard,
/// 4. repopulate component columns at their saved entity ids,
/// 5. restore saved resources.
///
/// GPU / physics handles referenced by the dropped components are **not**
/// torn down here — that's the caller's responsibility (the binary's
/// cell-unload path already owns it). This function is the pure ECS-data
/// half of a load.
pub fn restore_world(
    world: &mut World,
    registry: &SaveRegistry,
    snapshot: &Snapshot,
) -> Result<(), SaveError> {
    world.clear_entities();
    world.insert_resource(StringPool::from_dump(&snapshot.strings));
    world.set_next_entity(snapshot.next_entity);

    for (name, _save, load) in registry.component_entries() {
        if let Some(value) = snapshot.components.get(name) {
            load(world, value.clone())?;
        }
    }
    for (name, _save, load) in registry.resource_entries() {
        if let Some(value) = snapshot.resources.get(name) {
            load(world, value.clone())?;
        }
    }
    Ok(())
}
