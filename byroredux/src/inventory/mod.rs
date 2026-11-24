//! Native inventory presentation, container loot, and equipment mutations.
//!
//! The canonical state remains [`Inventory`] + [`EquipmentSlots`]. This module
//! carries immutable item-record metadata into the ECS, seeds the player
//! from the master NPC record, and translates activation/native-menu actions
//! back into those existing components.

use byroredux_core::ecs::components::{
    EquipmentSlots, EquippedWeapon, Inventory, InventoryIndex, ItemStack,
};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use byroredux_sdk::inventory::{ItemCategory, ItemMetadata};
use rustc_hash::FxHashMap;

use crate::npc_spawn::effective_actor_level;
use crate::systems::PlayerEntity;

mod actions;
mod catalog;
mod loot;
#[cfg(test)]
mod tests;

pub(crate) use actions::*;
pub(crate) use catalog::*;
pub(crate) use loot::*;

/// Every production file of this module, for the source-shape scans that used to `include_str!` the
/// single `inventory.rs` (#6356); `tests.rs` is excluded, as it never carried the scanned calls.
#[cfg(test)]
pub(crate) const SOURCES: &str = concat!(
    include_str!("mod.rs"),
    include_str!("catalog.rs"),
    include_str!("loot.rs"),
    include_str!("actions.rs"),
);

// #7211 — the Player base FormID and its per-game table live beside `NpcRecord` in the plugin crate, where the
// population and scripting crates can read them too.
pub(crate) use byroredux_plugin::esm::records::actor::{player_npc_form_id, PLAYER_NPC_FORM_ID};

/// Where an equippable item goes when the player toggles it.
///
/// #3112 — this used to be a bare `Option<u32>` slot mask, with weapons
/// assigned a "spare" bit 31. There is no spare bit: Skyrim+ `BOD2`/`BODT`
/// addresses all 32 clothing slots (bit 0 = body-part 30, so bit 31 =
/// body-part 61 / `FX01`), and since #6778 `EquipmentSlots` indexes
/// `MAX_BIPED_SLOTS = 64` occupants, Starfield's `BO64` spacesuit layer in
/// bits 32-63. An authored ARMO in the old weapon bit displaced the weapon and
/// dropped the player to unarmed damage. Modelling the two destinations
/// as distinct variants makes the collision unrepresentable instead of
/// relying on a bit nobody authors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EquipTarget {
    /// Authored biped-slot bits (FO3/FNV `BMDT`, Skyrim+ `BOD2`/`BODT`).
    /// The armor's biped mask, 64-bit for Starfield's `BO64` (#6778).
    BipedSlots(u64),
    /// The wielded-weapon slot, outside the biped occupancy array.
    Weapon,
    /// The player's chosen arrow type (player-archery BOW-AMM-001): a slot of its own, not a biped slot and not
    /// the wielded weapon. Only games with a bow row make ammunition equippable.
    Ammo,
    /// A carried light held in the left hand (Hand Equipment HND-SLOT-005). Only where the game has the
    /// hand model (Skyrim); elsewhere a carried light is not equippable.
    Torch,
}


/// One-frame-plus handoff queue for inventory actions arriving off the
/// main-thread mutation scope (the `inv.equip` debug command). Drained
/// through [`apply_action`] — the exact door the native menu's buttons use —
/// beside the other per-frame player actions. Runtime plumbing, never
/// serialized.
#[derive(Debug, Default)]
pub(crate) struct PendingInventoryActions(pub(crate) Vec<byroredux_native_ui::InventoryAction>);

impl Resource for PendingInventoryActions {}

/// Debug/console door to the same append invariants `transfer_loot` keeps:
/// a same-base merge or a whole-stack append, both of which leave every
/// equipment index above the insertion point stable. `&World` — the command
/// surface's interior-mutability convention. `None` when there is no player
/// or no `Inventory` to append into.
pub(crate) fn add_item(
    world: &World,
    form_id: u32,
    count: u32,
) -> Option<InventoryIndex> {
    // #7344 — a count of 0 adds no row and announces no transfer (both callers already refuse or clamp it).
    if count == 0 {
        return None;
    }
    let player = PlayerEntity::get(world)?;
    let index = {
        let mut inventories = world.query_mut::<Inventory>()?;
        let inventory = inventories.get_mut(player)?;
        add_stack(inventory, form_id, count)
    };
    // The loot-transfer path this mirrors announces what it moves; an item added from the
    // console reaches scripts the same way (an item's `OnAdd` block runs).
    byroredux_scripting::emit_item_transfers(
        world,
        player,
        [byroredux_scripting::ItemTransfer { item_form_id: form_id, count, added: true, stolen: false }],
    );
    Some(index)
}

