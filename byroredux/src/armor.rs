//! Skyrim armor rating and physical damage reduction (M43).
//!
//! UESP *Skyrim:Armor* ("Armor Rating", "Armor Cap") gives the approximate formulas this follows:
//!
//! * `item rating = CEILING[(base rating + quality bonus) × (1 + 0.4 × armor skill / 100)]` — the **quality
//!   bonus** is what improving adds ([`crate::smithing`]), the skill is Light Armor or Heavy Armor by the
//!   piece's armor type, and NPCs use a skill coefficient of `1.5` instead of `0.4`;
//! * `displayed rating = Σ item ratings` (a shield is one more piece);
//! * `damage reduction % = displayed rating × 0.12 + 3 × pieces worn`, the second term being the hidden armor
//!   rating of 25 a worn piece carries (clothing and robes get none), and the whole is capped at **80 %**;
//! * it reduces *physical* damage only, not magical.
//!
//! Every hit the damage pipeline resolves from a weapon, projectile or explosion is physical, so the reduction
//! applies to all of them; Skyrim only — Fallout uses Damage Threshold / Resistance, which are their own models.
//! The numbers (0.12 % per point, the hidden 3 %, the 80 % cap, the player / NPC skill multipliers) are the
//! [`ArmorModel`] a game's `CharacterRulesProfile` row carries, overlaid with the load order's own `fArmor*` /
//! `fMaxArmorRating` settings (#6567): this module never tests the game.
//! Not modelled: the perk multipliers on a piece (Custom Fit / Well Fitted, Matching Set, Agile Defender /
//! Juggernaut) and "armor effects" (Oakflesh and kin), which the page lists as further terms.

use byroredux_core::character::ArmorModel;
use byroredux_core::ecs::components::{EquipmentSlots, Inventory};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use std::collections::HashMap;
use std::sync::Arc;

/// The `BOD2` biped bit of the body (cuirass) slot — the "chest armor" column of the quality table.
const BODY_SLOT_BIT: u32 = 0x4;

/// Which skill governs a piece (`BOD2` armor type, xEdit `wbArmorTypeEnum`: 0 light, 1 heavy, 2 clothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArmorKind {
    Light,
    Heavy,
    Clothing,
}

/// One armor record's base rating and type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ArmorPiece {
    pub rating: f32,
    pub kind: ArmorKind,
    /// The piece occupies the body slot (the cuirass column for improvement bonuses).
    pub chest: bool,
}

/// Every armor of the load order, when the game uses armor rating. `NOT_SAVED_BY_DESIGN`: rebuilt from the
/// records every boot.
#[derive(Debug, Default, Clone)]
pub(crate) struct ArmorBook {
    pieces: Arc<HashMap<u32, ArmorPiece>>,
}

impl Resource for ArmorBook {}

/// Install (or clear) the [`ArmorModel`] and the [`ArmorBook`] for a freshly loaded index. The profile's
/// `armor` row decides whether the game has an armor-rating model at all, the load order's own settings retune
/// it (#6567); with no row the model resource is removed and the book is empty, so [`totals`] is zero.
pub(crate) fn install_armor_book(world: &mut World, index: &EsmIndex) -> usize {
    let model = index.character_rules.armor().map(|model| model.with_gmst(|name| index.game_setting_float(name)));
    match model {
        Some(model) => {
            world.insert_resource(model);
        }
        None => {
            world.remove_resource::<ArmorModel>();
        }
    }
    let pieces: HashMap<u32, ArmorPiece> = if model.is_some() {
        index
            .items
            .values()
            .filter_map(|item| match &item.kind {
                ItemKind::Armor { armor_rating_x100, armor_type, biped_flags, .. } => Some((
                    item.form_id,
                    ArmorPiece {
                        rating: *armor_rating_x100 as f32 / 100.0,
                        kind: match armor_type {
                            Some(0) => ArmorKind::Light,
                            Some(1) => ArmorKind::Heavy,
                            _ => ArmorKind::Clothing,
                        },
                        chest: biped_flags & BODY_SLOT_BIT != 0,
                    },
                )),
                _ => None,
            })
            .collect()
    } else {
        HashMap::new()
    };
    let count = pieces.len();
    world.insert_resource(ArmorBook { pieces: Arc::new(pieces) });
    count
}

/// What a wearer's equipment adds up to.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct ArmorTotals {
    pub displayed_rating: f32,
    pub pieces: usize,
    pub reduction_percent: f32,
}

fn armor_skill(world: &World, wearer: EntityId, kind: ArmorKind) -> f32 {
    let name = match kind {
        ArmorKind::Light => "LightArmor",
        ArmorKind::Heavy => "HeavyArmor",
        ArmorKind::Clothing => return 0.0,
    };
    let Some(skill) = world.try_resource::<byroredux_scripting::ActorValueNames>().and_then(|names| names.resolve(name)) else {
        return 0.0;
    };
    world.get::<byroredux_core::ecs::components::ActorValues>(wearer).map_or(0.0, |values| values.current(skill))
}

