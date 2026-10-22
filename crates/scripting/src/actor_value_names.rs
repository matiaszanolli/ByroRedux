//! Actor values by their Papyrus names (M47.2).
//!
//! Skyrim's Papyrus names an actor value by string (`SetAV("Variable06", 3)`)
//! where Fallout 4 passes the `ActorValue` form. Skyrim's `AVIF` editor IDs
//! are the Papyrus name with an `AV` prefix (`AVVariable06`, `AVHealth`), so
//! the table maps both spellings, case-insensitively, to the AVIF FormID. A
//! name whose AVIF uses a legacy editor ID (Illusion is `AVMysticism`) does
//! not resolve, and the effect skips. `NOT_SAVED_BY_DESIGN`: rebuilt from the
//! load order every boot.

use std::collections::HashSet;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

#[derive(Debug, Default, Clone)]
pub struct ActorValueNames(crate::name_table::EditorIdTable<u32>, HashSet<u32>);

impl Resource for ActorValueNames {}

impl ActorValueNames {
    pub fn from_index(index: &EsmIndex) -> Self {
        let mut names = crate::name_table::EditorIdTable::default();
        let mut forms = HashSet::new();
        for avif in index.actor_values.values() {
            forms.insert(avif.form_id);
            // The Papyrus spelling: the editor ID less its `AV` prefix, yielding to a record's own name.
            let editor_id = avif.editor_id.as_str();
            if editor_id.get(..2).is_some_and(|prefix| prefix.eq_ignore_ascii_case("av")) {
                names.insert_if_absent(&editor_id[2..], avif.form_id);
            }
            names.insert(editor_id, avif.form_id);
        }
        Self(names, forms)
    }

    /// Whether `form` is a loaded `AVIF` record.
    pub fn contains_form(&self, form: u32) -> bool {
        self.1.contains(&form)
    }

    /// The AVIF FormID `name` spells, if any.
    pub fn resolve(&self, name: &str) -> Option<u32> {
        self.0.get(name).copied()
    }
}

/// The game the load order belongs to, for the Papyrus constants whose numbers
/// differ per game (`SetMotionType`'s). `NOT_SAVED_BY_DESIGN`: set from the load
/// order at install.
#[derive(Debug, Clone, Copy)]
pub struct ScriptGame(pub byroredux_plugin::esm::reader::GameKind);

impl Resource for ScriptGame {}

impl ScriptGame {
    /// Whether the loaded game is Fallout 3 / New Vegas. False when no load
    /// order has been installed: an unknown game is never assumed to be one.
    pub fn is_fallout3_nv(world: &World) -> bool {
        world.try_resource::<ScriptGame>().is_some_and(|game| {
            matches!(game.0, byroredux_plugin::esm::reader::GameKind::Fallout3NV)
        })
    }
}