/// Queue one [`InventoryAction::ToggleEquip`] for the player's first row carrying `form_id` (`inv.equip`), to be
/// drained through [`apply_action`] on the main thread. Returns a user-facing error when the player has no such row.
/// The row is [`Inventory::first_row_of`]'s, as `inv.use` and `inv.drop` take it: a row zeroed in place is skipped
/// for a later one that holds the item (#7232).
pub(crate) fn queue_equip_by_form_id(world: &World, form_id: u32) -> Result<String, String> {
    queue_by_form_id(world, form_id, "equip toggle", |index, _| byroredux_native_ui::InventoryAction::ToggleEquip { index })
}

/// Queue one [`InventoryAction::EquipLeft`](byroredux_native_ui::InventoryAction::EquipLeft) for the
/// player's first row carrying `form_id` (`inv.equip <form> left`): a one-handed weapon into the left hand.
pub(crate) fn queue_equip_left_by_form_id(world: &World, form_id: u32) -> Result<String, String> {
    queue_by_form_id(world, form_id, "left-hand equip toggle", |index, _| byroredux_native_ui::InventoryAction::EquipLeft { index })
}

/// Queue the menu's own `Consume` of the player's row carrying `form_id` (`inv.use`): the same door a
/// click on "use" takes, drained by [`drain_pending_inventory_actions`].
pub(crate) fn queue_consume_by_form_id(world: &World, form_id: u32) -> Result<String, String> {
    queue_by_form_id(world, form_id, "use", |index, form_id| byroredux_native_ui::InventoryAction::Consume { index, form_id })
}

/// Queue the menu's own `Drop` of one item of the player's row carrying `form_id` (`inv.drop`).
pub(crate) fn queue_drop_by_form_id(world: &World, form_id: u32) -> Result<String, String> {
    queue_by_form_id(world, form_id, "drop", |index, form_id| byroredux_native_ui::InventoryAction::Drop { index, form_id })
}

/// Queue `action` for the player's first row carrying `form_id`; `verb` names it in the reply.
fn queue_by_form_id(
    world: &World,
    form_id: u32,
    verb: &str,
    action: impl FnOnce(u32, u32) -> byroredux_native_ui::InventoryAction,
) -> Result<String, String> {
    let player = PlayerEntity::get(world).ok_or_else(|| "no player".to_string())?;
    let index = world
        .get::<Inventory>(player)
        .and_then(|inventory| inventory.first_row_of(form_id))
        .ok_or_else(|| format!("no inventory row with form {form_id:08X}"))?;
    let Some(mut pending) = world.try_resource_mut::<PendingInventoryActions>() else {
        return Err("inventory action queue unavailable".to_string());
    };
    pending.0.push(action(index as u32, form_id));
    Ok(format!("queued {verb} of row {index} (form {form_id:08X})"))
}

