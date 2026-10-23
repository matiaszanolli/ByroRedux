//! Gameplay state parked while a placed reference is not resident.
//!
//! Unlike the package/position streaming scratch, these rows survive door and
//! worldspace transitions and are saved. A row is consumed on respawn: resident
//! ECS components are then the sole authority until the next eviction.
//! Instance payloads are owned inline, never as handles into the live pool that
//! unload is about to release. No entity IDs, string-pool IDs or GPU handles.

use std::collections::HashMap;

use super::load_order::LoadOrder;
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
    /// #4819 — the actor's `SpellList`, parked beside `actor_values` because
    /// the two only make sense together: a constant spell's value change
    /// lives in `actor_values`, its membership here. Parking only the values
    /// re-stamped the authored list on return, so a scripted ability's bonus
    /// outlived its membership (`RemoveSpell` then returned false and the
    /// bonus was permanent) and a second `AddSpell` applied it twice.
    /// Required, like `picked_up` (#4465): pre-v30 saves are rejected.
    spells: Option<Vec<u32>>,
    /// M47.2 — the actor's ranked `Perks`, so a scripted `AddPerk` /
    /// `RemovePerk` on an NPC outlives its cell's eviction (the respawn
    /// re-derives the authored list). Required: pre-v44 saves are rejected.
    perks: Option<byroredux_core::character::Perks>,
    /// #5017 — the actor's `ActorControlState` (restrained / unconscious),
    /// so a scripted wake-up survives eviction. Without it, the spawn-time
    /// "Starts Unconscious" flag would knock a woken robot out again on every
    /// reload. Required: pre-v32 saves are rejected.
    control: Option<byroredux_scripting::ActorControlState>,
    /// #5859 — the actor's `Bleedout`. A spared actor keeps Health ≤ 0 and
    /// only `bleedout_system` gets it back up, so parking the values without
    /// this left a respawned essential NPC standing at ≤ 0 Health, never to
    /// recover. Required: pre-v106 saves are rejected.
    bleedout: Option<byroredux_scripting::Bleedout>,
    /// #6072 — the ObScript `AddScriptPackage` slot. A constrained script
    /// package "stays until removed, replaced, evaluated away, or completed"
    /// (GECK), and an eviction is none of those; without it the respawned
    /// actor's first package tick fell back to its own schedule. Required:
    /// pre-v114 saves are rejected.
    script_package: Option<byroredux_scripting::script_packages::ScriptPackage>,
    dead: bool,
    /// #6073 — a corpse's root pose (translation, rotation) at eviction. The
    /// ragdoll never moves the actor root, so this is where it died; the
    /// respawn otherwise builds it at the authored placement and it dies a
    /// second time there. `None` for a live actor. Required: pre-v114 saves
    /// are rejected.
    corpse_pose: Option<(byroredux_core::math::Vec3, byroredux_core::math::Quat)>,
    /// P3 pickup tombstone: the player picked this placement's item up, so a
    /// respawned copy must come back hidden and uninteractive, not restocked.
    /// Required (not `serde(default)`) — pre-v25 saves are rejected by the
    /// FORMAT_MAJOR gate rather than silently default-filled (#4465 /
    /// SAVE-D2-01, same rule as v23's chargen fields).
    picked_up: bool,
    /// #6085 — the actor's `ActiveMagicEffects`, parked with `actor_values` as
    /// one unit. A *Recover* value effect applies `mod_permanent(+amount)` at
    /// start and the opposite when it finishes, so parking the modified
    /// values without the list that owns the pending revert made a timed
    /// buff or debuff permanent after one eviction round trip (the hazard
    /// #4819 fixed for `SpellList` vs `ActorValues`). Required: pre-v115
    /// saves are rejected.
    magic_effects: Option<byroredux_scripting::magic_effects::ActiveMagicEffects>,
    /// #6085 — ObScript `SetAlert` (`WeaponDrawn`) and `IgnoreCrime`: presence
    /// markers a script sets and nothing re-derives, so they reverted on every
    /// cell exit. Required: pre-v115 saves are rejected.
    weapon_drawn: bool,
    ignore_crime: bool,
}

/// #6071 — a reference's script state at eviction. Kept apart from
/// [`ReferenceState`] because it is consumed at a different boundary: the
/// script attach ([`restore_script_state`]), which stamps the authored
/// `TwoStateActivator` and a fresh `ScriptVariables` and runs after
/// [`restore`] — and which scripted activators, which carry no inventory and
/// so never reach [`restore`], go through too.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScriptState {
    two_state: Option<byroredux_scripting::TwoStateActivator>,
    variables: Option<byroredux_scripting::ScriptVariables>,
    /// #6085 — a spent trap/lever timer must not re-arm on re-entry.
    timer: Option<byroredux_scripting::ScriptTimer>,
    /// #6085 — the rumble activator's `Busy` state, likewise.
    rumble: Option<byroredux_scripting::papyrus_demo::RumbleOnActivate>,
}