/// Skyrim's actor-value enum (`wbActorValue`, the UESP *Actor Value Indices*
/// table) to the vanilla `AVIF` FormID each index names. `None`: the table lists
/// no record for the index. An authored `GetActorValue` (CTDA 14) stores one of
/// these indices, not a FormID (#5989). Transcribed mechanically from the
/// page's FormID column; whether a record exists is still checked against the
/// loaded `AVIF`s (a `*`-marked FormID no longer appears in the game data).
const SKYRIM_ACTOR_VALUE_FORMS: [Option<u32>; 164] = [
    Some(0x4B0), // 0 Aggression
    Some(0x4B1), // 1 Confidence
    Some(0x4B2), // 2 Energy
    Some(0x4B3), // 3 Morality
    Some(0x4B4), // 4 Mood
    Some(0x4B5), // 5 Assistance
    Some(0x44C), // 6 OneHanded
    Some(0x44D), // 7 TwoHanded
    Some(0x44E), // 8 Marksman
    Some(0x44F), // 9 Block
    Some(0x450), // 10 Smithing
    Some(0x451), // 11 HeavyArmor
    Some(0x452), // 12 LightArmor
    Some(0x453), // 13 Pickpocket
    Some(0x454), // 14 Lockpicking
    Some(0x455), // 15 Sneak
    Some(0x456), // 16 Alchemy
    Some(0x457), // 17 Speechcraft
    Some(0x458), // 18 Alteration
    Some(0x459), // 19 Conjuration
    Some(0x45A), // 20 Destruction
    Some(0x45B), // 21 Illusion
    Some(0x45C), // 22 Restoration
    Some(0x45D), // 23 Enchanting
    Some(0x3E8), // 24 Health
    Some(0x3E9), // 25 Magicka
    Some(0x3EA), // 26 Stamina
    Some(0x3EB), // 27 HealRate
    Some(0x3EC), // 28 MagickaRate
    Some(0x3ED), // 29 StaminaRate
    Some(0x3EE), // 30 SpeedMult
    Some(0x3EF), // 31 InventoryWeight
    Some(0x3F0), // 32 CarryWeight
    Some(0x3F1), // 33 CritChance
    Some(0x3F2), // 34 MeleeDamage
    Some(0x3F3), // 35 UnarmedDamage
    Some(0x3F4), // 36 Mass
    Some(0x3F5), // 37 VoicePoints
    Some(0x3F6), // 38 VoiceRate
    Some(0x5CE), // 39 DamageResist
    Some(0x5CF), // 40 PoisonResist
    Some(0x5D0), // 41 FireResist
    Some(0x5D1), // 42 ElectricResist
    Some(0x5D2), // 43 FrostResist
    Some(0x5D3), // 44 MagicResist
    Some(0x5D4), // 45 DiseaseResist
    Some(0x5D5), // 46 PerceptionCondition
    Some(0x5D6), // 47 EnduranceCondition
    Some(0x5D7), // 48 LeftAttackCondition
    Some(0x5D8), // 49 RightAttackCondition
    Some(0x5D9), // 50 LeftMobilityCondition
    Some(0x5DA), // 51 RightMobilityCondition
    Some(0x5DB), // 52 BrainCondition
    Some(0x5DC), // 53 Paralysis
    Some(0x5DD), // 54 Invisibility
    Some(0x5DE), // 55 NightEye
    Some(0x5DF), // 56 DetectLifeRange
    Some(0x5E0), // 57 WaterBreathing
    Some(0x5E1), // 58 WaterWalking
    Some(0x5E2), // 59 IgnoreCrippledLimbs
    Some(0x5E3), // 60 Fame
    Some(0x5E4), // 61 Infamy
    Some(0x5E5), // 62 JumpingBonus
    Some(0x5E6), // 63 WardPower
    Some(0x5E7), // 64 RightItemCharge/EquippedItemCharge
    Some(0x5E8), // 65 ArmorPerks
    Some(0x5E9), // 66 ShieldPerks
    Some(0x5EA), // 67 WardDeflection
    Some(0x5EB), // 68 Variable01
    Some(0x5EC), // 69 Variable02
    Some(0x5ED), // 70 Variable03
    Some(0x5EE), // 71 Variable04
    Some(0x5EF), // 72 Variable05
    Some(0x5F0), // 73 Variable06
    Some(0x5F1), // 74 Variable07
    Some(0x5F2), // 75 Variable08
    Some(0x5F3), // 76 Variable09
    Some(0x5F4), // 77 Variable10
    Some(0x5F5), // 78 BowSpeedBonus
    Some(0x5F6), // 79 FavorActive
    Some(0x5F7), // 80 FavorsPerDay
    Some(0x5F8), // 81 FavorsPerDayTimer
    Some(0x5F9), // 82 LeftItemCharge/EquippedStaffCharge
    Some(0x5FA), // 83 AbsorbChance
    Some(0x5FB), // 84 Blindness
    Some(0x5FC), // 85 WeaponSpeedMult
    Some(0x5FD), // 86 ShoutRecoveryMult
    Some(0x5FE), // 87 BowStaggerBonus
    Some(0x5FF), // 88 Telekinesis
    Some(0x600), // 89 FavorPointsBonus
    Some(0x601), // 90 LastBribedIntimidated
    Some(0x602), // 91 LastFlattered
    Some(0x603), // 92 MovementNoiseMult
    Some(0x604), // 93 BypassVendorStolenCheck
    Some(0x605), // 94 BypassVendorKeywordCheck
    Some(0x606), // 95 WaitingForPlayer
    Some(0x607), // 96 OneHandedMod
    Some(0x608), // 97 TwoHandedMod
    Some(0x609), // 98 MarksmanMod
    Some(0x60A), // 99 BlockMod
    Some(0x60B), // 100 SmithingMod
    Some(0x60C), // 101 HeavyArmorMod
    Some(0x60D), // 102 LightArmorMod
    Some(0x60E), // 103 PickPocketMod
    Some(0x60F), // 104 LockpickingMod
    Some(0x610), // 105 SneakMod
    Some(0x611), // 106 AlchemyMod
    Some(0x612), // 107 SpeechcraftMod
    Some(0x613), // 108 AlterationMod
    Some(0x614), // 109 ConjurationMod
    Some(0x615), // 110 DestructionMod
    Some(0x616), // 111 IllusionMod
    Some(0x617), // 112 RestorationMod
    Some(0x618), // 113 EnchantingMod
    Some(0x619), // 114 OneHandedSkillAdvance
    Some(0x61A), // 115 TwoHandedSkillAdvance
    Some(0x61B), // 116 MarksmanSkillAdvance
    Some(0x61C), // 117 BlockSkillAdvance
    Some(0x61D), // 118 SmithingSkillAdvance
    Some(0x61E), // 119 HeavyArmorSkillAdvance
    Some(0x61F), // 120 LightArmorSkillAdvance
    Some(0x620), // 121 PickPocketSkillAdvance
    Some(0x621), // 122 LockpickingSkillAdvance
    Some(0x622), // 123 SneakSkillAdvance
    Some(0x623), // 124 AlchemySkillAdvance
    Some(0x624), // 125 SpeechcraftSkillAdvance
    Some(0x625), // 126 AlterationSkillAdvance
    Some(0x626), // 127 ConjurationSkillAdvance
    Some(0x627), // 128 DestructionSkillAdvance
    Some(0x628), // 129 IllusionSkillAdvance
    Some(0x629), // 130 RestorationSkillAdvance
    Some(0x62A), // 131 EnchantingSkillAdvance
    Some(0x62B), // 132 LeftWeaponSpeedMult
    Some(0x62C), // 133 DragonSouls
    Some(0x62D), // 134 CombatHealthRegenMult
    Some(0x62E), // 135 OneHandedPowerMod
    Some(0x62F), // 136 TwoHandedPowerMod
    Some(0x630), // 137 MarksmanPowerMod
    Some(0x631), // 138 BlockPowerMod
    Some(0x632), // 139 SmithingPowerMod
    Some(0x633), // 140 HeavyArmorPowerMod
    Some(0x634), // 141 LightArmorPowerMod
    Some(0x635), // 142 PickPocketPowerMod
    Some(0x636), // 143 LockpickingPowerMod
    Some(0x637), // 144 SneakPowerMod
    Some(0x638), // 145 AlchemyPowerMod
    Some(0x639), // 146 SpeechcraftPowerMod
    Some(0x63A), // 147 AlterationPowerMod
    Some(0x63B), // 148 ConjurationPowerMod
    Some(0x63C), // 149 DestructionPowerMod
    Some(0x63D), // 150 IllusionPowerMod
    Some(0x63E), // 151 RestorationPowerMod
    Some(0x63F), // 152 EnchantingPowerMod
    Some(0x640), // 153 DragonRend
    Some(0x641), // 154 AttackDamageMult
    Some(0x642), // 155 HealRateMult/CombatHealthRegenMultMod
    Some(0x643), // 156 MagickaRateMult/CombatHealthRegenMultPowerMod
    Some(0x644), // 157 StaminaRateMult
    Some(0x645), // 158 WerewolfPerks
    Some(0x646), // 159 VampirePerks
    Some(0x647), // 160 GrabActorOffset
    Some(0x648), // 161 Grabbed
    Some(0x649), // 162 DEPRECATED05
    Some(0x64A), // 163 ReflectDamage
];