/// The weapon equips scripts asked for (`player.equipitem <weapon>`, the VCG02 tutorial's rifle), and the
/// player's shields and torches (Hand Equipment #7398 / #7399):
/// the player's row for each is equipped through the same action as the menu, honoring the item's
/// resolved slot. Another actor's request, or an item that is no weapon the player
/// carries, is logged and dropped (an NPC's wielded weapon is not driven from inventory rows yet).
fn drain_script_weapon_equips(world: &mut World) {
    let requests: Vec<_> = world
        .try_resource_mut::<byroredux_scripting::PendingNonArmorEquips>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    if requests.is_empty() {
        return;
    }
    let player = PlayerEntity::get(world);
    for (actor, form_id, hand) in requests {
        let is_weapon = world.try_resource::<InventoryCatalog>().is_some_and(|catalog| {
            catalog
                .entries
                .get(&form_id)
                .is_some_and(|entry| matches!(entry.equip_target, Some(EquipTarget::Weapon | EquipTarget::Torch)))
        });
        // Hand Equipment #7398 — the player's shield comes here too, for the hand rules.
        let is_shield = world
            .try_resource::<byroredux_scripting::hands::HandCatalog>()
            .is_some_and(|catalog| catalog.is_shield(form_id));
        if !is_weapon && !is_shield {
            log::debug!("script EquipItem {form_id:08X}: not a weapon the inventory can wield");
            continue;
        }
        if Some(actor) != player {
            log::debug!("script EquipItem {form_id:08X}: entity {actor} is not the player; its wielded weapon is not driven from rows");
            continue;
        }
        let row = world
            .get::<Inventory>(actor)
            .and_then(|inventory| inventory.first_row_of(form_id));
        let Some(row) = row else {
            log::debug!("script EquipItem {form_id:08X}: the player does not carry it");
            continue;
        };
        if actions::equip_in(world, row as u32, hand, actions::EquipMode::Equip) == MutationResult::Unavailable {
            log::debug!("script EquipItem {form_id:08X}: the item cannot equip in its slot");
        }
    }
}

/// Hand Equipment #7399 — a script's spell readied in the right hand takes the wielded weapon off in the
/// scripting crate, which cannot touch `EquippedWeapon`; the wielded-weapon cache follows the slots here
/// when it names a weapon they no longer wield.
fn drop_stale_player_weapon(world: &mut World) {
    let Some(player) = PlayerEntity::get(world) else {
        return;
    };
    let cached = world.get::<EquippedWeapon>(player).map(|weapon| weapon.inventory_index);
    let wielded = world.get::<EquipmentSlots>(player).and_then(|slots| slots.weapon);
    if cached.is_some() && cached != wielded {
        actions::reconcile_player_equipped_weapon(world, player);
    }
}

/// Drain the queued inventory actions through the canonical menu door.
/// Called on the main thread beside the other player-action steps.
pub(crate) fn drain_pending_inventory_actions(world: &mut World) {
    drop_stale_player_weapon(world);
    let actions = world.try_resource_mut::<PendingInventoryActions>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    for action in actions {
        if apply_action(world, action) == MutationResult::Unavailable {
            log::warn!("queued inventory action was unavailable for the current player/item");
        }
    }
    drain_script_weapon_equips(world);
}

/// M65 (SNK-DET-010) — the summed weight of what `actor` has equipped (each item
/// once, though it covers several slots): the detection formula's boot / armor
/// weight. 0 without an inventory, equipment or catalog.
pub(crate) fn equipped_weight(world: &World, actor: byroredux_core::ecs::EntityId) -> f32 {
    weight_of_equipped(world, actor, |slots| slots.equipped_indices().collect())
}

/// M65 (SNK-DET-005) — the weight of what `actor` wears in biped slot `bit` (Oblivion's boots, bit 5 Foot:
/// the CS detection formula's movement noise).
pub(crate) fn equipped_weight_at(world: &World, actor: byroredux_core::ecs::EntityId, bit: u8) -> f32 {
    weight_of_equipped(world, actor, |slots| slots.at(bit).into_iter().collect())
}

fn weight_of_equipped(
    world: &World,
    actor: byroredux_core::ecs::EntityId,
    pick: impl FnOnce(&byroredux_core::ecs::components::EquipmentSlots) -> Vec<byroredux_core::ecs::components::InventoryIndex>,
) -> f32 {
    let Some(catalog) = world.try_resource::<InventoryCatalog>() else {
        return 0.0;
    };
    let (Some(slots), Some(inventory)) = (
        world.get::<byroredux_core::ecs::components::EquipmentSlots>(actor),
        world.get::<byroredux_core::ecs::components::Inventory>(actor),
    ) else {
        return 0.0;
    };
    let mut indices = pick(&slots);
    indices.sort_unstable_by_key(|index| index.0);
    indices.dedup();
    indices
        .into_iter()
        .filter_map(|index| inventory.get(index))
        .filter_map(|stack| catalog.entries.get(&stack.base_form_id).map(|entry| entry.weight))
        .sum()
}
