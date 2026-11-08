//! The perk hook catalog (docs/prd/perk-entry-points.md §3.1, PERK-HOOK-001): every entry-point index of every
//! in-scope dialect, with its state, where the engine composes it (or why not) and the subjects of its
//! condition tabs. Data, not logic — `perk.hooks` prints it and a test holds it to the code:
//!
//! * every hook constant in [`super::fnv`] and [`super::skyrim`] has a row at its index;
//! * every **Consumed** row names a `path::function` whose source calls a composer
//!   (`compose_*`, or [`super::has_concentrated_fire`], the flag form of one).
//!
//! **Sources.** Names and, for Fallout 3 / New Vegas, the tab subjects are xEdit's `wbPERKEntryPoints`
//! (`wbDefinitionsFNV.pas`, the `epc*` class of each hook; FO3 shares indices 0-36, §3.4). Skyrim's names are
//! `wbEntryPointsEnum` (`wbDefinitionsTES5.pas`); xEdit gives no subjects for its tabs, so they are `None` until
//! PERK-SUB-001 sources them. *Inert* rows cite the GECK *Entry Point* page ("Not Implemented", PERK-HOOK-002) and the CK
//! *Perk Entry Point* page ("Not Used", PERK-HOOK-011).
//!
//! **Unclassified.** A hook whose calculation this catalog has not yet been checked against is
//! `Pending { id: UNCLASSIFIED }`: it is not claimed Blocked (that names an owning milestone, PERK-NG-002)
//! until the Phase 0 census draft (#6651) establishes it, and not claimed Consumed (no call site composes it).

use byroredux_core::character::PerkEntryPointDialect;

/// The §3.1 state of one hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookState {
    /// A calculation in the engine composes the hook; `site` is `path::function` of the caller.
    Consumed { site: &'static str },
    /// In scope; the composition call is not yet made (`id` is the requirement that makes it).
    Pending { id: &'static str },
    /// The calculation does not exist and building it belongs to another milestone.
    Blocked { system: &'static str, owner: &'static str },
    /// The original engine does not run the hook.
    Inert { source: &'static str },
}

impl HookState {
    /// The state's name, as `perk.hooks [state]` filters on it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Consumed { .. } => "consumed",
            Self::Pending { .. } => "pending",
            Self::Blocked { .. } => "blocked",
            Self::Inert { .. } => "inert",
        }
    }

    /// The §3.1 "Reported as" text.
    pub fn report(&self) -> String {
        match self {
            Self::Consumed { site } => format!("consumed: {site}"),
            Self::Pending { id } => format!("pending: {id}"),
            Self::Blocked { system, owner } => format!("blocked: {system}, {owner}"),
            Self::Inert { source } => format!("inert: {source}"),
        }
    }
}

/// One row of the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hook {
    pub index: u8,
    /// The xEdit name.
    pub name: &'static str,
    pub state: HookState,
    /// The subject of each condition tab above 0 (tab 0 is always the perk owner); `None` where no source has
    /// been checked in yet.
    pub subjects: Option<&'static [&'static str]>,
}

/// The `Pending` id of a hook the catalog has not yet been checked against (see the module docs).
pub const UNCLASSIFIED: &str = "PERK-HOOK-001: not yet classified (Phase 0 catalog draft, #6651)";

/// The catalog of one dialect; empty for a game with none.
pub fn hooks(dialect: PerkEntryPointDialect) -> &'static [Hook] {
    match dialect {
        PerkEntryPointDialect::Fallout3NV => FALLOUT3_NV_HOOKS,
        PerkEntryPointDialect::Skyrim => SKYRIM_HOOKS,
        PerkEntryPointDialect::None => &[],
    }
}

/// One hook of a dialect.
pub fn hook(dialect: PerkEntryPointDialect, index: u8) -> Option<&'static Hook> {
    hooks(dialect).get(usize::from(index)).filter(|hook| hook.index == index)
}

