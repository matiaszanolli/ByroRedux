//! Loot, ownership and pickup: loot-source classification, cell ownership stamps, theft owners, container transfer, placed-item pickup.

use super::*;

pub(crate) fn is_loot_source(world: &World, entity: byroredux_core::ecs::EntityId) -> bool {
    if PlayerEntity::get(world)
        == Some(entity)
    {
        return false;
    }
    if world
        .get::<byroredux_core::ecs::components::Dead>(entity)
        .is_some()
    {
        return true;
    }
    let Some(base) = world
        .get::<byroredux_scripting::SceneAliasCandidate>(entity)
        .map(|identity| identity.base_form_id)
    else {
        return false;
    };
    world
        .try_resource::<InventoryCatalog>()
        .is_some_and(|catalog| catalog.containers.contains(&base))
}

/// #4699 — CELL-level ownership (`XOWN`/`XRNK`/`XGLB` on the CELL) as the
/// fallback for placements that author none of their own. A REFR's own
/// `XOWN` overrides its cell's (#692, stamped in `synth_child`), so only
/// roots still without [`Owned`] take the cell's. Scoped to what the theft
/// rule reads — containers and loose items (catalogued bases) — so actors
/// and statics in an owned interior carry nothing. Called once per loaded
/// entity range, beside the `CellRoot` stamp for the same range.
///
/// [`Owned`]: byroredux_core::ecs::components::Owned
pub(crate) fn stamp_cell_ownership(
    world: &mut World,
    ownership: Option<&byroredux_plugin::esm::cell::CellOwnership>,
    first: byroredux_core::ecs::EntityId,
    last: byroredux_core::ecs::EntityId,
) {
    use byroredux_core::ecs::components::Owned;
    let Some(ownership) = ownership else {
        return;
    };
    let targets: Vec<_> = {
        let Some(catalog) = world.try_resource::<InventoryCatalog>() else {
            return;
        };
        let Some(identities) = world.query::<byroredux_scripting::SceneAliasCandidate>() else {
            return;
        };
        let owned = world.query::<Owned>();
        (first..last)
            .filter(|&entity| {
                identities.get(entity).is_some_and(|identity| {
                    catalog.containers.contains(&identity.base_form_id)
                        || catalog.is_pickup_base(identity.base_form_id)
                }) && owned
                    .as_ref()
                    .is_none_or(|owned| owned.get(entity).is_none())
            })
            .collect()
    };
    let owned = Owned {
        owner_form_id: ownership.owner_form_id,
        faction_rank: ownership.faction_rank,
        global_var_form_id: ownership.global_var_form_id,
    };
    world.insert_batch(targets.into_iter().map(|entity| (entity, owned)));
}

/// P3's theft rule: a transfer is theft when the source is owned and the
/// owner is neither the player nor a faction the player holds the required
/// rank in. The answer is the owner it steals from; what the theft costs the
/// player is [`crate::crime::report_theft`]'s.
///
/// #4699 — "the player" is the Player `NPC_` base (0x7), what vanilla
/// authors in `XOWN` on player-owned containers; the 0x14 PlayerRef
/// sentinel this used to compare against appears in no master's `XOWN`,
/// so every player-owned container read as stolen. The faction exemption
/// honours the `XRNK` rank bar when one is authored (any rank otherwise),
/// against the player's own seeded [`FactionRanks`]. Cell-level ownership
/// reaches `Owned` through [`stamp_cell_ownership`].
///
/// [`FactionRanks`]: byroredux_core::ecs::components::FactionRanks
pub(super) fn theft_owner(
    world: &World,
    player: byroredux_core::ecs::EntityId,
    source: byroredux_core::ecs::EntityId,
) -> Option<u32> {
    // #5866 — one ownership read: a script's `SetActorOwner` / `SetFactionOwner`
    // (a cleared owner is `Some(0)`) overrides the `XOWN` the cell loader
    // stamped, for the theft rule exactly as for `GetActorOwner`.
    let owned = match byroredux_scripting::owner_override(world, source) {
        Some(0) => return None,
        Some(owner) => byroredux_core::ecs::components::Owned {
            owner_form_id: owner,
            faction_rank: None,
            global_var_form_id: None,
        },
        None => world
            .get::<byroredux_core::ecs::components::Owned>(source)
            .map(|owned| *owned)?,
    };
    let player_base = world
        .try_resource::<InventoryCatalog>()
        .map(|catalog| catalog.player_base_form_id);
    if player_base == Some(owned.owner_form_id) {
        return None;
    }
    let member = world
        .get::<byroredux_core::ecs::components::FactionRanks>(player)
        .and_then(|ranks| ranks.rank(owned.owner_form_id))
        .is_some_and(|rank| {
            owned
                .faction_rank
                .is_none_or(|required| i32::from(rank) >= required)
        });
    (!member).then_some(owned.owner_form_id)
}