impl ScriptState {
    fn is_empty(&self) -> bool {
        self.two_state.is_none()
            && self.variables.is_none()
            && self.timer.is_none()
            && self.rumble.is_none()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct PersistentReferenceStates {
    // JSON cannot encode a struct as an object key. Serialize stable pairs as
    // rows, retaining O(1) lookup in the running engine.
    #[serde(with = "pair_rows")]
    rows: HashMap<FormIdPair, ReferenceState>,
    /// #6071 — required, like the row fields: pre-v114 saves are rejected.
    #[serde(with = "pair_rows")]
    scripts: HashMap<FormIdPair, ScriptState>,
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
    pub(super) fn serialize<S: serde::Serializer, T: Serialize>(
        rows: &HashMap<FormIdPair, T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        rows.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<HashMap<FormIdPair, T>, D::Error> {
        let rows = Vec::<(FormIdPair, T)>::deserialize(deserializer)?;
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
                        spells: None,
                        perks: None,
                        control: None,
                        bleedout: None,
                        script_package: None,
                        dead: false,
                        corpse_pose: None,
                        picked_up: false,
                        magic_effects: None,
                        weapon_drawn: false,
                        ignore_crime: false,
                    },
                ))
            })
            .collect()
    };
    let scripts = capture_script_states(world, &rows);
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
    if let Some(bleedout_q) = world.query::<byroredux_scripting::Bleedout>() {
        for (entity, _, state) in &mut rows {
            state.bleedout = bleedout_q.get(*entity).copied();
        }
    }
    if let Some(package_q) = world.query::<byroredux_scripting::script_packages::ScriptPackage>() {
        for (entity, _, state) in &mut rows {
            state.script_package = package_q.get(*entity).copied();
        }
    }
    if let Some(effects_q) = world.query::<byroredux_scripting::magic_effects::ActiveMagicEffects>()
    {
        for (entity, _, state) in &mut rows {
            state.magic_effects = effects_q.get(*entity).cloned();
        }
    }
    if let Some(drawn_q) = world.query::<byroredux_scripting::obscript_state::WeaponDrawn>() {
        for (entity, _, state) in &mut rows {
            state.weapon_drawn = drawn_q.get(*entity).is_some();
        }
    }
    if let Some(ignore_q) = world.query::<byroredux_scripting::obscript_state::IgnoreCrime>() {
        for (entity, _, state) in &mut rows {
            state.ignore_crime = ignore_q.get(*entity).is_some();
        }
    }
    if let Some(transform_q) = world.query::<byroredux_core::ecs::Transform>() {
        for (entity, _, state) in rows.iter_mut().filter(|(_, _, state)| state.dead) {
            state.corpse_pose = transform_q
                .get(*entity)
                .map(|transform| (transform.translation, transform.rotation));
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
            if items.is_none()
                && !state.dead
                && !state.picked_up
                && state.bleedout.is_none()
                && state.script_package.is_none()
                && state.magic_effects.is_none()
                && !state.weapon_drawn
                && !state.ignore_crime
            {
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
    if let Some(spells_q) = world.query::<byroredux_scripting::SpellList>() {
        for (entity, _, state) in &mut rows {
            state.spells = spells_q.get(*entity).map(|list| list.0.clone());
        }
    }
    if let Some(perks_q) = world.query::<byroredux_core::character::Perks>() {
        for (entity, _, state) in &mut rows {
            state.perks = perks_q.get(*entity).cloned();
        }
    }
    if let Some(control_q) = world.query::<byroredux_scripting::ActorControlState>() {
        for (entity, _, state) in &mut rows {
            state.control = control_q.get(*entity).copied();
        }
    }
    let mut store = world.resource_mut::<PersistentReferenceStates>();
    store
        .rows
        .extend(rows.into_iter().map(|(_, pair, state)| (pair, state)));
    store.scripts.extend(scripts);
}

/// #6071 — the script state of each victim that carries any. Every victim
/// with an identity qualifies, not only the ones [`capture`] keeps an actor
/// row for: a lever or a scripted static has no inventory.
fn capture_script_states(
    world: &World,
    rows: &[(EntityId, FormIdPair, ReferenceState)],
) -> Vec<(FormIdPair, ScriptState)> {
    let mut states: Vec<(FormIdPair, ScriptState)> = rows
        .iter()
        .map(|(_, pair, _)| {
            (
                *pair,
                ScriptState {
                    two_state: None,
                    variables: None,
                    timer: None,
                    rumble: None,
                },
            )
        })
        .collect();
    if let Some(two_state_q) = world.query::<byroredux_scripting::TwoStateActivator>() {
        for ((entity, _, _), (_, state)) in rows.iter().zip(&mut states) {
            state.two_state = two_state_q.get(*entity).copied();
        }
    }
    if let Some(variables_q) = world.query::<byroredux_scripting::ScriptVariables>() {
        for ((entity, _, _), (_, state)) in rows.iter().zip(&mut states) {
            state.variables = variables_q.get(*entity).cloned();
        }
    }
    if let Some(timer_q) = world.query::<byroredux_scripting::ScriptTimer>() {
        for ((entity, _, _), (_, state)) in rows.iter().zip(&mut states) {
            state.timer = timer_q.get(*entity).copied();
        }
    }
    if let Some(rumble_q) = world.query::<byroredux_scripting::papyrus_demo::RumbleOnActivate>() {
        for ((entity, _, _), (_, state)) in rows.iter().zip(&mut states) {
            state.rumble = rumble_q.get(*entity).copied();
        }
    }
    states.retain(|(_, state)| !state.is_empty());
    states
}

/// #6071 — put a respawned reference's script state back. Called by the
/// script attach after both attach arms ran, so the parked state overwrites
/// the authored `TwoStateActivator` and the fresh `ScriptVariables` they just
/// stamped, and before the attach's `OnCellLoadEvent` is consumed, so an
/// `OnLoad` block sees its own locals. Consumes the row, like [`restore`].
pub(crate) fn restore_script_state(world: &mut World, entity: EntityId) -> bool {
    let Some(pair) = identity(world, entity) else {
        return false;
    };
    let Some(state) = world
        .try_resource_mut::<PersistentReferenceStates>()
        .and_then(|mut store| store.scripts.remove(&pair))
    else {
        return false;
    };
    if let Some(two_state) = state.two_state {
        world.insert(entity, two_state);
    }
    if let Some(variables) = state.variables {
        world.insert(entity, variables);
    }
    if let Some(timer) = state.timer {
        world.insert(entity, timer);
    }
    if let Some(rumble) = state.rumble {
        world.insert(entity, rumble);
    }
    true
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
    // #6085 — as one unit with the values: the list owns the pending revert
    // of every Recover effect the parked values already carry.
    if let Some(effects) = state.magic_effects {
        world.insert(entity, effects);
    }
    // #4819 — after the spawn's `stamp_spell_list`, and together with the
    // values above, so membership and the value changes it implies agree.
    if let Some(spells) = state.spells {
        world.insert(entity, byroredux_scripting::SpellList(spells));
    }
    if let Some(perks) = state.perks {
        world.insert(entity, perks);
    }
    // #5017 — before the actor job's `apply_starts_unconscious`, which
    // defers to whatever state this brings back.
    if let Some(control) = state.control {
        world.insert(entity, control);
    }
    if let Some(bleedout) = state.bleedout {
        world.insert(entity, bleedout);
    }
    // #6072 — before the actor's first ambient tick, which reads the slot
    // ahead of its own stack.
    if let Some(script_package) = state.script_package {
        world.insert(entity, script_package);
    }
    if state.weapon_drawn {
        world.insert(entity, byroredux_scripting::obscript_state::WeaponDrawn);
    }
    if state.ignore_crime {
        world.insert(entity, byroredux_scripting::obscript_state::IgnoreCrime);
    }
    if state.dead {
        // #6073 — the root back where the actor died, before the Late
        // reconcile seeds the ragdoll from it. Same divergence rule as the
        // stream snapshot, so a corpse that never left its placement is left
        // exactly on the authored pose.
        if let Some((translation, rotation)) = state.corpse_pose {
            if let Some(mut transforms) = world.query_mut::<byroredux_core::ecs::Transform>() {
                if let Some(transform) = transforms.get_mut(entity) {
                    if translation.distance(transform.translation)
                        > super::stream_snapshot::POSITION_DIVERGENCE_EPSILON
                    {
                        transform.translation = translation;
                    }
                    if !super::stream_snapshot::rotation_matches(rotation, transform.rotation) {
                        transform.rotation = rotation;
                    }
                }
            }
        }
        world.insert(entity, Dead);
        // #5267 (GAME-D4-2026-10-05-01) — queue the death teardown for the
        // Late-stage sink (`reconcile_pending_dead_actors_system`) instead
        // of running it here, mirroring `apply_starts_dead` right below
        // (#4814). This restore runs at actor-job completion, on a freshly
        // respawned skeleton whose bone `GlobalTransform`s are still
        // un-propagated identity (new entities spawn with
        // `GlobalTransform::IDENTITY`; only the placement root is seeded),
        // so a synchronous `reconcile_dead_actor` would seed every ragdoll
        // body from `bone GT ∘ local offset` at the world origin — within
        // #5161's sanity bound that builds an origin ragdoll, beyond it the
        // activation is rejected with the `AnimationPlayer` strip already
        // applied and nothing to retry. The Late drain runs after PostUpdate
        // propagation, so the seed reads real bone globals.
        crate::combat::queue_dead_actor_reconciliation(world, entity);
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
        // entities for the bhk / trimesh ghosts, and this restore runs
        // *after* they spawn (`spawn.rs` registers shapes before
        // `synth_child.rs` gets to the per-placement state), so the marker
        // cannot cover them. Despawn them outright — `&mut World` makes the
        // full teardown available, and a not-yet-registered body (physics
        // hasn't ticked since the respawn) must not be left for
        // `collect_newcomers` to register the frame after this.
        //
        // #6132 — not all of them are standalone: the packed-Havok proxy is
        // `Parent(placement_root)` and a behaviour-graph node collider is
        // `Parent(node)`, both `add_child`-ed. `despawn_batch` frees rows
        // only, so detach them from the parents that survive (this root
        // does) or the dead id stays in `Children` and `validate_world`
        // refuses every later save.
        let colliders = crate::npc_spawn::loot_appearance::collision_entities_of(world, entity);
        if !colliders.is_empty() {
            super::unload::release_victim_rapier_bodies(world, &colliders);
            super::unload::detach_victims_from_surviving_parents(world, &colliders);
            world.despawn_batch(colliders);
        }
    }
    // #5034 — the restore overwrote the spawn job's `EquipmentSlots` with
    // the parked row, but the gear meshes the job spawned came from the
    // record outfit. Diff the live roots against the restored slots so
    // worn gear agrees with the state gameplay reads; the actor job has
    // fully assembled by the time this runs (`stamp_quest_reference` calls
    // restore only after `NpcSpawnProgress::Complete`).
    crate::npc_spawn::loot_appearance::reconcile_worn_gear(world, entity);
    true
}

/// #4814 — an `ACHR` authored "Starts Dead" is a corpse from its first frame:
/// `Dead` (so hostility, combat and package selection skip it, and it is a
/// loot source), with the death teardown — AI removal, ragdoll — queued for
/// the Late-stage sink every other death goes through. Queued rather than
/// run here because the ragdoll seeds from bone `GlobalTransform`s, which a
/// freshly spawned skeleton does not have in world space until the next
/// PostUpdate propagation. No-op when [`restore`] already made it dead
/// (restore queues its own reconciliation since #5267).
/// Called by the actor-job completion after [`restore`], never mid-job.
pub(crate) fn apply_starts_dead(world: &mut World, entity: EntityId) {
    if world.get::<Dead>(entity).is_some() {
        return;
    }
    world.insert(entity, Dead);
    crate::combat::queue_dead_actor_reconciliation(world, entity);
}

/// The base script's `SCRI` FormID for one placed reference, resolving
/// through the record maps that carry one (activators are the
/// dismember-trigger idiom's base; actors carry their own). `None` when
/// the base is not a scripted kind or the script does not resolve.
fn base_script_form_id(
    index: &byroredux_plugin::esm::records::EsmIndex,
    base_form_id: u32,
) -> Option<u32> {
    index
        .activators
        .get(&base_form_id)
        .map(|acti| acti.script_form_id)
        .or_else(|| {
            index
                .npcs
                .get(&base_form_id)
                .map(|npc| npc.script_form_id)
        })
}

/// #5304 — the load-order plugins (lowercased basenames) whose SCPT
/// records may define the kill-on-load scripts: the shipped FO3 GOTY and
/// FNV masters. A merged table is sound because a name can only sit in
/// the load order of the game that ships it. A patch or mod that
/// *overrides* one of these SCPTs keeps the vanilla form id (and so its
/// slot byte), so the form id alone cannot tell it apart; the load order's
/// per-SCPT last-writer table does (#5428), and the overriding plugin
/// fails this list — its conditional body is declined instead of
/// spawning live actors as corpses.
const VANILLA_KILL_SCRIPT_PLUGINS: [&str; 12] = [
    "fallout3.esm",
    "anchorage.esm",
    "thepitt.esm",
    "brokensteel.esm",
    "pointlookout.esm",
    "zeta.esm",
    "falloutnv.esm",
    "deadmoney.esm",
    "honesthearts.esm",
    "oldworldblues.esm",
    "lonesomeroad.esm",
    "gunrunnersarsenal.esm",
];

/// #5304 / #5428 — true when the winning SCPT record for this form id was
/// *written* by one of the shipped vanilla masters (see
/// [`VANILLA_KILL_SCRIPT_PLUGINS`]). The winner is the load order's
/// last-writer table: an override patch keeps the master's form id, so the
/// slot byte ([`plugin_for_form_id`](super::load_order::plugin_for_form_id))
/// would still name the vanilla master. The slot owner is only the
/// fallback for orders with no writer table (hand-built fixtures).
/// Unresolvable form ids decline: the recognizer only ever trusts
/// provably-vanilla scripts.
fn script_is_vanilla(script_form_id: u32, load_order: &LoadOrder) -> bool {
    load_order
        .script_winner(script_form_id)
        .or_else(|| super::load_order::plugin_for_form_id(script_form_id, load_order))
        .is_some_and(|plugin| VANILLA_KILL_SCRIPT_PLUGINS.contains(&plugin))
}

/// The exact `GenericBiped*DismembermentSCRIPT` EDIDs shipped by FO3 and
/// FNV, measured with `corpse_trigger_probe.rs` across every master of
/// both games (2026-10-05): FO3 defines all 11, FNV defines the
/// `Head`/`HeadArmsLegs`/`LeftLeg` subset, and no DLC defines any — so
/// this list is complete for shipped content. Shared with the census
/// test's independent pass-1 re-derivation.
const DISMEMBERMENT_FAMILY_EDIDS: [&str; 11] = [
    "GenericBipedHeadDismembermentSCRIPT",
    "GenericBipedHeadArmsDismembermentSCRIPT",
    "GenericBipedHeadArmsLegsDismembermentSCRIPT",
    "GenericBipedHeadLegsDismembermentSCRIPT",
    "GenericBipedLeftArmDismembermentSCRIPT",
    "GenericBipedLeftLegDismembermentSCRIPT",
    "GenericBipedLeftLegLeftArmDismembermentSCRIPT",
    "GenericBipedRandomMultiDismembermentSCRIPT",
    "GenericBipedRightArmDismembermentSCRIPT",
    "GenericBipedRightLegDismembermentSCRIPT",
    "GenericBipedRightLegRightArmDismembermentSCRIPT",
];

/// #5223 — FO3/FNV's second authored-corpse idiom, recognised from the
/// data because the engine runs no FO3 ObScript. A placed trigger whose
/// base script is the vanilla `GenericBiped*DismembermentSCRIPT` family
/// runs `linkedRef.killactor linkedRef <limb>` once in an unconditional
/// `Begin OnLoad` under `doOnce` — the corpse only exists after that
/// script fires in the real game, so these placements must load dead.
/// The sibling idiom is an actor base whose OWN `SCRI` kills it on load
/// (`GenericKillSCRIPT`, `OnLoadKillSelf`, or the dismember family
/// itself — FFEU04NPC1 carries `GenericBipedRandomMultiDismemberment
/// SCRIPT` on its base and dies the same way).
///
/// #5304 — the family is matched by the exact shipped EDIDs, and a match
/// only counts when the winning SCPT record is defined by a shipped
/// vanilla master ([`VANILLA_KILL_SCRIPT_PLUGINS`]). The original
/// substring match (`contains("DismembermentSCRIPT")`) admitted any mod
/// script whose editor id merely contained the substring, and an
/// override patch could replace a family script's body wholesale; both
/// would mark live actors dead.
///
/// Measured on the shipped masters (2026-10-04,
/// `crates/plugin/examples/corpse_trigger_probe.rs`): FO3 = 85 linked
/// targets + 3 self-killed placements; FNV = 10 + 2. 49 of the FO3
/// targets sit on positive-health bases (mostly the `LvlSuperMutant*DIS
/// MEMBER` leveled family) — the ones the #5005 base-health rule cannot
/// see; the audit also notes all 6 FO3 "XRGD over a live base" refs are
/// among the 85, so with this stamp FO3 `XRGD ⇒ corpse` holds 498/498.
///
/// **Locality warning (#5248)**: this per-call form only recognises a
/// trigger and target that share one `refs` slice. In production every
/// apply consults [`script_killed_corpse_forms_for_load_order`] instead —
/// the cross-cell union — because an exterior trigger and its
/// persistent-CELL target never share a call.
pub(crate) fn script_killed_corpse_forms(
    refs: &[byroredux_plugin::esm::cell::PlacedRef],
    index: &byroredux_plugin::esm::records::EsmIndex,
    load_order: &LoadOrder,
) -> std::collections::HashSet<u32> {
    const SELF_KILL_SCRIPTS: [&str; 2] = ["GenericKillSCRIPT", "OnLoadKillSelf"];

    let mut killed: std::collections::HashSet<u32> = std::collections::HashSet::new();
    // Pass 1 — dismember triggers: a linked-ref placement whose base
    // script is the family marks every XLKR target in the same cell.
    for placed in refs {
        if placed.linked_refs.is_empty() {
            continue;
        }
        let Some(script_form_id) = base_script_form_id(index, placed.base_form_id) else {
            continue;
        };
        let in_family = index
            .scripts
            .get(&script_form_id)
            .is_some_and(|script| DISMEMBERMENT_FAMILY_EDIDS.contains(&script.editor_id.as_str()));
        if !in_family || !script_is_vanilla(script_form_id, load_order) {
            continue;
        }
        for link in &placed.linked_refs {
            killed.insert(link.target);
        }
    }
    // Pass 2 — self-killing bases: an ACTOR placement whose base's own
    // SCRI is an unconditional kill-on-load script. Actor bases only —
    // the activator arm would otherwise re-match the kill *executors*
    // (the triggers themselves), which are not corpses.
    for placed in refs {
        if killed.contains(&placed.form_id) {
            continue;
        }
        let Some(npc) = index.npcs.get(&placed.base_form_id) else {
            continue;
        };
        let Some(script) = index.scripts.get(&npc.script_form_id) else {
            continue;
        };
        let recognized = DISMEMBERMENT_FAMILY_EDIDS.contains(&script.editor_id.as_str())
            || SELF_KILL_SCRIPTS.contains(&script.editor_id.as_str());
        if recognized && script_is_vanilla(script.form_id, load_order) {
            killed.insert(placed.form_id);
        }
    }
    killed
}

/// #5248 — the per-load-order script-killed corpse set, shared by every
/// reference apply. FormIDs are global, but a trigger and its target
/// never share a [`super::references::load_references_budgeted`] call:
/// the worldspace persistent CELL applies with its own `local_refs`
/// (which hold the persistent actors), while each temporary exterior
/// grid cell applies with `cell.references` (which hold the
/// non-persistent trigger activators). The per-call computation #5223
/// shipped therefore missed every exterior trigger → persistent-target
/// link — 29 live-base FO3 corpses (all five MS06 refs among them).
/// Computing over every cell once and consulting the cached set from
/// every apply makes recognition locality-independent by construction,
/// and also drops the per-resume recomputation (a re-scan cloning
/// editor-id strings across FNV's 4,495 `XLKR` placements on every
/// budgeted resume).
///
/// Valid for the process's load order, like `CharacterRuleset` — the
/// order is fixed at boot, so there is nothing to invalidate.
#[derive(Debug, Default, Clone)]
pub(crate) struct ScriptKilledCorpseForms(pub std::sync::Arc<std::collections::HashSet<u32>>);

impl Resource for ScriptKilledCorpseForms {}

/// The recognizer run over **every** cell in the index — interior,
/// exterior grid, and worldspace persistent — unioned. This is the
/// census view, and since #5248 the production one: the result is
/// cached as [`ScriptKilledCorpseForms`] and consulted by every
/// `load_references_budgeted` call, persistent CELL and grid cells
/// alike.
pub(crate) fn script_killed_corpse_forms_for_load_order(
    index: &byroredux_plugin::esm::records::EsmIndex,
    load_order: &LoadOrder,
) -> std::collections::HashSet<u32> {
    let mut killed = std::collections::HashSet::new();
    // #5450 — only FO3/FNV can match: `script_is_vanilla` accepts only the
    // twelve FO3/FNV master names in `VANILLA_KILL_SCRIPT_PLUGINS`, and the
    // family EDIDs are FO3/FNV scripts. On every other game the walk below
    // is a full pass over every placement (hash lookups per ref) on the
    // main thread at first-cell load that can only return the empty set.
    if !matches!(index.game, byroredux_plugin::esm::reader::GameKind::Fallout3NV) {
        return killed;
    }
    let cells = index
        .cells
        .cells
        .values()
        .chain(
            index
                .cells
                .exterior_cells
                .values()
                .flat_map(|tile| tile.values()),
        )
        .chain(index.cells.worldspace_persistent_cells.values());
    for cell in cells {
        killed.extend(script_killed_corpse_forms(&cell.references, index, load_order));
    }
    killed
}

/// #5017 — FO4+'s ACHR "Starts Unconscious" (bit 13): the actor spawns
/// unconscious, as if `SetUnconscious(true)` ran before its first frame. The
/// vanilla population is powered-down robots and turrets that a terminal,
/// pod or quest script wakes. A no-op when [`restore`] brought back a parked
/// `ActorControlState`: that is the actor's live state since it was last
/// resident, a scripted wake-up included. Called by the actor-job completion
/// after [`restore`].
pub(crate) fn apply_starts_unconscious(world: &mut World, entity: EntityId) {
    if world
        .get::<byroredux_scripting::ActorControlState>(entity)
        .is_some()
    {
        return;
    }
    byroredux_scripting::update_actor_control(world, entity, |state| state.set_unconscious(true));
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
        if restore_script_state(world, entity) {
            applied += 1;
        }
    }
    applied
}

/// #5054 — park the saved state of references the load did not respawn.
///
/// An exterior save holds resident cells out to the streaming hysteresis
/// ring (`radius_load + 1`), but the reload streams only `radius_load`, so
/// the ring's `FormIdPair`s never resolve and `apply_deltas` skips their
/// rows. Those placements were resident at save time, so the saved store
/// has no row for them either. Build the row eviction would have written
/// ([`capture`]'s shape and keep rule) from the snapshot's own columns, so
/// the reference comes back with its saved state when it streams in.
/// Merges into an existing row (a pickup tombstone). Item instances move
/// out of the restored pool into the row inline, freeing their slots, the
/// same ownership hand-off as an eviction. Call after the saved resources
/// (pool and store) are installed. Returns the number of rows parked.
pub(crate) fn park_unresolved_snapshot_rows(
    world: &mut World,
    snapshot: &byroredux_save::Snapshot,
    unresolved: &[(u32, FormIdPair)],
) -> usize {
    if unresolved.is_empty() || world.try_resource::<PersistentReferenceStates>().is_none() {
        return 0;
    }
    fn column<T: serde::de::DeserializeOwned>(
        snapshot: &byroredux_save::Snapshot,
        name: &str,
    ) -> HashMap<u32, T> {
        let Some(value) = snapshot.components.get(name) else {
            return HashMap::new();
        };
        match serde_json::from_value::<Vec<(u32, T)>>(value.clone()) {
            Ok(rows) => rows.into_iter().collect(),
            Err(e) => {
                log::warn!("save load: column '{name}' failed to decode for parking: {e}");
                HashMap::new()
            }
        }
    }
    let mut inventories = column::<Inventory>(snapshot, "Inventory");
    let mut equipment = column::<EquipmentSlots>(snapshot, "EquipmentSlots");
    let mut weapons = column::<EquippedWeapon>(snapshot, "EquippedWeapon");
    let mut values = column::<ActorValues>(snapshot, "ActorValues");
    let mut spells = column::<byroredux_scripting::SpellList>(snapshot, "SpellList");
    let mut perks = column::<byroredux_core::character::Perks>(snapshot, "Perks");
    let mut controls =
        column::<byroredux_scripting::ActorControlState>(snapshot, "ActorControlState");
    let mut bleedouts = column::<byroredux_scripting::Bleedout>(snapshot, "Bleedout");
    let mut script_packages =
        column::<byroredux_scripting::script_packages::ScriptPackage>(snapshot, "ScriptPackage");
    let mut transforms = column::<byroredux_core::ecs::Transform>(snapshot, "Transform");
    let mut two_states =
        column::<byroredux_scripting::TwoStateActivator>(snapshot, "TwoStateActivator");
    let mut variables =
        column::<byroredux_scripting::ScriptVariables>(snapshot, "ScriptVariables");
    let mut timers = column::<byroredux_scripting::ScriptTimer>(snapshot, "ScriptTimer");
    let mut rumbles = column::<byroredux_scripting::papyrus_demo::RumbleOnActivate>(
        snapshot,
        "RumbleOnActivate",
    );
    let mut effects = column::<byroredux_scripting::magic_effects::ActiveMagicEffects>(
        snapshot,
        "ActiveMagicEffects",
    );
    let weapon_drawn = column::<byroredux_scripting::obscript_state::WeaponDrawn>(
        snapshot,
        "WeaponDrawn",
    );
    let ignore_crime = column::<byroredux_scripting::obscript_state::IgnoreCrime>(
        snapshot,
        "IgnoreCrime",
    );
    let dead = column::<Dead>(snapshot, "Dead");

    // One resource guard at a time (#4982): read the store's keys, drop it,
    // then take the pool.
    let already_parked: std::collections::HashSet<FormIdPair> = world
        .resource::<PersistentReferenceStates>()
        .rows
        .keys()
        .copied()
        .collect();
    let mut parked = Vec::new();
    // #6071 — the script half parks for every unresolved reference that has
    // one, independent of the actor row's keep rule below.
    let parked_scripts: Vec<(FormIdPair, ScriptState)> = unresolved
        .iter()
        .filter_map(|&(old, pair)| {
            let state = ScriptState {
                two_state: two_states.remove(&old),
                variables: variables.remove(&old),
                timer: timers.remove(&old),
                rumble: rumbles.remove(&old),
            };
            (!state.is_empty()).then_some((pair, state))
        })
        .collect();
    {
        let mut pool = world.try_resource_mut::<ItemInstancePool>();
        for &(old, pair) in unresolved {
            let is_dead = dead.contains_key(&old);
            let items = inventories.remove(&old);
            // #5017 — a control state alone is worth parking: it is how a
            // scripted wake-up outlives the spawn-time "Starts Unconscious".
            let has_control = controls.contains_key(&old);
            let has_bleedout = bleedouts.contains_key(&old);
            let has_script_package = script_packages.contains_key(&old);
            let has_actor_flags = effects.contains_key(&old)
                || weapon_drawn.contains_key(&old)
                || ignore_crime.contains_key(&old);
            if items.is_none()
                && !is_dead
                && !has_control
                && !has_bleedout
                && !has_script_package
                && !has_actor_flags
                && !already_parked.contains(&pair)
            {
                continue;
            }
            let inventory = match items {
                Some(inventory) => {
                    let stored = inventory
                        .items
                        .into_iter()
                        .map(|stack| {
                            let instance = match stack.instance {
                                Some(id) => Some(pool.as_mut()?.release(id)?),
                                None => None,
                            };
                            Some(StoredStack {
                                base_form_id: stack.base_form_id,
                                count: stack.count,
                                instance,
                            })
                        })
                        .collect::<Option<Vec<_>>>();
                    if stored.is_none() {
                        log::error!(
                            "save load: dangling item instance on unresolved {pair:?}; \
                             cannot park inventory"
                        );
                        continue;
                    }
                    stored
                }
                None => None,
            };
            parked.push((
                pair,
                ReferenceState {
                    inventory,
                    equipment: equipment.remove(&old),
                    weapon: weapons.remove(&old),
                    actor_values: values.remove(&old),
                    spells: spells.remove(&old).map(|list| list.0),
                    perks: perks.remove(&old),
                    control: controls.remove(&old),
                    bleedout: bleedouts.remove(&old),
                    script_package: script_packages.remove(&old),
                    dead: is_dead,
                    corpse_pose: transforms
                        .remove(&old)
                        .filter(|_| is_dead)
                        .map(|transform| (transform.translation, transform.rotation)),
                    picked_up: false,
                    magic_effects: effects.remove(&old),
                    weapon_drawn: weapon_drawn.contains_key(&old),
                    ignore_crime: ignore_crime.contains_key(&old),
                },
            ));
        }
    }
    let count = parked.len();
    let mut store = world.resource_mut::<PersistentReferenceStates>();
    for (pair, state) in parked {
        match store.rows.get_mut(&pair) {
            Some(existing) => {
                let picked_up = existing.picked_up;
                *existing = state;
                existing.picked_up = picked_up;
            }
            None => {
                store.rows.insert(pair, state);
            }
        }
    }
    let count = count + parked_scripts.len();
    store.scripts.extend(parked_scripts);
    count
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
                    spells: None,
                    perks: None,
                    control: None,
                    bleedout: None,
                    script_package: None,
                    dead: false,
                    corpse_pose: None,
                    picked_up: true,
                    magic_effects: None,
                    weapon_drawn: false,
                    ignore_crime: false,
                },
            );
        }
    }
}