// Generated once from xEdit `wbPERKEntryPoints` (FNV) and `wbEntryPointsEnum` (TES5); states are the PRD's.
pub(super) const FALLOUT3_NV_HOOKS: &[Hook] = &[
    Hook { index: 0, name: "Calculate Weapon Damage", state: HookState::Consumed { site: "byroredux/src/combat.rs::attack_damage_against" }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 1, name: "Calculate My Critical Hit Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 2, name: "Calculate My Critical Hit Damage", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 3, name: "Calculate Weapon Attack AP Cost", state: HookState::Pending { id: "PERK-HOOK-005" }, subjects: Some(&["Weapon"]) },
    Hook { index: 4, name: "Calculate Mine Explode Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Item"]) },
    Hook { index: 5, name: "Adjust Range Penalty", state: HookState::Inert { source: "GECK Entry Point: \"Not Implemented\"" }, subjects: Some(&["Weapon"]) },
    Hook { index: 6, name: "Adjust Limb Damage", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Attacker", "Attacker Weapon"]) },
    Hook { index: 7, name: "Calculate Weapon Range", state: HookState::Inert { source: "GECK Entry Point: \"Not Implemented\"" }, subjects: Some(&["Weapon"]) },
    Hook { index: 8, name: "Calculate To Hit Chance", state: HookState::Pending { id: "§10 Q6" }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 9, name: "Adjust Experience Points", state: HookState::Consumed { site: "byroredux/src/combat.rs::award_kill_experience" }, subjects: Some(&[]) },
    Hook { index: 10, name: "Adjust Gained Skill Points", state: HookState::Consumed { site: "byroredux/src/combat.rs::grant_experience" }, subjects: Some(&[]) },
    Hook { index: 11, name: "Adjust Book Skill Points", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 12, name: "Modify Recovered Health", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 13, name: "Calculate Inventory AP Cost", state: HookState::Inert { source: "GECK Entry Point: \"Not Implemented\"" }, subjects: Some(&[]) },
    Hook { index: 14, name: "Get Disposition", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Target"]) },
    Hook { index: 15, name: "Get Should Attack", state: HookState::Inert { source: "GECK Entry Point: \"Not Implemented\"" }, subjects: Some(&["Attacker"]) },
    Hook { index: 16, name: "Get Should Assist", state: HookState::Inert { source: "GECK Entry Point: \"Not Implemented\"" }, subjects: Some(&["Attacker", "Attackee"]) },
    Hook { index: 17, name: "Calculate Buy Price", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Item"]) },
    Hook { index: 18, name: "Get Bad Karma", state: HookState::Inert { source: "GECK Entry Point: \"Not Implemented\"" }, subjects: Some(&[]) },
    Hook { index: 19, name: "Get Good Karma", state: HookState::Inert { source: "GECK Entry Point: \"Not Implemented\"" }, subjects: Some(&[]) },
    Hook { index: 20, name: "Ignore Locked Terminal", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 21, name: "Add Leveled List On Death", state: HookState::Pending { id: "PERK-HOOK-006" }, subjects: Some(&["Target"]) },
    Hook { index: 22, name: "Get Max Carry Weight", state: HookState::Consumed { site: "byroredux/src/encumbrance.rs::encumbrance_system" }, subjects: Some(&[]) },
    Hook { index: 23, name: "Modify Addiction Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 24, name: "Modify Addiction Duration", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 25, name: "Modify Positive Chem Duration", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 26, name: "Adjust Drinking Radiation", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 27, name: "Activate", state: HookState::Pending { id: "PERK-HOOK-007" }, subjects: Some(&["Target"]) },
    Hook { index: 28, name: "Mysterious Stranger", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 29, name: "Has Paralyzing Palm", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 30, name: "Hacking Science Bonus", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 31, name: "Ignore Running During Detection", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 32, name: "Ignore Broken Lock", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 33, name: "Has Concentrated Fire", state: HookState::Consumed { site: "byroredux/src/systems/vats.rs::vats_queue_part" }, subjects: Some(&[]) },
    Hook { index: 34, name: "Calculate Gun Spread", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 35, name: "Player Kill AP Reward", state: HookState::Pending { id: "PERK-HOOK-004" }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 36, name: "Modify Enemy Critical Hit Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 37, name: "Reload Speed", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 38, name: "Equip Speed", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 39, name: "Action Point Regen", state: HookState::Pending { id: "PERK-HOOK-005" }, subjects: Some(&["Weapon"]) },
    Hook { index: 40, name: "Action Point Cost", state: HookState::Pending { id: "PERK-HOOK-005" }, subjects: Some(&["Weapon"]) },
    Hook { index: 41, name: "Miss Fortune", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 42, name: "Modify Run Speed", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 43, name: "Modify Attack Speed", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 44, name: "Modify Radiation Consumed", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 45, name: "Has Pip Hacker", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 46, name: "Has Meltdown", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 47, name: "See Enemy Health", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 48, name: "Has Jury Rigging", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 49, name: "Modify Threat Range", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 50, name: "Modify Thread", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 51, name: "Has Fast Travel Always", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 52, name: "Knockdown Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 53, name: "Modify Weapon Strength Req", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 54, name: "Modify Aiming Move Speed", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 55, name: "Modify Light Items", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 56, name: "Modify Damage Threshold (defender)", state: HookState::Consumed { site: "byroredux/src/combat.rs::apply_damage_threshold" }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 57, name: "Modify Chance for Ammo Item", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 58, name: "Modify Damage Threshold (attacker)", state: HookState::Consumed { site: "byroredux/src/combat.rs::apply_damage_threshold" }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 59, name: "Modify Throwing Velocity", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 60, name: "Chance for Item on Fire", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 61, name: "Has Unarmed Forward Power Attack", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 62, name: "Has Unarmed Back Power Attack", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 63, name: "Has Unarmed Crouched Power Attack", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 64, name: "Has Unarmed Counter Attack", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon", "Target"]) },
    Hook { index: 65, name: "Has Unarmed Left Power Attack", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 66, name: "Has Unarmed Right Power Attack", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 67, name: "VATS HelperChance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 68, name: "Modify Item Damage", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 69, name: "Has Improved Detection", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 70, name: "Has Improved Spotting", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 71, name: "Has Improved Item Detection", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&[]) },
    Hook { index: 72, name: "Adjust Explosion Radius", state: HookState::Pending { id: UNCLASSIFIED }, subjects: Some(&["Weapon"]) },
    Hook { index: 73, name: "Reserved", state: HookState::Inert { source: "xEdit wbPERKEntryPoints names it 'Reserved'" }, subjects: Some(&["Weapon"]) },
];

pub(super) const SKYRIM_HOOKS: &[Hook] = &[
    Hook { index: 0, name: "Calculate Weapon Damage", state: HookState::Consumed { site: "byroredux/src/combat.rs::attack_damage_against" }, subjects: None },
    Hook { index: 1, name: "Calculate My Critical Hit Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 2, name: "Calculate My Critical Hit Damage", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 3, name: "Calculate Mine Explode Chance", state: HookState::Inert { source: "CK Perk Entry Point: \"Not Used\"" }, subjects: None },
    Hook { index: 4, name: "Adjust Limb Damage", state: HookState::Inert { source: "CK Perk Entry Point: \"Not Used\"" }, subjects: None },
    Hook { index: 5, name: "Adjust Book Skill Points", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 6, name: "Mod Recovered Health", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 7, name: "Get Should Attack", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 8, name: "Mod Buy Prices", state: HookState::Consumed { site: "byroredux/src/barter.rs::price_terms" }, subjects: None },
    Hook { index: 9, name: "Add Leveled List On Death", state: HookState::Pending { id: "PERK-HOOK-006" }, subjects: None },
    Hook { index: 10, name: "Get Max Carry Weight", state: HookState::Consumed { site: "byroredux/src/encumbrance.rs::encumbrance_system" }, subjects: None },
    Hook { index: 11, name: "Mod Addiction Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 12, name: "Mod Addiction Duration", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 13, name: "Mod Positive Chem Duration", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 14, name: "Activate", state: HookState::Pending { id: "PERK-HOOK-007" }, subjects: None },
    Hook { index: 15, name: "Ignore Running During Detection", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 16, name: "Ignore Broken Lock", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 17, name: "Mod Enemy Critical Hit Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 18, name: "Mod Sneak Attack Mult", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 19, name: "Mod Max Placeable Mines", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 20, name: "Mod Bow Zoom", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 21, name: "Mod Recover Arrow Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 22, name: "Mod Skill Use", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 23, name: "Mod Telekinesis Distance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 24, name: "Mod Telekinesis Damage Mult", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 25, name: "Mod Telekinesis Damage", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 26, name: "Mod Bashing Damage", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 27, name: "Mod Power Attack Stamina", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 28, name: "Mod Power Attack Damage", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 29, name: "Mod Spell Magnitude", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 30, name: "Mod Spell Duration", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 31, name: "Mod Secondary Value Weight", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 32, name: "Mod Armor Weight", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 33, name: "Mod Incoming Stagger", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 34, name: "Mod Target Stagger", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 35, name: "Mod Attack Damage", state: HookState::Consumed { site: "byroredux/src/combat.rs::attack_damage_against" }, subjects: None },
    Hook { index: 36, name: "Mod Incoming Damage", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 37, name: "Mod Target Damage Resistance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 38, name: "Mod Spell Cost", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 39, name: "Mod Percent Blocked", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 40, name: "Mod Shield Deflect Arrow Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 41, name: "Mod Incoming Spell Magnitude", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 42, name: "Mod Incoming Spell Duration", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 43, name: "Mod Player Intimidation", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 44, name: "Mod Player Reputation", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 45, name: "Mod Favor Points", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 46, name: "Mod Bribe Amount", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 47, name: "Mod Detection Light", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 48, name: "Mod Detection Movement", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 49, name: "Mod Soul Gem Recharge", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 50, name: "Set Sweep Attack", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 51, name: "Apply Combat Hit Spell", state: HookState::Pending { id: "PERK-HOOK-009" }, subjects: None },
    Hook { index: 52, name: "Apply Bashing Spell", state: HookState::Pending { id: "PERK-HOOK-009" }, subjects: None },
    Hook { index: 53, name: "Apply Reanimate Spell", state: HookState::Pending { id: "PERK-HOOK-009" }, subjects: None },
    Hook { index: 54, name: "Set Boolean Graph Variable", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 55, name: "Mod Spell Casting Sound Event", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 56, name: "Mod Pickpocket Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 57, name: "Mod Detection Sneak Skill", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 58, name: "Mod Falling Damage", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 59, name: "Mod Lockpick Sweet Spot", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 60, name: "Mod Sell Prices", state: HookState::Consumed { site: "byroredux/src/barter.rs::price_terms" }, subjects: None },
    Hook { index: 61, name: "Can Pickpocket Equipped Item", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 62, name: "Mod Lockpick Level Allowed", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 63, name: "Set Lockpick Starting Arc", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 64, name: "Set Progression Picking", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 65, name: "Make Lockpicks Unbreakable", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 66, name: "Mod Alchemy Effectiveness", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 67, name: "Apply Weapon Swing Spell", state: HookState::Pending { id: "PERK-HOOK-009" }, subjects: None },
    Hook { index: 68, name: "Mod Commanded Actor Limit", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 69, name: "Apply Sneaking Spell", state: HookState::Pending { id: "PERK-HOOK-009" }, subjects: None },
    Hook { index: 70, name: "Mod Player Magic Slowdown", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 71, name: "Mod Ward Magicka Absorption Pct", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 72, name: "Mod Initial Ingredient Effects Learned", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 73, name: "Purify Alchemy Ingredients", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 74, name: "Filter Activation", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 75, name: "Can Dual Cast Spell", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 76, name: "Mod Tempering Health", state: HookState::Consumed { site: "crates/scripting/src/tempering.rs::tempering_multiplier" }, subjects: None },
    Hook { index: 77, name: "Mod Enchantment Power", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 78, name: "Mod Soul Pct Captured to Weapon", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 79, name: "Mod Soul Gem Enchanting", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 80, name: "Mod # Applied Enchantments Allowed", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 81, name: "Set Activate Label", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 82, name: "Mod Shout OK", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 83, name: "Mod Poison Dose Count", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 84, name: "Should Apply Placed Item", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 85, name: "Mod Armor Rating", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 86, name: "Mod Lockpicking Crime Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 87, name: "Mod Ingredients Harvested", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 88, name: "Mod Spell Range (Target Loc.)", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 89, name: "Mod Potions Created", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 90, name: "Mod Lockpicking Key Reward Chance", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
    Hook { index: 91, name: "Allow Mount Actor", state: HookState::Pending { id: UNCLASSIFIED }, subjects: None },
];