/// Which stacks a loot transfer moves out of a source inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LootSelection {
    /// Every stack, then reset the source's equipment (take-all).
    All,
    /// One stack row by inventory index. The source row is zeroed in place
    /// (the `consume_item` convention) so the source's remaining equipment
    /// indices never shift; an instance handle travels with the moved row.
    /// #4464 — constructed nowhere yet: the P3 spec explicitly defers
    /// selective transfer ("a basic take-all action, not a container
    /// browser"), so this arm waits for that UI. `transfer_loot` implements
    /// and tests it end to end; only the constructor is missing.
    #[allow(dead_code)] // #4464 — forward-latent until the selective-take UI
    Stack(InventoryIndex),
}

/// What one validated transfer moved — drives the notification wording and
/// the `ItemEventBatch` rows both sides publish.
pub(crate) struct LootOutcome {
    pub item_count: u64,
    /// One `(base_form_id, count)` row per moved stack.
    pub rows: Vec<(u32, u32)>,
    /// #4464 — read nowhere yet: with take-all wired straight to activation
    /// no caller branches on theft; the witness/bounty consumer the P3 spec
    /// defers is what will read it.
    #[allow(dead_code)] // #4464 — forward-latent until theft consequences
    pub stolen: bool,
}

/// Move loot from `source` into the player's inventory. Shared by the
/// activation take-all path (`container_loot_system`) and the console smoke
/// path. All of the old take-all invariants are preserved: validation
/// before any mutation, whole-stack appends that keep the player's
/// equipment indices stable, instance handles never reallocated, and one
/// unequip event per source equipment index the transfer cleared.
///
/// Returns `None` (having moved nothing) when any validation fails: source
/// is the player, is not a loot source, is locked, or either side lacks an
/// `Inventory`.
pub(crate) fn transfer_loot(
    world: &World,
    player: byroredux_core::ecs::EntityId,
    source: byroredux_core::ecs::EntityId,
    selection: LootSelection,
) -> Option<LootOutcome> {
    if player == source
        || !is_loot_source(world, source)
        || world
            .get::<byroredux_core::ecs::components::Locked>(source)
            .is_some()
    {
        return None;
    }
    let stolen_from = theft_owner(world, player, source);
    let stolen = stolen_from.is_some();
    let mut outcome = LootOutcome {
        item_count: 0,
        rows: Vec::new(),
        stolen,
    };
    // Indices (not copies) of source rows this transfer emptied, each with
    // its base form id, so source equipment pointing at them can be released
    // after the storage guard drops. `All` clears every equipped index;
    // `Stack` at most one.
    let mut cleared_source_indices: Vec<(InventoryIndex, u32)> = Vec::new();
    let mut source_form_ids: Option<Vec<u32>> = None;
    {
        let mut inventories = world.query_mut::<Inventory>()?;
        // One storage write guard covers both inventories; validate the
        // destination before taking anything so a missing player loses no loot.
        inventories.get_mut(player)?;
        let source_inventory = inventories.get_mut(source)?;
        match selection {
            LootSelection::All => {
                let stacks = std::mem::take(&mut source_inventory.items);
                outcome.item_count = stacks.iter().map(|stack| u64::from(stack.count)).sum();
                outcome.rows = stacks
                    .iter()
                    .map(|stack| (stack.base_form_id, stack.count))
                    .collect();
                source_form_ids = Some(
                    stacks.iter().map(|stack| stack.base_form_id).collect(),
                );
                let destination = inventories
                    .get_mut(player)
                    .expect("validated under the same guard");
                // Existing equipment indices stay stable and distinct
                // instance-pool identities are never merged or discarded;
                // plain stacks fold into the matching row (#5871).
                for stack in stacks {
                    destination.add(stack);
                }
            }
            LootSelection::Stack(index) => {
                let stack = source_inventory
                    .items
                    .get(index.0 as usize)
                    .copied()
                    .filter(|stack| stack.count > 0)?;
                outcome.item_count = u64::from(stack.count);
                outcome.rows.push((stack.base_form_id, stack.count));
                // Zero in place, `consume_item`-style: the row keeps its
                // position (source equipment indices above it stay valid)
                // and the instance handle travels with the moved row.
                source_inventory.items[index.0 as usize] =
                    ItemStack::new(stack.base_form_id, 0);
                cleared_source_indices.push((index, stack.base_form_id));
                let destination = inventories
                    .get_mut(player)
                    .expect("validated under the same guard");
                destination.add(stack);
            }
        }
    }
    if outcome.item_count == 0 {
        return Some(outcome);
    }
    // Both sides observe the transfer: the player gains (`added`), the
    // source loses. Scripts on either entity read the same rows.
    byroredux_scripting::emit_item_transfers(
        world,
        player,
        outcome
            .rows
            .iter()
            .map(|&(item_form_id, count)| byroredux_scripting::ItemTransfer {
                item_form_id,
                count,
                added: true,
                stolen,
            }),
    );
    byroredux_scripting::emit_item_transfers(
        world,
        source,
        outcome
            .rows
            .iter()
            .map(|&(item_form_id, count)| byroredux_scripting::ItemTransfer {
                item_form_id,
                count,
                added: false,
                stolen,
            }),
    );
    let noun = if outcome.item_count == 1 { "item" } else { "items" };
    crate::notifications::push(
        world,
        if stolen {
            format!("Stolen {} {noun}", outcome.item_count)
        } else {
            format!("Took {} {noun}", outcome.item_count)
        },
    );
    if let Some(owner) = stolen_from {
        crate::crime::report_theft(world, player, owner, &outcome.rows);
    }
    // Equipment points into the source inventory, not the destination.
    // Snapshot unique indices and release every storage guard before
    // acquiring the next one or publishing the script event batch.
    if matches!(selection, LootSelection::All) {
        let mut equipped = Vec::new();
        if let Some(mut equipment) = world.query_mut::<EquipmentSlots>() {
            if let Some(slots) = equipment.get_mut(source) {
                equipped.extend(slots.equipped_indices());
                *slots = EquipmentSlots::new();
            }
        }
        if let Some(mut weapons) = world.query_mut::<EquippedWeapon>() {
            weapons.remove(source);
        }
        equipped.sort_unstable_by_key(|index| index.0);
        equipped.dedup();
        let form_ids = source_form_ids.unwrap_or_default();
        byroredux_scripting::emit_equipment_changes(
            world,
            source,
            equipped.into_iter().filter_map(|index| {
                form_ids.get(index.0 as usize).map(|&item_form_id| {
                    byroredux_scripting::EquipmentChange {
                        item_form_id,
                        equipped: false,
                    }
                })
            }),
        );
    } else {
        // A selectively taken row that the source still had equipped stops
        // being equipped: the armor left with the player. Release exactly
        // that index so a partially looted corpse's other slots survive.
        for (index, item_form_id) in cleared_source_indices {
            let mut was_equipped = false;
            if let Some(mut equipment) = world.query_mut::<EquipmentSlots>() {
                if let Some(slots) = equipment.get_mut(source) {
                    was_equipped = slots.is_equipped(index);
                    if was_equipped {
                        slots.release(index);
                    }
                }
            }
            if let Some(mut weapons) = world.query_mut::<EquippedWeapon>() {
                if weapons
                    .get(source)
                    .is_some_and(|weapon| weapon.inventory_index == index)
                {
                    weapons.remove(source);
                    was_equipped = true;
                }
            }
            if was_equipped {
                byroredux_scripting::emit_equipment_changes(
                    world,
                    source,
                    [byroredux_scripting::EquipmentChange {
                        item_form_id,
                        equipped: false,
                    }],
                );
            }
        }
    }
    Some(outcome)
}

