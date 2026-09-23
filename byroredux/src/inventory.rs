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
    restorations: FxHashMap<u32, Vec<byroredux_plugin::consumables::ConditionalRestoration>>,
    entries: FxHashMap<u32, InventoryItemDefinition>,
    containers: rustc_hash::FxHashSet<u32>,
}

impl InventoryCatalog {
    pub(crate) fn sdk_metadata(&self, form_id: u32) -> Option<ItemMetadata> {
        let definition = self.entries.get(&form_id)?;
        let category = match definition.category {
            "Misc" => ItemCategory::Misc,
            "Junk" => ItemCategory::Junk,
            "Mods" => ItemCategory::Mod,
            "Book" => ItemCategory::Book,
            "Scroll" => ItemCategory::Scroll,
            "Light" => ItemCategory::Light,
            "Apparatus" => ItemCategory::Apparatus,
            "Note" => ItemCategory::Note,
            "Ingredient" => ItemCategory::Ingredient,
            "Aid" => ItemCategory::Aid,
            "Key" => ItemCategory::Key,
            "Ammo" => ItemCategory::Ammo,
            "Armor" => ItemCategory::Armor,
            "Weapon" => ItemCategory::Weapon,
            _ => return None,
        };
        ItemMetadata::new(
            definition.name.clone(),
            category,
            definition.value,
            definition.weight,
        )
        .ok()
    }
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

/// The player's spawn-time CHARAL seed, stamped onto the body once it
/// exists (#4458). Built from the base Player `NPC_` record through
/// `derive_npc_actor_values` — the same derivation every NPC's
/// `stamp_actor_values` performs — so the consumable and drowning paths,
/// which gate on the player carrying `ActorValues` + `ActorVitals`, have a
/// populated (never empty/zero-SPECIAL) set to read. `None` fields mean
/// the derivation yielded nothing (no records / unsupported profile): the
/// body then carries no actor values, the pre-#4458 state, rather than a
/// wrong-by-construction empty one.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerCharacterTemplate {
    values: Option<byroredux_core::ecs::components::ActorValues>,
    vitals: Option<byroredux_core::ecs::components::ActorVitals>,
    /// #4678 (CHAR-2026-09-21-D4-03) — the Player record's own level. The
    /// same derivation that produced `values` is keyed on it, so a player
    /// without a `CharacterLevel` read 0 in `GetLevel` /
    /// `GetXPForNextLevel` while its Health assumed level 1.
    level: Option<u16>,
    /// #4678 — race/class provenance straight off the resolved Player
    /// record: exactly what `derive_npc_actor_values` already consumed,
    /// which is the honest value the old note claimed didn't exist.
    background: Option<byroredux_core::character::Background>,
}

impl Resource for PlayerCharacterTemplate {}

/// The native HUD vitals bars' canonical keys — (display label, AVIF FormID)
/// pairs resolved once per plugin load from the same AVIF table `ActorValues`
/// is keyed by. Presentation state only: never serialized, rebuilt from
/// records on every `install_catalog`, exactly like the restorable-effect
/// catalog it sits beside.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerVitals {
    bars: Vec<(&'static str, u32)>,
    /// #4675 — canonical AVIF editor id → resolved FormID, from the same
    /// `vital_bar_candidates` table. The HUD drivers' bar tables carry
    /// editor ids (never literal FormIDs — the old hardcoded 0x2C9/0x2D0
    /// were fallout.rs's unit-test fixture ids, not real AVIFs) and
    /// resolve through this once per lookup instead of embedding ids.
    by_editor_id: FxHashMap<&'static str, u32>,
}

impl PlayerVitals {
    /// The resolved key for a canonical AVIF editor id (#4675).
    pub(crate) fn resolved(&self, editor_id: &str) -> Option<u32> {
        self.by_editor_id.get(editor_id).copied()
    }
}

impl Resource for PlayerVitals {}

fn build_player_vitals(index: &EsmIndex) -> PlayerVitals {
    // #4679 — the roster is `CharacterRulesProfile` data (same move #4447
    // made for `body_condition_base`), so a pool the game does not author
    // — or an Oblivion profile whose pre-#3768 resolver cannot resolve
    // any AVIF yet — drops out per candidate instead of the consumer
    // guessing per GameKind.
    let mut by_editor_id = FxHashMap::default();
    let bars = index
        .character_rules
        .vital_pools()
        .iter()
        .filter_map(|&(label, editor_id)| {
            let avif = index.actor_value_form_id(editor_id)?;
            by_editor_id.insert(editor_id, avif);
            Some((label, avif))
        })
        .collect();
    PlayerVitals { bars, by_editor_id }
}

/// Compose the player's vitals bars from canonical `ActorValues`. Returns
/// `None` when there is no player body, it carries no actor values, or the
/// catalog resolved no bars — the HUD then draws nothing rather than empty
/// bars. `max` is the composed undamaged value (`base + permanent +
/// temporary`); `current` additionally subtracts the damage layer, so the
/// bar shows the red-equivalent missing share the same way combat damage
/// and restorative consumption read it.
pub(crate) fn vitals_snapshot(
    world: &World,
) -> Option<Vec<byroredux_debug_ui::VitalBarView>> {
    use byroredux_core::ecs::components::ActorValues;
    let player = world.try_resource::<PlayerEntity>().and_then(|r| r.0)?;
    let vitals = world.try_resource::<PlayerVitals>()?;
    let values = world.get::<ActorValues>(player)?;
    let bars: Vec<byroredux_debug_ui::VitalBarView> = vitals
        .bars
        .iter()
        .filter_map(|&(label, avif)| {
            let entry = values.get(avif)?;
            let max = entry.base + entry.permanent_mod + entry.temporary_mod;
            if max <= 0.0 {
                return None;
            }
            Some(byroredux_debug_ui::VitalBarView {
                label,
                current: max - entry.damage,
                max,
            })
        })
        .collect();
    (!bars.is_empty()).then_some(bars)
}

/// Derive the player's actor values from the base Player `NPC_` record —
/// never the placed reference (`0x14`) — mirroring `stamp_actor_values`
/// (`npc_spawn.rs`) line for line: `derive_npc_actor_values`, then vitals
/// keyed by the resolved Health AVIF only when Health actually landed.
///
/// #4674 (CHAR-2026-09-21-D4-01) — with one difference from the NPC path:
/// the ruleset's `PlayerOnly` rows (FO3/FNV/FO4 Health + AP, Skyrim Light
/// Armor, Oblivion's pools) are evaluated HERE, for the player, against
/// the remaining seed — and the NPC-baked carried answers for those same
/// keys are dropped first. The NPC path answers Health/AP by baked values
/// or an NPC curve; the captures state the player's live values come from
/// the player formulas (FO4: Health 85 / AP 70, not the carried 150/100
/// the PRPS-then-DNAM push order was leaving). Stamping the evaluated
/// values means every consumer — `GetActorValue`'s carried fast path,
/// `vitals_snapshot`, combat damage, drowning — reads them without
/// needing player identity.
fn build_player_character_template(index: &EsmIndex) -> PlayerCharacterTemplate {
    use byroredux_core::character::{DerivedOutput, DerivedScope};

    let Some(player) = index.npcs.get(&player_npc_form_id(index.game)) else {
        return PlayerCharacterTemplate::default();
    };
    // #4457 — resolve once, derive through the resolved view.
    let pairs = byroredux_plugin::esm::records::derive_resolved_actor_values(
        &byroredux_plugin::equip::ResolvedNpc::resolve(player, index),
        index,
    );
    if pairs.is_empty() {
        return PlayerCharacterTemplate::default();
    }
    // #4678 — level + Background ride the same resolved Player record the
    // derivation just consumed, so the stamps can't disagree with the
    // values they sit beside.
    let level = byroredux_plugin::esm::records::effective_actor_level(player);
    let background = Some(byroredux_core::character::Background {
        race_form_id: player.race_form_id,
        class_form_id: player.class_form_id,
    });
    // The ruleset is optional (profiles that build none); without it the
    // derivation stays the NPC answer, the pre-#4674 behaviour, rather
    // than an invented empty seed.
    match crate::npc_spawn::build_character_ruleset(index) {
        Some(ruleset) => {
            let player_only = ruleset.player_only_output_avifs();
            // Drop the NPC path's answers for player-only stats, then
            // evaluate the rows against the remaining SPECIAL/skills seed.
            let seed: Vec<_> = pairs
                .into_iter()
                .filter(|(id, _)| !player_only.contains(id))
                .collect();
            let mut values = byroredux_core::ecs::components::ActorValues::from_pairs(seed);
            let level = level.max(0) as u16;
            for &key in &player_only {
                // Same #2933 contract `GetActorValue` enforces: only
                // Absolute rows are actor-value readings; a Multiplier
                // row's eval is a ratio no consumer may read as a value.
                let Some(formula) = ruleset.derived_formula(key) else {
                    continue;
                };
                if formula.scope != DerivedScope::PlayerOnly
                    || formula.kind != DerivedOutput::Absolute
                {
                    continue;
                }
                if let Some(value) = ruleset.derived_value(key, &values, level) {
                    values.set_base(key, value);
                }
            }
            let health = index.health_actor_value_key().filter(|health| {
                values.get(*health).is_some()
            });
            PlayerCharacterTemplate {
                values: Some(values),
                vitals: health.map(|health| byroredux_core::ecs::components::ActorVitals {
                    health,
                }),
                level: Some(level),
                background,
            }
        }
        None => {
            let health = index
                .health_actor_value_key()
                .filter(|health| pairs.iter().any(|(form_id, _)| form_id == health));
            PlayerCharacterTemplate {
                values: Some(byroredux_core::ecs::components::ActorValues::from_pairs(
                    pairs,
                )),
                vitals: health.map(|health| byroredux_core::ecs::components::ActorVitals {
                    health,
                }),
                level: Some(level.max(0) as u16),
                background,
            }
        }
    }
}