/// The `AVIF` FormID an authored `GetActorValue` (CTDA 14) `param_1` names
/// (#5989), or `None` when it cannot be read in the loaded game.
///
/// Oblivion, Fallout 3 / New Vegas and Skyrim author the actor value as an
/// *enum index* (xEdit `ptActorValue`); only Fallout 4 onward authors an `AVIF`
/// FormID, which is what [`byroredux_core::ecs::components::ActorValues`] keys
/// on. The engine's own conditions (barter, combat, encumbrance, console) are
/// built with real FormIDs, so a `param_1` that is a loaded `AVIF` is a FormID
/// in every game — no index collides with one (indices are below 164, vanilla
/// `AVIF`s start at `0x3E8`, and mod records sit in a plugin slot >= 1). Anything
/// else is an index: Fallout 3 / New Vegas through the `GetAV` table verified
/// against the masters ([`crate::obscript_actor_values`]), Skyrim through
/// [`SKYRIM_ACTOR_VALUE_FORMS`]. Oblivion has no `AVIF` records, so it stays
/// unreadable. No game installed (tools, unit tests) passes `param_1` through,
/// as before.
pub fn condition_actor_value_form(world: &World, param_1: u32) -> Option<u32> {
    use byroredux_plugin::esm::reader::GameKind;
    let Some(game) = world.try_resource::<ScriptGame>().map(|game| game.0) else {
        return Some(param_1);
    };
    if !crate::condition::authored_actor_value_is_enum_code(game) {
        return Some(param_1);
    }
    let names = world.try_resource::<ActorValueNames>()?;
    if names.contains_form(param_1) {
        return Some(param_1);
    }
    let form = match game {
        GameKind::Fallout3NV => {
            crate::obscript_actor_values::fallout_actor_value_name(param_1).and_then(|name| names.resolve(name))
        }
        GameKind::Skyrim => SKYRIM_ACTOR_VALUE_FORMS
            .get(param_1 as usize)
            .copied()
            .flatten()
            .filter(|&form| names.contains_form(form)),
        _ => None,
    };
    form
}