/// Consume canonical activation events without draining them: scripts observe
/// the same activation later in Update. A container or corpse activation is
/// **Take All** (#4464): the P3 spec's "basic take-all action, not a
/// container browser" (`docs/engine/playable-vertical-slice.md`) — every
/// stack moves whole through [`transfer_loot`], the source's equipment
/// clears with one unequip event per slot, and locked/non-container
/// activations are rejected inside the transfer's own safety gates. An
/// already-empty source transfers nothing and says nothing. A loose world
/// item is picked up directly — the whole placed stack (`XCNT`, #4706).
pub(crate) fn container_loot_system(world: &World, _dt: f32) {
    let Some(player) = PlayerEntity::get(world)
    else {
        return;
    };
    // #4701 — a corpse at the controls loots nothing. Scripted activations
    // by the player (`script.activate`, fragments) hit the same gate as E.
    if !crate::systems::player_can_act(world) {
        return;
    }
    let events: Vec<_> = world
        .query::<byroredux_scripting::ActivateEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, event)| (entity, event.activator))
                .collect()
        })
        .unwrap_or_default();
    for (target, activator) in events {
        // M47.2 — a script-blocked reference keeps its OnActivate but not
        // its default pickup / container processing.
        if activator != player
            || target == player
            || byroredux_scripting::default_activation_blocked(world, target)
        {
            continue;
        }
        if is_loot_source(world, target) {
            transfer_loot(world, player, target, LootSelection::All);
        } else if pickup_loot(world, player, target) {
            // Handled: the item moved and the placement hid itself.
        }
    }
}

