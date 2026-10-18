//! Actor values by their ObScript argument code (M47.3).
//!
//! `GetAV Speech`, `player.ModAV Strength 1`, `ForceAV Fatigue 0` write the actor
//! value as a small integer, not as a form: the GECK *Actor Value Codes* page
//! lists Fallout 3 / New Vegas's enum (0 `Aggression` .. 76 `DamageThreshold`).
//! The engine's `ActorValues` are keyed by AVIF FormID, so the code is
//! translated to the AVIF whose editor ID names it, through
//! [`crate::ActorValueNames`].
//!
//! The table was checked against the vanilla masters rather than taken from the
//! page alone: `obscript_tools avcodes <ESM>` pairs every compiled `GetAV` /
//! `SetAV` / `ModAV` / `ForceAV` / `DamageAV` / `RestoreAV` argument with the
//! word in its `SCTX` source line, and all 120-odd (command, code) pairs agree
//! with the page (`Guns` 41, `SpeedMult` 21, `Assistance` 57, `Variable01` 62,
//! `RadiationRads` 54, ...).
//!
//! Where the AVIF editor ID is not the page's name the table says so: `Guns` is
//! `AVSmallGuns`, `Survival` (44) is `AVThrowing`, `Turbo` (51) is
//! `AVDetectLifeRange`, and `SleepDeprivation` is spelled `AVSleepDeprevation`
//! in the master. A code whose value has no AVIF record in the master
//! (`SpeedMult`, the resistances `ElectricResist` / `FrostResist` / `EmpResist`,
//! `Variable02`..`Variable10`) resolves to nothing, and the command is declined.
//!
//! Oblivion has no `AVIF` records (its actor values are hard-coded) and Skyrim
//! does not run ObScript, so only the Fallout pair translates.

use byroredux_core::ecs::World;

use crate::ActorValueNames;

/// The editor ID (without the `AV` prefix, any case) of the AVIF each code names
/// on Fallout 3 / New Vegas. `None` entries are codes the page does not number.
const FALLOUT_CODES: [&str; 77] = [
    "Aggression",           // 0
    "Confidence",           // 1
    "Energy",               // 2
    "Responsibility",       // 3
    "Mood",                 // 4
    "Strength",             // 5
    "Perception",           // 6
    "Endurance",            // 7
    "Charisma",             // 8
    "Intelligence",         // 9
    "Agility",              // 10
    "Luck",                 // 11
    "ActionPoints",         // 12
    "CarryWeight",          // 13
    "CritChance",           // 14
    "HealRate",             // 15
    "Health",               // 16
    "MeleeDamage",          // 17
    "DamageResist",         // 18
    "PoisonResist",         // 19
    "RadResist",            // 20
    "SpeedMult",            // 21
    "Fatigue",              // 22
    "Karma",                // 23
    "XP",                   // 24
    "PerceptionCondition",  // 25
    "EnduranceCondition",   // 26
    "LeftAttackCondition",  // 27
    "RightAttackCondition", // 28
    "LeftMobilityCondition", // 29
    "RightMobilityCondition", // 30
    "BrainCondition",       // 31
    "Barter",               // 32
    "BigGuns",              // 33
    "EnergyWeapons",        // 34
    "Explosives",           // 35
    "Lockpick",             // 36
    "Medicine",             // 37
    "MeleeWeapons",         // 38
    "Repair",               // 39
    "Science",              // 40
    "SmallGuns",            // 41 (the page's "Guns")
    "Sneak",                // 42
    "Speech",               // 43
    "Throwing",             // 44 (the page's "Survival")
    "Unarmed",              // 45
    "InventoryWeight",      // 46
    "Paralysis",            // 47
    "Invisibility",         // 48
    "Chameleon",            // 49
    "NightEye",             // 50
    "DetectLifeRange",      // 51 (the page's "Turbo")
    "FireResist",           // 52
    "WaterBreathing",       // 53
    "RadiationRads",        // 54
    "BloodyMess",           // 55
    "UnarmedDamage",        // 56
    "Assistance",           // 57
    "ElectricResist",       // 58
    "FrostResist",          // 59
    "EnergyResist",         // 60
    "EmpResist",            // 61
    "Variable01",           // 62
    "Variable02",           // 63
    "Variable03",           // 64
    "Variable04",           // 65
    "Variable05",           // 66
    "Variable06",           // 67
    "Variable07",           // 68
    "Variable08",           // 69
    "Variable09",           // 70
    "Variable10",           // 71
    "IgnoreCrippledLimbs",  // 72
    "Dehydration",          // 73
    "Hunger",               // 74
    "SleepDeprevation",     // 75 (the page's "SleepDeprivation")
    "DamageThreshold",      // 76
];

