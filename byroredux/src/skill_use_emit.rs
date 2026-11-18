//! Oblivion skill-use emitters at the hit site (#7051 Phase 2a; PRD `oblivion-skill-use-leveling`
//! OSL-USE-002 and OSL-USE-003; design `docs/engine/skill-use.md` §2).
//!
//! `combat_damage_system` resolves every `HitEvent` once, so this is the one place that knows a blow landed
//! on a living target. From it:
//!
//! * a **melee blow the player lands** feeds the skill of the equipped weapon (Blade for a blade, Blunt for a
//!   blunt weapon, Hand to Hand unarmed), usage 0: UESP's "each hit on a target";
//! * a **hit the player takes** feeds Light or Heavy Armor by the piece struck (`Oblivion.exe` `0x5e5a00`:
//!   a roll of 0 to 99 over the slots' `iArmorDamage*Chance` weights, re-rolled up to seven times while the
//!   struck slot is empty), usage 0.
//!
//! Emitters only queue ([`byroredux_scripting::oblivion_skill_use::queue_skill_use`]); nothing here touches
//! the player's actor values. A weapon the engine cannot feed (a staff casts, a bow has no shot path here)
//! feeds nothing, and a blocked hit feeds nothing: no producer models a target's own guard (#4324).

use byroredux_core::character::{ArmorHitSlot, ARMOR_HIT_ATTEMPTS};
use byroredux_core::ecs::components::{EquipmentSlots, EquippedWeapon, Inventory};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::ItemKind;

/// Oblivion skill actor-value codes this module feeds.
const BLADE: u32 = 14;
const BLUNT: u32 = 16;
const HAND_TO_HAND: u32 = 17;
const HEAVY_ARMOR: u32 = 18;
const LIGHT_ARMOR: u32 = 27;

/// The skill a melee weapon of Oblivion `WEAP` type `weapon_type` feeds (0 and 1 blades, 2 and 3 blunt
/// weapons; 4, a staff, casts instead, and 5, a bow, has no melee blow), or Hand to Hand for no weapon.
#[must_use]
pub(crate) fn weapon_skill(weapon_type: Option<u8>) -> Option<u32> {
    match weapon_type {
        None => Some(HAND_TO_HAND),
        Some(0 | 1) => Some(BLADE),
        Some(2 | 3) => Some(BLUNT),
        Some(_) => None,
    }
}

