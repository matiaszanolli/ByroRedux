//! Native inventory presentation and player-facing equipment mutations.
//!
//! The canonical state remains [`Inventory`] + [`EquipmentSlots`]. This module
//! only carries immutable item-record metadata into the ECS, seeds the player
//! from the master NPC record, and translates native-menu actions back into
//! those existing components.

use byroredux_core::ecs::components::{
    EquipmentSlots, EquippedWeapon, Inventory, InventoryIndex, ItemStack,
};
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use rustc_hash::FxHashMap;

use crate::npc_spawn::effective_actor_level;
use crate::systems::PlayerEntity;

const PLAYER_NPC_FORM_ID: u32 = 0x0000_0007;

/// Where an equippable item goes when the player toggles it.
///
/// #3112 — this used to be a bare `Option<u32>` slot mask, with weapons
/// assigned a "spare" bit 31. There is no spare bit: `EquipmentSlots`
/// indexes `MAX_BIPED_SLOTS = 32` occupants and Skyrim+ `BOD2` addresses
/// all 32 of them (bit 0 = body-part 30, so bit 31 = body-part 61 /
/// `FX01`), so an authored ARMO in that slot displaced the weapon and
/// dropped the player to unarmed damage. Modelling the two destinations
/// as distinct variants makes the collision unrepresentable instead of
/// relying on a bit nobody authors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EquipTarget {
    /// Authored biped-slot bits (FO3/FNV `BMDT`, Skyrim+ `BOD2`).
    BipedSlots(u32),
    /// The wielded-weapon slot, outside the biped occupancy array.
    Weapon,
}

/// Resolve the base `NPC_` record for the player, never the placed player
/// reference (`0x00000014`). Every currently supported master preserves the
/// lineage-wide `Player` base at FormID 7; keeping the exhaustive game table
/// here makes a future variant divergence an explicit compiler-visible edit.
fn player_npc_form_id(game: GameKind) -> u32 {
    match game {
        GameKind::Oblivion
        | GameKind::Fallout3NV
        | GameKind::Skyrim
        | GameKind::Fallout4
        | GameKind::Fallout76
        | GameKind::Starfield => PLAYER_NPC_FORM_ID,
    }
}

/// Immutable display/equipment metadata derived from the active plugin set.
#[derive(Debug, Clone)]
pub(crate) struct InventoryItemDefinition {
    pub(crate) name: String,
    pub(crate) category: &'static str,
    pub(crate) value: u32,
    pub(crate) weight: f32,
    pub(crate) details: String,
    pub(crate) equip_target: Option<EquipTarget>,
    pub(crate) weapon_damage: Option<f32>,
    /// Authored reach/speed, `0.0` when the source game's weapon layout
    /// isn't decoded (see `ItemKind::Weapon::reach`/`speed`). `combat.rs`
    /// treats `0.0` as "fall back to the unarmed constant."
    pub(crate) weapon_reach: f32,
    pub(crate) weapon_speed: f32,
}

/// Process-local item lookup rebuilt whenever the plugin index is installed.
#[derive(Debug, Clone, Default)]
pub(crate) struct InventoryCatalog {
    entries: FxHashMap<u32, InventoryItemDefinition>,
}

impl Resource for InventoryCatalog {}

/// Starting state copied onto the player body once it exists. It is metadata,
/// not live gameplay state: subsequent cell loads may replace this resource
/// but never overwrite an already-spawned player's inventory.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerInventoryTemplate {
    inventory: Inventory,
    equipment: EquipmentSlots,
    equipped_weapon: Option<EquippedWeapon>,
}

impl Resource for PlayerInventoryTemplate {}

/// Rebuild item presentation metadata and the base player's starting loadout
/// from the resolved plugin index.
pub(crate) fn install_catalog(world: &mut World, index: &EsmIndex) {
    let entries = index
        .items
        .iter()
        .map(|(&form_id, item)| {
            let authored_name = item.common.full_name.trim();
            let editor_id = item.common.editor_id.trim();
            let name = if !authored_name.is_empty() {
                authored_name.to_owned()
            } else if !editor_id.is_empty() {
                editor_id.to_owned()
            } else {
                format!("Item {form_id:08X}")
            };
            let (category, details, equip_target) = describe_kind(&item.kind);
            let (weapon_damage, weapon_reach, weapon_speed) = match &item.kind {
                ItemKind::Weapon {
                    damage,
                    reach,
                    speed,
                    ..
                } => (Some(*damage as f32), *reach, *speed),
                _ => (None, 0.0, 0.0),
            };
            (
                form_id,
                InventoryItemDefinition {
                    name,
                    category,
                    value: item.common.value,
                    weight: item.common.weight.max(0.0),
                    details,
                    equip_target,
                    weapon_damage,
                    weapon_reach,
                    weapon_speed,
                },
            )
        })
        .collect();
    world.insert_resource(InventoryCatalog { entries });
    world.insert_resource(build_player_template(index));
}

