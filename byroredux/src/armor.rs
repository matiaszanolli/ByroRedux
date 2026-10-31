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
//! Not modelled: the perk multipliers on a piece (Custom Fit / Well Fitted, Matching Set, Agile Defender /
//! Juggernaut) and "armor effects" (Oakflesh and kin), which the page lists as further terms.

use byroredux_core::ecs::components::{EquipmentSlots, Inventory};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use std::collections::HashMap;
use std::sync::Arc;

/// Per-point damage reduction (UESP: "each point of armor rating reduces damage by 0.12 %").
const REDUCTION_PER_RATING: f32 = 0.12;
/// The hidden reduction a worn piece adds, in percent (a hidden armor rating of 25 × 0.12).
const HIDDEN_PER_PIECE: f32 = 3.0;
/// The armor cap, in percent.
const MAX_REDUCTION: f32 = 80.0;
/// The armor-skill coefficient for the player and for NPCs.
const SKILL_COEFF_PLAYER: f32 = 0.4;
const SKILL_COEFF_NPC: f32 = 1.5;
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

pub(crate) fn install_armor_book(world: &mut World, index: &EsmIndex) -> usize {
    // Skyrim only: the other games' armor models are not this one.
    let pieces: HashMap<u32, ArmorPiece> = if index.game == byroredux_plugin::esm::reader::GameKind::Skyrim {
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

/// UESP's per-item formula: `CEILING[(base + quality bonus) × (1 + coeff × skill / 100)]`.
pub(crate) fn item_rating(base: f32, quality_bonus: f32, skill: f32, coeff: f32) -> f32 {
    ((base + quality_bonus) * (1.0 + coeff * skill / 100.0)).ceil()
}

/// The damage-reduction percentage for a displayed rating over `pieces` armor pieces worn.
pub(crate) fn reduction_percent(displayed_rating: f32, pieces: usize) -> f32 {
    (displayed_rating * REDUCTION_PER_RATING + HIDDEN_PER_PIECE * pieces as f32).clamp(0.0, MAX_REDUCTION)
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
    if book.is_empty() {
        return ArmorTotals::default();
    }
    let (Some(slots), Some(inventory)) = (
        world.get::<EquipmentSlots>(wearer).map(|slots| (*slots).clone()),
        world.get::<Inventory>(wearer).map(|inventory| (*inventory).clone()),
    ) else {
        return ArmorTotals::default();
    };
    let coeff = if crate::systems::PlayerEntity::get(world) == Some(wearer) { SKILL_COEFF_PLAYER } else { SKILL_COEFF_NPC };
    // A piece covering several slots occupies several; it is one piece.
    let mut rows: Vec<u32> = slots.occupants.iter().flatten().map(|index| index.0).collect();
    rows.sort_unstable();
    rows.dedup();
    let mut totals = ArmorTotals::default();
    for row in rows {
        let Some(stack) = inventory.items.get(row as usize) else { continue };
        let Some(piece) = book.get(&stack.base_form_id) else { continue };
        let quality = stack
            .instance
            .and_then(|instance| world.try_resource::<byroredux_core::ecs::resources::ItemInstancePool>()?.get(instance).map(|inst| inst.quality))
            .unwrap_or(0);
        let bonus = crate::smithing::armor_bonus(quality, piece.chest);
        totals.displayed_rating += item_rating(piece.rating, bonus, armor_skill(world, wearer, piece.kind), coeff);
        // Clothing and robes carry no hidden rating, so they add no piece.
        if piece.kind != ArmorKind::Clothing {
            totals.pieces += 1;
        }
    }
    totals.reduction_percent = reduction_percent(totals.displayed_rating, totals.pieces);
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

    /// UESP's worked figures: the reduction is 0.12 % per displayed point plus 3 % per piece, capped at 80 %, and
    /// "you'll reach the armor cap at 567 displayed armor rating when wearing all four pieces" — 567 × 0.12 + 12 =
    /// 80.04.
    #[test]
    fn the_reduction_follows_the_uesp_formula_and_cap() {
        assert!((reduction_percent(100.0, 4) - 24.0).abs() < 1e-4, "100 × 0.12 + 4 × 3");
        assert_eq!(reduction_percent(0.0, 0), 0.0);
        assert_eq!(reduction_percent(567.0, 4), 80.0, "the cap, reached at 567 with four pieces");
        assert_eq!(reduction_percent(5000.0, 4), 80.0);
        assert!(reduction_percent(566.0, 4) < 80.0);
    }

    /// The item formula: `CEILING[(base + quality) × (1 + 0.4 × skill/100)]`. Daedric cuirass 49 at 100 Heavy Armor
    /// is `49 × 1.4 = 68.6` → 69; an NPC's `1.5` coefficient at 50 skill gives `49 × 1.75 = 85.75` → 86; the
    /// quality bonus joins the base before the skill scales it.
    #[test]
    fn an_items_rating_scales_with_skill_and_includes_the_quality_bonus() {
        assert_eq!(item_rating(49.0, 0.0, 100.0, 0.4), 69.0);
        assert_eq!(item_rating(49.0, 0.0, 50.0, 1.5), 86.0);
        assert_eq!(item_rating(49.0, 20.0, 100.0, 0.4), 97.0, "(49 + 20) × 1.4 = 96.6");
        assert_eq!(item_rating(0.0, 0.0, 100.0, 0.4), 0.0);
    }

    const CUIRASS: u32 = 0x3BA0F;
    const HEAVY_ARMOR: u32 = 0x0000_0F4F;

    /// A wearer in a heavy cuirass (base 49, body slot) at Heavy Armor `skill`: the rating scales with skill, the
    /// improved copy's quality bonus joins the base, the hidden 3 % per piece applies, and the reduction takes its
    /// share of a hit.
    fn wearer(skill: f32, quality: u8, player: bool) -> (World, EntityId) {
        use byroredux_core::ecs::components::{ActorValues, InventoryIndex, ItemStack};
        use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<Inventory>();
        world.register::<EquipmentSlots>();
        world.register::<ActorValues>();
        let mut index = EsmIndex { game: byroredux_plugin::esm::reader::GameKind::Skyrim, ..Default::default() };
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
}