/// The AVIF FormID the ObScript actor-value `code` names in the loaded game, or
/// `None` when the game does not number actor values this way (Oblivion), the
/// code is outside the table, or the master has no AVIF of that name.
pub fn actor_value_form(world: &World, code: f32) -> Option<u32> {
    if !crate::ScriptGame::is_fallout3_nv(world) || code < 0.0 || code.fract() != 0.0 {
        return None;
    }
    let name = FALLOUT_CODES.get(code as usize)?;
    world.try_resource::<ActorValueNames>()?.resolve(name)
}

/// Clear Health and the limb-condition values' damage on `actor` (`ResetHealth`, and the health a
/// resurrected actor gets back). `false` when the load order names none of them.
pub fn reset_health(world: &World, actor: byroredux_core::ecs::storage::EntityId) -> bool {
    let vitals: Vec<u32> = RESET_HEALTH_CODES.iter().filter_map(|&code| actor_value_form(world, code as f32)).collect();
    if vitals.is_empty() {
        return false;
    }
    crate::fragment::clear_actor_value_damage(world, actor, &vitals);
    true
}

/// The vitals that crippling damage and `ResetHealth` clear: Health and the six
/// limb-condition values (`PerceptionCondition` is the head ... `BrainCondition`).
pub const RESET_HEALTH_CODES: [usize; 8] = [16, 25, 26, 27, 28, 29, 30, 31];

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{AvifRecord, EsmIndex};

    fn world_with(game: GameKind, avifs: &[(u32, &str)]) -> World {
        let mut index = EsmIndex::default();
        index.game = game;
        for &(form_id, editor_id) in avifs {
            index.actor_values.insert(
                form_id,
                AvifRecord { form_id, editor_id: editor_id.into(), ..Default::default() },
            );
        }
        let mut world = World::new();
        crate::install_actor_value_names(&mut world, &index);
        world
    }

    #[test]
    fn a_code_names_the_avif_whose_editor_id_spells_it() {
        let world = world_with(
            GameKind::Fallout3NV,
            &[(0x3E8, "AVStrength"), (0x4BB, "AVSpeech"), (0x4B9, "AVSmallGuns"), (0x450, "AVHealth")],
        );
        assert_eq!(actor_value_form(&world, 5.0), Some(0x3E8));
        assert_eq!(actor_value_form(&world, 43.0), Some(0x4BB));
        // The page's "Guns" is the AVIF `AVSmallGuns`.
        assert_eq!(actor_value_form(&world, 41.0), Some(0x4B9));
        assert_eq!(actor_value_form(&world, 16.0), Some(0x450));
    }

    #[test]
    fn a_code_with_no_avif_in_the_master_is_not_invented() {
        let world = world_with(GameKind::Fallout3NV, &[(0x3E8, "AVStrength")]);
        // `SpeedMult` has no AVIF in FalloutNV.esm.
        assert_eq!(actor_value_form(&world, 21.0), None);
        // Past the table, and not an integer.
        assert_eq!(actor_value_form(&world, 77.0), None);
        assert_eq!(actor_value_form(&world, 5.5), None);
        assert_eq!(actor_value_form(&world, -1.0), None);
    }

    #[test]
    fn only_the_fallout_pair_numbers_actor_values_this_way() {
        let world = world_with(GameKind::Oblivion, &[(0x3E8, "AVStrength")]);
        assert_eq!(actor_value_form(&world, 5.0), None);
    }
}