/// The displayed armor rating and damage reduction of everything `wearer` has equipped; zero for a wearer with
/// no equipment, or in a game that does not use armor rating.
pub(crate) fn totals(world: &World, wearer: EntityId) -> ArmorTotals {
    let Some(book) = world.try_resource::<ArmorBook>().map(|book| book.pieces.clone()) else {
        return ArmorTotals::default();
    };
    let Some(model) = world.try_resource::<ArmorModel>().map(|model| *model) else {
        return ArmorTotals::default();
    };
    if book.is_empty() {
        return ArmorTotals::default();
    }
    let (Some(slots), Some(inventory)) = (
        world.get::<EquipmentSlots>(wearer).map(|slots| (*slots).clone()),
        world.get::<Inventory>(wearer).map(|inventory| (*inventory).clone()),
    ) else {
        return ArmorTotals::default();
    };
    let player = crate::systems::PlayerEntity::get(world) == Some(wearer);
    // A piece covering several slots occupies several; it is one piece.
    let mut rows: Vec<u32> = slots.occupants.iter().flatten().map(|index| index.0).collect();
    rows.sort_unstable();
    rows.dedup();
    let mut totals = ArmorTotals::default();
    for row in rows {
        let Some(stack) = inventory.items.get(row as usize) else { continue };
        let Some(piece) = book.get(&stack.base_form_id) else { continue };
        let bonus = crate::smithing::armor_bonus(crate::smithing::stack_quality(world, stack), piece.chest);
        totals.displayed_rating += model.item_rating(piece.rating, bonus, armor_skill(world, wearer, piece.kind), player);
        // Clothing and robes carry no hidden rating, so they add no piece.
        if piece.kind != ArmorKind::Clothing {
            totals.pieces += 1;
        }
    }
    totals.reduction_percent = model.reduction_percent(totals.displayed_rating, totals.pieces);
    totals
}