#[cfg(test)]
mod script_kill_tests {
    use super::*;

    fn placed(
        form_id: u32,
        base_form_id: u32,
        target: Option<u32>,
    ) -> byroredux_plugin::esm::cell::PlacedRef {
        use byroredux_plugin::esm::cell::{LinkedRef, PlacedRef};
        PlacedRef {
            form_id,
            base_form_id,
            group_type: 0xFF,
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: 1.0,
            enable_parent: None,
            teleport: None,
            reputation_ref: None,
            primitive: None,
            activate_parents: Vec::new(),
            parent_activate_only: false,
            linked_refs: target
                .map(|target| vec![LinkedRef { keyword: 0, target }])
                .unwrap_or_default(),
            location_ref_types: Vec::new(),
            rooms: Vec::new(),
            portals: Vec::new(),
            radius_override: None,
            alt_texture_ref: None,
            land_texture_ref: None,
            texture_slot_swaps: Vec::new(),
            emissive_light_ref: None,
            material_swap_ref: None,
            layered_material_swap_refs: Vec::new(),
            ownership: None,
            script_instance: None,
            lock: None,
            water_velocity: None,
            item_count: None,
            initially_disabled: false,
            starts_dead: false,
            starts_unconscious: false,
            ragdoll_pose: Vec::new(),
            map_marker: None,
            encounter_zone_form: None,
        }
    }