fn describe_kind(kind: &ItemKind) -> (&'static str, String, Option<EquipTarget>) {
    match kind {
        ItemKind::Armor {
            biped_flags,
            dt,
            dr,
            armor_rating_x100,
            ..
        } => {
            let protection = if *armor_rating_x100 > 0 {
                format!("Armor rating {:.1}", *armor_rating_x100 as f32 / 100.0)
            } else if *dt > 0.0 || *dr > 0 {
                format!("DT {:.1} · DR {}", dt, dr)
            } else {
                "Armor".to_owned()
            };
            (
                "Armor",
                protection,
                // The authored mask verbatim — every bit of it addresses a
                // biped slot, including bit 31 (#3112).
                (*biped_flags != 0).then_some(EquipTarget::BipedSlots(*biped_flags)),
            )
        }
        ItemKind::Weapon { damage, .. } => (
            "Weapon",
            format!("Damage {damage}"),
            Some(EquipTarget::Weapon),
        ),
        ItemKind::Ammo { damage, .. } => ("Ammo", format!("Damage {damage:.1}"), None),
        ItemKind::Aid { .. } => ("Aid", "Consumable".to_owned(), None),
        ItemKind::Ingredient { .. } => ("Ingredient", "Ingredient".to_owned(), None),
        ItemKind::Book { .. } => ("Book", "Book".to_owned(), None),
        ItemKind::Note { .. } => ("Note", "Note".to_owned(), None),
        ItemKind::Key => ("Key", "Key".to_owned(), None),
        ItemKind::Mod { .. } => ("Mods", "Loose object modification".to_owned(), None),
        ItemKind::Junk => ("Junk", "Scrappable components".to_owned(), None),
        ItemKind::Misc => ("Misc", "Miscellaneous item".to_owned(), None),
    }
}

fn build_player_template(index: &EsmIndex) -> PlayerInventoryTemplate {
    build_player_template_for(index, player_npc_form_id(index.game))
}

fn build_player_template_for(index: &EsmIndex, player_form_id: u32) -> PlayerInventoryTemplate {
    let Some(player) = index.npcs.get(&player_form_id) else {
        log::error!(
            "Player inventory unavailable: {:?} NPC_ {:08X} is missing from the resolved index",
            index.game,
            player_form_id,
        );
        return PlayerInventoryTemplate::default();
    };
    let actor_level = effective_actor_level(player);
    let mut inventory = Inventory::new();
    let mut equipment = EquipmentSlots::new();
    let mut equipped_weapon = None;
    let mut expanded = Vec::new();

    // Skyrim+ authors initial worn gear through OTFT. Older games generally
    // equip armor directly from CNTO, so their carried armor is equipped below.
    if let Some(outfit_id) = player.default_outfit {
        if let Some(outfit) = index.outfits.get(&outfit_id) {
            for &form_id in &outfit.items {
                expanded.clear();
                byroredux_plugin::equip::expand_leveled_form_id(
                    form_id,
                    actor_level,
                    index,
                    &mut expanded,
                );
                for resolved in expanded.drain(..) {
                    let item_index = add_stack(&mut inventory, resolved, 1);
                    equip_armor(index, &mut equipment, resolved, item_index);
                    prefer_weapon(index, resolved, item_index, &mut equipped_weapon);
                }
            }
        }
    }

    let equip_carried_armor = player.default_outfit.is_none();
    for entry in byroredux_plugin::equip::resolve_inherited_inventory(player, actor_level, index) {
        let count = entry.count.max(0) as u32;
        if count == 0 {
            continue;
        }
        expanded.clear();
        byroredux_plugin::equip::expand_leveled_form_id(
            entry.item_form_id,
            actor_level,
            index,
            &mut expanded,
        );
        for resolved in expanded.drain(..) {
            let item_index = add_stack(&mut inventory, resolved, count);
            if equip_carried_armor {
                equip_armor(index, &mut equipment, resolved, item_index);
            }
            prefer_weapon(index, resolved, item_index, &mut equipped_weapon);
        }
    }

    PlayerInventoryTemplate {
        inventory,
        equipment,
        equipped_weapon,
    }
}

