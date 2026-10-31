//! UI-driven inventory mutation: snapshot, consume / equip actions, and the equipped-weapon reconcilers.

use super::*;

/// Build the presentation snapshot only while the native inventory is visible.
pub(crate) fn snapshot(world: &World) -> Option<byroredux_debug_ui::InventorySnapshot> {
    let player = PlayerEntity::get(world)?;
    // Clone each component before acquiring the next storage lock. The menu
    // is off the hot path, and this preserves the ECS invariant that callers
    // never hold independently-acquired component locks in an arbitrary order.
    let inventory = (*world.get::<Inventory>(player)?).clone();
    let equipment = world
        .get::<EquipmentSlots>(player)
        .map(|equipment| (*equipment).clone());
    let catalog = world.try_resource::<InventoryCatalog>();

    let mut items = Vec::with_capacity(inventory.items.len());
    for (raw_index, stack) in inventory.items.iter().enumerate() {
        if stack.count == 0 {
            continue;
        }
        let definition = catalog
            .as_ref()
            .and_then(|catalog| catalog.entries.get(&stack.base_form_id));
        let weight = definition.map_or(0.0, |definition| definition.weight);
        let index = raw_index as u32;
        items.push(byroredux_debug_ui::InventoryItemView {
            index,
            form_id: stack.base_form_id,
            name: definition.map_or_else(
                || format!("Item {:08X}", stack.base_form_id),
                |definition| definition.name.clone(),
            ),
            category: definition.map_or("Unknown", |definition| definition.category),
            details: definition.map_or_else(String::new, |definition| definition.details.clone()),
            count: stack.count,
            value: definition.map_or(0, |definition| definition.value),
            weight,
            // #3112 — `is_equipped`, not a bare `occupants` scan: the
            // wielded weapon lives outside the biped array and would
            // otherwise render as unequipped in the menu.
            equipped: equipment
                .as_ref()
                .is_some_and(|slots| slots.is_equipped(InventoryIndex(index))),
            equippable: definition.is_some_and(|definition| definition.equip_target.is_some()),
            consumable: catalog.as_ref().is_some_and(|catalog| {
                catalog.restorations.contains_key(&stack.base_form_id) || catalog.script_effects.contains_key(&stack.base_form_id)
            }),
        });
    }
    items.sort_by(|left, right| {
        left.category
            .cmp(right.category)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.index.cmp(&right.index))
    });

    Some(byroredux_debug_ui::InventorySnapshot {
        items,
        total_weight: catalog
            .as_ref()
            .map_or(0.0, |catalog| catalog.inventory_weight(&inventory)),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MutationResult {
    Consumed,
    /// One item left the inventory for the world (a created reference at the player).
    Dropped,
    Equipped,
    Unequipped,
    Unavailable,
}

/// Consume an item with a validated restoration plan. Preflight mutation and
/// leave a zero-count slot so equipment and UI indices never shift.
pub(super) fn consume_item(world: &mut World, index: u32, form_id: u32) -> MutationResult {
    use byroredux_core::ecs::components::{
        ActorValues, ActorVitals, Dead, TimedRestoration, TimedRestorations,
    };
    use byroredux_core::ecs::resources::ItemInstancePool;
    let Some(player) = PlayerEntity::get(world) else {
        return MutationResult::Unavailable;
    };
    if world.get::<Dead>(player).is_some() {
        return MutationResult::Unavailable;
    }
    let Some(stack) = world
        .get::<Inventory>(player)
        .and_then(|r| r.items.get(index as usize).copied())
        .filter(|s| s.base_form_id == form_id && s.count > 0)
    else {
        return MutationResult::Unavailable;
    };
    if world
        .get::<EquipmentSlots>(player)
        .is_some_and(|r| r.is_equipped(InventoryIndex(index)))
    {
        return MutationResult::Unavailable;
    }
    let Some((effects, scripted, name)) = world.try_resource::<InventoryCatalog>().and_then(|r| {
        let effects = r.restorations.get(&form_id).cloned();
        let scripted = r.script_effects.get(&form_id).cloned().unwrap_or_default();
        if effects.is_none() && scripted.is_empty() {
            return None;
        }
        Some((effects.unwrap_or_default(), scripted, r.entries.get(&form_id)?.name.clone()))
    }) else {
        return MutationResult::Unavailable;
    };
    // Drop the catalog guard before the evaluator acquires component locks.
    // Re-evaluate at use time: acquiring a perk must not require a cell reload.
    let mut effects = effects;
    let mut context = byroredux_scripting::condition::ConditionContext::for_subject(player);
    context.target = Some(player); // ingestible target and caster are the user
    effects.retain(|branch| {
        byroredux_scripting::condition::evaluate_modeled(&branch.conditions, world, &context)
    });
    let Some(health) = world.get::<ActorVitals>(player).map(|r| r.health) else {
        return MutationResult::Unavailable;
    };
    let Some(values) = world.get::<ActorValues>(player) else {
        return MutationResult::Unavailable;
    };
    // Scale against the live composed Medicine value at use time. Store the
    // resolved rate in timed state so later skill changes do not rewrite an
    // already-consumed dose. All calculations precede inventory/AV mutation.
    for effect in &mut effects {
        if let Some(scale) = effect.medicine {
            let Some(skill) = values.get(scale.actor_value).map(|v| v.current()) else {
                return MutationResult::Unavailable;
            };
            if !skill.is_finite() {
                return MutationResult::Unavailable;
            }
            effect.magnitude *= scale.base + scale.multiplier * skill.clamp(0.0, 100.0) / 100.0;
        }
    }
    if !values.current(health).is_finite()
        || values.current(health) <= 0.0
        || (effects.is_empty() && scripted.is_empty())
        || effects.iter().any(|effect| {
            !effect.magnitude.is_finite()
                || effect.magnitude <= 0.0
                || values
                    .get(effect.actor_value)
                    .is_none_or(|v| !v.current().is_finite() || !v.damage.is_finite())
        })
    {
        return MutationResult::Unavailable;
    }
    drop(values);
    if stack.instance.is_some_and(|instance| {
        world
            .try_resource::<ItemInstancePool>()
            .is_none_or(|pool| pool.get(instance).is_none())
    }) {
        return MutationResult::Unavailable;
    }
    // Exclusive World access prevents the preflighted components changing.
    {
        let values = world.get_mut::<ActorValues>(player).unwrap();
        for effect in &effects {
            if effect.duration == 0 {
                values.restore(effect.actor_value, effect.magnitude);
            }
        }
    }
    let timed: Vec<_> = effects
        .iter()
        .filter(|e| e.duration > 0)
        .map(|e| TimedRestoration {
            source_form_id: form_id,
            actor_value: e.actor_value,
            per_second: e.magnitude,
            remaining: f64::from(e.duration),
        })
        .collect();
    if !timed.is_empty() {
        if let Some(active) = world.get_mut::<TimedRestorations>(player) {
            active.effects.extend(timed);
        } else {
            world.insert(player, TimedRestorations { effects: timed });
        }
    }
    // The scripted (and, where no plan covers it, value) effects the caster runtime keeps running.
    if !scripted.is_empty() {
        byroredux_scripting::magic_effects::start_effects(world, player, Some(player), form_id, &scripted, false);
    }
    let released = {
        let inventory = world.get_mut::<Inventory>(player).unwrap();
        let stack = &mut inventory.items[index as usize];
        stack.count -= 1;
        if stack.count == 0 {
            stack.instance.take()
        } else {
            None
        }
    };
    if let Some(instance) = released {
        world.resource_mut::<ItemInstancePool>().release(instance);
    }
    crate::notifications::push(world, format!("Used {name}"));
    // New Vegas challenges that count items used (`Use an Item`, `Use an Item from a List`).
    byroredux_scripting::challenges::note(world, byroredux_scripting::challenges::ChallengeEvent::ItemUsed { item: form_id });
    MutationResult::Consumed
}

/// Drop one item of an inventory row into the world: it leaves the inventory (the row keeps its
/// position, like a use) and a created reference of the same base stands where the player does, which
/// the ordinary placement spawns and the pickup path can take back. Refused for an equipped row, a
/// dead player, an item with an unresolvable instance, or a base the pickup path would not take (a
/// container, an uncatalogued form) — a dropped item must be one the player can pick up again.
///
/// The Story Manager's `REMP` (Player Remove Item) needs the dropped reference as `R2`; the entity
/// exists only once the placement spawns it, so the drop is queued on [`PendingDroppedItems`] and
/// `dropped_item_story_event_system` raises the event then.
pub(super) fn drop_item(world: &mut World, index: u32, form_id: u32) -> MutationResult {
    use byroredux_core::ecs::resources::ItemInstancePool;
    if !crate::systems::player_can_act(world) {
        return MutationResult::Unavailable;
    }
    let Some(player) = PlayerEntity::get(world) else {
        return MutationResult::Unavailable;
    };
    let Some(stack) = world
        .get::<Inventory>(player)
        .and_then(|inventory| inventory.items.get(index as usize).copied())
        .filter(|stack| stack.base_form_id == form_id && stack.count > 0)
    else {
        return MutationResult::Unavailable;
    };
    if world
        .get::<EquipmentSlots>(player)
        .is_some_and(|slots| slots.is_equipped(InventoryIndex(index)))
        || !world
            .try_resource::<InventoryCatalog>()
            .is_some_and(|catalog| catalog.is_pickup_base(form_id))
        || stack.instance.is_some_and(|instance| {
            world
                .try_resource::<ItemInstancePool>()
                .is_none_or(|pool| pool.get(instance).is_none())
        })
    {
        return MutationResult::Unavailable;
    }
    // Where it lands: the player's own spot, in the cell the player occupies.
    let Some(placement) = player_drop_placement(world, player) else {
        return MutationResult::Unavailable;
    };
    let Some(created) = world.try_resource::<byroredux_scripting::CreatedReferences>().map(|created| created.scripted_ids()) else {
        return MutationResult::Unavailable;
    };
    let Some(reference) = created.next() else {
        return MutationResult::Unavailable;
    };
    let released = {
        let inventory = world.get_mut::<Inventory>(player).unwrap();
        let row = &mut inventory.items[index as usize];
        row.count -= 1;
        if row.count == 0 {
            row.instance.take()
        } else {
            None
        }
    };
    if let Some(instance) = released {
        world.resource_mut::<ItemInstancePool>().release(instance);
    }
    {
        let mut created = world.resource_mut::<byroredux_scripting::CreatedReferences>();
        created.create_scripted(reference, form_id, byroredux_scripting::PLAYER_REFERENCE, false);
        created.place(reference, placement);
    }
    byroredux_scripting::emit_item_transfers(
        world,
        player,
        [byroredux_scripting::ItemTransfer { item_form_id: form_id, count: 1, added: false, stolen: false }],
    );
    if let Some(mut pending) = world.try_resource_mut::<PendingDroppedItems>() {
        pending.0.push((reference, 0.0));
    }
    let name = world
        .try_resource::<InventoryCatalog>()
        .and_then(|catalog| catalog.entries.get(&form_id).map(|entry| entry.name.clone()))
        .unwrap_or_else(|| format!("Item {form_id:08X}"));
    crate::notifications::push(world, format!("Dropped {name}"));
    MutationResult::Dropped
}

/// The player's own position as a created reference's placement (Z-up world units).
fn player_drop_placement(world: &World, player: byroredux_core::ecs::EntityId) -> Option<byroredux_scripting::CreatedPlacement> {
    let position = byroredux_core::math::coord::yup_to_zup_pos(
        world.get::<byroredux_core::ecs::components::GlobalTransform>(player)?.translation.to_array(),
    );
    crate::cell_loader::created::placement_at_position(world, position)
}

/// Dropped items whose reference has not spawned yet, with the seconds they have waited: the
/// Story Manager's `REMP` is raised when the entity exists. `NOT_SAVED_BY_DESIGN`: the created
/// reference itself is saved; only the not-yet-raised event is dropped by a load.
#[derive(Default)]
pub(crate) struct PendingDroppedItems(pub(crate) Vec<(u32, f32)>);

impl byroredux_core::ecs::Resource for PendingDroppedItems {}

/// How long a drop waits for its placement to spawn before its `REMP` is abandoned (the cell may
/// have unloaded around it).
const DROP_EVENT_WAIT_SECS: f32 = 10.0;

/// System: raise `REMP` for each dropped item whose reference has spawned.
pub(crate) fn dropped_item_story_event_system(world: &World, dt: f32) {
    let Some(mut pending) = world.try_resource_mut::<PendingDroppedItems>() else {
        return;
    };
    if pending.0.is_empty() {
        return;
    }
    let spawned = world
        .try_resource::<crate::cell_loader::created::CreatedReferenceEntities>();
    let mut ready = Vec::new();
    pending.0.retain_mut(|(reference, waited)| {
        if let Some(entity) = spawned.as_ref().and_then(|spawned| spawned.0.get(reference).copied()) {
            ready.push(entity);
            return false;
        }
        *waited += dt;
        *waited < DROP_EVENT_WAIT_SECS
    });
    drop(spawned);
    drop(pending);
    if let Some(player) = PlayerEntity::get(world) {
        for item in ready {
            crate::systems::story_events::raise_remove_from_player_story_event(world, player, item);
        }
    }
}

/// Apply a native-menu action to the canonical player components.
///
/// #4707 — a refusal is told to the player, not only logged: every
/// `Unavailable` pushes one "Can't … now" notification, the counterpart
/// of the "Used {name}" line a successful use pushes.
pub(crate) fn apply_action(
    world: &mut World,
    action: byroredux_debug_ui::InventoryAction,
) -> MutationResult {
    use byroredux_debug_ui::InventoryAction;
    let result = match action {
        InventoryAction::Consume { index, form_id } => consume_item(world, index, form_id),
        InventoryAction::ToggleEquip { index } => toggle_equip(world, index),
        InventoryAction::Drop { index, form_id } => drop_item(world, index, form_id),
    };
    if result == MutationResult::Unavailable {
        notify_refused_action(world, action);
    }
    result
}

/// The player-visible half of a refused menu action (#4707). The item is
/// named from the catalog; the row's base form stands in when the catalog
/// has no entry, the same fallback [`pickup_loot`] uses.
pub(super) fn notify_refused_action(world: &World, action: byroredux_debug_ui::InventoryAction) {
    use byroredux_debug_ui::InventoryAction;
    let (verb, form_id) = match action {
        InventoryAction::Consume { form_id, .. } => ("use", Some(form_id)),
        InventoryAction::Drop { form_id, .. } => ("drop", Some(form_id)),
        InventoryAction::ToggleEquip { index } => (
            "equip",
            PlayerEntity::get(world)
                .and_then(|player| {
                    world
                        .get::<Inventory>(player)?
                        .get(InventoryIndex(index))
                        .map(|stack| stack.base_form_id)
                }),
        ),
    };
    let name = form_id.map(|form_id| {
        world
            .try_resource::<InventoryCatalog>()
            .and_then(|catalog| {
                catalog
                    .entries
                    .get(&form_id)
                    .map(|entry| entry.name.clone())
            })
            .unwrap_or_else(|| format!("Item {form_id:08X}"))
    });
    crate::notifications::push(
        world,
        format!(
            "Can't {verb} {} now",
            name.as_deref().unwrap_or("that item")
        ),
    );
}

/// Equip or unequip one player inventory row through the catalog's
/// recorded destination.
pub(super) fn toggle_equip(world: &mut World, index: u32) -> MutationResult {
    // #4701 — `consume_item` carries its own `Dead` refusal; equip is the
    // other half of the menu and refuses the same way.
    if !crate::systems::player_can_act(world) {
        return MutationResult::Unavailable;
    }
    let Some(player) = PlayerEntity::get(world)
    else {
        return MutationResult::Unavailable;
    };
    let inventory_index = InventoryIndex(index);
    let Some(inventory_form_ids) = world.get::<Inventory>(player).map(|inventory| {
        inventory
            .items
            .iter()
            .map(|stack| stack.base_form_id)
            .collect::<Vec<_>>()
    }) else {
        return MutationResult::Unavailable;
    };
    let Some(form_id) = world
        .get::<Inventory>(player)
        .and_then(|inventory| inventory.get(inventory_index).copied())
        .filter(|stack| stack.count > 0)
        .map(|stack| stack.base_form_id)
    else {
        return MutationResult::Unavailable;
    };
    let Some(equip_target) = world
        .try_resource::<InventoryCatalog>()
        .and_then(|catalog| {
            catalog
                .entries
                .get(&form_id)
                .and_then(|item| item.equip_target)
        })
    else {
        return MutationResult::Unavailable;
    };
    let Some(mut equipment_query) = world.query_mut::<EquipmentSlots>() else {
        return MutationResult::Unavailable;
    };
    let Some(equipment) = equipment_query.get_mut(player) else {
        return MutationResult::Unavailable;
    };

    // #3112 — release/equip through the whole-entry helpers so a toggle
    // reaches both destinations, and route the equip through the variant the
    // catalog recorded rather than a slot mask that could address either.
    let mut changes = Vec::new();
    let mutation = if equipment.release(inventory_index) {
        changes.push(byroredux_scripting::EquipmentChange {
            item_form_id: form_id,
            equipped: false,
        });
        log::info!("inventory: player unequipped {form_id:08X}");
        MutationResult::Unequipped
    } else {
        let displaced = match equip_target {
            EquipTarget::BipedSlots(mask) => equipment.equip(mask, inventory_index),
            EquipTarget::Weapon => equipment
                .equip_weapon(inventory_index)
                .into_iter()
                .collect(),
        };
        for displaced in displaced {
            if !equipment.is_equipped(displaced) {
                if let Some(item_form_id) = inventory_form_ids.get(displaced.0 as usize).copied() {
                    changes.push(byroredux_scripting::EquipmentChange {
                        item_form_id,
                        equipped: false,
                    });
                }
            }
        }
        changes.push(byroredux_scripting::EquipmentChange {
            item_form_id: form_id,
            equipped: true,
        });
        log::info!("inventory: player equipped {form_id:08X}");
        MutationResult::Equipped
    };
    // Only a weapon toggle can change what is wielded — an armor toggle
    // must leave `EquippedWeapon` alone. Pre-#3112 this re-derived from
    // `occupants[31]` after *every* toggle, so equipping an armor whose
    // BOD2 set bit 31 silently unequipped the player's weapon.
    let weapon_changed = equip_target == EquipTarget::Weapon;
    let equipped_weapon = equipment.weapon;
    drop(equipment_query);
    if weapon_changed {
        reconcile_equipped_weapon(world, player, equipped_weapon);
    }
    byroredux_scripting::emit_equipment_changes(world, player, changes);
    mutation
}

/// Rebuild the runtime [`EquippedWeapon`] consequence of the persisted
/// `EquipmentSlots.weapon` + `Inventory` facts, after a live load has
/// overlaid them.
///
/// The save overlay is **additive-only** (`byroredux_save::apply_deltas`):
/// it can update or insert a component row, never remove one. Runtime
/// removals that are consequences of a persisted fact must be rebuilt by
/// the binary afterwards — `reconcile_dead_actor_runtime_state` is the model
/// this follows (#3022).
///
/// `EquippedWeapon` is removed at runtime by [`reconcile_equipped_weapon`]'s
/// `else` arm whenever the player unequips their weapon through the pause
/// menu. Without this call the removal could not survive a load: the player
/// body is spawned in `scene::setup_scene` with no `CellRoot`, so
/// `unload_cell_inner` never collects it and it keeps every component the
/// *current* session gave it. Saving while unarmed, equipping a weapon and
/// quickloading left the player still holding it — with the restored
/// `EquipmentSlots.weapon == None` and a freshly-overlaid `Inventory`
/// contradicting the surviving `EquippedWeapon`, whose `inventory_index`
/// pointed into an inventory that had just been wholesale replaced. Melee
/// then dealt the live session's weapon damage rather than the saved
/// unarmed damage, with no log line anywhere (#3488 / SAVE-D1-2026-08-27-01).
///
/// Re-deriving is the whole point: the same function that owns the runtime
/// transition owns the reload, so the two can't drift.
pub(crate) fn reconcile_player_equipped_weapon(
    world: &mut World,
    player: byroredux_core::ecs::EntityId,
) {
    let weapon_slot = world
        .get::<EquipmentSlots>(player)
        .and_then(|equipment| equipment.weapon);
    reconcile_equipped_weapon(world, player, weapon_slot);
}

/// #5058 — post-load faction reset for the process-lifetime player. The
/// wholesale `restore_resources` reinstalls the alias-injection ledger with
/// `factions` empty (its EntityId keys cannot survive a save), and the
/// player — unlike an NPC — is not respawned, so the pre-load session's
/// alias-injected memberships are still in its `FactionRanks`. The next
/// alias refresh then reads each stale rank as an authored membership
/// (`original_rank = Some`), and the release branch only strips
/// `original_rank: None` — the injected faction would outlive every later
/// alias release in the process and feed `faction_hostility_system`,
/// `GetInFaction` and dialogue conditions the loaded save does not have.
///
/// Resetting to the authored record answer (the same Use-Factions terminal
/// read the boot seed uses) gives the player the posture a respawned NPC
/// already gets: record ranks, with the loaded save's running quests
/// re-injecting their aliases on the next refresh. Scripted rank edits are
/// lost exactly as they are for NPCs — `FactionRanks` is re-derived, not
/// saved (SAVE-D2 classification).
pub(crate) fn reset_player_factions_to_record(
    world: &mut World,
    player: byroredux_core::ecs::EntityId,
) {
    let Some(index_resource) = world.try_resource::<crate::cell_loader::LoadedCellIndex>() else {
        return;
    };
    let index = index_resource.0.clone();
    drop(index_resource);
    let player_form_id = player_npc_form_id(index.game);
    let Some(npc) = index.npcs.get(&player_form_id) else {
        return;
    };
    let resolved = byroredux_plugin::equip::ResolvedNpc::resolve(npc, &index);
    match crate::npc_spawn::faction_ranks_of(&resolved) {
        Some(ranks) => {
            world.insert(player, ranks);
        }
        None => {
            // The record seeds no membership: any live FactionRanks on the
            // player is session residue (stale alias injects included).
            let _ = world.remove::<byroredux_core::ecs::components::FactionRanks>(player);
        }
    }
}

pub(super) fn reconcile_equipped_weapon(
    world: &mut World,
    player: byroredux_core::ecs::EntityId,
    inventory_index: Option<InventoryIndex>,
) {
    let candidate = inventory_index.and_then(|inventory_index| {
        let stack = world
            .get::<Inventory>(player)
            .and_then(|inventory| inventory.get(inventory_index).copied())
            .filter(|stack| stack.count > 0)?;
        let form_id = stack.base_form_id;
        // M43 — an improved weapon (Skyrim tempering) hits harder by its quality's rating bonus.
        let quality_bonus = stack
            .instance
            .and_then(|instance| world.try_resource::<byroredux_core::ecs::resources::ItemInstancePool>()?.get(instance).map(|inst| inst.quality))
            .map_or(0.0, crate::smithing::weapon_bonus);
        let (damage, reach, speed) =
            world
                .try_resource::<InventoryCatalog>()
                .and_then(|catalog| {
                    let item = catalog.entries.get(&form_id)?;
                    Some((item.weapon_damage?, item.weapon_reach, item.weapon_speed))
                })?;
        Some(EquippedWeapon {
            inventory_index,
            base_form_id: form_id,
            damage: damage + quality_bonus,
            reach,
            speed,
        })
    });
    if let Some(weapon) = candidate {
        world.insert(player, weapon);
    } else {
        let _ = world.remove::<EquippedWeapon>(player);
    }
}