/// splitmix64: a deterministic roll source for the armor-piece pick, seeded by the hit.
fn mix(mut seed: u64) -> u64 {
    seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The skill (Light or Heavy Armor) of the piece a hit strikes on `wearer`, or `None` when seven rolls
/// find every struck slot empty. `seed` makes the rolls deterministic per hit.
#[must_use]
pub(crate) fn struck_armor_skill(world: &World, wearer: EntityId, seed: u64) -> Option<u32> {
    let chances = ArmorHitSlot::ROLLED.map(|(_, name, default)| {
        byroredux_scripting::obscript_state::game_setting(world, name).map_or(default, |value| value.max(0.0) as u32)
    });
    let slots = world.get::<EquipmentSlots>(wearer).map(|slots| (*slots).clone())?;
    let inventory = world.get::<Inventory>(wearer).map(|inventory| (*inventory).clone())?;
    let index = world.try_resource::<crate::components::LoadedCellIndex>()?;
    for attempt in 0..ARMOR_HIT_ATTEMPTS {
        let roll = (mix(seed.wrapping_add(attempt as u64)) % 100) as u32;
        let slot = ArmorHitSlot::from_roll(chances, roll);
        let mask = slot.biped_mask();
        let piece = (0..16)
            .filter(|bit| mask & (1 << bit) != 0)
            .find_map(|bit| slots.occupants[bit as usize])
            .and_then(|row| inventory.items.get(row.0 as usize))
            .and_then(|stack| index.0.items.get(&stack.base_form_id));
        if let Some(ItemKind::Armor { armor_type, .. }) = piece.map(|item| &item.kind) {
            return Some(if *armor_type == Some(1) { HEAVY_ARMOR } else { LIGHT_ARMOR });
        }
    }
    None
}

/// The Oblivion `WEAP` type of the weapon `aggressor` wields: `Some(None)` unarmed, `Some(Some(type))` for a
/// weapon, `None` when the equipped item is not a known weapon.
fn wielded_type(world: &World, aggressor: EntityId) -> Option<Option<u8>> {
    let Some(weapon) = world.get::<EquippedWeapon>(aggressor).map(|weapon| weapon.base_form_id) else {
        return Some(None);
    };
    let index = world.try_resource::<crate::components::LoadedCellIndex>()?;
    match &index.0.items.get(&weapon)?.kind {
        ItemKind::Weapon { oblivion_type, .. } => Some(Some(*oblivion_type)),
        _ => None,
    }
}

/// One `HitEvent` that landed on the living `target`: queue the skill uses it earns the player.
/// `seed` varies the armor-piece roll from hit to hit.
pub(crate) fn on_landed_hit(world: &World, aggressor: EntityId, target: EntityId, blocked: bool, seed: u64) {
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    // A blocked blow never landed; a block is the aggressor's own stance here (#4324).
    if blocked || aggressor == target {
        return;
    }
    if aggressor == player {
        let skill = wielded_type(world, player).and_then(weapon_skill);
        if let Some(skill) = skill {
            byroredux_scripting::oblivion_skill_use::queue_skill_use(world, skill, 0, 0.0);
        }
    } else if target == player
        && let Some(skill) = struck_armor_skill(world, player, seed)
    {
        byroredux_scripting::oblivion_skill_use::queue_skill_use(world, skill, 0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UESP's weapon pages: one-handed and two-handed blades feed Blade, blunts feed Blunt, the unarmed blow
    /// Hand to Hand; a staff and a bow feed no melee skill.
    #[test]
    fn a_weapon_type_names_its_skill() {
        assert_eq!(weapon_skill(None), Some(HAND_TO_HAND));
        assert_eq!(weapon_skill(Some(0)), Some(BLADE));
        assert_eq!(weapon_skill(Some(1)), Some(BLADE));
        assert_eq!(weapon_skill(Some(2)), Some(BLUNT));
        assert_eq!(weapon_skill(Some(3)), Some(BLUNT));
        assert_eq!(weapon_skill(Some(4)), None, "a staff casts");
        assert_eq!(weapon_skill(Some(5)), None, "no bow shot path");
    }

    /// The roll source is deterministic and spreads over 0..100.
    #[test]
    fn the_roll_is_deterministic_and_spread() {
        assert_eq!(mix(7), mix(7));
        assert_ne!(mix(7), mix(8));
        let mut seen = [false; 10];
        for seed in 0..200u64 {
            seen[(mix(seed) % 100 / 10) as usize] = true;
        }
        assert!(seen.iter().all(|&decile| decile), "{seen:?}");
    }

    use byroredux_core::character::{CharacterRuleset, LevelingModel};
    use byroredux_core::ecs::components::{InventoryIndex, ItemStack};
    use byroredux_plugin::esm::records::{EsmIndex, ItemRecord};
    use byroredux_scripting::oblivion_skill_use::{PendingSkillUse, SkillUseRecord};

    const BLADE_WEAPON: u32 = 0x100;
    const STAFF: u32 = 0x101;
    const CUIRASS: u32 = 0x200;
    const LIGHT_BOOTS: u32 = 0x201;

    fn weapon(form_id: u32, oblivion_type: u8) -> ItemRecord {
        let mut kind = ItemKind::weapon_stub(10, 0);
        if let ItemKind::Weapon { oblivion_type: slot, .. } = &mut kind {
            *slot = oblivion_type;
        }
        ItemRecord { form_id, common: Default::default(), kind }
    }

    fn armor(form_id: u32, biped_flags: u64, heavy: bool) -> ItemRecord {
        ItemRecord {
            form_id,
            common: Default::default(),
            kind: ItemKind::Armor {
                biped_flags,
                dt: 0.0,
                dr: 0,
                health: 0,
                slot_mask: 0,
                armor_rating_x100: 0,
                armor_type: Some(u32::from(heavy)),
                female_model_path: String::new(),
                armatures: Vec::new(),
            },
        }
    }

    /// A world with Oblivion's ruleset, a player wearing a heavy cuirass (Upper Body) and light boots (Foot),
    /// and one blade and one staff to wield.
    fn world() -> (World, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<Inventory>();
        world.register::<EquipmentSlots>();
        world.register::<EquippedWeapon>();
        world.insert_resource(CharacterRuleset::new(LevelingModel::OBLIVION));
        world.insert_resource(PendingSkillUse::default());
        let mut index = EsmIndex::default();
        for record in [weapon(BLADE_WEAPON, 1), weapon(STAFF, 4), armor(CUIRASS, 1 << 2, true), armor(LIGHT_BOOTS, 1 << 5, false)] {
            index.items.insert(record.form_id, record);
        }
        world.insert_resource(crate::components::LoadedCellIndex(std::sync::Arc::new(index)));
        let player = world.spawn();
        let npc = world.spawn();
        let mut inventory = Inventory::new();
        inventory.push(ItemStack { base_form_id: CUIRASS, count: 1, instance: None });
        inventory.push(ItemStack { base_form_id: LIGHT_BOOTS, count: 1, instance: None });
        world.insert(player, inventory);
        let mut slots = EquipmentSlots::new();
        slots.occupants[2] = Some(InventoryIndex(0));
        slots.occupants[5] = Some(InventoryIndex(1));
        world.insert(player, slots);
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        (world, player, npc)
    }

    fn queued(world: &World) -> Vec<u32> {
        world.resource::<PendingSkillUse>().0.iter().map(|SkillUseRecord { skill, .. }| *skill).collect()
    }

    fn wield(world: &mut World, player: EntityId, base_form_id: u32) {
        let weapon = EquippedWeapon { inventory_index: InventoryIndex(0), base_form_id, damage: 10.0, reach: 0.0, speed: 0.0 };
        world.insert(player, weapon);
    }

    /// OSL-USE-002: the player's landed blow feeds the wielded weapon's skill (once, usage 0), Hand to Hand
    /// when unarmed; a blocked blow, a staff, and a blow on oneself feed nothing.
    #[test]
    fn a_landed_melee_blow_feeds_the_weapons_skill() {
        let (mut world, player, npc) = world();
        on_landed_hit(&world, player, npc, false, 1);
        assert_eq!(queued(&world), vec![HAND_TO_HAND], "unarmed");
        wield(&mut world, player, BLADE_WEAPON);
        on_landed_hit(&world, player, npc, false, 2);
        assert_eq!(queued(&world), vec![HAND_TO_HAND, BLADE]);
        on_landed_hit(&world, player, npc, true, 3);
        on_landed_hit(&world, player, player, false, 4);
        wield(&mut world, player, STAFF);
        on_landed_hit(&world, player, npc, false, 5);
        assert_eq!(queued(&world).len(), 2, "blocked, self-inflicted and staff blows feed nothing");
        // The amount is the SKIL value of the action: Blade 0.5, Hand to Hand 0.6.
        let amounts: Vec<f32> = world.resource::<PendingSkillUse>().0.iter().map(|r| r.amount).collect();
        assert_eq!(amounts, vec![0.6, 0.5]);
    }

    /// OSL-USE-003: a hit on the player feeds the struck piece's skill: the heavy cuirass 25 of 100 rolls,
    /// the light boots 10, and the shield, helmet, greaves and gauntlets slots (all empty here) re-roll,
    /// so across many hits Heavy Armor outnumbers Light Armor and some hits strike nothing. An NPC's own
    /// armor, and a blow the player lands, feed no armor skill.
    #[test]
    fn a_hit_the_player_takes_feeds_the_struck_pieces_skill() {
        let (world, player, npc) = world();
        for seed in 0..400 {
            on_landed_hit(&world, npc, player, false, seed);
        }
        let heavy = queued(&world).iter().filter(|&&skill| skill == HEAVY_ARMOR).count();
        let light = queued(&world).iter().filter(|&&skill| skill == LIGHT_ARMOR).count();
        assert_eq!(heavy + light, queued(&world).len(), "only armor skills");
        assert!(heavy > light && light > 0, "heavy {heavy}, light {light}");
        assert!(heavy + light < 400, "an empty slot re-rolls, but seven empty rolls strike nothing");
        // A blocked hit never landed.
        let before = queued(&world).len();
        for seed in 0..50 {
            on_landed_hit(&world, npc, player, true, seed);
        }
        assert_eq!(queued(&world).len(), before);
    }

    /// A ruleset that does not level by skill use queues nothing, so no other game feeds Oblivion's skills.
    #[test]
    fn another_ruleset_queues_nothing() {
        let (mut world, player, npc) = world();
        world.insert_resource(CharacterRuleset::new(LevelingModel::SKYRIM));
        on_landed_hit(&world, player, npc, false, 1);
        on_landed_hit(&world, npc, player, false, 1);
        assert!(queued(&world).is_empty());
    }
}