fn add_stack(inventory: &mut Inventory, form_id: u32, count: u32) -> InventoryIndex {
    if let Some(index) = inventory
        .items
        .iter()
        .position(|stack| stack.base_form_id == form_id && stack.instance.is_none())
    {
        inventory.items[index].count = inventory.items[index].count.saturating_add(count);
        return InventoryIndex(index as u32);
    }
    inventory.push(ItemStack::new(form_id, count))
}

fn equip_armor(
    index: &EsmIndex,
    equipment: &mut EquipmentSlots,
    form_id: u32,
    inventory_index: InventoryIndex,
) {
    let Some(ItemKind::Armor { biped_flags, .. }) =
        index.items.get(&form_id).map(|item| &item.kind)
    else {
        return;
    };
    if *biped_flags != 0 {
        equipment.equip(*biped_flags, inventory_index);
    }
}

/// Select one deterministic weapon from authored inventory: highest base
/// damage wins, then lowest FormID breaks ties. Multiple LVLI outcomes can
/// still be carried, but only one becomes live combat state.
fn prefer_weapon(
    index: &EsmIndex,
    form_id: u32,
    inventory_index: InventoryIndex,
    equipped: &mut Option<EquippedWeapon>,
) {
    let Some(ItemKind::Weapon {
        damage,
        reach,
        speed,
        ..
    }) = index.items.get(&form_id).map(|item| &item.kind)
    else {
        return;
    };
    let candidate = EquippedWeapon {
        inventory_index,
        base_form_id: form_id,
        damage: *damage as f32,
        reach: *reach,
        speed: *speed,
    };
    if equipped.is_none_or(|current| {
        candidate.damage > current.damage
            || (candidate.damage == current.damage && candidate.base_form_id < current.base_form_id)
    }) {
        *equipped = Some(candidate);
    }
}

/// Attach canonical inventory state to a newly-created player body.
pub(crate) fn attach_to_player(world: &mut World, player: byroredux_core::ecs::EntityId) {
    let template = world
        .try_resource::<PlayerInventoryTemplate>()
        .map(|template| template.clone())
        .unwrap_or_default();
    world.insert(player, template.inventory);
    let mut equipment = template.equipment;
    // #3112 — mirror the template's starting weapon into the equipment's
    // weapon slot. `prefer_weapon` only fills `equipped_weapon`, so pre-fix
    // the slots and the component disagreed from spawn: the very first menu
    // toggle re-derived `EquippedWeapon` from an empty slot and dropped the
    // authored starting weapon.
    if let Some(weapon) = template.equipped_weapon {
        equipment.equip_weapon(weapon.inventory_index);
    }
    world.insert(player, equipment);
    if let Some(weapon) = template.equipped_weapon {
        world.insert(player, weapon);
    }
}