    /// #5223 — the recognizer's measured population on the shipped
    /// masters: FO3 = 85 dismember-trigger link targets + 3 self-killed
    /// placements (88 distinct); FNV = 10 + 2 (12 distinct). Pinned so a
    /// decode or recognizer regression shows as a count change, exactly
    /// like the #5005 corpse census beside it.
    ///
    /// #5304 — the recognizer additionally requires the matching SCPT to
    /// resolve to a shipped vanilla master via the load order; the census
    /// parses a single master with no remap, where every defined record
    /// carries that master's own top-byte-0 slot, so a one-entry
    /// [`LoadOrder`] reproduces the production attribution exactly.
    ///
    /// #5248 — the census is locality-aware: each cell's refs are fed to
    /// the recognizer separately (that is what production did per call),
    /// and the shared per-load-order set every apply consults must equal
    /// the union — plus the MS06 pin that a persistent-CELL target whose
    /// trigger sits in a temporary exterior cell is reached only by the
    /// shared set, never by the persistent cell's own refs.
    #[test]
    #[ignore = "needs FO3/FNV game data on disk"]
    fn fo3_fnv_script_killed_corpses_match_the_measured_census() {
        use byroredux_plugin::esm::test_paths;
        for (data_dir, master, linked, self_killed) in [
            (test_paths::fo3_data_dir as fn() -> std::path::PathBuf, "Fallout3.esm", 85, 3),
            (test_paths::fnv_data_dir as fn() -> std::path::PathBuf, "FalloutNV.esm", 10, 2),
        ] {
            let dir = data_dir();
            if !dir.is_dir() {
                test_paths::require_or_skip(&format!("{master}'s game data {dir:?}"));
                continue;
            }
            let bytes = std::fs::read(dir.join(master)).expect("read master");
            let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse master");
            // A single-master load order: every record the parse produced
            // is defined by this master and carries its own slot byte.
            let load_order = LoadOrder::new(
                vec![master.to_ascii_lowercase()],
                vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
            );
            let cells = index.cells.cells.values().chain(
                index
                    .cells
                    .exterior_cells
                    .values()
                    .flat_map(|tile| tile.values()),
            ).chain(index.cells.worldspace_persistent_cells.values());
            // A persistent placement can be walked in more than one cell's
            // children; the recognizer runs per cell at load, but the
            // census is the cross-cell union of form ids.
            let mut union = std::collections::HashSet::new();
            let mut linked_union = std::collections::HashSet::new();
            // #5248 — the persistent CELL's own per-call view, the set the
            // persistent apply consulted before the fix. An exterior
            // trigger's target never shares its call, so cross-bucket
            // links are invisible here.
            let mut persistent_cell_union = std::collections::HashSet::new();
            for cell in cells {
                let recognized =
                    script_killed_corpse_forms(&cell.references, &index, &load_order);
                union.extend(recognized.iter().copied());
                for p in &cell.references {
                    if p.linked_refs.is_empty() {
                        continue;
                    }
                    // Independent re-derivation of pass 1 under the #5304
                    // gate: exact family EDID + vanilla defining plugin.
                    let recognized = super::base_script_form_id(&index, p.base_form_id)
                        .and_then(|script| {
                            index
                                .scripts
                                .get(&script)
                                .map(|s| (script, s.editor_id.clone()))
                        })
                        .is_some_and(|(script, edid)| {
                            DISMEMBERMENT_FAMILY_EDIDS.contains(&edid.as_str())
                                && script_is_vanilla(script, &load_order)
                        });
                    if recognized {
                        linked_union.extend(p.linked_refs.iter().map(|l| l.target));
                    }
                }
            }
            for cell in index.cells.worldspace_persistent_cells.values() {
                persistent_cell_union
                    .extend(script_killed_corpse_forms(&cell.references, &index, &load_order));
            }
            // The load-order-wide set every apply consults since #5248
            // must equal the census union — same recognizer, made
            // locality-independent by construction.
            let load_order_wide =
                script_killed_corpse_forms_for_load_order(&index, &load_order);
            assert_eq!(
                load_order_wide, union,
                "{master}: the shared per-load-order set must equal the per-cell union"
            );
            if master == "Fallout3.esm" {
                // #5248 — MS06 "Head of State": trigger 000645DF-class
                // placements sit in temporary dcworld09 grid cells while
                // their targets (000645E0 among them) live ONLY in the
                // worldspace persistent CELL. The persistent apply's own
                // refs could never see the trigger — the set it consulted
                // pre-fix — while the shared set reaches the target.
                const MS06_GUN_DEAD: u32 = 0x0006_45E0;
                assert!(
                    !persistent_cell_union.contains(&MS06_GUN_DEAD),
                    "{master}: the persistent CELL's own refs must not recognise the \
                     cross-cell MS06 target (locality premise)"
                );
                assert!(
                    load_order_wide.contains(&MS06_GUN_DEAD),
                    "{master}: the shared set must reach the persistent-CELL MS06 target"
                );
            }
            assert_eq!(
                linked_union.len(),
                linked,
                "{master}: dismember-trigger link targets"
            );
            assert_eq!(
                union.len(),
                linked + self_killed,
                "{master}: linked + self-killed, no overlap"
            );
        }
    }

    /// #5304 — the kill-on-load recognizer must not trust a script by name
    /// alone. A mod script whose editor id merely contains
    /// `DismembermentSCRIPT` (a name-alike) is declined, as is an exact
    /// vanilla EDID whose defining plugin is NOT a shipped vanilla master
    /// (a same-EDID override patch). Only an exact vanilla EDID resolving
    /// to a vanilla-master-defined SCPT marks corpses.
    #[test]
    fn kill_on_load_recognition_requires_exact_edid_and_vanilla_master() {
        use byroredux_plugin::esm::records::{NpcRecord, ScriptRecord};

        // FNV plus one mod: form ids with top byte 0 resolve to the
        // vanilla master, top byte 1 to the mod plugin.
        let load_order = LoadOrder::new(
            vec!["falloutnv.esm".to_owned(), "CoolMod.esp".to_owned()],
            vec![
                byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
                byroredux_plugin::esm::reader::GlobalSlot::Regular(1),
            ],
        );

        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        let mut script = |form_id: u32, editor_id: &str| {
            index.scripts.insert(
                form_id,
                ScriptRecord {
                    form_id,
                    editor_id: editor_id.to_owned(),
                    ..Default::default()
                },
            );
        };
        // The real vanilla family script, defined by FalloutNV.esm …
        script(0x0000_1111, "GenericBipedHeadDismembermentSCRIPT");
        // … a mod script whose EDID merely contains the substring …
        script(0x0100_2222, "MyDismembermentSCRIPT");
        // … a NEW same-EDID self-kill script defined under the mod's own
        // slot (not an override — that case is pinned by
        // `kill_on_load_recognition_declines_a_true_override_of_a_vanilla_script`) …
        script(0x0100_3333, "GenericKillSCRIPT");
        // … and a vanilla-defined self-kill script.
        script(0x0000_4444, "GenericKillSCRIPT");

        let mut npc = |form_id: u32, script_form_id: u32| {
            index.npcs.insert(
                form_id,
                NpcRecord {
                    form_id,
                    script_form_id,
                    ..Default::default()
                },
            );
        };
        npc(0x0000_A001, 0x0000_1111); // vanilla dismember family
        npc(0x0000_A002, 0x0100_2222); // name-alike
        npc(0x0000_A003, 0x0000_4444); // vanilla self-kill
        npc(0x0000_A004, 0x0100_3333); // same-EDID script under a mod slot


        // Pass 1 triggers (linked targets) and pass 2 actors.
        let refs = vec![
            placed(0x0000_6001, 0x0000_A001, Some(0x0000_5001)),
            placed(0x0000_6002, 0x0000_A002, Some(0x0000_5002)),
            placed(0x0000_6003, 0x0000_A003, None),
            placed(0x0000_6004, 0x0000_A004, None),
        ];

        let killed = script_killed_corpse_forms(&refs, &index, &load_order);
        assert!(
            killed.contains(&0x0000_5001),
            "an exact vanilla family EDID defined by a vanilla master must mark its link target"
        );
        assert!(
            !killed.contains(&0x0000_5002),
            "a name-alike mod EDID must NOT mark its link target"
        );
        assert!(
            killed.contains(&0x0000_6003),
            "a vanilla-defined self-kill base must mark its placement"
        );
        assert!(
            !killed.contains(&0x0000_6004),
            "a same-EDID self-kill script defined by a non-vanilla plugin must NOT mark its placement"
        );
    }