/// Install the load order's [`ActorValueNames`]. Returns how many names map.
pub fn install_actor_value_names(world: &mut World, index: &EsmIndex) -> usize {
    let names = ActorValueNames::from_index(index);
    let count = names.0.len();
    world.insert_resource(names);
    world.insert_resource(ScriptGame(index.game));
    world.init_resource::<crate::cell_kind::PlayerCellKind>();
    world.insert_resource(crate::idle_names::IdleNames::from_index(index));
    world.insert_resource(crate::obscript_state::ItemTypeCodes::from_index(index));
    world.insert_resource(crate::obscript_state::GameSettings::from_index(index));
    world.insert_resource(crate::obscript_state::WeaponAnimTypes::from_index(index));
    world.insert_resource(crate::challenges::ChallengeDefs::from_index(index));
    world.insert_resource(crate::obscript_more::MusicTracks::from_index(index));
    world.insert_resource(crate::obscript_more::AshPiles::from_index(index));
    world.insert_resource(crate::obscript_more::CellZones::from_index(index));
    world.insert_resource(crate::obscript_more::ChildRaces::from_index(index));
    world.insert_resource(crate::obscript_more::PlayableRaces::from_index(index));
    world.insert_resource(crate::obscript_state::RegionAreas::from_index(index));
    world.insert_resource(crate::magic_shaders::ShaderCatalog::from_index(index));
    world.insert_resource(crate::skills::SkillScaling::from_index(index));
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::AvifRecord;

    #[test]
    fn papyrus_names_and_editor_ids_both_resolve() {
        let mut index = EsmIndex::default();
        for (form_id, editor_id) in [(0x5F0, "AVVariable06"), (0x3E8, "AVHealth")] {
            index.actor_values.insert(
                form_id,
                AvifRecord {
                    form_id,
                    editor_id: editor_id.into(),
                    ..Default::default()
                },
            );
        }
        let names = ActorValueNames::from_index(&index);
        assert_eq!(names.resolve("Variable06"), Some(0x5F0));
        assert_eq!(names.resolve("health"), Some(0x3E8));
        assert_eq!(names.resolve("AVHealth"), Some(0x3E8));
        assert_eq!(names.resolve("Illusion"), None);
    }
}
