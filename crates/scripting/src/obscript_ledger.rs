//! The saved state of the ObScript commands that keep a value nothing else in the engine owns:
//! the player's fame and infamy, bounty, the per-faction "the player did this to them" flags, who has
//! talked to the player, and the flags a `Set...` command stores for its `Get...`.
//!
//! One resource, [`ObScriptLedger`], saved whole (`FORMAT_MAJOR` 112): every field is plain data keyed
//! by FormIDs, so no entity ids or interned strings ride in a save.
//!
//! What each part is (the CS / GECK pages):
//! * **Fame / infamy / bounty** — *GetPCFame* "the current fame value for the player character. Only
//!   the player character has fame or infamy values"; *ModPCFame*; *GetCrimeGold* / *SetCrimeGold* /
//!   *ModCrimeGold* "the current crime gold on an actor ... the current fine that an actor owes for
//!   crimes committed".
//! * **Faction flags** — *SetPCFactionMurder* / *Steal* / *Attack*: "the flag is set automatically
//!   whenever the player murders a member of that faction ... You can also set this flag manually";
//!   *SetPCExpelled*: "expels the player from the specified faction, and sets the expelled flag";
//!   *SetPCEnemyOfFaction* / *ClearFactionPlayerEnemyFlag*: the enemy flag a major crime sets on a
//!   crime-tracking faction.
//! * **Talked to the player** — *GetTalkedToPC*: "1 if the NPC has ever spoken to the player"; the
//!   dialogue selection notes the speaker.
//!
//! A command whose value nothing in the engine would read back is not stored here: it declines
//! (*AddTopic*, *ModDisposition*, *SetQuestObject*, *SetItemValue* — no topic gate, disposition
//! term, quest-item marker or barter value consumes one) rather than report a no-op as done.

use std::collections::{BTreeMap, BTreeSet};

use byroredux_core::ecs::{Resource, World};

/// A per-faction flag of what the player did to (or was done by) a faction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum FactionFlag {
    Murder,
    Steal,
    Attack,
    Expelled,
    /// The enemy flag a major crime against a crime-tracking faction sets.
    Enemy,
}

/// See the module docs.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ObScriptLedger {
    pub fame: f32,
    pub infamy: f32,
    /// The player's bounty (crime gold).
    pub crime_gold: i32,
    /// "The total value of stolen goods that the player has sold."
    pub amount_sold_stolen: i32,
    faction_flags: BTreeSet<(FactionFlag, u32)>,
    /// Actors that have spoken to the player (`GetTalkedToPC`).
    talked_to_player: BTreeSet<u32>,
    /// `Set...` commands' values for their getters, keyed by `(command id, subject reference)`; a
    /// global one uses subject `0`.
    #[cfg_attr(feature = "save", serde(with = "crate::serde_pairs"))]
    values: BTreeMap<(u16, u32), f32>,
    /// A form argument stored the same way (the combat style of an actor, a talking activator's actor).
    #[cfg_attr(feature = "save", serde(with = "crate::serde_pairs"))]
    forms: BTreeMap<(u16, u32), u32>,
    /// Per `(holder reference, weapon base)` condition percent (`ModWeaponHealthPerc`,
    /// `AddItemHealthPercent`).
    #[cfg_attr(feature = "save", serde(with = "crate::serde_pairs"))]
    weapon_condition: BTreeMap<(u32, u32), f32>,
    /// The last actor each actor hit (`GetOwnerLastTarget`), by reference.
    last_targets: BTreeMap<u32, u32>,
    /// The numeric faction reactions scripts set (`SetFactionReaction`, `ModFactionReaction`), by
    /// `(faction, other faction)`.
    #[cfg_attr(feature = "save", serde(with = "crate::serde_pairs"))]
    reactions: BTreeMap<(u32, u32), f32>,
    /// The player's tagged skills: slot (0..=2) to the skill's actor-value code.
    tag_skills: BTreeMap<u8, u8>,
    /// Friendly hits taken, by `(victim, attacker)` reference (`GetFriendHit`).
    #[cfg_attr(feature = "save", serde(with = "crate::serde_pairs"))]
    friend_hits: BTreeMap<(u32, u32), u32>,
}

impl Resource for ObScriptLedger {}

impl ObScriptLedger {
    pub fn flag(&self, flag: FactionFlag, faction: u32) -> bool {
        self.faction_flags.contains(&(flag, faction))
    }

    pub fn set_flag(&mut self, flag: FactionFlag, faction: u32, on: bool) {
        if on {
            self.faction_flags.insert((flag, faction));
        } else {
            self.faction_flags.remove(&(flag, faction));
        }
    }

    /// Whether the player is flagged `flag` with any of `factions`.
    pub fn flagged_with_any(&self, flag: FactionFlag, factions: &[u32]) -> bool {
        factions.iter().any(|&faction| self.flag(flag, faction))
    }

    pub fn talked_to_player(&self, actor: u32) -> bool {
        self.talked_to_player.contains(&actor)
    }

    pub fn note_talked_to_player(&mut self, actor: u32) {
        self.talked_to_player.insert(actor);
    }

    /// `ModCrimeGold`: saturating, so mod-supplied bytecode cannot wrap the bounty negative.
    pub fn mod_crime_gold(&mut self, delta: i32) {
        self.crime_gold = self.crime_gold.saturating_add(delta);
    }