    /// #5428 — a TRUE override keeps the vanilla master's FormID (and so
    /// its slot byte), and wins the merged index under it. The recognizer
    /// must decline it by the load order's last-writer table, not by the
    /// FormID's slot: a patch that rewrites a kill script's body must not
    /// spawn live actors as corpses. Covers both passes.
    #[test]
    fn kill_on_load_recognition_declines_a_true_override_of_a_vanilla_script() {
        use byroredux_plugin::esm::records::{NpcRecord, ScriptRecord};

        let build = |patch_wins_family: bool, patch_wins_self_kill: bool| {
            let mut load_order = LoadOrder::new(
                vec!["falloutnv.esm".to_owned(), "KillPatch.esp".to_owned()],
                vec![
                    byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
                    byroredux_plugin::esm::reader::GlobalSlot::Regular(1),
                ],
            );
            let mut index = byroredux_plugin::esm::records::EsmIndex::default();
            // Both scripts keep their vanilla (slot 0) FormIDs, exactly as an
            // override record does.
            for (form_id, edid, patched) in [
                (0x0000_1111_u32, "GenericBipedHeadDismembermentSCRIPT", patch_wins_family),
                (0x0000_4444_u32, "GenericKillSCRIPT", patch_wins_self_kill),
            ] {
                index.scripts.insert(
                    form_id,
                    ScriptRecord {
                        form_id,
                        editor_id: edid.to_owned(),
                        ..Default::default()
                    },
                );
                load_order.note_script_writer(form_id, 0);
                if patched {
                    load_order.note_script_writer(form_id, 1);
                }
            }
            for (npc, script) in [(0x0000_A001_u32, 0x0000_1111_u32), (0x0000_A003, 0x0000_4444)] {
                index.npcs.insert(
                    npc,
                    NpcRecord {
                        form_id: npc,
                        script_form_id: script,
                        ..Default::default()
                    },
                );
            }
            let refs = vec![
                placed(0x0000_6001, 0x0000_A001, Some(0x0000_5001)),
                placed(0x0000_6003, 0x0000_A003, None),
            ];
            script_killed_corpse_forms(&refs, &index, &load_order)
        };

        let untouched = build(false, false);
        assert!(untouched.contains(&0x0000_5001) && untouched.contains(&0x0000_6003));

        let family_patched = build(true, false);
        assert!(
            !family_patched.contains(&0x0000_5001),
            "an overridden dismember-family script must not mark its link target"
        );
        assert!(family_patched.contains(&0x0000_6003));

        let self_kill_patched = build(false, true);
        assert!(
            !self_kill_patched.contains(&0x0000_6003),
            "an overridden self-kill script must not mark its placement"
        );
        assert!(self_kill_patched.contains(&0x0000_5001));
    }

