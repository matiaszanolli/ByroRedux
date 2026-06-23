//! Save / restore drivers — the glue between a live [`World`] and a
//! [`Snapshot`].
//!
//! [`save_world`] needs only `&World` (it reads through `query` /
//! `try_resource`) and is safe to call from a Late-stage exclusive
//! system. [`restore_world`] needs `&mut World` — it clears every
//! storage and repopulates — so it must run off-frame, drained by the
//! binary between scheduler ticks.

use std::collections::{BTreeMap, HashMap};

use byroredux_core::ecs::components::FormIdComponent;
use byroredux_core::ecs::world::World;
use byroredux_core::form_id::{FormIdPair, FormIdPool};
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

/// Restore the snapshot's saved **resources** wholesale (e.g.
/// `ItemInstancePool`) onto a reloaded world.
///
/// Resources aren't entity-keyed, so they're replaced outright rather
/// than remapped. Call this before [`apply_deltas`] so inventory deltas
/// whose `ItemInstanceId`s index the pool resolve against the restored
/// arena. Resource columns absent from the snapshot leave the live
/// resource untouched.
pub fn restore_resources(
    world: &mut World,
    registry: &SaveRegistry,
    snapshot: &Snapshot,
) -> Result<(), SaveError> {
    for (name, _save, load) in registry.resource_entries() {
        if let Some(value) = snapshot.resources.get(name) {
            load(world, value.clone())?;
        }
    }
    Ok(())
}

/// Build the `saved-entity-id → live-entity-id` remap used by
/// [`apply_deltas`], keyed on the stable [`FormIdPair`].
///
/// This is the M45.1 "change form" bridge: after a cell is reloaded its
/// entities have fresh ids, so a saved delta can only be re-targeted by
/// matching the saved entity's form id to the live entity that now
/// carries the same `FormIdPair`. Entities without a form id (NIF child
/// nodes, particles) aren't remappable and are simply absent from the
/// returned map — their saved deltas (if any) are skipped.
///
/// Returns an empty map if the registry has no form-id column or the
/// snapshot carries no form-id rows.
pub fn build_form_id_remap(
    world: &World,
    registry: &SaveRegistry,
    snapshot: &Snapshot,
) -> HashMap<u32, u32> {
    let Some(column) = registry.form_id_column() else {
        return HashMap::new();
    };
    let Some(value) = snapshot.components.get(column) else {
        return HashMap::new();
    };
    let saved: Vec<(u32, FormIdPair)> = match serde_json::from_value(value.clone()) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("load: form-id column '{column}' failed to decode for remap: {e}");
            return HashMap::new();
        }
    };

    // pair → live entity id, from the freshly reloaded world.
    let pair_to_live: HashMap<FormIdPair, u32> = match (
        world.query::<FormIdComponent>(),
        world.try_resource::<FormIdPool>(),
    ) {
        (Some(q), Some(pool)) => q
            .iter()
            .filter_map(|(eid, comp)| pool.resolve(comp.0).copied().map(|pair| (pair, eid)))
            .collect(),
        _ => HashMap::new(),
    };

    saved
        .into_iter()
        .filter_map(|(old, pair)| pair_to_live.get(&pair).map(|&live| (old, live)))
        .collect()
}

/// Apply saved component deltas onto a freshly reloaded world, remapping
/// each saved entity id to its live id via `remap`.
///
/// `columns` is the curated set of **mutable game-state** component keys
/// to overlay (e.g. Transform, Inventory, EquipmentSlots, ScriptTimer) —
/// explicitly *not* structural/identity columns (Parent / Children / Name /
/// the form-id key), which the reloaded cell already owns. Note that each
/// row's entity *key* is remapped (saved id → live id) but the component
/// *value* is moved verbatim, so any column whose value embeds a session-local
/// reference (an `EntityId` or registry handle) must NOT be overlaid — see the
/// caller's `MUTABLE_DELTA_COLUMNS` (#1696). Unknown column names and columns
/// absent from the snapshot are skipped. Returns the total rows applied across
/// columns.
pub fn apply_deltas(
    world: &mut World,
    registry: &SaveRegistry,
    snapshot: &Snapshot,
    remap: &HashMap<u32, u32>,
    columns: &[&str],
) -> Result<usize, SaveError> {
    let mut applied = 0;
    for &name in columns {
        let (Some(value), Some(apply)) =
            (snapshot.components.get(name), registry.component_apply(name))
        else {
            continue;
        };
        applied += apply(world, value.clone(), remap)?;
    }
    Ok(applied)
}