    pub fn value(&self, key: u16, subject: u32) -> Option<f32> {
        self.values.get(&(key, subject)).copied()
    }

    /// Every subject a `Set...` command stored a value for under `key`, with the value.
    pub fn values_of(&self, key: u16) -> Vec<(u32, f32)> {
        self.values.range((key, 0)..=(key, u32::MAX)).map(|(&(_, subject), &value)| (subject, value)).collect()
    }

    pub fn set_value(&mut self, key: u16, subject: u32, value: f32) {
        self.values.insert((key, subject), value);
    }

    pub fn form(&self, key: u16, subject: u32) -> Option<u32> {
        self.forms.get(&(key, subject)).copied()
    }

    pub fn set_form(&mut self, key: u16, subject: u32, form: u32) {
        self.forms.insert((key, subject), form);
    }

    /// The condition percent of `weapon` held by `holder`; 100 when no script changed it.
    pub fn weapon_condition(&self, holder: u32, weapon: u32) -> f32 {
        self.weapon_condition.get(&(holder, weapon)).copied().unwrap_or(100.0)
    }

    /// Note that `attacker` struck `victim`.
    pub fn note_hit(&mut self, attacker: u32, victim: u32) {
        self.last_targets.insert(attacker, victim);
    }

    /// The last actor `attacker` struck.
    pub fn last_target(&self, attacker: u32) -> Option<u32> {
        self.last_targets.get(&attacker).copied()
    }

    /// The numeric reaction `faction` has toward `other` that scripts set; 0 when none did.
    pub fn reaction(&self, faction: u32, other: u32) -> f32 {
        self.reactions.get(&(faction, other)).copied().unwrap_or(0.0)
    }

    pub fn set_reaction(&mut self, faction: u32, other: u32, value: f32) {
        self.reactions.insert((faction, other), value);
    }

    pub fn set_tag_skill(&mut self, slot: u8, skill: u8) -> Option<u8> {
        self.tag_skills.insert(slot, skill)
    }

    pub fn is_tag_skill(&self, skill: u8) -> bool {
        self.tag_skills.values().any(|&tagged| tagged == skill)
    }

    pub fn note_friend_hit(&mut self, victim: u32, attacker: u32) {
        *self.friend_hits.entry((victim, attacker)).or_default() += 1;
    }

    pub fn friend_hits(&self, victim: u32, attacker: u32) -> u32 {
        self.friend_hits.get(&(victim, attacker)).copied().unwrap_or(0)
    }

    /// Whether any faction carries `flag`.
    pub fn any_flag(&self, flag: FactionFlag) -> bool {
        self.faction_flags.iter().any(|&(kind, _)| kind == flag)
    }

    pub fn set_weapon_condition(&mut self, holder: u32, weapon: u32, percent: f32) {
        self.weapon_condition.insert((holder, weapon), percent.clamp(0.0, 100.0));
    }
}

/// This subsystem's resource.
pub fn install(world: &mut World) {
    world.init_resource::<ObScriptLedger>();
}

/// This subsystem's saved column. Called by the binary's `build_save_registry` in its row order.
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<ObScriptLedger>("ObScriptLedger");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_values_and_talkers_round_trip_through_their_accessors() {
        let mut ledger = ObScriptLedger::default();
        ledger.set_flag(FactionFlag::Murder, 0x10, true);
        assert!(ledger.flag(FactionFlag::Murder, 0x10) && !ledger.flag(FactionFlag::Steal, 0x10));
        assert!(ledger.flagged_with_any(FactionFlag::Murder, &[0x11, 0x10]));
        ledger.set_flag(FactionFlag::Murder, 0x10, false);
        assert!(!ledger.flag(FactionFlag::Murder, 0x10));
        assert!(!ledger.talked_to_player(0x20));
        ledger.note_talked_to_player(0x20);
        assert!(ledger.talked_to_player(0x20));
        ledger.set_value(0x10d3, 0x40, 1.0);
        assert_eq!((ledger.value(0x10d3, 0x40), ledger.value(0x10d3, 0x41)), (Some(1.0), None));
        ledger.set_weapon_condition(0x50, 0x60, 150.0);
        assert_eq!(ledger.weapon_condition(0x50, 0x60), 100.0, "clamped");
        assert_eq!(ledger.weapon_condition(0x50, 0x61), 100.0, "untouched weapons are whole");
    }

    /// Every pair-keyed ledger map holding an entry still makes a JSON save:
    /// VCG02's stage-10 `SetAlly` put the first row in one, and the derived
    /// object form failed every quicksave after it ("key must be a string").
    #[cfg(feature = "save")]
    #[test]
    fn a_populated_ledger_survives_a_json_save() {
        let mut ledger = ObScriptLedger::default();
        ledger.set_value(0x1159, 0x14, 3.0);
        ledger.set_form(0x10a3, 0x0010_4E85, 0x0002_C5F2);
        ledger.set_weapon_condition(0x14, 0x0008_ED0B, 40.0);
        ledger.set_reaction(0x0010_4C5F, 0x0001_B2A4, 1.0);
        ledger.note_friend_hit(0x0010_4E85, 0x14);
        let json = serde_json::to_value(&ledger).expect("a populated ledger serializes");
        assert_eq!(serde_json::from_value::<ObScriptLedger>(json).unwrap(), ledger);
    }
}