/// A world-placed item that can be picked up on activation: a base the item
/// catalog knows (anything with a name/weight — MISC, WEAP, ARMO, ALCH, …)
/// that is neither a container (those browse, see [`is_loot_source`]) nor
/// carrying its own `Inventory` (NPCs, already-looted containers).
pub(crate) fn is_pickup_target(world: &World, entity: byroredux_core::ecs::EntityId) -> bool {
    if world.get::<Inventory>(entity).is_some()
        || world
            .get::<byroredux_core::ecs::components::Dead>(entity)
            .is_some()
        || world.get::<PickedUp>(entity).is_some()
    {
        return false;
    }
    let Some(base) = world
        .get::<byroredux_scripting::SceneAliasCandidate>(entity)
        .map(|identity| identity.base_form_id)
    else {
        return false;
    };
    world
        .try_resource::<InventoryCatalog>()
        .is_some_and(|catalog| catalog.is_pickup_base(base))
}

/// Marker on a placement whose item the player already picked up. Keeps the
/// entity resident-but-hidden (render + interaction skip it) and re-parks a
/// `picked_up` tombstone on cell eviction so a respawned copy stays gone.
/// Never serialized itself — the tombstone row is the durable half.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PickedUp;
impl Component for PickedUp {
    type Storage = SparseSetStorage<Self>;
}

/// Authored stack size of an item placement — the REFR's `XCNT` (#4706).
/// Stamped on the placement root at spawn only when the REFR authors one;
/// an unstamped placement is a single item. Re-derived from the plugin on
/// every load, so it is never serialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacedItemCount(pub(crate) u32);
impl Component for PlacedItemCount {
    type Storage = SparseSetStorage<Self>;
}

