//! Gameplay state parked while a placed reference is not resident.
//!
//! Unlike the package/position streaming scratch, these rows survive door and
//! worldspace transitions and are saved. A row is consumed on respawn: resident
//! ECS components are then the sole authority until the next eviction.
//! Instance payloads are owned inline, never as handles into the live pool that
//! unload is about to release. No entity IDs, string-pool IDs or GPU handles.

use std::collections::HashMap;

use byroredux_core::ecs::components::{
    ActorValues, Dead, EquipmentSlots, EquippedWeapon, FormIdComponent, Inventory, ItemStack,
};
use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
use byroredux_core::ecs::{EntityId, Resource, World};
use byroredux_core::form_id::{FormIdPair, FormIdPool};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredStack {
    base_form_id: u32,
    count: u32,
    instance: Option<ItemInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReferenceState {
    inventory: Option<Vec<StoredStack>>,
    equipment: Option<EquipmentSlots>,
    weapon: Option<EquippedWeapon>,
    actor_values: Option<ActorValues>,
    dead: bool,
    /// P3 pickup tombstone: the player picked this placement's item up, so a
    /// respawned copy must come back hidden and uninteractive, not restocked.
    /// Required (not `serde(default)`) — pre-v25 saves are rejected by the
    /// FORMAT_MAJOR gate rather than silently default-filled (#4465 /
    /// SAVE-D2-01, same rule as v23's chargen fields).
    picked_up: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct PersistentReferenceStates {
    // JSON cannot encode a struct as an object key. Serialize stable pairs as
    // rows, retaining O(1) lookup in the running engine.
    #[serde(with = "pair_rows")]
    rows: HashMap<FormIdPair, ReferenceState>,
}

impl Resource for PersistentReferenceStates {}

/// A save reload must not capture or consume the outgoing session's rows.
/// Put them back even on a normal reload failure; the caller replaces them
/// with saved resources only after its reload has succeeded.
pub(crate) fn without_parked_state<T>(
    world: &mut World,
    reload: impl FnOnce(&mut World) -> T,
) -> T {
    let parked = world.remove_resource::<PersistentReferenceStates>();
    // The transient package/position cache also belongs to the outgoing
    // session. Do not let either teardown capture or respawn consume it.
    let had_stream_cache = world
        .remove_resource::<super::stream_snapshot::StreamStateSnapshots>()
        .is_some();
    let outcome = reload(world);
    if let Some(parked) = parked {
        world.insert_resource(parked);
    }
    if had_stream_cache {
        world.insert_resource(super::stream_snapshot::StreamStateSnapshots::default());
    }
    outcome
}

mod pair_rows {
    use super::*;
    pub(super) fn serialize<S: serde::Serializer>(
        rows: &HashMap<FormIdPair, ReferenceState>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        rows.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<HashMap<FormIdPair, ReferenceState>, D::Error> {
        let rows = Vec::<(FormIdPair, ReferenceState)>::deserialize(deserializer)?;
        Ok(rows.into_iter().collect())
    }
}

fn identity(world: &World, entity: EntityId) -> Option<FormIdPair> {
    let id = world.get::<FormIdComponent>(entity).map(|id| id.0)?;
    world.try_resource::<FormIdPool>()?.resolve(id).copied()
}

/// Called before unload frees live item-instance handles and despawns victims.
pub(crate) fn capture(world: &mut World, victims: &[EntityId]) {
    if world.try_resource::<PersistentReferenceStates>().is_none() {
        return;
    }
    let player = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|p| p.0);
    let mut rows = Vec::new();
    for &entity in victims {
        if player == Some(entity) {
            continue;
        }
        let Some(pair) = identity(world, entity) else {
            continue;
        };
        let inventory = world.get::<Inventory>(entity).map(|inv| inv.items.clone());
        let dead = world.get::<Dead>(entity).is_some();
        let picked_up = world
            .get::<crate::inventory::PickedUp>(entity)
            .is_some();
        if inventory.is_none() && !dead && !picked_up {
            continue;
        }
        let stored = inventory.map(|items| {
            let pool = world.try_resource::<ItemInstancePool>();
            items
                .into_iter()
                .map(|stack| {
                    let instance = match stack.instance {
                        Some(id) => Some(pool.as_ref()?.get(id)?.clone()),
                        None => None,
                    };
                    Some(StoredStack {
                        base_form_id: stack.base_form_id,
                        count: stack.count,
                        instance,
                    })
                })
                .collect::<Option<Vec<_>>>()
        });
        let inventory = match stored {
            Some(Some(items)) => Some(items),
            Some(None) => {
                log::error!(
                    "reference-state: dangling item instance on {pair:?}; cannot capture inventory"
                );
                continue;
            }
            None => None,
        };
        rows.push((
            pair,
            ReferenceState {
                inventory,
                equipment: world
                    .get::<EquipmentSlots>(entity)
                    .map(|slots| slots.clone()),
                weapon: world.get::<EquippedWeapon>(entity).map(|weapon| *weapon),
                actor_values: world
                    .get::<ActorValues>(entity)
                    .map(|values| values.clone()),
                dead,
                picked_up,
            },
        ));
    }
    world
        .resource_mut::<PersistentReferenceStates>()
        .rows
        .extend(rows);
}

/// Restore after authored inventory/equipment/AI have been attached. Consuming
/// the parked row prevents stale state from overriding later resident changes.
pub(crate) fn restore(world: &mut World, entity: EntityId) -> bool {
    let Some(pair) = identity(world, entity) else {
        return false;
    };
    let Some(state) = world
        .try_resource_mut::<PersistentReferenceStates>()
        .and_then(|mut store| store.rows.remove(&pair))
    else {
        return false;
    };
    if let Some(stacks) = state.inventory {
        // Free the freshly authored inventory being replaced, not the stored
        // payloads. They have no live arena IDs until allocation below.
        super::unload::release_victim_item_instances(world, &[entity]);
        if world.try_resource::<ItemInstancePool>().is_none() {
            world.insert_resource(ItemInstancePool::new());
        }
        let items = {
            let mut pool = world.resource_mut::<ItemInstancePool>();
            stacks
                .into_iter()
                .map(|stack| ItemStack {
                    base_form_id: stack.base_form_id,
                    count: stack.count,
                    instance: stack.instance.map(|instance| pool.allocate(instance)),
                })
                .collect()
        };
        world.insert(entity, Inventory { items });
    }
    if let Some(equipment) = state.equipment {
        world.insert(entity, equipment);
    }
    if let Some(weapon) = state.weapon {
        world.insert(entity, weapon);
    } else {
        world.remove::<EquippedWeapon>(entity);
    }
    if let Some(values) = state.actor_values {
        world.insert(entity, values);
    }
    if state.dead {
        world.insert(entity, Dead);
        crate::combat::reconcile_dead_actor(world, entity);
    }
    if state.picked_up {
        // The item left with the player in a previous session/visit; the
        // respawned placement must not restock it. The marker hides the
        // meshes and bars interaction; the row is consumed exactly like
        // every other restore because eviction re-captures the marker
        // (see `capture`).
        world.insert(entity, crate::inventory::PickedUp);
    }
    true
}

/// Park a pickup tombstone for a placement whose item the player just took
/// (P3). Writes `picked_up` onto an existing row when the reference already
/// parked state (looted-then-evicted container edge), else inserts a minimal
/// row. Durable across evictions and saves; consumed by [`restore`].
///
/// Takes `&World` (every body operation is interior-mutable resource
/// access) — its one caller, `pickup_item`, holds `&World`.
pub(crate) fn mark_picked_up(world: &World, entity: EntityId) {
    let Some(pair) = identity(world, entity) else {
        return;
    };
    if world.try_resource::<PersistentReferenceStates>().is_none() {
        return;
    }
    let mut store = world.resource_mut::<PersistentReferenceStates>();
    match store.rows.get_mut(&pair) {
        Some(state) => state.picked_up = true,
        None => {
            store.rows.insert(
                pair,
                ReferenceState {
                    inventory: None,
                    equipment: None,
                    weapon: None,
                    actor_values: None,
                    dead: false,
                    picked_up: true,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::InventoryIndex;
    use byroredux_core::form_id::{LocalFormId, PluginId};

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        world.insert_resource(ItemInstancePool::new());
        world.insert_resource(PersistentReferenceStates::default());
        world
    }

    fn reference(world: &mut World, plugin: &str, local: u32) -> EntityId {
        let id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
            plugin: PluginId::from_filename(plugin),
            local: LocalFormId(local),
        });
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(id));
        entity
    }

    fn evict(world: &mut World, entities: &[EntityId]) {
        capture(world, entities);
        super::super::unload::release_victim_item_instances(world, entities);
        world.despawn_batch(entities.to_vec());
    }

    #[test]
    fn save_reload_does_not_capture_or_consume_outgoing_rows() {
        let mut world = world();
        world.insert_resource(super::super::stream_snapshot::StreamStateSnapshots::default());
        let old = reference(&mut world, "One.esm", 0x100);
        world.insert(old, Inventory::new());
        world.insert(old, byroredux_core::ecs::components::Transform::IDENTITY);
        world.insert(old, byroredux_core::ecs::components::Traveled);
        super::super::stream_snapshot::capture_actor_snapshots(&mut world, &[old]);
        assert_eq!(world.resource::<super::super::stream_snapshot::StreamStateSnapshots>().len(), 1);
        capture(&mut world, &[old]);
        world
            .get_mut::<Inventory>(old)
            .unwrap()
            .items
            .push(ItemStack::new(0xAAAA, 5));
        let failed: Option<()> = without_parked_state(&mut world, |world| {
            assert!(world
                .try_resource::<super::super::stream_snapshot::StreamStateSnapshots>()
                .is_none());
            capture(world, &[old]);
            assert!(!restore(world, old));
            None
        });
        assert!(failed.is_none());
        assert!(world
            .resource::<super::super::stream_snapshot::StreamStateSnapshots>()
            .is_empty());
        assert!(restore(&mut world, old));
        assert!(world.get::<Inventory>(old).unwrap().is_empty());
    }

    #[test]
    fn empty_container_and_dead_actor_survive_repeated_evictions() {
        let mut world = world();
        let chest = reference(&mut world, "Skyrim.esm", 0x100);
        let corpse = reference(&mut world, "Skyrim.esm", 0x200);
        world.insert(chest, Inventory::new());
        world.insert(corpse, Inventory::new());
        world.insert(corpse, Dead);
        world.insert(corpse, EquipmentSlots::new());
        world.insert(corpse, ActorValues::from_pairs([(0x2D4, -8.0)]));
        evict(&mut world, &[chest, corpse]);
        for _ in 0..2 {
            let chest = reference(&mut world, "Skyrim.esm", 0x100);
            let corpse = reference(&mut world, "Skyrim.esm", 0x200);
            for entity in [chest, corpse] {
                world.insert(
                    entity,
                    Inventory {
                        items: vec![ItemStack::new(0xAAAA, 9)],
                    },
                );
            }
            let mut equipment = EquipmentSlots::new();
            equipment.equip_weapon(InventoryIndex(0));
            world.insert(corpse, equipment);
            world.insert(
                corpse,
                EquippedWeapon {
                    inventory_index: InventoryIndex(0),
                    base_form_id: 0xAAAA,
                    damage: 12.0,
                    reach: 1.0,
                    speed: 1.0,
                },
            );
            assert!(restore(&mut world, chest));
            assert!(restore(&mut world, corpse));
            assert!(world.get::<Inventory>(chest).unwrap().is_empty());
            assert!(world.get::<Inventory>(corpse).unwrap().is_empty());
            assert!(world.get::<Dead>(corpse).is_some());
            assert!(world.get::<EquippedWeapon>(corpse).is_none());
            assert_eq!(
                world.get::<ActorValues>(corpse).unwrap().current(0x2D4),
                -8.0
            );
            assert!(world
                .resource::<PersistentReferenceStates>()
                .rows
                .is_empty());
            assert!(!restore(&mut world, chest));
            evict(&mut world, &[chest, corpse]);
        }
    }

    #[test]
    fn parked_instances_own_payloads_not_recycled_live_handles() {
        let mut world = world();
        let actor = reference(&mut world, "Fallout4.esm", 0x100);
        let original = world
            .resource_mut::<ItemInstancePool>()
            .allocate(ItemInstance::default());
        world.insert(
            actor,
            Inventory {
                items: vec![ItemStack {
                    base_form_id: 0xAAAA,
                    count: 1,
                    instance: Some(original),
                }],
            },
        );
        evict(&mut world, &[actor]);
        assert_eq!(world.resource::<ItemInstancePool>().live_count(), 0);
        let unrelated = world
            .resource_mut::<ItemInstancePool>()
            .allocate(ItemInstance::default());
        assert_eq!(unrelated, original, "exercise reuse of the released slot");
        let actor = reference(&mut world, "Fallout4.esm", 0x100);
        let spawned = world
            .resource_mut::<ItemInstancePool>()
            .allocate(ItemInstance::default());
        world.insert(
            actor,
            Inventory {
                items: vec![ItemStack {
                    base_form_id: 0xBBBB,
                    count: 1,
                    instance: Some(spawned),
                }],
            },
        );
        assert!(restore(&mut world, actor));
        let stack = world.get::<Inventory>(actor).unwrap().items[0];
        assert_eq!(stack.base_form_id, 0xAAAA);
        assert_ne!(stack.instance, Some(unrelated));
        assert!(world
            .resource::<ItemInstancePool>()
            .get(stack.instance.unwrap())
            .is_some());
        assert!(world
            .resource::<ItemInstancePool>()
            .get(unrelated)
            .is_some());
        assert_eq!(
            world.resource::<ItemInstancePool>().live_count(),
            2,
            "replacement must free authored instances"
        );
    }

    #[test]
    fn parked_state_round_trips_through_real_save_and_keeps_plugin_identity() {
        let mut src = world();
        src.insert_resource(byroredux_core::string::StringPool::new());
        let first = reference(&mut src, "One.esm", 0x100);
        let second = reference(&mut src, "Two.esm", 0x100);
        src.insert(first, Inventory::new());
        src.insert(
            second,
            Inventory {
                items: vec![ItemStack::new(0xAAAA, 7)],
            },
        );
        evict(&mut src, &[first, second]);
        let registry = crate::save_io::build_save_registry();
        let snapshot = byroredux_save::save_world(&src, &registry).unwrap();
        let bytes = byroredux_save::encode(&snapshot, registry.schema_fingerprint()).unwrap();
        let decoded = byroredux_save::decode(&bytes, registry.schema_fingerprint()).unwrap();
        let mut dst = world();
        byroredux_save::restore_resources(&mut dst, &registry, &decoded).unwrap();
        let second = reference(&mut dst, "Two.esm", 0x100);
        let first = reference(&mut dst, "One.esm", 0x100);
        assert!(restore(&mut dst, second));
        assert!(restore(&mut dst, first));
        assert_eq!(dst.get::<Inventory>(second).unwrap().items[0].count, 7);
        assert!(dst.get::<Inventory>(first).unwrap().is_empty());
    }
}
