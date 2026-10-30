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
use byroredux_plugin::esm::reader::GameKind;
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

const PLAYER_NPC_FORM_ID: u32 = 0x0000_0007;

/// Where an equippable item goes when the player toggles it.
///
/// #3112 — this used to be a bare `Option<u32>` slot mask, with weapons
/// assigned a "spare" bit 31. There is no spare bit: `EquipmentSlots`
/// indexes `MAX_BIPED_SLOTS = 32` occupants and Skyrim+ `BOD2`/`BODT` addresses
/// all 32 of them (bit 0 = body-part 30, so bit 31 = body-part 61 /
/// `FX01`), so an authored ARMO in that slot displaced the weapon and
/// dropped the player to unarmed damage. Modelling the two destinations
/// as distinct variants makes the collision unrepresentable instead of
/// relying on a bit nobody authors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EquipTarget {
    /// Authored biped-slot bits (FO3/FNV `BMDT`, Skyrim+ `BOD2`/`BODT`).
    BipedSlots(u32),
    /// The wielded-weapon slot, outside the biped occupancy array.
    Weapon,
}

/// Resolve the base `NPC_` record for the player, never the placed player
/// reference (`0x00000014`). Every currently supported master preserves the
/// lineage-wide `Player` base at FormID 7; keeping the exhaustive game table
/// here makes a future variant divergence an explicit compiler-visible edit.
pub(crate) fn player_npc_form_id(game: GameKind) -> u32 {
    match game {
        GameKind::Oblivion
        | GameKind::Fallout3NV
        | GameKind::Skyrim
        | GameKind::Fallout4
        | GameKind::Fallout76
        | GameKind::Starfield => PLAYER_NPC_FORM_ID,
    }
}

/// One-frame-plus handoff queue for inventory actions arriving off the
/// main-thread mutation scope (the `inv.equip` debug command). Drained
/// through [`apply_action`] — the exact door the native menu's buttons use —
/// beside the other per-frame player actions. Runtime plumbing, never
/// serialized.
#[derive(Debug, Default)]
pub(crate) struct PendingInventoryActions(pub(crate) Vec<byroredux_debug_ui::InventoryAction>);

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

/// Queue one [`InventoryAction::ToggleEquip`] for the row whose base form is
/// `form_id`, to be drained through [`apply_action`] on the main thread.
/// Returns a user-facing error when the player has no such row.
pub(crate) fn queue_equip_by_form_id(world: &World, form_id: u32) -> Result<String, String> {
    let player = PlayerEntity::get(world)
        .ok_or_else(|| "no player".to_string())?;
    let index = world
        .get::<Inventory>(player)
        .and_then(|inventory| {
            inventory
                .items
                .iter()
                .position(|stack| stack.base_form_id == form_id)
        })
        .ok_or_else(|| format!("no inventory row with form {form_id:08X}"))?;
    let Some(mut pending) = world.try_resource_mut::<PendingInventoryActions>() else {
        return Err("inventory action queue unavailable".to_string());
    };
    pending
        .0
        .push(byroredux_debug_ui::InventoryAction::ToggleEquip {
            index: index as u32,
        });
    Ok(format!(
        "queued equip toggle for row {index} (form {form_id:08X})"
    ))
}

/// Queue the menu's own `Consume` of the player's row carrying `form_id` (`inv.use`): the same door a
/// click on "use" takes, drained by [`drain_pending_inventory_actions`].
pub(crate) fn queue_consume_by_form_id(world: &World, form_id: u32) -> Result<String, String> {
    let player = PlayerEntity::get(world).ok_or_else(|| "no player".to_string())?;
    let index = world
        .get::<Inventory>(player)
        .and_then(|inventory| inventory.items.iter().position(|stack| stack.base_form_id == form_id && stack.count > 0))
        .ok_or_else(|| format!("no inventory row with form {form_id:08X}"))?;
    let Some(mut pending) = world.try_resource_mut::<PendingInventoryActions>() else {
        return Err("inventory action queue unavailable".to_string());
    };
    pending.0.push(byroredux_debug_ui::InventoryAction::Consume { index: index as u32, form_id });
    Ok(format!("queued use of row {index} (form {form_id:08X})"))
}

/// Queue the menu's own `Drop` of one item of the player's row carrying `form_id` (`inv.drop`).
pub(crate) fn queue_drop_by_form_id(world: &World, form_id: u32) -> Result<String, String> {
    let player = PlayerEntity::get(world).ok_or_else(|| "no player".to_string())?;
    let index = world
        .get::<Inventory>(player)
        .and_then(|inventory| inventory.items.iter().position(|stack| stack.base_form_id == form_id && stack.count > 0))
        .ok_or_else(|| format!("no inventory row with form {form_id:08X}"))?;
    let Some(mut pending) = world.try_resource_mut::<PendingInventoryActions>() else {
        return Err("inventory action queue unavailable".to_string());
    };
    pending.0.push(byroredux_debug_ui::InventoryAction::Drop { index: index as u32, form_id });
    Ok(format!("queued drop of row {index} (form {form_id:08X})"))
}

/// Drain the queued inventory actions through the canonical menu door.
/// Called on the main thread beside the other player-action steps.
pub(crate) fn drain_pending_inventory_actions(world: &mut World) {
    let Some(mut pending) = world.try_resource_mut::<PendingInventoryActions>() else {
        return;
    };
    if pending.0.is_empty() {
        return;
    }
    let actions = std::mem::take(&mut pending.0);
    drop(pending);
    for action in actions {
        if apply_action(world, action) == MutationResult::Unavailable {
            log::warn!("queued inventory action was unavailable for the current player/item");
        }
    }
}