    /// #5450 — the whole-load-order corpse scan only has a result on
    /// FO3/FNV. The same trigger placement that marks its link target on
    /// an FO3/FNV index must cost a game-gated early return (and yield the
    /// empty set) on every other game, rather than a full pass over every
    /// placement.
    #[test]
    fn load_order_scan_is_gated_to_fo3_fnv() {
        use byroredux_plugin::esm::cell::CellData;
        use byroredux_plugin::esm::reader::GameKind;
        use byroredux_plugin::esm::records::{NpcRecord, ScriptRecord};

        let load_order = LoadOrder::new(
            vec!["falloutnv.esm".to_owned()],
            vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
        );
        let build = |game: GameKind| {
            let mut index = byroredux_plugin::esm::records::EsmIndex {
                game,
                ..Default::default()
            };
            index.scripts.insert(
                0x0000_1111,
                ScriptRecord {
                    form_id: 0x0000_1111,
                    editor_id: "GenericBipedHeadDismembermentSCRIPT".to_owned(),
                    ..Default::default()
                },
            );
            index.npcs.insert(
                0x0000_A001,
                NpcRecord {
                    form_id: 0x0000_A001,
                    script_form_id: 0x0000_1111,
                    ..Default::default()
                },
            );
            let cell = CellData {
                form_id: 0x0001_2345,
                editor_id: "Cell".to_owned(),
                references: vec![placed(0x0000_6001, 0x0000_A001, Some(0x0000_5001))],
                is_interior: true,
                ..Default::default()
            };
            index.cells.cells.insert("cell".to_owned(), cell);
            index
        };

        let fnv = script_killed_corpse_forms_for_load_order(&build(GameKind::Fallout3NV), &load_order);
        assert!(fnv.contains(&0x0000_5001), "positive control: FO3/FNV still recognizes");
        for game in [GameKind::Oblivion, GameKind::Skyrim, GameKind::Fallout4] {
            assert!(
                script_killed_corpse_forms_for_load_order(&build(game), &load_order).is_empty(),
                "{game:?} cannot match and must not scan"
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

    /// #4819 — a scripted `AddSpell` survives the actor's cell eviction with
    /// its membership, not only its value change: the respawn re-stamps the
    /// authored list, and restore must put the parked list back over it so
    /// a later `RemoveSpell` finds the spell and undoes the bonus.
    #[test]
    fn spell_list_is_parked_and_restored_with_actor_values() {
        use byroredux_scripting::SpellList;
        let mut world = world();
        let actor = reference(&mut world, "Skyrim.esm", 0x500);
        world.insert(actor, Inventory::new());
        world.insert(actor, ActorValues::from_pairs([(0x2D4, 15.0)]));
        world.insert(actor, SpellList(vec![0x10, 0xAB]));
        evict(&mut world, &[actor]);

        let actor = reference(&mut world, "Skyrim.esm", 0x500);
        world.insert(actor, Inventory::new());
        world.insert(actor, ActorValues::from_pairs([(0x2D4, 10.0)]));
        world.insert(actor, SpellList(vec![0x10]));
        assert!(restore(&mut world, actor));
        assert_eq!(world.get::<SpellList>(actor).unwrap().0, vec![0x10, 0xAB]);
    }

    /// M47.2 — a scripted perk on an NPC survives its cell's eviction.
    #[test]
    fn perks_are_parked_and_restored() {
        use byroredux_core::character::Perks;
        let mut world = world();
        let actor = reference(&mut world, "Skyrim.esm", 0x501);
        world.insert(actor, Inventory::new());
        let mut perks = Perks::default();
        perks.set_rank(0xBEEF, 1);
        world.insert(actor, perks.clone());
        evict(&mut world, &[actor]);

        let actor = reference(&mut world, "Skyrim.esm", 0x501);
        world.insert(actor, Inventory::new());
        world.insert(actor, Perks::default());
        assert!(restore(&mut world, actor));
        assert_eq!(world.get::<Perks>(actor).unwrap().rank(0xBEEF), 1);
    }

    /// #5017 — a "Starts Unconscious" robot spawns unconscious; a script
    /// wakes it; the wake-up survives eviction, because the parked control
    /// state is restored before the spawn-time flag gets a say.
    #[test]
    fn a_woken_starts_unconscious_actor_stays_awake_across_eviction() {
        use byroredux_scripting::{is_unconscious, update_actor_control};
        let mut world = world();
        world.register::<byroredux_scripting::ActorControlState>();
        let robot = reference(&mut world, "Fallout4.esm", 0x600);
        world.insert(robot, Inventory::new());
        apply_starts_unconscious(&mut world, robot);
        assert!(is_unconscious(&world, robot), "dormant at spawn");

        update_actor_control(&world, robot, |state| state.set_unconscious(false));
        evict(&mut world, &[robot]);

        // Respawn: restore first, then the actor job's spawn-time flag.
        let robot = reference(&mut world, "Fallout4.esm", 0x600);
        world.insert(robot, Inventory::new());
        assert!(restore(&mut world, robot));
        apply_starts_unconscious(&mut world, robot);
        assert!(!is_unconscious(&world, robot), "the scripted wake-up holds");

        // A robot never parked (first visit) still spawns dormant.
        let fresh = reference(&mut world, "Fallout4.esm", 0x601);
        apply_starts_unconscious(&mut world, fresh);
        assert!(is_unconscious(&world, fresh));
    }

    /// #5859 — an essential NPC evicted mid-bleedout comes back still down,
    /// with the timer it left on, and then gets back up. Without the parked
    /// `Bleedout` it respawned standing at ≤ 0 Health and never recovered.
    #[test]
    fn an_actor_evicted_mid_bleedout_still_bleeds_out_and_recovers() {
        use byroredux_core::ecs::components::ActorVitals;
        use byroredux_scripting::{bleedout_system, is_bleeding_out, Bleedout, BleedoutRules};
        const HEALTH: u32 = 0x3E8;
        let mut world = world();
        world.register::<Bleedout>();
        world.insert_resource(BleedoutRules {
            death_time_secs: 20.0,
            health_regain: 0.3,
        });
        let down = |health: f32| {
            let mut values = ActorValues::new();
            values.set_base(HEALTH, 100.0);
            values.apply_damage(HEALTH, 100.0 - health);
            values
        };
        let npc = reference(&mut world, "Skyrim.esm", 0x700);
        world.insert(npc, Inventory::new());
        world.insert(npc, down(-5.0));
        world.insert(npc, Bleedout { remaining_secs: Some(12.0) });
        evict(&mut world, &[npc]);

        // Respawn at full authored health, then the parked row.
        let npc = reference(&mut world, "Skyrim.esm", 0x700);
        world.insert(npc, Inventory::new());
        world.insert(npc, down(100.0));
        world.insert(npc, ActorVitals { health: HEALTH });
        assert!(restore(&mut world, npc));
        assert_eq!(
            world.get::<Bleedout>(npc).map(|state| state.remaining_secs),
            Some(Some(12.0))
        );
        assert!(world.get::<ActorValues>(npc).unwrap().current(HEALTH) <= 0.0);

        bleedout_system(&world, 12.5);
        assert!(!is_bleeding_out(&world, npc), "the timer runs out");
        assert_eq!(world.get::<ActorValues>(npc).unwrap().current(HEALTH), 30.0);
    }

    /// #5859 — the save-reload park path carries `Bleedout` too, even for an
    /// actor with no other state worth parking.
    #[test]
    fn an_unresolved_bleedout_row_is_parked_for_the_next_respawn() {
        use byroredux_scripting::Bleedout;
        let mut src = world();
        src.register::<Bleedout>();
        src.insert_resource(byroredux_core::string::StringPool::new());
        let down = reference(&mut src, "Band.esm", 0x400);
        src.insert(down, Bleedout { remaining_secs: Some(7.0) });
        let registry = crate::save_io::build_save_registry();
        let snapshot = byroredux_save::save_world(&src, &registry).unwrap();
        let bytes = byroredux_save::encode(&snapshot, registry.schema_fingerprint()).unwrap();
        let decoded = byroredux_save::decode(&bytes, registry.schema_fingerprint()).unwrap();

        let mut dst = world();
        dst.register::<Bleedout>();
        byroredux_save::restore_resources(&mut dst, &registry, &decoded).unwrap();
        let remap = byroredux_save::build_form_id_remap(&dst, &registry, &decoded);
        let unresolved = byroredux_save::unresolved_form_id_pairs(&registry, &decoded, &remap);
        assert_eq!(park_unresolved_snapshot_rows(&mut dst, &decoded, &unresolved), 1);

        let down = reference(&mut dst, "Band.esm", 0x400);
        assert!(restore(&mut dst, down));
        assert_eq!(
            dst.get::<Bleedout>(down).map(|state| state.remaining_secs),
            Some(Some(7.0))
        );
    }

    /// #6071 — an opened two-state lever and an object script's local outlive
    /// the eviction. The respawn's script attach stamps the authored
    /// (closed, never-activated) state and a fresh local set; the parked row
    /// then puts the visit's state back over them. The lever has no inventory,
    /// so this row is one [`restore`] would never have reached.
    #[test]
    fn script_state_survives_eviction_over_the_attach_defaults() {
        use byroredux_scripting::{ScriptVariables, TwoStateActivator};
        let mut world = world();
        world.register::<TwoStateActivator>();
        world.register::<ScriptVariables>();
        let authored = TwoStateActivator {
            is_open: false,
            is_animating: false,
            do_once: true,
            activated_once: false,
        };
        let lever = reference(&mut world, "Skyrim.esm", 0x800);
        world.insert(
            lever,
            TwoStateActivator {
                is_open: true,
                activated_once: true,
                ..authored
            },
        );
        let mut locals = ScriptVariables::default();
        locals.set_by_name("bDone", 1.0);
        world.insert(lever, locals);
        let untouched = reference(&mut world, "Skyrim.esm", 0x801);
        evict(&mut world, &[lever, untouched]);
        assert_eq!(
            world.resource::<PersistentReferenceStates>().scripts.len(),
            1,
            "a reference with no script state parks no script row"
        );

        // Respawn: the attach stamps the authored defaults, then restores.
        let lever = reference(&mut world, "Skyrim.esm", 0x800);
        world.insert(lever, authored);
        world.insert(lever, ScriptVariables::default());
        assert!(!restore(&mut world, lever), "no actor row was parked");
        assert!(restore_script_state(&mut world, lever));
        let state = world.get::<TwoStateActivator>(lever).map(|state| *state);
        assert_eq!(state.map(|s| (s.is_open, s.activated_once)), Some((true, true)));
        assert_eq!(
            world.get::<ScriptVariables>(lever).unwrap().get_by_name("bDone"),
            Some(1.0)
        );
        assert!(!restore_script_state(&mut world, lever), "the row is consumed");
    }

    /// #6071 — the save-reload park path carries the script half too, for a
    /// reference with no actor row at all.
    #[test]
    fn an_unresolved_script_state_is_parked_for_the_next_respawn() {
        use byroredux_scripting::{ScriptVariables, TwoStateActivator};
        let mut src = world();
        src.register::<TwoStateActivator>();
        src.register::<ScriptVariables>();
        src.insert_resource(byroredux_core::string::StringPool::new());
        let gate = reference(&mut src, "Band.esm", 0x410);
        src.insert(
            gate,
            TwoStateActivator {
                is_open: true,
                is_animating: false,
                do_once: false,
                activated_once: true,
            },
        );
        let registry = crate::save_io::build_save_registry();
        let snapshot = byroredux_save::save_world(&src, &registry).unwrap();
        let bytes = byroredux_save::encode(&snapshot, registry.schema_fingerprint()).unwrap();
        let decoded = byroredux_save::decode(&bytes, registry.schema_fingerprint()).unwrap();

        let mut dst = world();
        dst.register::<TwoStateActivator>();
        dst.register::<ScriptVariables>();
        byroredux_save::restore_resources(&mut dst, &registry, &decoded).unwrap();
        let remap = byroredux_save::build_form_id_remap(&dst, &registry, &decoded);
        let unresolved = byroredux_save::unresolved_form_id_pairs(&registry, &decoded, &remap);
        assert_eq!(park_unresolved_snapshot_rows(&mut dst, &decoded, &unresolved), 1);

        let gate = reference(&mut dst, "Band.esm", 0x410);
        assert!(restore_script_state(&mut dst, gate));
        assert!(dst.get::<TwoStateActivator>(gate).is_some_and(|state| state.is_open));
    }

    /// #6085 — a timed *Recover* effect applies `mod_permanent(+amount)` at
    /// its start and the opposite when it finishes. Parking the modified
    /// `ActorValues` without the `ActiveMagicEffects` that owns the pending
    /// revert made the buff permanent after one eviction round trip.
    #[test]
    fn a_timed_buff_still_wears_off_after_an_eviction_round_trip() {
        use byroredux_scripting::magic_effects::{
            magic_effect_system, start_effects, ActiveMagicEffects, CastEffect, ValueEffect,
        };
        const STRENGTH: u32 = 0x5E1;
        let authored = || {
            let mut values = ActorValues::new();
            values.set_base(STRENGTH, 50.0);
            values
        };
        let mut world = world();
        world.register::<ActiveMagicEffects>();
        let npc = reference(&mut world, "FalloutNV.esm", 0xA00);
        world.insert(npc, Inventory::new());
        world.insert(npc, authored());
        let buff = CastEffect {
            mgef: 0x77,
            script: 0,
            duration: 10.0,
            magnitude: 25.0,
            value: Some(ValueEffect {
                actor_value: STRENGTH,
                amount: 25.0,
                recover: true,
            }),
        };
        assert_eq!(start_effects(&world, npc, None, 0x99, &[buff], false), 1);
        magic_effect_system(&world, 0.0);
        assert_eq!(world.get::<ActorValues>(npc).unwrap().current(STRENGTH), 75.0);
        evict(&mut world, &[npc]);

        // Respawn on the authored values, then the parked row.
        let npc = reference(&mut world, "FalloutNV.esm", 0xA00);
        world.insert(npc, Inventory::new());
        world.insert(npc, authored());
        assert!(restore(&mut world, npc));
        assert_eq!(world.get::<ActorValues>(npc).unwrap().current(STRENGTH), 75.0);
        assert!(world.get::<ActiveMagicEffects>(npc).is_some());

        magic_effect_system(&world, 11.0);
        assert_eq!(
            world.get::<ActorValues>(npc).unwrap().current(STRENGTH),
            50.0,
            "the effect's revert survived the eviction"
        );
    }

    /// #6085 — the ObScript `SetAlert` / `IgnoreCrime` markers and the
    /// rumble/timer script state outlive a cell exit.
    #[test]
    fn scripted_actor_flags_and_trap_timers_survive_eviction() {
        use byroredux_scripting::obscript_state::{IgnoreCrime, WeaponDrawn};
        use byroredux_scripting::papyrus_demo::{RumbleOnActivate, RumbleState};
        use byroredux_scripting::ScriptTimer;
        let mut world = world();
        world.register::<WeaponDrawn>();
        world.register::<IgnoreCrime>();
        world.register::<ScriptTimer>();
        world.register::<RumbleOnActivate>();
        let guard = reference(&mut world, "FalloutNV.esm", 0xA10);
        world.insert(guard, Inventory::new());
        world.insert(guard, WeaponDrawn);
        world.insert(guard, IgnoreCrime);
        let trap = reference(&mut world, "FalloutNV.esm", 0xA11);
        world.insert(trap, ScriptTimer { id: 7, remaining: 3.5 });
        let rumble = RumbleOnActivate {
            camera_intensity: 0.5,
            duration: 2.0,
            repeatable: false,
            shake_left: 0.1,
            shake_right: 0.2,
            state: RumbleState::Busy {
                wait_remaining_secs: 1.25,
            },
        };
        world.insert(trap, rumble);
        evict(&mut world, &[guard, trap]);

        let guard = reference(&mut world, "FalloutNV.esm", 0xA10);
        world.insert(guard, Inventory::new());
        assert!(restore(&mut world, guard));
        assert!(world.has::<WeaponDrawn>(guard) && world.has::<IgnoreCrime>(guard));

        let trap = reference(&mut world, "FalloutNV.esm", 0xA11);
        assert!(restore_script_state(&mut world, trap));
        assert_eq!(world.get::<ScriptTimer>(trap).map(|t| (t.id, t.remaining)), Some((7, 3.5)));
        assert!(matches!(
            world.get::<RumbleOnActivate>(trap).map(|r| r.state),
            Some(RumbleState::Busy { wait_remaining_secs }) if wait_remaining_secs == 1.25
        ));
    }

    /// #6085 — the save-reload ring park carries the same new columns.
    #[test]
    fn unresolved_effects_flags_and_timers_are_parked_for_the_next_respawn() {
        use byroredux_scripting::magic_effects::{ActiveEffect, ActiveMagicEffects};
        use byroredux_scripting::obscript_state::WeaponDrawn;
        use byroredux_scripting::ScriptTimer;
        let mut src = world();
        src.register::<ActiveMagicEffects>();
        src.register::<WeaponDrawn>();
        src.register::<ScriptTimer>();
        src.insert_resource(byroredux_core::string::StringPool::new());
        let npc = reference(&mut src, "Band.esm", 0x420);
        src.insert(npc, WeaponDrawn);
        src.insert(
            npc,
            ActiveMagicEffects(vec![ActiveEffect {
                source: 1,
                mgef: 2,
                caster: 0,
                script: 0,
                duration: 30.0,
                indefinite: false,
                elapsed: 4.0,
                started: true,
                ending: false,
                value: None,
                ticks: 0,
                locals: Vec::new(),
                ref_locals: Vec::new(),
            }]),
        );
        let trap = reference(&mut src, "Band.esm", 0x421);
        src.insert(trap, ScriptTimer { id: 3, remaining: 9.0 });
        let registry = crate::save_io::build_save_registry();
        let snapshot = byroredux_save::save_world(&src, &registry).unwrap();
        let bytes = byroredux_save::encode(&snapshot, registry.schema_fingerprint()).unwrap();
        let decoded = byroredux_save::decode(&bytes, registry.schema_fingerprint()).unwrap();

        let mut dst = world();
        dst.register::<ActiveMagicEffects>();
        dst.register::<WeaponDrawn>();
        dst.register::<ScriptTimer>();
        byroredux_save::restore_resources(&mut dst, &registry, &decoded).unwrap();
        let remap = byroredux_save::build_form_id_remap(&dst, &registry, &decoded);
        let unresolved = byroredux_save::unresolved_form_id_pairs(&registry, &decoded, &remap);
        assert_eq!(park_unresolved_snapshot_rows(&mut dst, &decoded, &unresolved), 2);

        let npc = reference(&mut dst, "Band.esm", 0x420);
        assert!(restore(&mut dst, npc));
        assert!(dst.has::<WeaponDrawn>(npc));
        assert_eq!(
            dst.get::<ActiveMagicEffects>(npc).map(|e| e.0.len()),
            Some(1),
            "the effect list parks with the actor"
        );
        let trap = reference(&mut dst, "Band.esm", 0x421);
        assert!(restore_script_state(&mut dst, trap));
        assert_eq!(dst.get::<ScriptTimer>(trap).map(|t| t.id), Some(3));
    }

    /// #6072 — a script package outlives the eviction, so the respawned
    /// actor's first package tick still reads the slot ahead of its schedule.
    #[test]
    fn a_script_package_survives_eviction() {
        use byroredux_scripting::script_packages::ScriptPackage;
        let mut world = world();
        world.register::<ScriptPackage>();
        let courier = reference(&mut world, "FalloutNV.esm", 0x900);
        world.insert(courier, Inventory::new());
        let slot = ScriptPackage {
            package: 0x0012_3456,
            constrained: true,
            running_secs: Some(42.0),
        };
        world.insert(courier, slot);
        evict(&mut world, &[courier]);

        let courier = reference(&mut world, "FalloutNV.esm", 0x900);
        world.insert(courier, Inventory::new());
        assert!(world.get::<ScriptPackage>(courier).is_none());
        assert!(restore(&mut world, courier));
        assert_eq!(world.get::<ScriptPackage>(courier).map(|s| *s), Some(slot));
    }

    /// #6073 — a corpse comes back where it died, not at its authored
    /// placement; one that died on its placement is left on the authored pose.
    #[test]
    fn a_displaced_corpse_comes_back_where_it_died() {
        use byroredux_core::ecs::Transform;
        use byroredux_core::math::{Quat, Vec3};
        let mut world = world();
        world.register::<Transform>();
        world.insert_resource(crate::combat::PendingDeathReconciliations::default());
        let authored = Transform::new(Vec3::new(100.0, 0.0, 50.0), Quat::IDENTITY, 1.0);
        let fell_at = Transform::new(
            Vec3::new(2400.0, 10.0, 50.0),
            Quat::from_rotation_y(1.2),
            1.0,
        );
        let corpse = reference(&mut world, "FalloutNV.esm", 0x910);
        world.insert(corpse, Inventory::new());
        world.insert(corpse, Dead);
        world.insert(corpse, fell_at);
        let in_place = reference(&mut world, "FalloutNV.esm", 0x911);
        world.insert(in_place, Inventory::new());
        world.insert(in_place, Dead);
        let nudged = Vec3::new(101.0, 0.0, 50.0);
        world.insert(in_place, Transform::new(nudged, Quat::IDENTITY, 1.0));
        evict(&mut world, &[corpse, in_place]);

        let corpse = reference(&mut world, "FalloutNV.esm", 0x910);
        world.insert(corpse, Inventory::new());
        world.insert(corpse, authored);
        let in_place = reference(&mut world, "FalloutNV.esm", 0x911);
        world.insert(in_place, Inventory::new());
        world.insert(in_place, authored);
        assert!(restore(&mut world, corpse));
        assert!(restore(&mut world, in_place));

        let restored = world.get::<Transform>(corpse).map(|t| *t).unwrap();
        assert_eq!(restored.translation, fell_at.translation);
        assert_eq!(restored.rotation, fell_at.rotation);
        assert_eq!(
            world.get::<Transform>(in_place).map(|t| t.translation),
            Some(authored.translation),
            "inside the divergence epsilon the authored pose stays"
        );
        assert!(world.get::<Dead>(corpse).is_some());
    }

    /// #4814 — an authored "Starts Dead" actor is `Dead` at spawn completion
    /// (a loot source, invisible to hostility and package selection) and its
    /// teardown is queued for the Late-stage sink; a corpse `restore` already
    /// made dead is not queued a second time.
    #[test]
    fn starts_dead_actor_is_a_queued_corpse_at_completion() {
        let mut world = world();
        world.insert_resource(crate::combat::PendingDeathReconciliations::default());
        let corpse = reference(&mut world, "Skyrim.esm", 0x300);
        world.insert(
            corpse,
            Inventory {
                items: vec![ItemStack::new(0xAAAA, 1)],
            },
        );
        apply_starts_dead(&mut world, corpse);
        assert!(world.get::<Dead>(corpse).is_some());
        assert!(crate::inventory::is_loot_source(&world, corpse));
        assert_eq!(
            world
                .resource::<crate::combat::PendingDeathReconciliations>()
                .queued(),
            &[corpse]
        );

        let restored = reference(&mut world, "Skyrim.esm", 0x301);
        world.insert(restored, Dead);
        apply_starts_dead(&mut world, restored);
        assert_eq!(
            world
                .resource::<crate::combat::PendingDeathReconciliations>()
                .queued(),
            &[corpse],
            "a corpse restore already reconciled is left alone"
        );
    }

    /// #5267 (GAME-D4-2026-10-05-01) — a restored corpse's death teardown is
    /// QUEUED for the Late sink, not run at the restore point. Restore runs
    /// at actor-job completion on an un-propagated skeleton (bone globals
    /// still identity), so a synchronous reconcile would seed the ragdoll
    /// from the world origin — the exact hazard #4814 queued
    /// `apply_starts_dead` to avoid on the sibling path.
    #[test]
    fn dead_restore_queues_the_teardown_for_the_late_sink() {
        use byroredux_core::animation::AnimationPlayer;

        let mut world = world();
        world.insert_resource(crate::combat::PendingDeathReconciliations::default());
        world.register::<AnimationPlayer>();
        world.register::<crate::ragdoll::RagdollActive>();

        // Park a dead row: evict a live actor marked Dead.
        let parked = reference(&mut world, "Skyrim.esm", 0x310);
        world.insert(parked, Inventory::new());
        world.insert(parked, Dead);
        evict(&mut world, &[parked]);

        // Respawn + restore at the actor-job completion point, with an
        // AnimationPlayer to observe the (deferred) teardown against.
        let corpse = reference(&mut world, "Skyrim.esm", 0x310);
        world.insert(corpse, Inventory::new());
        world.insert(corpse, AnimationPlayer::new(0));
        assert!(restore(&mut world, corpse));
        assert!(world.get::<Dead>(corpse).is_some());
        assert_eq!(
            world
                .resource::<crate::combat::PendingDeathReconciliations>()
                .queued(),
            &[corpse],
            "the death teardown must be queued, not run synchronously"
        );
        assert!(
            world.get::<AnimationPlayer>(corpse).is_some(),
            "the animation strip must not run before PostUpdate propagation"
        );
        assert!(world.get::<crate::ragdoll::RagdollActive>(corpse).is_none());

        // The Late drain performs the teardown.
        crate::combat::reconcile_pending_dead_actors_system(&world, 0.0);
        assert!(
            world.get::<AnimationPlayer>(corpse).is_none(),
            "the Late drain owns the death teardown"
        );
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

    /// #5054 — a reference resident at save time (the exterior hysteresis
    /// ring) that the load does not respawn must not lose its saved state:
    /// its snapshot rows are parked the way an eviction would park them,
    /// item instances move inline out of the restored pool, and the next
    /// respawn restores them.
    #[test]
    fn unresolved_snapshot_rows_are_parked_for_the_next_respawn() {
        let mut src = world();
        src.insert_resource(byroredux_core::string::StringPool::new());
        let near = reference(&mut src, "Band.esm", 0x100);
        let band_corpse = reference(&mut src, "Band.esm", 0x200);
        let band_idle = reference(&mut src, "Band.esm", 0x300);
        src.insert(near, Inventory::new());
        let unique = src
            .resource_mut::<ItemInstancePool>()
            .allocate(ItemInstance::default());
        src.insert(
            band_corpse,
            Inventory {
                items: vec![
                    ItemStack::new(0xAAAA, 3),
                    ItemStack {
                        base_form_id: 0xBBBB,
                        count: 1,
                        instance: Some(unique),
                    },
                ],
            },
        );
        src.insert(band_corpse, Dead);
        src.insert(band_corpse, ActorValues::from_pairs([(0x2D4, 0.0)]));
        src.insert(band_idle, ActorValues::from_pairs([(0x2D4, 50.0)]));
        let registry = crate::save_io::build_save_registry();
        let snapshot = byroredux_save::save_world(&src, &registry).unwrap();
        let bytes = byroredux_save::encode(&snapshot, registry.schema_fingerprint()).unwrap();
        let decoded = byroredux_save::decode(&bytes, registry.schema_fingerprint()).unwrap();

        // The reload streams only the inner radius: `near` respawns, the
        // band references do not.
        let mut dst = world();
        byroredux_save::restore_resources(&mut dst, &registry, &decoded).unwrap();
        reference(&mut dst, "Band.esm", 0x100);
        let remap = byroredux_save::build_form_id_remap(&dst, &registry, &decoded);
        let unresolved = byroredux_save::unresolved_form_id_pairs(&registry, &decoded, &remap);
        assert_eq!(unresolved.len(), 2);
        assert_eq!(
            park_unresolved_snapshot_rows(&mut dst, &decoded, &unresolved),
            1,
            "only the corpse carries state eviction would park"
        );
        assert_eq!(
            dst.resource::<ItemInstancePool>().live_count(),
            0,
            "the parked row owns the instance payload inline"
        );

        let corpse = reference(&mut dst, "Band.esm", 0x200);
        assert!(restore(&mut dst, corpse));
        assert!(dst.get::<Dead>(corpse).is_some());
        let items = dst.get::<Inventory>(corpse).unwrap().items.clone();
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].base_form_id, items[0].count), (0xAAAA, 3));
        assert_eq!(items[1].base_form_id, 0xBBBB);
        assert!(dst
            .resource::<ItemInstancePool>()
            .get(items[1].instance.expect("the unique stack keeps an instance"))
            .is_some());
    }

    // ---- #6273 — drift guard: every parked component, through every pass.
    //
    // Adding a parked component edits the row struct, `capture`, `restore`,
    // `park_unresolved_snapshot_rows`, its keep rule and the completeness
    // table. The partition test proves the table; THESE prove that each pass
    // actually carries every field, so a field added to the struct and to one
    // pass but missed in another fails here instead of silently dropping that
    // state on a save load. The destructures below name every field with no
    // `..`: a new field does not compile until it is accounted for.

    /// Insert every component a [`ReferenceState`] / [`ScriptState`] row parks.
    fn register_parked_components(world: &mut World) {
        use byroredux_scripting::magic_effects::ActiveMagicEffects;
        use byroredux_scripting::obscript_state::{IgnoreCrime, WeaponDrawn};
        use byroredux_scripting::papyrus_demo::RumbleOnActivate;
        use byroredux_scripting::script_packages::ScriptPackage;
        use byroredux_scripting::{
            ActorControlState, Bleedout, ScriptTimer, ScriptVariables, SpellList, TwoStateActivator,
        };
        world.register::<byroredux_core::character::Perks>();
        world.register::<ActorControlState>();
        world.register::<Bleedout>();
        world.register::<ScriptPackage>();
        world.register::<ActiveMagicEffects>();
        world.register::<WeaponDrawn>();
        world.register::<IgnoreCrime>();
        world.register::<SpellList>();
        world.register::<TwoStateActivator>();
        world.register::<ScriptVariables>();
        world.register::<ScriptTimer>();
        world.register::<RumbleOnActivate>();
        world.register::<crate::inventory::PickedUp>();
        world.register::<EquipmentSlots>();
        world.register::<EquippedWeapon>();
        world.register::<ActorValues>();
        world.register::<Dead>();
        world.register::<byroredux_core::ecs::Transform>();
        world.insert_resource(byroredux_core::string::StringPool::new());
    }

    /// Give `actor` every component the actor row parks, each non-default.
    fn populate_actor(world: &mut World, actor: EntityId) {
        use byroredux_scripting::magic_effects::{ActiveEffect, ActiveMagicEffects};
        use byroredux_scripting::obscript_state::{IgnoreCrime, WeaponDrawn};
        use byroredux_scripting::script_packages::ScriptPackage;
        use byroredux_scripting::{ActorControlState, Bleedout, SpellList};
        world.insert(actor, Inventory { items: vec![ItemStack::new(0xAAAA, 2)] });
        let mut slots = EquipmentSlots::default();
        slots.occupants[3] = Some(InventoryIndex(0));
        world.insert(actor, slots);
        world.insert(
            actor,
            EquippedWeapon { inventory_index: InventoryIndex(0), base_form_id: 0xAAAA, damage: 7.0, reach: 1.0, speed: 1.0 },
        );
        world.insert(actor, ActorValues::from_pairs([(0x2D4, 12.0)]));
        world.insert(actor, SpellList(vec![0x10, 0xAB]));
        let mut perks = byroredux_core::character::Perks::default();
        perks.set_rank(0xBEEF, 2);
        world.insert(actor, perks);
        let mut control = ActorControlState::default();
        control.set_unconscious(true);
        world.insert(actor, control);
        world.insert(actor, Bleedout { remaining_secs: Some(5.0) });
        world.insert(actor, ScriptPackage { package: 0x0012_3456, constrained: true, running_secs: Some(4.0) });
        world.insert(actor, Dead);
        world.insert(
            actor,
            byroredux_core::ecs::Transform::new(
                byroredux_core::math::Vec3::new(5.0, 6.0, 7.0),
                byroredux_core::math::Quat::IDENTITY,
                1.0,
            ),
        );
        world.insert(
            actor,
            ActiveMagicEffects(vec![ActiveEffect {
                source: 1,
                mgef: 2,
                caster: 0,
                script: 0,
                duration: 30.0,
                indefinite: false,
                elapsed: 4.0,
                started: true,
                ending: false,
                value: None,
                ticks: 0,
                locals: Vec::new(),
                ref_locals: Vec::new(),
            }]),
        );
        world.insert(actor, WeaponDrawn);
        world.insert(actor, IgnoreCrime);
    }

    /// Give `object` every component the script row parks.
    fn populate_scripted_object(world: &mut World, object: EntityId) {
        use byroredux_scripting::papyrus_demo::{RumbleOnActivate, RumbleState};
        use byroredux_scripting::{ScriptTimer, ScriptVariables, TwoStateActivator};
        world.insert(
            object,
            TwoStateActivator { is_open: true, is_animating: false, do_once: true, activated_once: true },
        );
        let mut locals = ScriptVariables::default();
        locals.set_by_name("bDone", 1.0);
        world.insert(object, locals);
        world.insert(object, ScriptTimer { id: 7, remaining: 3.5 });
        world.insert(
            object,
            RumbleOnActivate {
                camera_intensity: 0.5,
                duration: 2.0,
                repeatable: false,
                shake_left: 0.1,
                shake_right: 0.2,
                state: RumbleState::Busy { wait_remaining_secs: 1.25 },
            },
        );
    }

    /// Every field of an actor row carries state. `picked_up` is the pickup
    /// tombstone: eviction captures it, a save-load park never sets it.
    fn assert_actor_row_is_complete(row: &ReferenceState, expect_picked_up: bool, via: &str) {
        let ReferenceState {
            inventory,
            equipment,
            weapon,
            actor_values,
            spells,
            perks,
            control,
            bleedout,
            script_package,
            dead,
            corpse_pose,
            picked_up,
            magic_effects,
            weapon_drawn,
            ignore_crime,
        } = row;
        assert!(inventory.as_ref().is_some_and(|items| items.len() == 1), "{via}: inventory");
        assert!(equipment.is_some(), "{via}: equipment");
        assert!(weapon.is_some(), "{via}: weapon");
        assert!(actor_values.is_some(), "{via}: actor_values");
        assert_eq!(spells.as_deref(), Some(&[0x10, 0xAB][..]), "{via}: spells");
        assert_eq!(perks.as_ref().map(|p| p.rank(0xBEEF)), Some(2), "{via}: perks");
        assert!(control.is_some_and(|c| c.unconscious), "{via}: control");
        assert!(bleedout.is_some(), "{via}: bleedout");
        assert!(script_package.is_some(), "{via}: script_package");
        assert!(*dead, "{via}: dead");
        assert!(corpse_pose.is_some(), "{via}: corpse_pose");
        assert_eq!(*picked_up, expect_picked_up, "{via}: picked_up");
        assert!(magic_effects.as_ref().is_some_and(|e| e.0.len() == 1), "{via}: magic_effects");
        assert!(*weapon_drawn, "{via}: weapon_drawn");
        assert!(*ignore_crime, "{via}: ignore_crime");
    }

    fn assert_script_row_is_complete(row: &ScriptState, via: &str) {
        let ScriptState { two_state, variables, timer, rumble } = row;
        assert!(two_state.is_some_and(|s| s.is_open), "{via}: two_state");
        assert!(variables.as_ref().is_some_and(|v| v.get_by_name("bDone") == Some(1.0)), "{via}: variables");
        assert!(timer.is_some_and(|t| t.id == 7), "{via}: timer");
        assert!(rumble.is_some(), "{via}: rumble");
    }

    /// Eviction: `capture` records every field, and `restore` puts every
    /// component back on a respawned reference.
    #[test]
    fn eviction_captures_and_restores_every_parked_component() {
        use byroredux_scripting::magic_effects::ActiveMagicEffects;
        use byroredux_scripting::obscript_state::{IgnoreCrime, WeaponDrawn};
        use byroredux_scripting::papyrus_demo::RumbleOnActivate;
        use byroredux_scripting::script_packages::ScriptPackage;
        use byroredux_scripting::{
            ActorControlState, Bleedout, ScriptTimer, ScriptVariables, SpellList, TwoStateActivator,
        };
        let mut world = world();
        register_parked_components(&mut world);
        let actor = reference(&mut world, "Band.esm", 0x700);
        populate_actor(&mut world, actor);
        world.insert(actor, crate::inventory::PickedUp);
        let object = reference(&mut world, "Band.esm", 0x701);
        populate_scripted_object(&mut world, object);
        evict(&mut world, &[actor, object]);

        {
            let store = world.resource::<PersistentReferenceStates>();
            let actor_pair = FormIdPair { plugin: PluginId::from_filename("Band.esm"), local: LocalFormId(0x700) };
            let object_pair = FormIdPair { plugin: PluginId::from_filename("Band.esm"), local: LocalFormId(0x701) };
            assert_actor_row_is_complete(store.rows.get(&actor_pair).expect("actor row"), true, "capture");
            assert_script_row_is_complete(store.scripts.get(&object_pair).expect("script row"), "capture");
        }

        // The respawn: a bare reference, then restore.
        let actor = reference(&mut world, "Band.esm", 0x700);
        world.insert(actor, Inventory::new());
        assert!(restore(&mut world, actor));
        assert!(world.get::<Inventory>(actor).is_some_and(|inv| inv.items.len() == 1), "inventory");
        assert!(world.has::<EquipmentSlots>(actor), "equipment");
        assert!(world.has::<EquippedWeapon>(actor), "weapon");
        assert!(world.has::<ActorValues>(actor), "actor_values");
        assert!(world.has::<SpellList>(actor), "spells");
        assert!(world.has::<byroredux_core::character::Perks>(actor), "perks");
        assert!(world.has::<ActorControlState>(actor), "control");
        assert!(world.has::<Bleedout>(actor), "bleedout");
        assert!(world.has::<ScriptPackage>(actor), "script_package");
        assert!(world.has::<Dead>(actor), "dead");
        assert!(world.has::<ActiveMagicEffects>(actor), "magic_effects");
        assert!(world.has::<WeaponDrawn>(actor), "weapon_drawn");
        assert!(world.has::<IgnoreCrime>(actor), "ignore_crime");
        let object = reference(&mut world, "Band.esm", 0x701);
        assert!(restore_script_state(&mut world, object));
        assert!(world.has::<TwoStateActivator>(object), "two_state");
        assert!(world.has::<ScriptVariables>(object), "variables");
        assert!(world.has::<ScriptTimer>(object), "timer");
        assert!(world.has::<RumbleOnActivate>(object), "rumble");
    }

    /// The save-reload ring: `park_unresolved_snapshot_rows` builds, from the
    /// snapshot's own columns, the same complete row eviction would have.
    #[test]
    fn save_reload_parking_records_every_parked_component() {
        let mut src = world();
        register_parked_components(&mut src);
        let actor = reference(&mut src, "Band.esm", 0x710);
        populate_actor(&mut src, actor);
        let object = reference(&mut src, "Band.esm", 0x711);
        populate_scripted_object(&mut src, object);
        let registry = crate::save_io::build_save_registry();
        let snapshot = byroredux_save::save_world(&src, &registry).unwrap();
        let bytes = byroredux_save::encode(&snapshot, registry.schema_fingerprint()).unwrap();
        let decoded = byroredux_save::decode(&bytes, registry.schema_fingerprint()).unwrap();

        let mut dst = world();
        register_parked_components(&mut dst);
        byroredux_save::restore_resources(&mut dst, &registry, &decoded).unwrap();
        let remap = byroredux_save::build_form_id_remap(&dst, &registry, &decoded);
        let unresolved = byroredux_save::unresolved_form_id_pairs(&registry, &decoded, &remap);
        assert_eq!(unresolved.len(), 2);
        assert_eq!(park_unresolved_snapshot_rows(&mut dst, &decoded, &unresolved), 2);

        let store = dst.resource::<PersistentReferenceStates>();
        let actor_pair = FormIdPair { plugin: PluginId::from_filename("Band.esm"), local: LocalFormId(0x710) };
        let object_pair = FormIdPair { plugin: PluginId::from_filename("Band.esm"), local: LocalFormId(0x711) };
        assert_actor_row_is_complete(store.rows.get(&actor_pair).expect("actor row"), false, "park");
        assert_script_row_is_complete(store.scripts.get(&object_pair).expect("script row"), "park");
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
        // #6132 — the packed-Havok proxy is a CHILD of the placement root.
        let proxy = world.spawn();
        world.insert(proxy, byroredux_core::ecs::components::PhysicsSourceForm(root_form));
        world.insert(proxy, byroredux_core::ecs::Parent(respawned));
        crate::helpers::add_child(&mut world, respawned, proxy);
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
        // #6132 — the parented proxy went too, and left no dangling `Children`
        // entry behind: the placement survives, so a save would be refused.
        assert!(world.get::<byroredux_core::ecs::Parent>(proxy).is_none());
        assert!(
            world
                .get::<byroredux_core::ecs::Children>(respawned)
                .is_none_or(|children| !children.0.contains(&proxy)),
            "the swept proxy must be detached from the surviving placement root"
        );
        assert!(
            byroredux_save::validate::validate_world(&world).is_empty(),
            "a save after the sweep must not be refused: {:?}",
            byroredux_save::validate::validate_world(&world)
        );
        // The row was consumed: a second pass finds nothing to do.
        assert_eq!(restore_resident(&mut world), 0);
        assert!(world.resource::<PersistentReferenceStates>().rows.is_empty());
    }
}