/// Rebuild item presentation metadata and the base player's starting loadout
/// from the resolved plugin index.
pub(crate) fn install_catalog(world: &mut World, index: &EsmIndex) {
    let restorations: FxHashMap<_, _> = index
        .items
        .keys()
        .filter_map(|&id| {
            byroredux_plugin::consumables::restoration_plan(index, id).map(|effects| (id, effects))
        })
        .collect();
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
            let (category, mut details, equip_target) = describe_kind(&item.kind);
            if matches!(item.kind, ItemKind::Aid { .. }) {
                details = if let Some(plan) = restorations.get(&form_id) {
                    if plan.iter().any(|e| e.duration > 0) {
                        "Restores vital resources over time".to_owned()
                    } else if plan.iter().any(|e| !e.conditions.is_empty()) {
                        "Restores vital resources when its conditions are met".to_owned()
                    } else {
                        "Restores vital resources immediately".to_owned()
                    }
                } else {
                    "Consumable (effects unavailable)".to_owned()
                };
            }
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
    world.insert_resource(InventoryCatalog {
        restorations,
        entries,
        containers: index.containers.keys().copied().collect(),
    });
    world.insert_resource(build_player_template(index));
    world.insert_resource(build_player_character_template(index));
    world.insert_resource(build_player_vitals(index));
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
        ItemKind::Scroll { .. } => ("Scroll", "Scroll (casting unavailable)".to_owned(), None),
        ItemKind::Apparatus { .. } => (
            "Apparatus",
            "Alchemy apparatus (crafting unavailable)".to_owned(),
            None,
        ),
        ItemKind::Light { .. } => (
            "Light",
            "Carried light (equipping unavailable)".to_owned(),
            None,
        ),
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
    // #4457 — the player path resolves the TPLT view once and reads both
    // the derivation and the CNTO carry list through it.
    let resolved = byroredux_plugin::equip::ResolvedNpc::resolve(player, index);
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
    for entry in &resolved.inventory.inventory {
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
    // #4458 — the player's CHARAL seed rides the same one-shot attach as
    // the inventory seed: `ActorValues` + `ActorVitals` derived from the
    // base Player NPC_ record, the exact stamps `stamp_actor_values` puts
    // on every NPC. The consumable (`consume_item`) and drowning
    // (`apply_player_drowning_damage`) paths gate on precisely these two
    // components; before this they were structurally Unavailable in every
    // fresh session.
    let character = world
        .try_resource::<PlayerCharacterTemplate>()
        .map(|template| template.clone())
        .unwrap_or_default();
    if let Some(values) = character.values {
        world.insert(player, values);
    }
    if let Some(vitals) = character.vitals {
        world.insert(player, vitals);
    }
    // #4678 — the level the Health derivation assumed, and the race/class
    // provenance from the same record: `GetLevel`/`GetXPForNextLevel` and
    // the loot/melee level-1 defaults now read real data instead of
    // divergent fallbacks (0 in one consumer, 1 in the next).
    if let Some(level) = character.level {
        world.insert(
            player,
            byroredux_core::character::CharacterLevel {
                level,
                xp: 0,
            },
        );
    }
    if let Some(background) = character.background {
        world.insert(player, background);
    }
}

pub(crate) fn is_loot_source(world: &World, entity: byroredux_core::ecs::EntityId) -> bool {
    if world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
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

/// P3's minimal theft rule (no witness/bounty system — recorded, not
/// punished): a transfer is theft when the source is owned and the owner is
/// neither the player's own reference (0x14) nor a faction the player holds
/// any rank in. `XRNK` rank minimums are not evaluated yet; membership is
/// the exemption bar.
fn transfer_is_theft(
    world: &World,
    player: byroredux_core::ecs::EntityId,
    source: byroredux_core::ecs::EntityId,
) -> bool {
    let Some(owned) = world.get::<byroredux_core::ecs::components::Owned>(source) else {
        return false;
    };
    let player_reference = world
        .get::<byroredux_scripting::SceneAliasCandidate>(player)
        .map(|identity| identity.reference_form_id)
        .unwrap_or(0);
    if owned.owner_form_id == player_reference {
        return false;
    }
    if let Some(ranks) = world.get::<byroredux_core::ecs::components::FactionRanks>(player) {
        if ranks.rank(owned.owner_form_id).is_some() {
            return false;
        }
    }
    true
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
    let stolen = transfer_is_theft(world, player, source);
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
                // Append whole stacks: existing equipment indices stay stable
                // and distinct instance-pool identities are never merged or
                // discarded.
                destination.items.extend(stacks);
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
                destination.items.push(stack);
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
/// item is picked up directly — one item per placement, the Bethesda REFR
/// convention.
pub(crate) fn container_loot_system(world: &World, _dt: f32) {
    let Some(player) = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return;
    };
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
        if activator != player || target == player {
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
    world.try_resource::<InventoryCatalog>().is_some_and(|catalog| {
        !catalog.containers.contains(&base) && catalog.entries.contains_key(&base)
    })
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

/// Execute a pickup: append one stack of the placement's base item to the
/// player's inventory, hide the placement's meshes, park the tombstone, and
/// publish the item/activation events. `false` when `target` is not a
/// pickup target or the player cannot receive items.
pub(crate) fn pickup_loot(
    world: &World,
    player: byroredux_core::ecs::EntityId,
    target: byroredux_core::ecs::EntityId,
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
    let stolen = transfer_is_theft(world, player, target);
    {
        let Some(mut inventories) = world.query_mut::<Inventory>() else {
            return false;
        };
        let Some(destination) = inventories.get_mut(player) else {
            return false;
        };
        destination.items.push(ItemStack::new(base, 1));
    }
    // A `&World` system inserts through the query write guard (the
    // `apply_player_drowning_damage` pattern), which needs the storage to
    // exist — `boot/world.rs` pre-registers `PickedUp` for exactly this.
    if let Some(mut markers) = world.query_mut::<PickedUp>() {
        markers.insert(target, PickedUp);
        // #4571 — the marker hides the placement's MESHES, but the meshes
        // are descendants: the root never carries a MeshHandle
        // (spawn_placement_root), and both render skips read the marker on
        // the mesh/skinned entity itself. Stamp the subtree like the
        // NpcAppearanceHidden sibling does, so the consumers see it without
        // a per-frame ancestor walk.
        for entity in
            crate::npc_spawn::loot_appearance::mesh_entities_under(world, target)
        {
            markers.insert(entity, PickedUp);
        }
    }
    crate::cell_loader::reference_state::mark_picked_up(world, target);
    let name = world
        .try_resource::<InventoryCatalog>()
        .and_then(|catalog| catalog.entries.get(&base).map(|entry| entry.name.clone()))
        .unwrap_or_else(|| format!("Item {base:08X}"));
    crate::notifications::push(
        world,
        if stolen {
            format!("Stolen {name}")
        } else {
            format!("Added {name}")
        },
    );
    byroredux_scripting::emit_item_transfers(
        world,
        player,
        [byroredux_scripting::ItemTransfer {
            item_form_id: base,
            count: 1,
            added: true,
            stolen,
        }],
    );
    true
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
            consumable: catalog
                .as_ref()
                .is_some_and(|catalog| catalog.restorations.contains_key(&stack.base_form_id)),
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
    Consumed,
    Equipped,
    Unequipped,
    Unavailable,
}

/// Consume an item with a validated restoration plan. Preflight mutation and
/// leave a zero-count slot so equipment and UI indices never shift.
fn consume_item(world: &mut World, index: u32, form_id: u32) -> MutationResult {
    use byroredux_core::ecs::components::{
        ActorValues, ActorVitals, Dead, TimedRestoration, TimedRestorations,
    };
    use byroredux_core::ecs::resources::ItemInstancePool;
    let Some(player) = world.try_resource::<PlayerEntity>().and_then(|r| r.0) else {
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
    let Some((effects, name)) = world.try_resource::<InventoryCatalog>().and_then(|r| {
        Some((
            r.restorations.get(&form_id)?.clone(),
            r.entries.get(&form_id)?.name.clone(),
        ))
    }) else {
        return MutationResult::Unavailable;
    };
    // Drop the catalog guard before the evaluator acquires component locks.
    // Re-evaluate at use time: acquiring a perk must not require a cell reload.
    let mut effects = effects;
    let mut context = byroredux_scripting::condition::ConditionContext::for_subject(player);
    context.target = Some(player); // ingestible target and caster are the user
    effects.retain(|branch| {
        byroredux_scripting::condition::evaluate(&branch.conditions, world, &context)
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
        || effects.is_empty()
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
    MutationResult::Consumed
}

/// Apply a native-menu action to the canonical player components.
pub(crate) fn apply_action(
    world: &mut World,
    action: byroredux_debug_ui::InventoryAction,
) -> MutationResult {
    if let byroredux_debug_ui::InventoryAction::Consume { index, form_id } = action {
        return consume_item(world, index, form_id);
    }
    let byroredux_debug_ui::InventoryAction::ToggleEquip { index } = action else {
        unreachable!()
    };
    let Some(player) = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
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
    use byroredux_core::ecs::components::{Children, MeshHandle, Parent};

    /// #4571 — pickup_loot's marker must reach the placement's MESH
    /// entities, not just the root: the root never carries a MeshHandle
    /// (spawn_placement_root), and both render skips read `PickedUp` on the
    /// mesh/skinned entity itself, so a root-only marker never hid anything.
    /// The two test asserts the render skips' own docs claimed ("will hide
    /// the placement's meshes") were testing the mesh entity directly and
    /// could not see the placement-root shape.
    #[test]
    fn pickup_stamps_the_subtree_meshes_not_just_the_root() {
        let mut world = World::new();
        world.register::<PickedUp>();
        world.register::<MeshHandle>();
        world.register::<Parent>();
        world.register::<Children>();
        // placement root -> mesh child (the cell-loader shape).
        let root = world.spawn();
        let mesh = world.spawn();
        world.insert(mesh, Parent(root));
        crate::helpers::add_child(&mut world, root, mesh);
        world.insert(mesh, MeshHandle(7));

        if let Some(mut markers) = world.query_mut::<PickedUp>() {
            markers.insert(root, PickedUp);
            for entity in crate::npc_spawn::loot_appearance::mesh_entities_under(&world, root) {
                markers.insert(entity, PickedUp);
            }
        }

        let q = world.query::<PickedUp>().unwrap();
        assert!(q.get(root).is_some(), "the root keeps its marker");
        assert!(
            q.get(mesh).is_some(),
            "the mesh descendant must carry the marker too — the render skips \
             read it there (#4571)"
        );
    }

    fn restoration(
        actor_value: u32,
        magnitude: f32,
    ) -> byroredux_plugin::consumables::ConditionalRestoration {
        byroredux_plugin::consumables::ConditionalRestoration {
            actor_value,
            magnitude,
            duration: 0,
            conditions: Vec::new(),
            medicine: None,
        }
    }

    fn restorative_fixture() -> (World, byroredux_core::ecs::EntityId) {
        use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead};
        let (mut world, player) = fixture();
        world.register::<ActorValues>();
        world.register::<ActorVitals>();
        world.register::<Dead>();
        let mut values = ActorValues::from_pairs([(1000, 100.0)]);
        values.apply_damage(1000, 60.0);
        world.insert(player, values);
        world.insert(player, ActorVitals { health: 1000 });
        world.get_mut::<Inventory>(player).unwrap().items[1].count = 2;
        world
            .resource_mut::<InventoryCatalog>()
            .restorations
            .insert(0x5678, vec![restoration(1000, 25.0)]);
        world.insert_resource(crate::notifications::PlayerNotifications::default());
        (world, player)
    }

    #[test]
    fn timed_consumption_ticks_partial_frames_and_stops_on_death() {
        use byroredux_core::ecs::components::{ActorValues, Dead, TimedRestorations};
        let (mut world, player) = restorative_fixture();
        world
            .resource_mut::<InventoryCatalog>()
            .restorations
            .get_mut(&0x5678)
            .unwrap()[0]
            .duration = 3;
        let action = byroredux_debug_ui::InventoryAction::Consume {
            index: 1,
            form_id: 0x5678,
        };
        assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            40.0
        );
        assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 1);
        crate::systems::restoration::restoration_system(&world, 0.5);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            52.5
        );
        crate::systems::restoration::restoration_system(&world, 0.0);
        assert_eq!(
            world.get::<TimedRestorations>(player).unwrap().effects[0].remaining,
            2.5
        );
        world.insert(player, Dead);
        crate::systems::restoration::restoration_system(&world, 1.0);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            52.5
        );
        assert!(world
            .get::<TimedRestorations>(player)
            .unwrap()
            .effects
            .is_empty());
        world.remove::<Dead>(player);
        assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
        // Health may reach zero before the structural death marker is stamped.
        world
            .get_mut::<ActorValues>(player)
            .unwrap()
            .apply_damage(1000, 52.5);
        crate::systems::restoration::restoration_system(&world, 1.0);
        assert_eq!(world.get::<ActorValues>(player).unwrap().current(1000), 0.0);
        assert!(world
            .get::<TimedRestorations>(player)
            .unwrap()
            .effects
            .is_empty());
    }

    #[test]
    fn medicine_scaling_uses_live_composed_skill_and_preflights_missing_or_invalid_values() {
        use byroredux_core::ecs::components::ActorValues;
        use byroredux_plugin::consumables::MedicineScaling;
        let (mut world, player) = restorative_fixture();
        let scale = Some(MedicineScaling {
            actor_value: 2000,
            base: 1.0,
            multiplier: 2.0,
        });
        world
            .resource_mut::<InventoryCatalog>()
            .restorations
            .get_mut(&0x5678)
            .unwrap()[0]
            .medicine = scale;
        let action = byroredux_debug_ui::InventoryAction::Consume {
            index: 1,
            form_id: 0x5678,
        };
        for bad in [None, Some(f32::NAN), Some(f32::INFINITY)] {
            if let Some(value) = bad {
                world
                    .get_mut::<ActorValues>(player)
                    .unwrap()
                    .set_base(2000, value);
            }
            assert_eq!(
                apply_action(&mut world, action),
                MutationResult::Unavailable
            );
            assert_eq!(
                world.get::<ActorValues>(player).unwrap().current(1000),
                40.0
            );
            assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 2);
        }
        let values = world.get_mut::<ActorValues>(player).unwrap();
        values.set_base(2000, 50.0);
        values.set_base(3000, 100.0);
        values.apply_damage(3000, 80.0);
        let mut limb = restoration(3000, 25.0);
        limb.medicine = scale;
        world
            .resource_mut::<InventoryCatalog>()
            .restorations
            .get_mut(&0x5678)
            .unwrap()
            .push(limb);
        assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            90.0
        );
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(3000),
            70.0
        );
        world
            .get_mut::<ActorValues>(player)
            .unwrap()
            .mod_permanent(2000, 50.0);
        assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            100.0
        );
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(3000),
            100.0
        );
        assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 0);
    }

    #[test]
    #[ignore = "requires installed Fallout 3 and New Vegas masters"]
    fn real_stimpaks_restore_scaled_health_and_limbs_but_hardcore_only_health() {
        use byroredux_core::character::Perks;
        use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead, TimedRestorations};
        use byroredux_core::ecs::resources::HardcoreMode;
        for (directory, master, nv) in [
            ("Fallout 3 goty", "Fallout3.esm", false),
            ("Fallout New Vegas", "FalloutNV.esm", true),
        ] {
            let path = format!("/mnt/data/SteamLibrary/steamapps/common/{directory}/Data/{master}");
            let index = byroredux_plugin::esm::parse_esm(&std::fs::read(path).unwrap()).unwrap();
            let health = index.health_actor_value_key().unwrap();
            let medicine = index.actor_value_form_id("Medicine").unwrap();
            let derived =
                byroredux_plugin::esm::records::derive_npc_actor_values(&index.npcs[&7], &index);
            let base = ActorValues::from_pairs(derived);
            assert!(
                base.get(medicine).is_some(),
                "player spawn must seed Medicine"
            );
            for name in byroredux_plugin::consumables::BODY_CONDITION_VALUES {
                assert_eq!(
                    base.current(index.actor_value_form_id(name).unwrap()),
                    100.0,
                    "{master} {name}"
                );
            }
            for hardcore in [false, true]
                .into_iter()
                .filter(|&hardcore| nv || !hardcore)
            {
                for perk in [false, true] {
                    let (mut world, player) = fixture();
                    world.register::<Dead>();
                    install_catalog(&mut world, &index);
                    world.insert_resource(HardcoreMode { enabled: hardcore });
                    let mut values = base.clone();
                    values.set_base(health, 1000.0);
                    values.apply_damage(health, 960.0);
                    values.set_base(medicine, 50.0);
                    for name in byroredux_plugin::consumables::BODY_CONDITION_VALUES {
                        values.apply_damage(index.actor_value_form_id(name).unwrap(), 80.0);
                    }
                    world.insert(player, values);
                    world.insert(player, ActorVitals { health });
                    world.insert(
                        player,
                        Inventory {
                            items: vec![ItemStack::new(0x15169, 1)],
                        },
                    );
                    let mut perks = Perks::default();
                    if perk {
                        perks.set_rank(0x94ebf, 1);
                    }
                    world.insert(player, perks);
                    assert!(snapshot(&world).unwrap().items[0].consumable);
                    assert_eq!(
                        apply_action(
                            &mut world,
                            byroredux_debug_ui::InventoryAction::Consume {
                                index: 0,
                                form_id: 0x15169
                            }
                        ),
                        MutationResult::Consumed
                    );
                    let amount = if perk { 72.0 } else { 60.0 };
                    assert_eq!(
                        world.get::<ActorValues>(player).unwrap().current(health),
                        if hardcore { 40.0 } else { 40.0 + amount }
                    );
                    if hardcore {
                        assert_eq!(
                            world
                                .get::<TimedRestorations>(player)
                                .unwrap()
                                .effects
                                .len(),
                            1
                        );
                        // The dose snapshots Medicine rather than changing as
                        // a skill buff expires partway through the duration.
                        world
                            .get_mut::<ActorValues>(player)
                            .unwrap()
                            .set_base(medicine, 0.0);
                        crate::systems::restoration::restoration_system(&world, 3.0);
                        assert_eq!(
                            world.get::<ActorValues>(player).unwrap().current(health),
                            40.0 + amount * 0.5
                        );
                        crate::systems::restoration::restoration_system(&world, 100.0);
                        assert_eq!(
                            world.get::<ActorValues>(player).unwrap().current(health),
                            40.0 + amount
                        );
                        assert!(world
                            .get::<TimedRestorations>(player)
                            .unwrap()
                            .effects
                            .is_empty());
                    }
                    for name in byroredux_plugin::consumables::BODY_CONDITION_VALUES {
                        let id = index.actor_value_form_id(name).unwrap();
                        assert_eq!(
                            world.get::<ActorValues>(player).unwrap().current(id),
                            if hardcore { 20.0 } else { 20.0 + amount },
                            "{master} {name}"
                        );
                    }
                    assert_eq!(world.get::<Inventory>(player).unwrap().items[0].count, 0);
                }
            }
        }
    }

    #[test]
    fn conditional_consumption_observes_perks_at_use_time() {
        use byroredux_core::character::Perks;
        use byroredux_core::ecs::components::ActorValues;
        use byroredux_plugin::esm::reader::SubRecord;
        use byroredux_plugin::esm::records::{parse_alch_for_game, AvifRecord, MgefRecord};
        let (mut world, player) = restorative_fixture();
        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index.actor_values.insert(
            1000,
            AvifRecord {
                form_id: 1000,
                editor_id: "AVHealth".into(),
                ..Default::default()
            },
        );
        index.magic_effects.insert(
            20,
            MgefRecord {
                instant_restoration_av: Some(24),
                ..Default::default()
            },
        );
        let mut subs = vec![SubRecord {
            sub_type: *b"ENIT",
            data: vec![0; 20],
        }];
        for (magnitude, comparand) in [(10f32, 0f32), (30.0, 1.0)] {
            let mut condition = vec![0; 32];
            condition[4..8].copy_from_slice(&comparand.to_le_bytes());
            condition[8..10].copy_from_slice(&448u16.to_le_bytes());
            condition[12..16].copy_from_slice(&77u32.to_le_bytes());
            condition[20..24].copy_from_slice(&1u32.to_le_bytes()); // target is consumer
            subs.extend([
                SubRecord {
                    sub_type: *b"EFID",
                    data: 20u32.to_le_bytes().to_vec(),
                },
                SubRecord {
                    sub_type: *b"EFIT",
                    data: [magnitude.to_le_bytes(), [0; 4], [0; 4]].concat(),
                },
                SubRecord {
                    sub_type: *b"CTDA",
                    data: condition,
                },
            ]);
        }
        index.items.insert(
            0x5678,
            parse_alch_for_game(0x5678, &subs, index.game, &None),
        );
        install_catalog(&mut world, &index);
        let action = byroredux_debug_ui::InventoryAction::Consume {
            index: 1,
            form_id: 0x5678,
        };
        assert!(
            snapshot(&world)
                .unwrap()
                .items
                .iter()
                .find(|i| i.index == 1)
                .unwrap()
                .consumable
        );
        assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            50.0
        );
        let mut perks = Perks::default();
        perks.set_rank(77, 1);
        world.insert(player, perks);
        // Same catalog; runtime ownership selects the other branch.
        assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            80.0
        );
        assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 0);
        assert_eq!(crate::notifications::drain(&world).len(), 2);

        // All-false conditions leave both inventory and health unchanged.
        world.get_mut::<Inventory>(player).unwrap().items[1].count = 1;
        world
            .resource_mut::<InventoryCatalog>()
            .restorations
            .get_mut(&0x5678)
            .unwrap()
            .truncate(1);
        assert_eq!(
            apply_action(&mut world, action),
            MutationResult::Unavailable
        );
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            80.0
        );
        assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 1);
        assert!(crate::notifications::drain(&world).is_empty());
    }

    #[test]
    fn consuming_restores_damage_decrements_once_and_keeps_equipment_indices() {
        use byroredux_core::ecs::components::ActorValues;
        let (mut world, player) = restorative_fixture();
        world
            .get_mut::<EquipmentSlots>(player)
            .unwrap()
            .equip_weapon(InventoryIndex(2));
        assert!(
            snapshot(&world)
                .unwrap()
                .items
                .iter()
                .find(|i| i.index == 1)
                .unwrap()
                .consumable
        );
        let action = byroredux_debug_ui::InventoryAction::Consume {
            index: 1,
            form_id: 0x5678,
        };
        for health in [65.0, 90.0] {
            assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
            assert_eq!(
                world.get::<ActorValues>(player).unwrap().current(1000),
                health
            );
        }
        assert_eq!(
            apply_action(&mut world, action),
            MutationResult::Unavailable
        );
        assert_eq!(world.get::<Inventory>(player).unwrap().items.len(), 3);
        assert_eq!(
            world.get::<EquipmentSlots>(player).unwrap().weapon,
            Some(InventoryIndex(2))
        );
        assert_eq!(crate::notifications::drain(&world).len(), 2);
    }

    #[test]
    fn unavailable_consumption_never_changes_health_or_count() {
        use byroredux_core::ecs::components::{ActorValues, Dead};
        for variant in 0..5 {
            let (mut world, player) = restorative_fixture();
            let mut form_id = 0x5678;
            match variant {
                0 => form_id = 0x1234, // stale UI identity
                1 => {
                    world.insert(player, Dead);
                }
                2 => {
                    world
                        .resource_mut::<InventoryCatalog>()
                        .restorations
                        .clear();
                }
                3 => {
                    world
                        .resource_mut::<InventoryCatalog>()
                        .restorations
                        .get_mut(&form_id)
                        .unwrap()
                        .push(restoration(9999, 10.0));
                }
                _ => {
                    world
                        .get_mut::<EquipmentSlots>(player)
                        .unwrap()
                        .equip_weapon(InventoryIndex(1));
                }
            }
            assert_eq!(
                apply_action(
                    &mut world,
                    byroredux_debug_ui::InventoryAction::Consume { index: 1, form_id }
                ),
                MutationResult::Unavailable
            );
            assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 2);
            assert_eq!(
                world.get::<ActorValues>(player).unwrap().current(1000),
                40.0
            );
            assert!(crate::notifications::drain(&world).is_empty());
        }
    }

    #[test]
    fn consuming_invalid_instance_is_atomic() {
        use byroredux_core::ecs::components::ActorValues;
        use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
        for missing_pool in [false, true] {
            for count in [1, 2] {
                let (mut world, player) = restorative_fixture();
                let mut pool = ItemInstancePool::new();
                let id = pool.allocate(ItemInstance::default());
                pool.release(id).unwrap();
                if !missing_pool {
                    world.insert_resource(pool);
                }
                {
                    let inventory = world.get_mut::<Inventory>(player).unwrap();
                    inventory.items[1].count = count;
                    inventory.items[1].instance = Some(id);
                }
                assert_eq!(
                    apply_action(
                        &mut world,
                        byroredux_debug_ui::InventoryAction::Consume {
                            index: 1,
                            form_id: 0x5678,
                        },
                    ),
                    MutationResult::Unavailable,
                    "missing_pool={missing_pool}, count={count}"
                );
                let inventory = world.get::<Inventory>(player).unwrap();
                assert_eq!(inventory.items[1].count, count);
                assert_eq!(inventory.items[1].instance, Some(id));
                drop(inventory);
                assert_eq!(
                    world.get::<ActorValues>(player).unwrap().current(1000),
                    40.0
                );
                assert!(crate::notifications::drain(&world).is_empty());
                if !missing_pool {
                    let mut pool = world.resource_mut::<ItemInstancePool>();
                    assert_eq!(pool.live_count(), 0);
                    assert_eq!(pool.allocate(ItemInstance::default()), id);
                    assert_ne!(pool.allocate(ItemInstance::default()), id);
                }
            }
        }
    }

    #[test]
    fn consuming_final_instance_releases_pool_and_never_overheals() {
        use byroredux_core::ecs::components::ActorValues;
        use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
        let (mut world, player) = restorative_fixture();
        let mut pool = ItemInstancePool::new();
        let id = pool.allocate(ItemInstance::default());
        world.insert_resource(pool);
        {
            let inventory = world.get_mut::<Inventory>(player).unwrap();
            inventory.items[1].count = 1;
            inventory.items[1].instance = Some(id);
        }
        world
            .resource_mut::<InventoryCatalog>()
            .restorations
            .insert(0x5678, vec![restoration(1000, 9999.0)]);
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::Consume {
                    index: 1,
                    form_id: 0x5678
                }
            ),
            MutationResult::Consumed
        );
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            100.0
        );
        assert_eq!(world.resource::<ItemInstancePool>().live_count(), 0);
        assert!(world.get::<Inventory>(player).unwrap().items[1]
            .instance
            .is_none());
    }

    #[test]
    #[ignore = "requires installed Skyrim SE master"]
    fn real_skyrim_healing_potion_runs_through_native_action() {
        use byroredux_core::ecs::components::ActorValues;
        let path = "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim.esm";
        let index =
            byroredux_plugin::esm::parse_esm(&std::fs::read(path).expect("Skyrim master required"))
                .unwrap();
        let (mut world, player) = restorative_fixture();
        install_catalog(&mut world, &index);
        world.get_mut::<Inventory>(player).unwrap().items[1] = ItemStack::new(0x3EADD, 1);
        assert_eq!(
            apply_action(
                &mut world,
                byroredux_debug_ui::InventoryAction::Consume {
                    index: 1,
                    form_id: 0x3EADD
                }
            ),
            MutationResult::Consumed
        );
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(1000),
            65.0
        );
        assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 0);
    }

    #[test]
    #[ignore = "requires installed Fallout 3 master"]
    fn real_fallout3_bloodpack_selects_live_perk_bonus() {
        use byroredux_core::character::Perks;
        use byroredux_core::ecs::components::{ActorValues, ActorVitals};
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm";
        let index = byroredux_plugin::esm::parse_esm(
            &std::fs::read(path).expect("Fallout 3 master required"),
        )
        .unwrap();
        assert_eq!(index.items[&0x34051].common.editor_id, "BloodPack");
        let health = index.health_actor_value_key().unwrap();
        let (mut world, player) = restorative_fixture();
        install_catalog(&mut world, &index);
        world.get_mut::<Inventory>(player).unwrap().items[1] = ItemStack::new(0x34051, 2);
        let mut values = ActorValues::from_pairs([(health, 100.0)]);
        values.apply_damage(health, 60.0);
        world.insert(player, values);
        world.insert(player, ActorVitals { health });
        let action = byroredux_debug_ui::InventoryAction::Consume {
            index: 1,
            form_id: 0x34051,
        };
        assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(health),
            41.0
        );
        let mut perks = Perks::default();
        perks.set_rank(0x3131, 1);
        world.insert(player, perks);
        assert_eq!(apply_action(&mut world, action), MutationResult::Consumed);
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(health),
            61.0
        );
        assert_eq!(world.get::<Inventory>(player).unwrap().items[1].count, 0);
        assert_eq!(crate::notifications::drain(&world).len(), 2);
    }

    fn fixture() -> (World, byroredux_core::ecs::EntityId) {
        let mut world = World::new();
        world.register::<Inventory>();
        world.register::<EquipmentSlots>();
        world.register::<EquippedWeapon>();
        world.register::<byroredux_scripting::EquipmentEventBatch>();
        let player = world.spawn();
        let mut inventory = Inventory::new();
        inventory.push(ItemStack::new(0x1234, 1));
        inventory.push(ItemStack::new(0x5678, 12));
        inventory.push(ItemStack::new(0x9ABC, 1));
        world.insert(player, inventory);
        world.insert(player, EquipmentSlots::new());
        world.insert_resource(PlayerEntity(Some(player)));
        world.insert_resource(InventoryCatalog {
            restorations: Default::default(),
            containers: Default::default(),
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

    fn activated_container(
        world: &mut World,
        player: byroredux_core::ecs::EntityId,
    ) -> byroredux_core::ecs::EntityId {
        world
            .resource_mut::<InventoryCatalog>()
            .containers
            .insert(0xCAFE);
        let container = world.spawn();
        world.insert(
            container,
            byroredux_scripting::SceneAliasCandidate {
                reference_form_id: 0xDEAD,
                base_form_id: 0xCAFE,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        world.insert(
            container,
            Inventory {
                items: vec![ItemStack::new(0x5678, 7)],
            },
        );
        world.insert(
            container,
            byroredux_scripting::ActivateEvent { activator: player },
        );
        container
    }

    #[test]
    fn container_loot_preserves_stacks_instances_equipment_and_activation() {
        let (mut world, player) = fixture();
        let container = activated_container(&mut world, player);
        world
            .get_mut::<EquipmentSlots>(player)
            .unwrap()
            .equip_weapon(InventoryIndex(2));
        let instance =
            byroredux_core::ecs::components::ItemInstanceId(std::num::NonZeroU32::new(17).unwrap());
        let stacks = vec![
            ItemStack::new(0x5678, u32::MAX),
            ItemStack {
                base_form_id: 0x9ABC,
                count: 1,
                instance: Some(instance),
            },
            ItemStack {
                base_form_id: 0x1111,
                count: 0,
                instance: Some(byroredux_core::ecs::components::ItemInstanceId(
                    std::num::NonZeroU32::new(18).unwrap(),
                )),
            },
        ];
        world.get_mut::<Inventory>(container).unwrap().items = stacks.clone();
        let original = world.get::<Inventory>(player).unwrap().items.clone();
        container_loot_system(&world, 0.0);
        container_loot_system(&world, 0.0);
        let inventory = world.get::<Inventory>(player).unwrap();
        assert_eq!(&inventory.items[..original.len()], original.as_slice());
        assert_eq!(&inventory.items[original.len()..], stacks.as_slice());
        drop(inventory);
        assert!(world.get::<Inventory>(container).unwrap().is_empty());
        assert_eq!(
            world.get::<EquipmentSlots>(player).unwrap().weapon,
            Some(InventoryIndex(2))
        );
        assert!(
            world
                .get::<byroredux_scripting::ActivateEvent>(container)
                .is_some(),
            "script consumers must still observe the activation"
        );
    }

    #[test]
    fn container_loot_rejects_locked_nonplayer_and_noncontainer_targets() {
        let (mut world, player) = fixture();
        let locked = activated_container(&mut world, player);
        world.insert(
            locked,
            byroredux_core::ecs::components::Locked {
                lock_level: 25,
                key_form_id: None,
            },
        );
        let npc = world.spawn();
        let npc_activation = activated_container(&mut world, npc);
        let actor_inventory = activated_container(&mut world, player);
        world
            .get_mut::<byroredux_scripting::SceneAliasCandidate>(actor_inventory)
            .unwrap()
            .base_form_id = 0xBEEF;
        container_loot_system(&world, 0.0);
        assert_eq!(world.get::<Inventory>(player).unwrap().len(), 3);
        for target in [locked, npc_activation, actor_inventory] {
            assert_eq!(
                world.get::<Inventory>(target).unwrap().items,
                vec![ItemStack::new(0x5678, 7)]
            );
        }
    }

    #[test]
    fn corpse_loot_clears_source_equipment_and_emits_each_unequip_once() {
        let (mut world, player) = fixture();
        let corpse = activated_container(&mut world, player);
        world.remove::<byroredux_scripting::SceneAliasCandidate>(corpse);
        world.insert(corpse, byroredux_core::ecs::components::Dead);
        world.get_mut::<Inventory>(corpse).unwrap().items =
            vec![ItemStack::new(0x1234, 1), ItemStack::new(0x9ABC, 1)];
        let mut slots = EquipmentSlots::new();
        slots.equip(0b110, InventoryIndex(0));
        slots.equip_weapon(InventoryIndex(1));
        world.insert(corpse, slots);
        world.insert(
            corpse,
            EquippedWeapon {
                inventory_index: InventoryIndex(1),
                base_form_id: 0x9ABC,
                damage: 12.0,
                reach: 1.0,
                speed: 1.0,
            },
        );
        container_loot_system(&world, 0.0);
        container_loot_system(&world, 0.0);
        assert!(world.get::<Inventory>(corpse).unwrap().is_empty());
        assert_eq!(world.get::<Inventory>(player).unwrap().len(), 5);
        assert_eq!(
            world
                .get::<EquipmentSlots>(corpse)
                .unwrap()
                .equipped_indices()
                .count(),
            0
        );
        assert!(world.get::<EquippedWeapon>(corpse).is_none());
        assert_eq!(
            world
                .get::<byroredux_scripting::EquipmentEventBatch>(corpse)
                .unwrap()
                .0,
            vec![
                byroredux_scripting::EquipmentChange {
                    item_form_id: 0x1234,
                    equipped: false
                },
                byroredux_scripting::EquipmentChange {
                    item_form_id: 0x9ABC,
                    equipped: false
                },
            ]
        );
        world.insert(player, byroredux_core::ecs::components::Dead);
        assert!(
            !is_loot_source(&world, player),
            "the player must not target their own inventory"
        );
    }

    #[test]
    fn container_loot_keeps_source_when_player_inventory_is_missing() {
        let (mut world, player) = fixture();
        let container = activated_container(&mut world, player);
        world.remove::<Inventory>(player);
        container_loot_system(&world, 0.0);
        assert_eq!(
            world.get::<Inventory>(container).unwrap().items,
            vec![ItemStack::new(0x5678, 7)]
        );
    }

    /// #3488 — the removal direction across a live load. The save overlay is
    /// additive-only, so a snapshot taken while unarmed carries no
    /// `EquippedWeapon` row at all; the live component on the surviving
    /// player body is never touched by `apply_deltas`. This reconstructs the
    /// post-overlay state — restored `EquipmentSlots.weapon == None`, live
    /// `EquippedWeapon` still standing — and pins that the reconciler clears
    /// it.
    #[test]
    fn load_reconciler_clears_a_weapon_the_save_did_not_have() {
        let (mut world, player) = fixture();

        // Live session: the player equips the Iron Sword (index 2).
        world
            .get_mut::<EquipmentSlots>(player)
            .unwrap()
            .equip_weapon(InventoryIndex(2));
        reconcile_player_equipped_weapon(&mut world, player);
        assert_eq!(
            world.get::<EquippedWeapon>(player).map(|w| w.base_form_id),
            Some(0x9ABC),
            "precondition: the live session is holding the sword"
        );

        // The overlay lands the saved (unarmed) `EquipmentSlots` — and
        // cannot remove the live `EquippedWeapon`, which is the whole bug.
        world.insert(player, EquipmentSlots::new());
        assert!(
            world.get::<EquippedWeapon>(player).is_some(),
            "apply_deltas is additive-only: the stale weapon is still here"
        );

        reconcile_player_equipped_weapon(&mut world, player);
        assert!(
            world.get::<EquippedWeapon>(player).is_none(),
            "the load reconciler must clear a weapon the save did not have — \
             otherwise melee keeps dealing the live session's weapon damage \
             instead of the saved unarmed damage (#3488)"
        );
    }

    /// The other direction, so the reconciler cannot pass by removing
    /// unconditionally: a save taken while armed must re-derive the weapon
    /// onto a body that is currently unarmed.
    #[test]
    fn load_reconciler_restores_a_weapon_the_save_did_have() {
        let (mut world, player) = fixture();
        world
            .get_mut::<EquipmentSlots>(player)
            .unwrap()
            .equip_weapon(InventoryIndex(2));

        reconcile_player_equipped_weapon(&mut world, player);
        let weapon = world
            .get::<EquippedWeapon>(player)
            .expect("armed slots must re-derive the component");
        assert_eq!(weapon.base_form_id, 0x9ABC);
        assert_eq!(weapon.inventory_index, InventoryIndex(2));
        assert_eq!(weapon.damage, 12.0);
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
        let metadata = world
            .resource::<InventoryCatalog>()
            .sdk_metadata(0x1234)
            .unwrap();
        assert_eq!(metadata.name(), "Iron Armor");
        assert_eq!(metadata.category(), ItemCategory::Armor);
        assert_eq!((metadata.value(), metadata.weight()), (125, 30.0));
    }

    #[test]
    fn scroll_catalog_exposes_metadata_without_claiming_cast_support() {
        let mut index = EsmIndex::default();
        let item = byroredux_plugin::esm::records::parse_scrl(0x1234, &[], &None);
        index.items.insert(0x1234, item);
        let mut world = World::new();
        install_catalog(&mut world, &index);
        let catalog = world.resource::<InventoryCatalog>();
        let definition = &catalog.entries[&0x1234];
        assert_eq!(definition.category, "Scroll");
        assert!(definition.equip_target.is_none());
        assert!(definition.details.contains("casting unavailable"));
        assert_eq!(
            catalog.sdk_metadata(0x1234).unwrap().category(),
            ItemCategory::Scroll
        );
    }

    #[test]
    fn carried_light_catalog_does_not_claim_equipping_support() {
        let mut index = EsmIndex::default();
        let mut data = vec![0; 48];
        data[12] = 2;
        let item = byroredux_plugin::esm::records::parse_carryable_light(
            0x1234,
            &[byroredux_plugin::esm::reader::SubRecord {
                sub_type: *b"DATA",
                data,
            }],
            GameKind::Skyrim,
            &None,
        )
        .unwrap();
        index.items.insert(0x1234, item);
        let mut world = World::new();
        install_catalog(&mut world, &index);
        let catalog = world.resource::<InventoryCatalog>();
        assert!(catalog.entries[&0x1234].equip_target.is_none());
        assert!(catalog.entries[&0x1234]
            .details
            .contains("equipping unavailable"));
        assert_eq!(
            catalog.sdk_metadata(0x1234).unwrap().category(),
            ItemCategory::Light
        );
    }

    #[test]
    fn apparatus_catalog_exposes_metadata_without_claiming_crafting() {
        let mut index = EsmIndex::default();
        let item =
            byroredux_plugin::esm::records::parse_appa(0x1234, &[], GameKind::Oblivion, &None)
                .unwrap();
        index.items.insert(0x1234, item);
        let mut world = World::new();
        install_catalog(&mut world, &index);
        let catalog = world.resource::<InventoryCatalog>();
        assert!(catalog.entries[&0x1234].equip_target.is_none());
        assert!(catalog.entries[&0x1234]
            .details
            .contains("crafting unavailable"));
        assert_eq!(
            catalog.sdk_metadata(0x1234).unwrap().category(),
            ItemCategory::Apparatus
        );
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
        assert_eq!(
            world
                .get::<byroredux_scripting::EquipmentEventBatch>(player)
                .unwrap()
                .0,
            vec![
                byroredux_scripting::EquipmentChange {
                    item_form_id: 0x1234,
                    equipped: true,
                },
                byroredux_scripting::EquipmentChange {
                    item_form_id: 0x1234,
                    equipped: false,
                },
                byroredux_scripting::EquipmentChange {
                    item_form_id: 0x9ABC,
                    equipped: true,
                },
                byroredux_scripting::EquipmentChange {
                    item_form_id: 0x9ABC,
                    equipped: false,
                },
            ]
        );
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
                female_model_path: String::new(),
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
                    female_model_path: String::new(),
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

    /// #4458 — the player's CHARAL seed derives from the base Player
    /// `NPC_` record through the same population path every NPC takes.
    /// Pre-fix, `consume_item` / `apply_player_drowning_damage` gated on
    /// the player carrying `ActorValues` + `ActorVitals` while no
    /// production path stamped either, so every consumable use silently
    /// returned `Unavailable`; the tests that covered those paths
    /// hand-inserted the components. The seed must be a POPULATED
    /// derivation (the scene.rs #3158 note's zero-SPECIAL warning is why
    /// an empty stub was never acceptable), and it must degrade to `None`
    /// — the pre-fix state — when the index has nothing to derive from.
    #[test]
    fn player_character_template_derives_from_the_player_npc_record() {
        use byroredux_core::character::CharacterRulesProfile;
        use byroredux_core::ecs::components::ActorVitals;
        use byroredux_plugin::esm::records::{AvifRecord, ClassRecord, NpcRecord};

        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT_NEW_VEGAS,
            game: GameKind::Fallout3NV,
            ..EsmIndex::default()
        };
        for (fid, name) in [
            (0x100u32, "AVStrength"),
            (0x101, "AVPerception"),
            (0x102, "AVEndurance"),
            (0x103, "AVCharisma"),
            (0x104, "AVIntelligence"),
            (0x105, "AVAgility"),
            (0x106, "AVLuck"),
            (0x2C9, "AVHealth"),
            (0x2D1, "AVActionPoints"),
        ] {
            index.actor_values.insert(
                fid,
                AvifRecord {
                    form_id: fid,
                    editor_id: name.to_owned(),
                    ..Default::default()
                },
            );
        }
        index.classes.insert(
            0x2000,
            ClassRecord {
                form_id: 0x2000,
                base_attributes: [5, 6, 5, 4, 7, 6, 5],
                ..Default::default()
            },
        );
        index.npcs.insert(
            PLAYER_NPC_FORM_ID,
            NpcRecord {
                class_form_id: 0x2000,
                level: 1,
                ..Default::default()
            },
        );

        let template = build_player_character_template(&index);
        let values = template.values.expect("a populated derivation");
        let health = index.health_actor_value_key().expect("Health AVIF");
        assert_eq!(
            values.current(health),
            200.0,
            "FNV Health 95 + 20·END(5) + 5·L(1), the NPC curve the player \
             record's class derives"
        );
        assert_eq!(values.current(index.actor_value_form_id("Luck").unwrap()), 5.0);
        // #4674 — AP is PlayerOnly and was never seeded pre-fix (read 0.0);
        // the player formula 65 + 3·AGI(6) now applies at stamping.
        assert_eq!(
            values.current(index.actor_value_form_id("AVActionPoints").unwrap()),
            83.0,
            "FNV player AP = 65 + 3·AGI(6), evaluated from the PlayerOnly row"
        );
        assert_eq!(template.vitals, Some(ActorVitals { health }));

        // Degradation: no Player NPC_ in the index → no seed, never an
        // empty zero-SPECIAL component.
        let empty = build_player_character_template(&EsmIndex::default());
        assert!(empty.values.is_none());
        assert_eq!(empty.vitals, None);
    }

    /// #4458 — the production path end to end: `install_catalog` builds
    /// the seed resource, `attach_to_player` stamps both components onto
    /// the body. This is the leg every pre-fix test faked with
    /// hand-inserted `ActorValues`/`ActorVitals`.
    #[test]
    fn attach_to_player_stamps_the_character_seed() {
        use byroredux_core::character::CharacterRulesProfile;
        use byroredux_core::ecs::components::{ActorValues, ActorVitals};
        use byroredux_plugin::esm::records::{AvifRecord, ClassRecord, NpcRecord};

        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT_NEW_VEGAS,
            game: GameKind::Fallout3NV,
            ..EsmIndex::default()
        };
        index.actor_values.insert(
            0x2C9,
            AvifRecord {
                form_id: 0x2C9,
                editor_id: "AVHealth".to_owned(),
                ..Default::default()
            },
        );
        index.classes.insert(
            0x2000,
            ClassRecord {
                form_id: 0x2000,
                base_attributes: [5, 5, 5, 5, 5, 5, 5],
                ..Default::default()
            },
        );
        index.npcs.insert(
            PLAYER_NPC_FORM_ID,
            NpcRecord {
                class_form_id: 0x2000,
                race_form_id: 0x0007,
                level: 1,
                ..Default::default()
            },
        );

        let mut world = World::new();
        install_catalog(&mut world, &index);
        let player = world.spawn();
        attach_to_player(&mut world, player);

        let values = world
            .get::<ActorValues>(player)
            .expect("the player body must carry its derived actor values");
        assert_eq!(
            values.current(index.health_actor_value_key().unwrap()),
            200.0,
            "95 + 20·5 + 5·1"
        );
        assert_eq!(
            world.get::<ActorVitals>(player).map(|v| *v),
            Some(ActorVitals {
                health: index.health_actor_value_key().unwrap()
            })
        );
        // #4678 — level + Background ride the same attach, off the same
        // Player record the derivation consumed: GetLevel reads 1 (not 0),
        // GetXPForNextLevel computes for L1 (not L0), and the race/class
        // provenance is the record the values actually came from.
        assert_eq!(
            world.get::<byroredux_core::character::CharacterLevel>(player).map(|l| l.level),
            Some(1)
        );
        assert_eq!(
            world.get::<byroredux_core::character::Background>(player).map(|b| *b),
            Some(byroredux_core::character::Background {
                race_form_id: 0x0007,
                class_form_id: 0x2000,
            })
        );
    }

    #[test]
    #[ignore = "needs FNV game data on disk; parses the whole master (~850 MB resident)"]
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

    #[test]
    fn vitals_snapshot_composes_layers_and_skips_absent_values() {
        let mut world = World::new();
        world.register::<byroredux_core::ecs::components::ActorValues>();
        let player = world.spawn();
        world.insert_resource(PlayerEntity(Some(player)));
        world.insert_resource(PlayerVitals {
            bars: vec![("Health", 1000), ("Magicka", 1001), ("Fatigue", 1002)],
            by_editor_id: FxHashMap::from_iter([
                ("Health", 1000),
                ("Magicka", 1001),
                ("Fatigue", 1002),
            ]),
        });
        let mut values = byroredux_core::ecs::components::ActorValues::new();
        values.set_base(1000, 100.0);
        values.apply_damage(1000, 35.0);
        values.set_base(1001, 50.0);
        values.mod_temporary(1001, 20.0);
        // 1002 (Fatigue) carries no entry at all — the absent-AV contract.
        world.insert(player, values);

        let bars = vitals_snapshot(&world).expect("a populated player must compose bars");
        assert_eq!(
            bars,
            vec![
                byroredux_debug_ui::VitalBarView {
                    label: "Health",
                    current: 65.0,
                    max: 100.0
                },
                byroredux_debug_ui::VitalBarView {
                    label: "Magicka",
                    current: 70.0,
                    max: 70.0
                },
            ],
            "current subtracts the damage layer; absent values drop their bar"
        );
    }

    #[test]
    fn vitals_snapshot_without_a_player_or_bars_is_none() {
        let mut world = World::new();
        assert!(vitals_snapshot(&world).is_none(), "no player resource");

        world.insert_resource(PlayerEntity(None));
        world.insert_resource(PlayerVitals::default());
        let player = world.spawn();
        world.insert_resource(PlayerEntity(Some(player)));
        assert!(
            vitals_snapshot(&world).is_none(),
            "no resolvable bars → the HUD draws nothing"
        );
    }

    #[test]
    fn install_catalog_resolves_per_game_vital_keys() {
        use byroredux_core::character::CharacterRulesProfile;
        use byroredux_plugin::esm::records::AvifRecord;
        let avif = |form_id: u32, editor_id: &str| AvifRecord {
            form_id,
            editor_id: editor_id.to_owned(),
            ..Default::default()
        };
        // #4679 — the roster comes from the CHARACTER RULES PROFILE now,
        // not the GameKind match, so each fixture names its profile.
        let build = |game: GameKind, profile: CharacterRulesProfile| {
            let mut index = EsmIndex {
                game,
                character_rules: profile,
                ..Default::default()
            };
            index.actor_values.insert(0x3E8, avif(0x3E8, "AVHealth"));
            index.actor_values.insert(0x3E9, avif(0x3E9, "AVMagicka"));
            index.actor_values.insert(0x3EA, avif(0x3EA, "Fatigue"));
            index.actor_values.insert(0x3EB, avif(0x3EB, "ActionPoints"));
            index
        };

        let mut world = World::new();
        install_catalog(
            &mut world,
            &build(GameKind::Oblivion, CharacterRulesProfile::OBLIVION),
        );
        let labels: Vec<_> = world
            .try_resource::<PlayerVitals>()
            .unwrap()
            .bars
            .iter()
            .map(|&(label, _)| label)
            .collect();
        assert_eq!(labels, vec!["Health", "Magicka", "Fatigue"]);

        let mut world = World::new();
        install_catalog(
            &mut world,
            &build(GameKind::Fallout3NV, CharacterRulesProfile::FALLOUT_NEW_VEGAS),
        );
        let labels: Vec<_> = world
            .try_resource::<PlayerVitals>()
            .unwrap()
            .bars
            .iter()
            .map(|&(label, _)| label)
            .collect();
        assert_eq!(
            labels,
            vec!["HP", "AP"],
            "FO3/FNV vocabulary, and the Magicka/Fatigue AVIFs this game does not use drop out"
        );
    }

    /// #4674 (CHAR-2026-09-21-D4-01) — the real-master leg. Reads the
    /// actual Player `NPC_` 0x7 records: FO4's PRPS authors Health 40 /
    /// ActionPoints 0 and DNAM pushes calc_health 150 / calc_ap 100 after
    /// them, so the pre-fix stamp (NPC answer verbatim) left the player at
    /// Health 150 / AP 100 — 1.76×/1.43× off the capture's player formulas
    /// (85 / 70). FNV's ActionPoints was never seeded at all (read 0.0;
    /// the capture gives 80 at AGI 5). Run with
    /// `cargo test -p byroredux --bin byroredux
    ///  real_master_player_seed -- --ignored`.
    #[test]
    #[ignore = "requires installed Fallout 4 and New Vegas masters"]
    fn real_master_player_seed_evaluates_the_player_only_rows() {
        let fo4 = "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/Fallout4.esm";
        let index = byroredux_plugin::esm::parse_esm(&std::fs::read(fo4).unwrap()).unwrap();
        let template = build_player_character_template(&index);
        let values = template.values.expect("FO4 player seed");
        let health = index.actor_value_form_id("Health").expect("Health AVIF");
        let ap = index.actor_value_form_id("ActionPoints").expect("AP AVIF");
        assert_eq!(
            values.current(health),
            85.0,
            "FO4 player Health must come from the capture's player formula, \
             not the NPC-baked 150 the PRPS/DNAM push order produced"
        );
        assert_eq!(
            values.current(ap),
            70.0,
            "FO4 player AP must come from the capture's player formula, \
             not the NPC-baked 100"
        );

        let fnv =
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm";
        let index = byroredux_plugin::esm::parse_esm(&std::fs::read(fnv).unwrap()).unwrap();
        let template = build_player_character_template(&index);
        let values = template.values.expect("FNV player seed");
        let ap = index.actor_value_form_id("ActionPoints").expect("AP AVIF");
        assert_eq!(
            values.current(ap),
            80.0,
            "FNV player AP = 65 + 3·AGI(5) — pre-#4674 it was never seeded"
        );
    }

    /// #4675 — the HUD's bar keys resolve through `PlayerVitals` (real
    /// FNV AVIF ids `AVHealth 0x450` / `AVActionPoints 0x44C` here, not
    /// the fallout.rs test-fixture 0x2C9/0x2D0 the drivers used to
    /// hardcode), and `fraction` reads the PLAYER only: an NPC inserted
    /// first — whose values the old first-hit storage scan would have
    /// reported — must not move the bar.
    #[test]
    fn hud_bars_resolve_through_player_vitals_and_read_the_player_only() {
        use crate::hud::fraction;
        use byroredux_core::character::CharacterRulesProfile;
        use byroredux_core::ecs::components::ActorValues;
        use byroredux_plugin::esm::records::AvifRecord;

        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT_NEW_VEGAS,
            game: GameKind::Fallout3NV,
            ..EsmIndex::default()
        };
        for (fid, name) in [(0x450u32, "AVHealth"), (0x44C, "AVActionPoints")] {
            index.actor_values.insert(
                fid,
                AvifRecord {
                    form_id: fid,
                    editor_id: name.to_owned(),
                    ..Default::default()
                },
            );
        }
        let vitals = build_player_vitals(&index);
        assert_eq!(vitals.resolved("Health"), Some(0x450));
        assert_eq!(vitals.resolved("ActionPoints"), Some(0x44C));
        assert_eq!(vitals.resolved("Magicka"), None, "FNV authors no Magicka");

        let mut world = World::new();
        world.register::<ActorValues>();
        // An NPC carrying Health, inserted FIRST — the pre-#4675 scan
        // would have taken this dense-slot-first entry.
        let npc = world.spawn();
        let mut npc_values = ActorValues::new();
        npc_values.set_base(0x450, 10.0);
        world.insert(npc, npc_values);
        // The player, at half health.
        let player = world.spawn();
        let mut player_values = ActorValues::new();
        player_values.set_base(0x450, 100.0);
        player_values.apply_damage(0x450, 50.0);
        player_values.set_base(0x44C, 80.0);
        world.insert(player, player_values);
        world.insert_resource(PlayerEntity(Some(player)));
        world.insert_resource(vitals);

        let health = index.actor_value_form_id("Health").unwrap();
        assert_eq!(
            fraction(&world, Some(health), None),
            0.5,
            "the bar must read the player's 50/100, not the first-scanned \
             NPC's 10/10"
        );
        // Unpinned absent key → full bar; pins still win.
        assert_eq!(fraction(&world, Some(0x999), None), 1.0);
        assert_eq!(fraction(&world, Some(health), Some(0.3)), 0.3);
    }
}
