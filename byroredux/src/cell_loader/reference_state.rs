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
    // #4616 — one guard per component per call, not a TypeId lookup +
    // tracked read per probe per victim. #4982 — and only ONE guard live at
    // a time: each probe is its own pass over the surviving victims. Holding
    // them together recorded `Dead → ActorValues` and
    // `Inventory → ItemInstancePool`, the reverse of
    // `commit_actor_value_deaths` and `validate_inventory_instances`, and
    // kept the lock-order lane red even though `&mut World` rules out a
    // live deadlock here. The `PersistentReferenceStates` write happens
    // strictly after every pass.
    let mut raw_ids: Vec<(EntityId, _)> = Vec::with_capacity(victims.len());
    if let Some(form_q) = world.query::<FormIdComponent>() {
        raw_ids.extend(
            victims
                .iter()
                .filter(|&&entity| player != Some(entity))
                .filter_map(|&entity| form_q.get(entity).map(|c| (entity, c.0))),
        );
    }
    let mut rows: Vec<(EntityId, FormIdPair, ReferenceState)> = {
        let Some(pool) = world.try_resource::<FormIdPool>() else {
            return;
        };
        raw_ids
            .into_iter()
            .filter_map(|(entity, id)| {
                let pair = *pool.resolve(id)?;
                Some((
                    entity,
                    pair,
                    ReferenceState {
                        inventory: None,
                        equipment: None,
                        weapon: None,
                        actor_values: None,
                        dead: false,
                        picked_up: false,
                    },
                ))
            })
            .collect()
    };
    if let Some(dead_q) = world.query::<Dead>() {
        for (entity, _, state) in &mut rows {
            state.dead = dead_q.get(*entity).is_some();
        }
    }
    if let Some(picked_up_q) = world.query::<crate::inventory::PickedUp>() {
        for (entity, _, state) in &mut rows {
            state.picked_up = picked_up_q.get(*entity).is_some();
        }
    }
    let mut live_items: Vec<Option<Vec<ItemStack>>> = vec![None; rows.len()];
    if let Some(inventory_q) = world.query::<Inventory>() {
        for ((entity, _, _), items) in rows.iter().zip(&mut live_items) {
            *items = inventory_q.get(*entity).map(|inv| inv.items.clone());
        }
    }
    // Instance payloads resolve after the `Inventory` guard is gone.
    {
        let instances = world.try_resource::<ItemInstancePool>();
        let mut kept = Vec::with_capacity(rows.len());
        for ((entity, pair, mut state), items) in rows.into_iter().zip(live_items) {
            if items.is_none() && !state.dead && !state.picked_up {
                continue;
            }
            let stored = items.map(|items| {
                items
                    .into_iter()
                    .map(|stack| {
                        let instance = match stack.instance {
                            Some(id) => Some(instances.as_ref()?.get(id)?.clone()),
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
            state.inventory = match stored {
                Some(Some(items)) => Some(items),
                Some(None) => {
                    log::error!(
                        "reference-state: dangling item instance on {pair:?}; cannot capture inventory"
                    );
                    continue;
                }
                None => None,
            };
            kept.push((entity, pair, state));
        }
        rows = kept;
    }
    if let Some(equipment_q) = world.query::<EquipmentSlots>() {
        for (entity, _, state) in &mut rows {
            state.equipment = equipment_q.get(*entity).cloned();
        }
    }
    if let Some(weapon_q) = world.query::<EquippedWeapon>() {
        for (entity, _, state) in &mut rows {
            state.weapon = weapon_q.get(*entity).copied();
        }
    }
    if let Some(values_q) = world.query::<ActorValues>() {
        for (entity, _, state) in &mut rows {
            state.actor_values = values_q.get(*entity).cloned();
        }
    }
    world
        .resource_mut::<PersistentReferenceStates>()
        .rows
        .extend(rows.into_iter().map(|(_, pair, state)| (pair, state)));
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
        // respawned placement must not restock it. The marker bars
        // interaction and hides the placement's meshes; the row is consumed
        // exactly like every other restore because eviction re-captures the
        // marker (see `capture`). #4571 — the meshes are descendants, so
        // the marker lands on the subtree's mesh entities too (the render
        // skips read it there, not on this root).
        world.insert(entity, crate::inventory::PickedUp);
        // #4983 — walk before taking the marker write (see `pickup_loot`).
        let meshes = crate::npc_spawn::loot_appearance::mesh_entities_under(world, entity);
        if let Some(mut markers) = world.query_mut::<crate::inventory::PickedUp>() {
            for mesh in meshes {
                markers.insert(mesh, crate::inventory::PickedUp);
            }
        }
        // #4818 — the respawned placement's colliders are standalone
        // entities, not descendants, and this restore runs *after* they
        // spawn (`spawn.rs` registers shapes before `synth_child.rs` gets
        // to the per-placement state), so the marker cannot cover them.
        // Despawn them outright — `&mut World` makes the full teardown
        // available, and a not-yet-registered body (physics hasn't ticked
        // since the respawn) must not be left for `collect_newcomers` to
        // register the frame after this.
        let colliders = crate::npc_spawn::loot_appearance::collision_entities_of(world, entity);
        if !colliders.is_empty() {
            super::unload::release_victim_rapier_bodies(world, &colliders);
            world.despawn_batch(colliders);
        }
    }
    true
}

/// #4695 — re-run the per-placement restore for every reference currently
/// resident, consuming any parked row that applies. The save-load path
/// needs this: its cell reload runs inside [`without_parked_state`], whose
/// whole point is that the store is *absent* while the cell respawns — so
/// every spawn-time [`restore`] call bails, and the saved rows only arrive
/// with the post-reload resource restore, after the population already
/// exists. Without this pass a picked-up placement in the saved cell comes
/// back restocked while the player's loaded inventory already holds the
/// item — the duplication GAME-D7-2026-09-21-01 measured. The same window
/// covers every other parked fact (inventory, equipment, weapon, actor
/// values, dead), which is why this re-runs the whole [`restore`], not a
/// picked-up-only sweep.
///
/// Rows whose reference is not resident are untouched — they stay parked
/// for the next eviction/respawn, as always. Call only after the saved
/// [`PersistentReferenceStates`] is installed; while the store is absent
/// every call is a no-op by construction.
pub(crate) fn restore_resident(world: &mut World) -> usize {
    let candidates: Vec<EntityId> = match world.query::<FormIdComponent>() {
        Some(query) => query.iter().map(|(entity, _)| entity).collect(),
        None => return 0,
    };
    let mut applied = 0;
    for entity in candidates {
        if restore(world, entity) {
            applied += 1;
        }
    }
    applied
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

    fn lock_order_check_enabled() -> bool {
        std::env::var_os("BYRO_LOCK_ORDER_CHECK").as_deref() == Some(std::ffi::OsStr::new("1"))
    }

    /// #4982 — both unload capture passes probe one storage at a time. The
    /// orders below are the ones production systems take
    /// (`commit_actor_value_deaths`, `validate_inventory_instances`,
    /// `travel_system_inner`, the ambient movers' FormID resolve); a capture
    /// that holds the reverse pair closes a cycle and the detector panics.
    #[test]
    fn unload_captures_do_not_invert_production_lock_orders() {
        if !lock_order_check_enabled() {
            return;
        }
        use super::super::stream_snapshot::{capture_actor_snapshots, StreamStateSnapshots};
        use byroredux_core::ecs::components::travel::{TravelState, Traveled};
        use byroredux_core::ecs::components::Transform;

        let mut world = world();
        world.insert_resource(StreamStateSnapshots::default());
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<crate::inventory::PickedUp>();
        world.register::<crate::components::AmbientPackageRuntime>();
        world.register::<byroredux_core::ecs::components::sandbox::Seated>();
        let actor = reference(&mut world, "Skyrim.esm", 0x300);
        world.insert(actor, Transform::IDENTITY);
        world.insert(actor, Traveled);
        world.insert(actor, Dead);
        world.insert(actor, Inventory::new());
        world.insert(actor, ActorValues::from_pairs([(0x2D4, 10.0)]));
        world.insert(actor, EquipmentSlots::new());

        {
            let _values = world.query::<ActorValues>().unwrap();
            let _dead = world.query::<Dead>().unwrap();
        }
        {
            let _pool = world.resource::<ItemInstancePool>();
            let _inventory = world.query::<Inventory>().unwrap();
        }
        {
            let _traveled = world.query::<Traveled>().unwrap();
            let _state = world.query::<TravelState>().unwrap();
        }
        {
            let _transform = world.query::<Transform>().unwrap();
            let _form = world.query::<FormIdComponent>().unwrap();
        }

        capture_actor_snapshots(&mut world, &[actor]);
        capture(&mut world, &[actor]);
        assert_eq!(world.resource::<StreamStateSnapshots>().len(), 1);
        assert_eq!(world.resource::<PersistentReferenceStates>().rows.len(), 1);
    }

    /// #4983 — `restore` walks the placement's meshes before it takes the
    /// `PickedUp` write, so the marker stays a sink after the hierarchy/skin
    /// cluster the render skips hold (`Children → GlobalTransform →
    /// PickedUp`).
    #[test]
    fn picked_up_restore_walks_meshes_before_taking_the_marker() {
        if !lock_order_check_enabled() {
            return;
        }
        use byroredux_core::ecs::components::{Children, GlobalTransform, MeshHandle, Parent};

        let mut world = world();
        world.register::<crate::inventory::PickedUp>();
        world.register::<GlobalTransform>();
        world.register::<MeshHandle>();
        world.register::<Parent>();
        world.register::<Children>();
        let root = reference(&mut world, "Skyrim.esm", 0x400);
        let mesh = world.spawn();
        world.insert(mesh, Parent(root));
        crate::helpers::add_child(&mut world, root, mesh);
        world.insert(mesh, MeshHandle(3));
        mark_picked_up(&world, root);

        {
            let _children = world.query::<Children>().unwrap();
            let _global = world.query::<GlobalTransform>().unwrap();
            let _picked = world.query::<crate::inventory::PickedUp>().unwrap();
        }

        assert!(restore(&mut world, root));
        assert!(world.get::<crate::inventory::PickedUp>(mesh).is_some());
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

    /// #4695 — the save-reload window. A pickup tombstone parked while the
    /// placement is resident is saved in `PersistentReferenceStates` whole,
    /// but the load path runs its cell reload inside `without_parked_state`
    /// (store absent → spawn-time `restore` bails) and installs the saved
    /// store only afterwards. `restore_resident` is the pass that re-runs
    /// the per-placement restore once the saved store is live: the
    /// tombstone re-stamps `PickedUp` (and sweeps the standalone collision
    /// entities, #4818), the row is consumed, and a second pass is a no-op.
    #[test]
    fn save_reload_window_tombstone_is_applied_by_restore_resident() {
        use crate::npc_spawn::loot_appearance::collision_entities_of;

        let mut world = world();
        let root = reference(&mut world, "FalloutNV", 0x1234);
        let root_form = world.get::<FormIdComponent>(root).map(|form| form.0).unwrap();
        // The placement's collision proxies are standalone entities sharing
        // the placement's stable form id (#1698) — never descendants.
        let collider = world.spawn();
        world.insert(collider, crate::inventory::PickedUp);
        world.remove::<crate::inventory::PickedUp>(collider);
        world.insert(collider, byroredux_core::ecs::components::PhysicsSourceForm(root_form));

        // The player takes the item: marker + tombstone row.
        world.insert(root, crate::inventory::PickedUp);
        mark_picked_up(&world, root);
        assert!(world
            .resource::<PersistentReferenceStates>()
            .rows
            .iter()
            .any(|(_, state)| state.picked_up));

        // The session ends; a load respawns the placement as a NEW entity
        // carrying the same stable identity. The reload runs inside
        // `without_parked_state`, so the spawn-time restore bails — the
        // exact window the bug lives in.
        world.despawn(root);
        let mut respawned = None;
        without_parked_state(&mut world, |world| {
            let entity = reference(world, "FalloutNV", 0x1234);
            let applied = restore(world, entity);
            assert!(!applied, "store is absent during the reload — restore must bail");
            respawned = Some(entity);
        });
        let respawned = respawned.expect("reload spawned the placement");
        assert!(world.get::<crate::inventory::PickedUp>(respawned).is_none());
        assert!(
            world.get::<byroredux_core::ecs::components::PhysicsSourceForm>(collider).is_some(),
            "the respawned collider is still there pre-pass — as in the live reload"
        );

        // The saved store is installed (here: restored by
        // `without_parked_state`), and the post-reload pass consumes the
        // row against the now-resident placement.
        assert_eq!(restore_resident(&mut world), 1);
        assert!(world.get::<crate::inventory::PickedUp>(respawned).is_some());
        assert!(
            collision_entities_of(&world, respawned).is_empty(),
            "#4818 — the tombstone's collision entities must be swept with it"
        );
        // The row was consumed: a second pass finds nothing to do.
        assert_eq!(restore_resident(&mut world), 0);
        assert!(world.resource::<PersistentReferenceStates>().rows.is_empty());
    }
}