/// `damage` after `target`'s armor.
pub(crate) fn mitigate(world: &World, target: EntityId, damage: f32) -> f32 {
    let reduction = totals(world, target).reduction_percent;
    if reduction <= 0.0 {
        return damage;
    }
    damage * (1.0 - reduction / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #6568 — the Skyrim defaults read CHARAL's skill-scaling constants instead of restating them: UESP's `0.4`
    /// and `1.5` come out of `0.004` / `0.015`, and the player's one is the very constant the Light Armor
    /// `DamageResist` row is built from. (The formula itself is pinned in `byroredux_core::character::armor`.)
    #[test]
    fn the_skill_multipliers_are_charals() {
        let model = ArmorModel::SKYRIM;
        let coeff = byroredux_core::character::skyrim::LIGHT_ARMOR_RATING_COEFF;
        assert!((model.skill_multiplier(true, 100.0) - (1.0 + coeff * 100.0)).abs() < 1e-6);
        assert!((model.skill_multiplier(false, 100.0) - 2.5).abs() < 1e-6);
    }

    const CUIRASS: u32 = 0x3BA0F;
    const HEAVY_ARMOR: u32 = 0x0000_0F4F;

    /// A wearer in a heavy cuirass (base 49, body slot) at Heavy Armor `skill`: the rating scales with skill, the
    /// improved copy's quality bonus joins the base, the hidden 3 % per piece applies, and the reduction takes its
    /// share of a hit.
    fn wearer(skill: f32, quality: u8, player: bool) -> (World, EntityId) {
        wearer_with_scaling(skill, quality, player, 0.12)
    }

    fn armor_gmst(name: &str, value: f32) -> byroredux_plugin::esm::records::GameSetting {
        byroredux_plugin::esm::records::GameSetting {
            form_id: 0x1000 + name.len() as u32,
            editor_id: name.into(),
            value: byroredux_plugin::esm::records::SettingValue::Float(value),
        }
    }

    /// `wearer` with the load order authoring `fArmorScalingFactor = scaling_factor`.
    fn wearer_with_scaling(skill: f32, quality: u8, player: bool, scaling_factor: f32) -> (World, EntityId) {
        use byroredux_core::ecs::components::{ActorValues, InventoryIndex, ItemStack};
        use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<Inventory>();
        world.register::<EquipmentSlots>();
        world.register::<ActorValues>();
        let mut index = EsmIndex {
            game: byroredux_plugin::esm::reader::GameKind::Skyrim,
            character_rules: byroredux_core::character::CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
        index.insert_game_setting(armor_gmst("fArmorScalingFactor", scaling_factor));
        index.actor_values.insert(
            HEAVY_ARMOR,
            byroredux_plugin::esm::records::AvifRecord { form_id: HEAVY_ARMOR, editor_id: "AVHeavyArmor".into(), ..Default::default() },
        );
        index.items.insert(
            CUIRASS,
            byroredux_plugin::esm::records::ItemRecord {
                form_id: CUIRASS,
                common: Default::default(),
                kind: ItemKind::Armor {
                    biped_flags: BODY_SLOT_BIT,
                    dt: 0.0,
                    dr: 0,
                    health: 0,
                    slot_mask: 0,
                    armor_rating_x100: 4900,
                    armor_type: Some(1),
                    female_model_path: String::new(),
                    armatures: Vec::new(),
                },
            },
        );
        byroredux_scripting::install_actor_value_names(&mut world, &index);
        install_armor_book(&mut world, &index);
        let mut pool = ItemInstancePool::new();
        let instance = (quality > 0).then(|| pool.allocate(ItemInstance { quality }));
        world.insert_resource(pool);
        let entity = world.spawn();
        let mut inventory = Inventory::new();
        inventory.push(ItemStack { base_form_id: CUIRASS, count: 1, instance });
        world.insert(entity, inventory);
        let mut slots = EquipmentSlots::new();
        slots.occupants[2] = Some(InventoryIndex(0));
        world.insert(entity, slots);
        world.insert(entity, ActorValues::from_pairs([(HEAVY_ARMOR, skill)]));
        world.insert_resource(crate::systems::PlayerEntity(player.then_some(entity)));
        (world, entity)
    }

    #[test]
    fn a_worn_improved_cuirass_reduces_a_hit_by_the_sourced_amount() {
        // Unimproved, 100 Heavy Armor, player: rating ceil(49 × 1.4) = 69; reduction 69 × 0.12 + 3 = 11.28 %.
        let (world, wearer_entity) = wearer(100.0, 0, true);
        let plain = totals(&world, wearer_entity);
        assert_eq!((plain.displayed_rating, plain.pieces), (69.0, 1));
        assert!((plain.reduction_percent - 11.28).abs() < 1e-3, "got {}", plain.reduction_percent);
        assert!((mitigate(&world, wearer_entity, 100.0) - 88.72).abs() < 1e-2);

        // Exquisite (quality 3, chest bonus +10): ceil((49 + 10) × 1.4) = 83 → 83 × 0.12 + 3 = 12.96 %.
        let (world, improved) = wearer(100.0, 3, true);
        assert_eq!(totals(&world, improved).displayed_rating, 83.0);
        assert!((mitigate(&world, improved, 100.0) - 87.04).abs() < 1e-2, "improving makes the armor protect more");

        // An NPC uses the 1.5 coefficient: ceil(49 × (1 + 1.5 × 0.5)) = 86 at skill 50.
        let (world, npc) = wearer(50.0, 0, false);
        assert_eq!(totals(&world, npc).displayed_rating, 86.0);
    }

    #[test]
    fn nothing_worn_or_another_game_means_no_reduction() {
        let (mut world, _) = wearer(100.0, 0, true);
        let stranger = world.spawn();
        assert_eq!(mitigate(&world, stranger, 100.0), 100.0);
    }

    /// #6567 — the model is the load order's, not a compiled-in constant: an armor-rebalance mod that retunes
    /// `fArmorScalingFactor` changes the damage a worn cuirass blocks. 69 displayed × 0.24 + 3 = 19.56 %.
    #[test]
    fn a_retuned_scaling_factor_reaches_the_damage_pipeline() {
        let (world, wearer_entity) = wearer_with_scaling(100.0, 0, true, 0.24);
        assert!((totals(&world, wearer_entity).reduction_percent - 19.56).abs() < 1e-3);
        assert!((mitigate(&world, wearer_entity, 100.0) - 80.44).abs() < 1e-2);
        let (default_world, default_wearer) = wearer(100.0, 0, true);
        assert!(mitigate(&default_world, default_wearer, 100.0) > mitigate(&world, wearer_entity, 100.0));
    }

    /// #6567 — whether the model applies is the profile row, not a game test in the consumer: a profile with no
    /// `armor` row installs no model and an empty book, whatever the index's `game` says, and a later
    /// install over a model-less index clears the previous game's resource.
    #[test]
    fn no_armor_row_means_no_model_and_no_book() {
        let (mut world, wearer_entity) = wearer(100.0, 0, true);
        assert!(world.try_resource::<ArmorModel>().is_some());
        let index = EsmIndex {
            game: byroredux_plugin::esm::reader::GameKind::Skyrim,
            character_rules: byroredux_core::character::CharacterRulesProfile::FALLOUT3,
            ..Default::default()
        };
        assert_eq!(install_armor_book(&mut world, &index), 0, "no armor row, no pieces — even on a Skyrim-tagged index");
        assert!(world.try_resource::<ArmorModel>().is_none(), "the previous load's model is cleared");
        assert_eq!(totals(&world, wearer_entity), ArmorTotals::default());
    }

    /// #6567 — the consumer carries no hand-written game test or armor constants of its own.
    #[test]
    fn the_consumer_holds_no_game_gate_or_armor_constants() {
        let production = byroredux_core::source_scan::production_text(include_str!("armor.rs"));
        assert!(!production.contains("GameKind::"), "the profile row decides, not a game test");
        for needle in ["REDUCTION_PER_RATING", "HIDDEN_PER_PIECE", "MAX_REDUCTION", "SKILL_COEFF_"] {
            assert!(!production.contains(needle), "{needle} belongs to ArmorModel");
        }
    }
}