/// Execute a pickup: append one stack of the placement's base item (its
/// authored [`PlacedItemCount`], else one) to the player's inventory, hide
/// the placement's meshes, park the tombstone, and publish the
/// item/activation events. `false` when `target` is not a
/// pickup target or the player cannot receive items.
pub(crate) fn pickup_loot(
    world: &World,
    player: byroredux_core::ecs::EntityId,
    target: byroredux_core::ecs::EntityId,
) -> bool {
    take_placed_item(world, player, target, true)
}

/// [`pickup_loot`] for any actor. An NPC taker (a Find package picking up
/// what it was sent for) moves the item the same way - inventory, hidden
/// placement, tombstone, item-transfer events - but says nothing to the player
/// and cannot commit the *player's* theft: ownership is a rule about the
/// player's hands.
pub(crate) fn take_placed_item(
    world: &World,
    player: byroredux_core::ecs::EntityId,
    target: byroredux_core::ecs::EntityId,
    player_is_taker: bool,
) -> bool {
    if !is_pickup_target(world, target) {
        return false;
    }
    let Some(base) = world
        .get::<byroredux_scripting::SceneAliasCandidate>(target)
        .map(|identity| identity.base_form_id)
    else {
        return false;
    };
    let stolen_from = if player_is_taker { theft_owner(world, player, target) } else { None };
    let stolen = stolen_from.is_some();
    let count = world
        .get::<PlacedItemCount>(target)
        .map_or(1, |count| count.0.max(1));
    {
        let Some(mut inventories) = world.query_mut::<Inventory>() else {
            return false;
        };
        let Some(destination) = inventories.get_mut(player) else {
            return false;
        };
        destination.add(ItemStack::new(base, count));
    }
    // A `&World` system inserts through the query write guard (the
    // `apply_player_drowning_damage` pattern), which needs the storage to
    // exist — `boot/world.rs` pre-registers `PickedUp` for exactly this.
    // #4571 — the marker hides the placement's MESHES, but the meshes are
    // descendants: the root never carries a MeshHandle
    // (spawn_placement_root), and both render skips read the marker on the
    // mesh/skinned entity itself. Stamp the subtree like the
    // NpcAppearanceHidden sibling does, so the consumers see it without a
    // per-frame ancestor walk. #4983 — walk BEFORE taking the marker write:
    // holding it across the `Children`/`MeshHandle` walk recorded
    // `PickedUp → Children`, closing a cycle against the render skips'
    // `GlobalTransform → … → PickedUp` read. `PickedUp` stays a sink.
    let meshes = crate::npc_spawn::loot_appearance::mesh_entities_under(world, target);
    if let Some(mut markers) = world.query_mut::<PickedUp>() {
        markers.insert(target, PickedUp);
        for entity in meshes {
            markers.insert(entity, PickedUp);
        }
    }
    crate::cell_loader::reference_state::mark_picked_up(world, target);
    // #4818 — the collision entities are standalone (world-composed
    // transforms, no `Parent`), so the PickedUp subtree marker above cannot
    // reach them: without this sweep the taken item leaves an invisible
    // solid in the aim line that blocks selection of whatever sits behind
    // it and can be shoved around as a ghost body.
    crate::npc_spawn::loot_appearance::remove_collision_bodies(world, target);
    let mut name = world
        .try_resource::<InventoryCatalog>()
        .and_then(|catalog| catalog.entries.get(&base).map(|entry| entry.name.clone()))
        .unwrap_or_else(|| format!("Item {base:08X}"));
    if count > 1 {
        name = format!("{name} ({count})");
    }
    if player_is_taker {
        crate::notifications::push(
            world,
            if stolen {
                format!("Stolen {name}")
            } else {
                format!("Added {name}")
            },
        );
    }
    if let Some(owner) = stolen_from {
        crate::crime::report_theft(world, player, owner, &[(base, count)]);
    }
    byroredux_scripting::emit_item_transfers(
        world,
        player,
        [byroredux_scripting::ItemTransfer {
            item_form_id: base,
            count,
            added: true,
            stolen,
        }],
    );
    true
}