/// Build the presentation snapshot only while the native inventory is visible.
pub(crate) fn snapshot(world: &World) -> Option<byroredux_debug_ui::InventorySnapshot> {
    let player = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)?;
    // Clone each component before acquiring the next storage lock. The menu
    // is off the hot path, and this preserves the ECS invariant that callers
    // never hold independently-acquired component locks in an arbitrary order.
    let inventory = (*world.get::<Inventory>(player)?).clone();
    let equipment = world
        .get::<EquipmentSlots>(player)
        .map(|equipment| (*equipment).clone());
    let catalog = world.try_resource::<InventoryCatalog>();

    let mut items = Vec::with_capacity(inventory.items.len());
    let mut total_weight = 0.0;
    for (raw_index, stack) in inventory.items.iter().enumerate() {
        if stack.count == 0 {
            continue;
        }
        let definition = catalog
            .as_ref()
            .and_then(|catalog| catalog.entries.get(&stack.base_form_id));
        let weight = definition.map_or(0.0, |definition| definition.weight);
        total_weight += weight * stack.count as f32;
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
        total_weight,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MutationResult {
    Equipped,
    Unequipped,
    Unavailable,
}

/// Apply a native-menu equipment action to the canonical player components.
pub(crate) fn apply_action(
    world: &mut World,
    action: byroredux_debug_ui::InventoryAction,
) -> MutationResult {
    let byroredux_debug_ui::InventoryAction::ToggleEquip { index } = action;
    let Some(player) = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return MutationResult::Unavailable;
    };
    let inventory_index = InventoryIndex(index);
    let Some(form_id) = world
        .get::<Inventory>(player)
        .and_then(|inventory| inventory.get(inventory_index).copied())
        .filter(|stack| stack.count > 0)
        .map(|stack| stack.base_form_id)
    else {
        return MutationResult::Unavailable;
    };
    let Some(equip_target) = world.try_resource::<InventoryCatalog>().and_then(|catalog| {
        catalog
            .entries
            .get(&form_id)
            .and_then(|item| item.equip_target)
    }) else {
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
    let mutation = if equipment.release(inventory_index) {
        log::info!("inventory: player unequipped {form_id:08X}");
        MutationResult::Unequipped
    } else {
        match equip_target {
            EquipTarget::BipedSlots(mask) => {
                equipment.equip(mask, inventory_index);
            }
            EquipTarget::Weapon => {
                equipment.equip_weapon(inventory_index);
            }
        }
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
    mutation
}

fn reconcile_equipped_weapon(
    world: &mut World,
    player: byroredux_core::ecs::EntityId,
    inventory_index: Option<InventoryIndex>,
) {
    let candidate = inventory_index.and_then(|inventory_index| {
        let form_id = world
            .get::<Inventory>(player)
            .and_then(|inventory| inventory.get(inventory_index).copied())
            .filter(|stack| stack.count > 0)
            .map(|stack| stack.base_form_id)?;
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
            damage,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (World, byroredux_core::ecs::EntityId) {
        let mut world = World::new();
        world.register::<Inventory>();
        world.register::<EquipmentSlots>();
        world.register::<EquippedWeapon>();
        let player = world.spawn();
        let mut inventory = Inventory::new();
        inventory.push(ItemStack::new(0x1234, 1));
        inventory.push(ItemStack::new(0x5678, 12));
        inventory.push(ItemStack::new(0x9ABC, 1));
        world.insert(player, inventory);
        world.insert(player, EquipmentSlots::new());
        world.insert_resource(PlayerEntity(Some(player)));
        world.insert_resource(InventoryCatalog {
            entries: FxHashMap::from_iter([
                (
                    0x1234,
                    InventoryItemDefinition {
                        name: "Iron Armor".to_owned(),
                        category: "Armor",
                        value: 125,
                        weight: 30.0,
                        details: "Armor rating 25.0".to_owned(),
                        equip_target: Some(EquipTarget::BipedSlots(1 << 12)),
                        weapon_damage: None,
                        weapon_reach: 0.0,
                        weapon_speed: 0.0,
                    },
                ),
                (
                    0x5678,
                    InventoryItemDefinition {
                        name: "Iron Arrow".to_owned(),
                        category: "Ammo",
                        value: 1,
                        weight: 0.0,
                        details: "Damage 8.0".to_owned(),
                        equip_target: None,
                        weapon_damage: None,
                        weapon_reach: 0.0,
                        weapon_speed: 0.0,
                    },
                ),
                (
                    0x9ABC,
                    InventoryItemDefinition {
                        name: "Iron Sword".to_owned(),
                        category: "Weapon",
                        value: 100,
                        weight: 8.0,
                        details: "Damage 12".to_owned(),
                        equip_target: Some(EquipTarget::Weapon),
                        weapon_damage: Some(12.0),
                        weapon_reach: 90.0,
                        weapon_speed: 1.1,
                    },
                ),
            ]),
        });
        (world, player)
    }

    #[test]
    fn snapshot_resolves_metadata_and_equipped_state() {
        let (mut world, player) = fixture();
        world
            .get_mut::<EquipmentSlots>(player)
            .unwrap()
            .equip(1 << 12, InventoryIndex(0));

        let snapshot = snapshot(&world).unwrap();
        assert_eq!(snapshot.items.len(), 3);
        let armor = snapshot
            .items
            .iter()
            .find(|item| item.form_id == 0x1234)
            .unwrap();
        assert_eq!(armor.name, "Iron Armor");
        assert!(armor.equipped);
        assert!(armor.equippable);
        assert_eq!(snapshot.total_weight, 38.0);
    }

    #[test]
    fn toggle_equip_mutates_only_supported_items() {
        let (mut world, player) = fixture();
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 }
            ),
            MutationResult::Equipped
        );
        assert_eq!(
            world.get::<EquipmentSlots>(player).unwrap().at(12),
            Some(InventoryIndex(0))
        );
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 }
            ),
            MutationResult::Unequipped
        );
        assert_eq!(world.get::<EquipmentSlots>(player).unwrap().at(12), None);
        assert!(world.get::<EquippedWeapon>(player).is_none());
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::ToggleEquip { index: 1 }
            ),
            MutationResult::Unavailable
        );
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 }
            ),
            MutationResult::Equipped
        );
        let weapon = world.get::<EquippedWeapon>(player).unwrap();
        assert_eq!(weapon.inventory_index, InventoryIndex(2));
        assert_eq!(weapon.base_form_id, 0x9ABC);
        assert_eq!(weapon.damage, 12.0);
        assert_eq!(weapon.reach, 90.0);
        assert_eq!(weapon.speed, 1.1);
        drop(weapon);
        assert_eq!(
            world.get::<EquipmentSlots>(player).unwrap().weapon,
            Some(InventoryIndex(2))
        );
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 }
            ),
            MutationResult::Unequipped
        );
        assert!(world.get::<EquippedWeapon>(player).is_none());
    }

    #[test]
    fn fallout_categories_are_preserved_for_native_inventory() {
        assert_eq!(describe_kind(&ItemKind::Mod { was_junk: false }).0, "Mods");
        assert_eq!(describe_kind(&ItemKind::Junk).0, "Junk");
        assert_eq!(describe_kind(&ItemKind::Misc).0, "Misc");
    }

    // ── weapon slot vs. authored biped masks (#3112) ────────────────────

    /// The headline regression: a Skyrim/FO4 ARMO whose `BOD2` sets bit 31
    /// (body-part 61 / `FX01`) is a real authorable record. Pre-fix the
    /// weapon lived at `occupants[31]`, so equipping such an armor wrote
    /// over it and `reconcile_equipped_weapon` — reading that same bit and
    /// finding an armor with `weapon_damage: None` — removed the player's
    /// `EquippedWeapon`, silently dropping melee damage to `UNARMED_DAMAGE`.
    #[test]
    fn equipping_a_bit31_armor_does_not_unequip_the_weapon() {
        let (mut world, player) = fixture();
        // Re-author the armor with the exact mask that used to collide.
        world
            .try_resource_mut::<InventoryCatalog>()
            .unwrap()
            .entries
            .get_mut(&0x1234)
            .unwrap()
            .equip_target = Some(EquipTarget::BipedSlots(1 << 31));

        // Wield the sword first.
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 }
            ),
            MutationResult::Equipped
        );
        assert!(world.get::<EquippedWeapon>(player).is_some());

        // Now equip the bit-31 armor — the weapon must survive.
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 }
            ),
            MutationResult::Equipped
        );
        let weapon = world
            .get::<EquippedWeapon>(player)
            .expect("a bit-31 armor must not unequip the wielded weapon (#3112)");
        assert_eq!(weapon.inventory_index, InventoryIndex(2));
        assert_eq!(weapon.damage, 12.0);
        drop(weapon);

        // And the armor really did land in biped bit 31 — the two occupy
        // genuinely disjoint storage, not one displacing the other.
        let slots = world.get::<EquipmentSlots>(player).unwrap();
        assert_eq!(slots.at(31), Some(InventoryIndex(0)));
        assert_eq!(slots.weapon, Some(InventoryIndex(2)));
    }

    /// The reverse direction from the same finding: wielding a weapon must
    /// not evict an armor that occupies bit 31.
    #[test]
    fn equipping_a_weapon_does_not_displace_a_bit31_armor() {
        let (mut world, player) = fixture();
        world
            .try_resource_mut::<InventoryCatalog>()
            .unwrap()
            .entries
            .get_mut(&0x1234)
            .unwrap()
            .equip_target = Some(EquipTarget::BipedSlots(1 << 31));

        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 },
        );
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 },
        );

        let slots = world.get::<EquipmentSlots>(player).unwrap();
        assert_eq!(
            slots.at(31),
            Some(InventoryIndex(0)),
            "wielding a weapon must not displace an armor from biped bit 31"
        );
        assert_eq!(slots.weapon, Some(InventoryIndex(2)));
    }

    /// `describe_kind` is the only producer of `EquipTarget`, so the
    /// collision is closed at the source: no `ItemKind::Armor` — whatever
    /// its authored mask — can ever yield `EquipTarget::Weapon`.
    #[test]
    fn no_armor_describe_kind_output_can_target_the_weapon_slot() {
        for biped_flags in [1u32 << 31, u32::MAX, 0x8000_1000, 1, 0] {
            let (_, _, target) = describe_kind(&ItemKind::Armor {
                biped_flags,
                dt: 0.0,
                dr: 0,
                health: 0,
                slot_mask: 0,
                armor_rating_x100: 0,
                armor_type: None,
                armatures: Vec::new(),
            });
            assert_ne!(
                target,
                Some(EquipTarget::Weapon),
                "armor with biped_flags {biped_flags:#010X} must never target the weapon slot",
            );
        }
    }

    /// An armor toggle must leave `EquippedWeapon` untouched entirely —
    /// not merely "re-derive to the same value". Pins the `weapon_changed`
    /// gate in `apply_action`.
    #[test]
    fn armor_toggles_never_touch_the_equipped_weapon() {
        let (mut world, player) = fixture();
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 2 },
        );
        let before = *world.get::<EquippedWeapon>(player).unwrap();

        // Equip then unequip the armor.
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 },
        );
        apply_action(
            &mut world,
            byroredux_debug_ui::InventoryAction::ToggleEquip { index: 0 },
        );

        let after = *world.get::<EquippedWeapon>(player).unwrap();
        assert_eq!(before.inventory_index, after.inventory_index);
        assert_eq!(before.base_form_id, after.base_form_id);
        assert_eq!(before.damage, after.damage);
    }

    #[test]
    fn player_template_uses_authored_inventory_and_equipment_slots() {
        use byroredux_plugin::esm::records::common::CommonItemFields;
        use byroredux_plugin::esm::records::{ItemRecord, NpcInventoryEntry, NpcRecord};

        let armor_form = 0x0001_2345;
        let ammo_form = 0x0001_2346;
        let synthetic_player_form = 0x00AB_CDEF;
        let mut index = EsmIndex::default();
        index.npcs.insert(
            synthetic_player_form,
            NpcRecord {
                inventory: vec![
                    NpcInventoryEntry {
                        item_form_id: armor_form,
                        count: 1,
                    },
                    NpcInventoryEntry {
                        item_form_id: ammo_form,
                        count: 20,
                    },
                ],
                level: 1,
                ..Default::default()
            },
        );
        index.items.insert(
            armor_form,
            ItemRecord {
                form_id: armor_form,
                common: CommonItemFields::default(),
                kind: ItemKind::Armor {
                    biped_flags: 1 << 12,
                    dt: 0.0,
                    dr: 0,
                    health: 0,
                    slot_mask: 0,
                    armor_rating_x100: 0,
                    armor_type: None,
                    armatures: Vec::new(),
                },
            },
        );
        index.items.insert(
            ammo_form,
            ItemRecord {
                form_id: ammo_form,
                common: CommonItemFields::default(),
                kind: ItemKind::Ammo {
                    damage: 8.0,
                    dt_mult: 1.0,
                    spread: 0.0,
                    projectile_form: 0,
                    casing_form: 0,
                    health: 0,
                    clip_rounds: 0,
                },
            },
        );

        let template = build_player_template_for(&index, synthetic_player_form);
        assert_eq!(template.inventory.items.len(), 2);
        assert_eq!(
            template
                .inventory
                .items
                .iter()
                .find(|stack| stack.base_form_id == ammo_form)
                .unwrap()
                .count,
            20
        );
        assert_eq!(template.equipment.at(12), Some(InventoryIndex(0)));
    }

    #[test]
    fn player_npc_form_id_is_canonical_across_supported_games() {
        for game in [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ] {
            assert_eq!(player_npc_form_id(game), 0x0000_0007, "{game:?}");
        }
    }

    #[test]
    fn installed_fnv_player_base_builds_a_nonempty_template() {
        let data = std::env::var_os("BYROREDUX_FNV_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
                )
            });
        let esm = data.join("FalloutNV.esm");
        let Ok(bytes) = std::fs::read(&esm) else {
            eprintln!(
                "skipping real-data #3099 regression: {} absent",
                esm.display()
            );
            return;
        };
        let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse FalloutNV.esm");
        let player_form_id = player_npc_form_id(index.game);
        let player = index
            .npcs
            .get(&player_form_id)
            .expect("canonical Player NPC_ 00000007");

        assert_eq!(player.editor_id, "Player");
        assert!(
            index.npcs.get(&0x0000_0014).is_none(),
            "00000014 is the placed player reference, not an NPC_ base",
        );
        assert!(
            !build_player_template(&index).inventory.items.is_empty(),
            "the authored Player inventory must reach the startup template",
        );
    }
}
