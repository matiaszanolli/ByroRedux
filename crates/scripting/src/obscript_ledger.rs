//! The saved state of the ObScript commands that keep a value nothing else in the engine owns:
//! the player's fame and infamy, bounty, the per-faction "the player did this to them" flags, the
//! topics the player knows, who has talked to the player, the quest-object flags, the dispositions
//! scripts changed, and the flags a `Set...` command stores for its `Get...`.
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
//! * **Topics** — *AddTopic*: "adds a topic to the player's list of known topics".
//! * **Disposition** — *ModDisposition* changes how much an NPC likes the player; the change is
//!   remembered ("NPCs remember their disposition towards you from one conversation to the next").

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
    known_topics: BTreeSet<u32>,
    /// Actors that have spoken to the player (`GetTalkedToPC`).
    talked_to_player: BTreeSet<u32>,
    /// Objects flagged as quest items (`SetQuestObject`).
    quest_objects: BTreeSet<u32>,
    /// Dispositions towards the player scripts changed, by actor reference.
    disposition_mods: BTreeMap<u32, i32>,
    /// `Set...` commands' values for their getters, keyed by `(command id, subject reference)`; a
    /// global one uses subject `0`.
    values: BTreeMap<(u16, u32), f32>,
    /// A form argument stored the same way (the combat style of an actor, a talking activator's actor).
    forms: BTreeMap<(u16, u32), u32>,
    /// Base-record value overrides (`SetItemValue`), by item base.
    item_values: BTreeMap<u32, i32>,
    /// Per `(holder reference, weapon base)` condition percent (`ModWeaponHealthPerc`,
    /// `AddItemHealthPercent`).
    weapon_condition: BTreeMap<(u32, u32), f32>,
    /// The last actor each actor hit (`GetOwnerLastTarget`), by reference.
    last_targets: BTreeMap<u32, u32>,
    /// The numeric faction reactions scripts set (`SetFactionReaction`, `ModFactionReaction`), by
    /// `(faction, other faction)`.
    reactions: BTreeMap<(u32, u32), f32>,
    /// The player's tagged skills: slot (0..=2) to the skill's actor-value code.
    tag_skills: BTreeMap<u8, u8>,
    /// Friendly hits taken, by `(victim, attacker)` reference (`GetFriendHit`).
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

    pub fn knows_topic(&self, topic: u32) -> bool {
        self.known_topics.contains(&topic)
    }

    pub fn add_topic(&mut self, topic: u32) -> bool {
        self.known_topics.insert(topic)
    }

    pub fn talked_to_player(&self, actor: u32) -> bool {
        self.talked_to_player.contains(&actor)
    }

    pub fn note_talked_to_player(&mut self, actor: u32) {
        self.talked_to_player.insert(actor);
    }

    pub fn is_quest_object(&self, form: u32) -> bool {
        self.quest_objects.contains(&form)
    }

    pub fn set_quest_object(&mut self, form: u32, on: bool) {
        if on {
            self.quest_objects.insert(form);
        } else {
            self.quest_objects.remove(&form);
        }
    }

    /// `ModCrimeGold`: saturating, so mod-supplied bytecode cannot wrap the bounty negative.
    pub fn mod_crime_gold(&mut self, delta: i32) {
        self.crime_gold = self.crime_gold.saturating_add(delta);
    }

    pub fn disposition_mod(&self, actor: u32) -> i32 {
        self.disposition_mods.get(&actor).copied().unwrap_or(0)
    }

    pub fn mod_disposition(&mut self, actor: u32, delta: i32) {
        let slot = self.disposition_mods.entry(actor).or_default();
        *slot = slot.saturating_add(delta);
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

    pub fn item_value(&self, item: u32) -> Option<i32> {
        self.item_values.get(&item).copied()
    }

    pub fn set_item_value(&mut self, item: u32, value: i32) {
        self.item_values.insert(item, value);
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

    /// The skill (actor-value code) tagged in `slot`.
    pub fn tag_skill(&self, slot: u8) -> Option<u8> {
        self.tag_skills.get(&slot).copied()
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
    fn flags_topics_dispositions_and_values_round_trip_through_their_accessors() {
        let mut ledger = ObScriptLedger::default();
        ledger.set_flag(FactionFlag::Murder, 0x10, true);
        assert!(ledger.flag(FactionFlag::Murder, 0x10) && !ledger.flag(FactionFlag::Steal, 0x10));
        assert!(ledger.flagged_with_any(FactionFlag::Murder, &[0x11, 0x10]));
        ledger.set_flag(FactionFlag::Murder, 0x10, false);
        assert!(!ledger.flag(FactionFlag::Murder, 0x10));
        assert!(ledger.add_topic(0x20) && !ledger.add_topic(0x20), "a topic is added once");
        ledger.mod_disposition(0x30, 5);
        ledger.mod_disposition(0x30, -2);
        assert_eq!(ledger.disposition_mod(0x30), 3);
        ledger.set_value(0x10d3, 0x40, 1.0);
        assert_eq!((ledger.value(0x10d3, 0x40), ledger.value(0x10d3, 0x41)), (Some(1.0), None));
        ledger.set_weapon_condition(0x50, 0x60, 150.0);
        assert_eq!(ledger.weapon_condition(0x50, 0x60), 100.0, "clamped");
        assert_eq!(ledger.weapon_condition(0x50, 0x61), 100.0, "untouched weapons are whole");
    }
}
