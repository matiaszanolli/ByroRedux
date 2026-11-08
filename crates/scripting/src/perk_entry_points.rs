//! Perk entry-point composition (M47.2).
//!
//! A `PERK` record's `Entry Point` entries are hooks into hardcoded game
//! calculations: the damage a weapon does, the experience a kill yields, the
//! weight an actor can carry. When the engine runs such a calculation it asks
//! this module to compose the base value with every entry the actor's perks
//! contribute to that hook:
//!
//! ```text
//! value = base
//! for each (perk, entry) of the actor, ascending (priority, perk order, entry order):
//!     if the entry's rank is the perk's current rank
//!        and every condition tab passes:
//!         value = function(value, entry data)
//! ```
//!
//! Everything below is grounded in xEdit's `wbDefinitionsFNV` /
//! `wbDefinitionsTES5` `PERK` definitions and checked against the vanilla
//! masters (`crates/plugin/examples/perk_entry_census.rs`):
//!
//! * **Rank.** `PRKE`'s rank is zero-based and an entry is authored once *per
//!   rank* it applies at ("If a specific perk entry is applicable at multiple
//!   ranks, it must be specified multiple times"). FalloutNV.esm's
//!   `DemolitionExpert` carries ×1.2 / ×1.4 / ×1.6 at ranks 0 / 1 / 2: each
//!   rank's entry is the *total* at that rank, so an actor holding rank `N`
//!   (`Perks` is one-based, 0 = not owned) gets exactly the entries with
//!   `rank == N - 1` — never the lower ranks' as well.
//! * **Functions** (the DATA byte): 1 Set, 2 Add, 3 Multiply, 4 Add Range
//!   (`value + Random(min, max)`), 5 Add AV Mult (`value + AV * x`),
//!   6 Absolute, 7 Negative Absolute, 12 Set to AV Mult (`AV * x`),
//!   13 Multiply AV Mult (`value * AV * x`), 14 Multiply 1 + AV Mult
//!   (`value * (1 + AV * x)`) — the formulas of the Creation Kit's *Perk Entry
//!   Point* page. Functions 8–11 and 15 produce a list, a choice, a spell or a
//!   text rather than a number; they are not value math and are skipped here.
//! * **Subtract table.** For three FNV hooks (53 `Modify Weapon Strength Req`,
//!   56 / 58 `Modify Damage Threshold`) function 1 is *labelled* "Subtract
//!   Value" in the GECK; vanilla `WeaponHandling` (hook 53, function 1, 2.0:
//!   "−2 Strength requirement") and `PiercingStrike` (hook 58, function 1,
//!   15.0) confirm it subtracts.
//! * **Condition tabs.** Each entry's conditions are split by `PRKC` tab. Tab 0
//!   is the perk owner; the other tabs are subjects the *calculation* supplies
//!   (the weapon, the target, the attacker, …). A tab whose subject the caller
//!   did not supply and that carries conditions makes the entry inapplicable —
//!   guessing "yes" would hand every perk bonus to every weapon. A weapon is not
//!   an entity, so the weapon tab's subject is [`WeaponSubject`], a scratch entity
//!   lent the attacker's weapon base form for one composition
//!   (`docs/prd/perk-entry-points.md`, PERK-SUB-002, §10 Q4).
//! * **Priority.** Entries compose in ascending priority, ties in the order the
//!   actor's perks and the record's entries were authored. The Skyrim CK says a
//!   higher number "takes precedence" while the Fallout 4 CK says the lower
//!   number does, and no vanilla FO3/FNV entry has a non-zero priority, so the
//!   choice is only observable on Skyrim (where a `Set Value` carries the
//!   high number, e.g. `EagleEye30`'s priority 15 against ×-multipliers at 0 —
//!   a `Set` that "takes precedence" is one applied last). It is one constant,
//!   [`compose_order_key`].
//!
//! The table is built once per load order from `EsmIndex::perks`
//! ([`install_perk_entry_points`]) and is not saved — it is re-derived from the
//! masters every boot, like the Story Manager tree. FO3/FNV and Skyrim only:
//! the other games' `PERK` entry layout is not established, so their table is
//! empty and every composition returns its base value.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::character::Perks;
use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::character::PerkEntryPointDialect;
use byroredux_plugin::esm::records::{
    ActorValueRef, EsmIndex, ItemKind, PerkConditionTab, PerkEntryBody, PerkFunctionData,
};

use crate::condition::{evaluate, ConditionContext};
use crate::story_manager::StoryManagerRng;

/// FO3 / FNV entry-point indices (`wbPERKEntryPoints`, xEdit
/// `wbDefinitionsFNV`; FO3 shares 0..=36). Only the hooks the engine
/// consumes are named.
pub mod fnv {
    pub const CALCULATE_WEAPON_DAMAGE: u8 = 0;
    pub const CALCULATE_MY_CRITICAL_HIT_CHANCE: u8 = 1;
    pub const CALCULATE_MY_CRITICAL_HIT_DAMAGE: u8 = 2;
    pub const ADJUST_EXPERIENCE_POINTS: u8 = 9;
    pub const ADJUST_GAINED_SKILL_POINTS: u8 = 10;
    pub const GET_MAX_CARRY_WEIGHT: u8 = 22;
    /// `Add Leveled List On Death`: the owner's kill adds the entry's leveled list to the victim.
    pub const ADD_LEVELED_LIST_ON_DEATH: u8 = 21;
    /// `Player Kill AP Reward`: the AP a VATS sequence that kills restores.
    pub const PLAYER_KILL_AP_REWARD: u8 = 35;
    /// `Has Concentrated Fire` (xEdit `wbDefinitionsFNV` / `FO3` entry 33): the vanilla
    /// `ConcentratedFire` perk authors one Set-to-1.0 entry here.
    pub const HAS_CONCENTRATED_FIRE: u8 = 33;
    pub const MODIFY_DAMAGE_THRESHOLD_DEFENDER: u8 = 56;
    pub const MODIFY_DAMAGE_THRESHOLD_ATTACKER: u8 = 58;
}

/// Skyrim entry-point indices (`wbEntryPointsEnum`, xEdit `wbDefinitionsTES5`).
pub mod skyrim {
    pub const CALCULATE_WEAPON_DAMAGE: u8 = 0;
    pub const CALCULATE_MY_CRITICAL_HIT_CHANCE: u8 = 1;
    pub const CALCULATE_MY_CRITICAL_HIT_DAMAGE: u8 = 2;
    pub const GET_MAX_CARRY_WEIGHT: u8 = 10;
    /// `Add Leveled List On Death` (`wbEntryPointsEnum` index 9).
    pub const ADD_LEVELED_LIST_ON_DEATH: u8 = 9;
    pub const MOD_SNEAK_ATTACK_MULT: u8 = 18;
    pub const MOD_ATTACK_DAMAGE: u8 = 35;
    pub const MOD_INCOMING_DAMAGE: u8 = 36;
    /// `Mod Tempering Health` (xEdit `wbEntryPointsEnum`, TES5 index 76): what the Smithing material perks
    /// (Steel Smithing and kin) multiply when improving an item.
    pub const MOD_TEMPERING_HEALTH: u8 = 76;
}

/// An engine calculation a perk can modify (PERK-DIA-002). A consumer names the calculation; the dialect maps it
/// to a hook index here and nowhere else, so the per-game difference is a row of this table, never a branch in
/// combat, encumbrance or tempering. A calculation the dialect has no hook for composes to its base value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerkCalculation {
    /// The experience a kill awards (FO3 / FNV *Adjust Experience Points*).
    ExperienceGain,
    /// The skill points a level-up banks (FO3 / FNV *Adjust Gained Skill Points*).
    GainedSkillPoints,
    /// The weight an actor can carry.
    MaxCarryWeight,
    /// Whether VATS offers Concentrated Fire (FO3 / FNV *Has Concentrated Fire*), a flag composed from 0.
    ConcentratedFire,
    /// The improvement a smithing material perk gives a tempered item (Skyrim *Mod Tempering Health*).
    TemperingHealth,
    /// *Add Leveled List On Death* (FO3 / FNV 21, Skyrim 9): the leveled lists a kill adds to the victim.
    DeathLeveledList,
    /// The action points a VATS sequence that kills restores (FO3 / FNV *Player Kill AP Reward*), from a base of 0.
    PlayerKillApReward,
}

impl PerkCalculation {
    /// The hook index of this calculation in `dialect`, if it has one.
    pub fn hook(self, dialect: PerkEntryPointDialect) -> Option<u8> {
        use PerkEntryPointDialect::{Fallout3NV, Skyrim};
        match (self, dialect) {
            (Self::ExperienceGain, Fallout3NV) => Some(fnv::ADJUST_EXPERIENCE_POINTS),
            (Self::GainedSkillPoints, Fallout3NV) => Some(fnv::ADJUST_GAINED_SKILL_POINTS),
            (Self::MaxCarryWeight, Fallout3NV) => Some(fnv::GET_MAX_CARRY_WEIGHT),
            (Self::MaxCarryWeight, Skyrim) => Some(skyrim::GET_MAX_CARRY_WEIGHT),
            (Self::ConcentratedFire, Fallout3NV) => Some(fnv::HAS_CONCENTRATED_FIRE),
            (Self::TemperingHealth, Skyrim) => Some(skyrim::MOD_TEMPERING_HEALTH),
            (Self::PlayerKillApReward, Fallout3NV) => Some(fnv::PLAYER_KILL_AP_REWARD),
            (Self::DeathLeveledList, Fallout3NV) => Some(fnv::ADD_LEVELED_LIST_ON_DEATH),
            (Self::DeathLeveledList, Skyrim) => Some(skyrim::ADD_LEVELED_LIST_ON_DEATH),
            _ => None,
        }
    }
}

/// Compose `base` through the hook of `calculation` in the loaded dialect (PERK-DIA-002); `base` when the world
/// has no perk table or the dialect has no hook for it. `tab_subjects` are as for [`compose_entry_point`].
pub fn compose_calculation(
    world: &World,
    owner: EntityId,
    calculation: PerkCalculation,
    base: f32,
    tab_subjects: &[Option<EntityId>],
) -> f32 {
    let dialect = world
        .try_resource::<PerkEntryTable>()
        .map_or(PerkEntryPointDialect::None, |table| table.dialect());
    match calculation.hook(dialect) {
        Some(hook) => compose_entry_point(world, owner, hook, base, tab_subjects),
        None => base,
    }
}

/// The value math of one entry, resolved from its function byte and EPFD.
#[derive(Debug, Clone, PartialEq)]
pub enum PerkOp {
    /// `value = x` — or `value - x` on the FNV subtract-table hooks.
    Set(f32),
    Subtract(f32),
    Add(f32),
    Multiply(f32),
    /// `value + Random(min, max)`.
    AddRange { min: f32, max: f32 },
    /// `value + AV * mult` (function 5).
    AddAvMult { avif: Option<u32>, mult: f32 },
    /// `AV * mult` (function 12).
    SetToAvMult { avif: Option<u32>, mult: f32 },
    /// `value * AV * mult` (function 13).
    MultiplyAvMult { avif: Option<u32>, mult: f32 },
    /// `value * (1 + AV * mult)` (function 14).
    MultiplyOnePlusAvMult { avif: Option<u32>, mult: f32 },
    Absolute,
    NegativeAbsolute,
}

/// One entry-point entry, resolved for composition.
#[derive(Debug, Clone, PartialEq)]
pub struct PerkEntryPointEntry {
    /// Zero-based rank the entry applies at (see the module docs).
    pub rank: u8,
    pub priority: u8,
    pub entry_point: u8,
    pub op: PerkOp,
    /// Conditions per `PRKC` tab.
    pub tabs: Vec<PerkConditionTab>,
}

/// What a `Quest` or `Ability` entry carries (PERK-BODY): neither is a value on a hook, so neither is in
/// the composition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerkBodyKind {
    /// "Spells and Abilities added in this way will be removed when the Perk is removed" (CK *Perk*).
    Ability { spell: u32 },
    /// An `Add Leveled List` function entry (function 8) on a hook: the list the perk owner's kill adds to the
    /// victim (PERK-HOOK-006). Not value math, so not in the composition table.
    LeveledList { entry_point: u8, list: u32 },
    /// "A Quest Perk Entry will start a specified quest and set it to a specified stage" (CK *Perk*).
    Quest { quest: u32, stage: u16 },
}

/// One `Quest` / `Ability` entry, resolved for the acquisition path.
#[derive(Debug, Clone, PartialEq)]
pub struct PerkBodyEntry {
    /// Zero-based rank the entry is authored for, as [`PerkEntryPointEntry::rank`].
    pub rank: u8,
    pub kind: PerkBodyKind,
    /// Conditions per `PRKC` tab. No vanilla master authors any on a Quest or Ability entry.
    pub tabs: Vec<PerkConditionTab>,
}

/// Whether an entry authored for zero-based `entry_rank` applies to an actor holding a perk at the one-based
/// `held_rank` (`0` = not held). One rule for every body and every hook: an entry is authored once per rank
/// it applies at (GECK *Perk Entry*, FO4 CK *Perk*), and each rank's entry is the total at that rank.
pub fn entry_applies_at(entry_rank: u8, held_rank: u8) -> bool {
    held_rank != 0 && entry_rank == held_rank - 1
}

/// What a load order's perks contribute to composition (PERK-DX-004), counted once at install.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PerkInstallSummary {
    pub dialect: PerkEntryPointDialect,
    pub perks: usize,
    /// Entry-point entries across every perk (`Quest` and `Ability` bodies are counted apart).
    pub entry_point_entries: usize,
    /// On a Consumed hook and resolved to value math: the entries a composition can apply.
    pub composed: usize,
    /// On a Consumed hook but not value math (a function or parameter shape that did not decode).
    pub skipped_undecoded: usize,
    /// On a hook that is not Consumed, by its state.
    pub pending: usize,
    pub blocked: usize,
    pub inert: usize,
    /// On a dialect with no catalog (FO4, until Phase 4) or an index the catalog has no row for.
    pub uncatalogued: usize,
    pub ability_entries: usize,
    pub quest_entries: usize,
}

impl std::fmt::Display for PerkInstallSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "perk entry points: dialect {:?}, {} perks, {} entry-point entries: {} composed, {} skipped (undecoded shape); \
             on other hooks {} pending / {} blocked / {} inert, {} uncatalogued; bodies {} ability + {} quest",
            self.dialect,
            self.perks,
            self.entry_point_entries,
            self.composed,
            self.skipped_undecoded,
            self.pending,
            self.blocked,
            self.inert,
            self.uncatalogued,
            self.ability_entries,
            self.quest_entries,
        )
    }
}

/// What the level-up menu reads of a `PERK` record (PERK-GATE): not the entries, the record's own fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PerkGateRecord {
    /// `DATA` level ("The minimum player level where this perk becomes available", FO4 CK *Perk*).
    pub level: u8,
    /// `DATA` playable: "the perk can be chosen by the player when leveling up" (CK *Perk*).
    pub playable: bool,
    /// `NNAM` "Next Perk" ("The Perk used for the next rank", FO4 CK *Perk*), `0` when absent.
    pub next_perk: u32,
    /// The record-level conditions ("Level-Up Menu Conditions", FO4 CK *Perk*).
    pub conditions: byroredux_plugin::esm::records::condition::ConditionList,
}

/// Perk entry-point entries and authored rank limits, keyed by PERK FormID.
/// `NOT_SAVED_BY_DESIGN`: re-derived from the load order every boot.
#[derive(Debug, Default, Clone)]
pub struct PerkEntryTable {
    summary: PerkInstallSummary,
    gates: HashMap<u32, Arc<PerkGateRecord>>,
    dialect: PerkEntryPointDialect,
    perks: HashMap<u32, Arc<Vec<PerkEntryPointEntry>>>,
    /// `Quest` / `Ability` entries by perk, in every game that authors them — FO4 included, whose
    /// composition table is empty.
    bodies: HashMap<u32, Arc<Vec<PerkBodyEntry>>>,
    rank_limits: HashMap<u32, u8>,
    /// #6619 — the profile's `add_perk_raises_rank` row, read once here. A
    /// game with no composed entries (FO4) still has the rank rule.
    add_raises_rank: bool,
}

impl Resource for PerkEntryTable {}

impl PerkEntryTable {
    /// Fold `index.perks` into composition entries and rank limits. Entries
    /// whose function is not value math, or whose parameters did not decode,
    /// are left out of the composition table but retain their rank limit.
    pub fn from_index(index: &EsmIndex) -> Self {
        // #5921 — the numbering is the character profile's row, not a game
        // match.
        let dialect = index.character_rules.perk_entry_points();
        let add_raises_rank = index.character_rules.add_perk_raises_rank();
        let rank_limits = index
            .perks
            .iter()
            .map(|(&form_id, perk)| (form_id, perk.num_ranks))
            .collect();
        let gates: HashMap<u32, Arc<PerkGateRecord>> = index
            .perks
            .iter()
            .map(|(&form_id, perk)| {
                let gate = PerkGateRecord {
                    level: perk.level,
                    playable: perk.playable,
                    next_perk: perk.next_perk,
                    conditions: perk.conditions.clone(),
                };
                (form_id, Arc::new(gate))
            })
            .collect();
        let mut bodies = HashMap::new();
        for (&form_id, perk) in &index.perks {
            let entries: Vec<PerkBodyEntry> = perk
                .entries
                .iter()
                .filter_map(|entry| {
                    let kind = match entry.body {
                        PerkEntryBody::Ability { spell_form_id } => PerkBodyKind::Ability { spell: spell_form_id },
                        PerkEntryBody::Quest { quest_form_id, stage } => {
                            PerkBodyKind::Quest { quest: quest_form_id, stage }
                        }
                        PerkEntryBody::EntryPoint {
                            entry_point_index,
                            function: 8,
                            function_data: PerkFunctionData::FormId(list),
                            ..
                        } => PerkBodyKind::LeveledList { entry_point: entry_point_index, list },
                        PerkEntryBody::EntryPoint { .. } => return None,
                    };
                    Some(PerkBodyEntry { rank: entry.rank, kind, tabs: entry.condition_tabs.clone() })
                })
                .collect();
            if !entries.is_empty() {
                bodies.insert(form_id, Arc::new(entries));
            }
        }
        let mut summary = PerkInstallSummary { dialect, perks: index.perks.len(), ..Default::default() };
        for perk in index.perks.values() {
            for entry in &perk.entries {
                match &entry.body {
                    PerkEntryBody::Ability { .. } => summary.ability_entries += 1,
                    PerkEntryBody::Quest { .. } => summary.quest_entries += 1,
                    PerkEntryBody::EntryPoint { entry_point_index, function, function_data, .. } => {
                        summary.entry_point_entries += 1;
                        match hook(dialect, *entry_point_index).map(|row| row.state) {
                            None => summary.uncatalogued += 1,
                            Some(HookState::Consumed { .. }) => {
                                if resolve_op(index, dialect, *entry_point_index, *function, function_data).is_some() {
                                    summary.composed += 1;
                                } else {
                                    summary.skipped_undecoded += 1;
                                }
                            }
                            Some(HookState::Pending { .. }) => summary.pending += 1,
                            Some(HookState::Blocked { .. }) => summary.blocked += 1,
                            Some(HookState::Inert { .. }) => summary.inert += 1,
                        }
                    }
                }
            }
        }
        if dialect == PerkEntryPointDialect::None {
            return Self {
                summary,
                dialect,
                perks: HashMap::new(),
                gates,
                bodies,
                rank_limits,
                add_raises_rank,
            };
        }
        let mut perks = HashMap::new();
        for (&form_id, perk) in &index.perks {
            let mut entries = Vec::new();
            for entry in &perk.entries {
                let PerkEntryBody::EntryPoint {
                    entry_point_index,
                    function,
                    function_data,
                    ..
                } = &entry.body
                else {
                    continue;
                };
                let Some(op) =
                    resolve_op(index, dialect, *entry_point_index, *function, function_data)
                else {
                    continue;
                };
                entries.push(PerkEntryPointEntry {
                    rank: entry.rank,
                    priority: entry.priority,
                    entry_point: *entry_point_index,
                    op,
                    tabs: entry.condition_tabs.clone(),
                });
            }
            if !entries.is_empty() {
                perks.insert(form_id, Arc::new(entries));
            }
        }
        Self {
            summary,
            dialect,
            perks,
            gates,
            bodies,
            rank_limits,
            add_raises_rank,
        }
    }

    /// What this load order's perks contribute (PERK-DX-004).
    pub fn summary(&self) -> &PerkInstallSummary {
        &self.summary
    }

    /// The level-up fields of one `PERK` record. `None` means the FormID is not a known PERK.
    pub fn gate(&self, perk_form_id: u32) -> Option<&PerkGateRecord> {
        self.gates.get(&perk_form_id).map(|gate| gate.as_ref())
    }

    /// The `Quest` / `Ability` entries of one perk, if it authors any.
    pub fn bodies(&self, perk_form_id: u32) -> Option<&[PerkBodyEntry]> {
        self.bodies.get(&perk_form_id).map(|entries| entries.as_slice())
    }

    /// How many perks author a `Quest` or `Ability` entry.
    pub fn body_perk_count(&self) -> usize {
        self.bodies.len()
    }

    /// The entry-point numbering the table was folded under.
    pub fn dialect(&self) -> PerkEntryPointDialect {
        self.dialect
    }

    pub fn len(&self) -> usize {
        self.perks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.perks.is_empty()
    }

    /// The entries of one perk, if it contributes any.
    pub fn entries(&self, perk_form_id: u32) -> Option<&[PerkEntryPointEntry]> {
        self.perks.get(&perk_form_id).map(|entries| entries.as_slice())
    }

    /// The authored maximum rank for a PERK record, including perks with no
    /// value-math entries. `None` means the FormID is not a known PERK.
    pub fn num_ranks(&self, perk_form_id: u32) -> Option<u8> {
        self.rank_limits.get(&perk_form_id).copied()
    }

    /// The rank cap a scripted `AddPerk` raises a held perk towards (#6619).
    /// `None` when it must not raise: the game's sources do not say it does,
    /// or the FormID is not a known PERK, so there is no Num Ranks to honour.
    pub fn add_perk_rank_cap(&self, perk_form_id: u32) -> Option<u8> {
        if self.add_raises_rank {
            self.num_ranks(perk_form_id)
        } else {
            None
        }
    }
}

/// The scratch entity an attacker's weapon base form is lent to, so weapon-tab conditions have a
/// subject. Created once at boot by [`install_weapon_subject`]. `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Clone, Copy)]
pub struct WeaponSubject(pub EntityId);

impl Resource for WeaponSubject {}

/// Create the scratch [`WeaponSubject`].
pub fn install_weapon_subject(world: &mut World) {
    world.register::<crate::scene::SceneAliasCandidate>();
    let subject = world.spawn();
    world.insert_resource(WeaponSubject(subject));
}

/// Run `f` with `subject` standing for `base_form` in conditions, then take the loan back: a
/// `SceneAliasCandidate` left on a scratch entity would be a standing, placeless alias candidate for
/// `refresh_scene_actor_bindings`, which checks no liveness or position (#6571). No two compositions
/// may lend one subject at once: every lender today runs in an exclusive system or a console command,
/// so none overlaps another. The caller must hold no storage guard.
pub(crate) fn lend_base_form<R>(world: &World, subject: EntityId, base_form: u32, f: impl FnOnce() -> R) -> R {
    if let Some(mut candidates) = world.query_mut::<crate::scene::SceneAliasCandidate>() {
        candidates.insert(
            subject,
            crate::scene::SceneAliasCandidate { base_form_id: base_form, ..Default::default() },
        );
    }
    let result = f();
    if let Some(mut candidates) = world.query_mut::<crate::scene::SceneAliasCandidate>() {
        candidates.remove(subject);
    }
    result
}

/// Each weapon base's skill, an actor-value code (`ItemKind::Weapon`'s `skill_actor_value`). Empty on
/// games whose weapon decode has no skill. `NOT_SAVED_BY_DESIGN`: re-derived from the load order every
/// boot.
#[derive(Debug, Default, Clone)]
pub struct WeaponSkillTable(HashMap<u32, u32>);

impl Resource for WeaponSkillTable {}

impl WeaponSkillTable {
    /// The skill of the weapon base `form_id`, if the table knows it.
    pub fn skill(&self, form_id: u32) -> Option<u32> {
        self.0.get(&form_id).copied()
    }

    fn from_index(index: &EsmIndex) -> Self {
        Self(
            index
                .items
                .iter()
                .filter_map(|(&form_id, item)| match item.kind {
                    ItemKind::Weapon { skill_actor_value: Some(skill), .. } => Some((form_id, skill)),
                    _ => None,
                })
                .collect(),
        )
    }
}

/// Run `f` with `attacker`'s weapon as a condition subject; `None` when it has no weapon form.
fn with_attacker_weapon<R>(world: &World, attacker: EntityId, f: impl FnOnce(Option<EntityId>) -> R) -> R {
    let subject = world.try_resource::<WeaponSubject>().map(|subject| subject.0);
    match (subject, crate::condition::attacking_weapon(world, attacker)) {
        (Some(subject), Some(weapon)) => lend_base_form(world, subject, weapon, || f(Some(subject))),
        _ => f(None),
    }
}

/// FNV hooks whose function 1 subtracts (`epftSubtract` in xEdit).
fn is_subtract_hook(dialect: PerkEntryPointDialect, entry_point: u8) -> bool {
    dialect == PerkEntryPointDialect::Fallout3NV && matches!(entry_point, 53 | 56 | 58)
}

fn resolve_op(
    index: &EsmIndex,
    dialect: PerkEntryPointDialect,
    entry_point: u8,
    function: u8,
    data: &PerkFunctionData,
) -> Option<PerkOp> {
    let av = |actor_value: u32| index.resolve_actor_value(ActorValueRef::Index(actor_value));
    // PERK-DIA-003: the dialect says which functions are value math.
    if !dialect.has_value_function(function) {
        return None;
    }
    Some(match (function, data) {
        (1, PerkFunctionData::Float(x)) if is_subtract_hook(dialect, entry_point) => {
            PerkOp::Subtract(*x)
        }
        (1, PerkFunctionData::Float(x)) => PerkOp::Set(*x),
        (2, PerkFunctionData::Float(x)) => PerkOp::Add(*x),
        (3, PerkFunctionData::Float(x)) => PerkOp::Multiply(*x),
        (4, PerkFunctionData::Range { min, max }) => PerkOp::AddRange {
            min: *min,
            max: *max,
        },
        (5, PerkFunctionData::ActorValueMult { actor_value, mult }) => PerkOp::AddAvMult {
            avif: av(*actor_value),
            mult: *mult,
        },
        (6, _) => PerkOp::Absolute,
        (7, _) => PerkOp::NegativeAbsolute,
        (12, PerkFunctionData::ActorValueMult { actor_value, mult }) => {
            PerkOp::SetToAvMult {
                avif: av(*actor_value),
                mult: *mult,
            }
        }
        (13, PerkFunctionData::ActorValueMult { actor_value, mult }) => {
            PerkOp::MultiplyAvMult {
                avif: av(*actor_value),
                mult: *mult,
            }
        }
        (14, PerkFunctionData::ActorValueMult { actor_value, mult }) => {
            PerkOp::MultiplyOnePlusAvMult {
                avif: av(*actor_value),
                mult: *mult,
            }
        }
        _ => return None,
    })
}

/// The damage `attacker` deals `target` once the attacker's perks have had
/// their say.
///
/// FO3/FNV have one hook, `Calculate Weapon Damage` (0). Skyrim has two that
/// both act on a strike: `Mod Attack Damage` (35), "applied to the base weapon
/// damage", and `Calculate Weapon Damage` (0), the "projected damage to a
/// target"; they are composed in that order — the CK describes them but does
/// not pin their relative order, and vanilla's 58 `Mod Attack Damage` against
/// 9 `Calculate Weapon Damage` entries are all multiplicative, where order
/// among them is immaterial. The weapon tab (1) is left unsupplied — equipped
/// weapons are not entities — so entries conditioned on the weapon do not
/// fire; the target is tab 2. Games without a table return `base`.
pub fn compose_outgoing_damage(
    world: &World,
    attacker: EntityId,
    target: EntityId,
    base: f32,
) -> f32 {
    let dialect = world
        .try_resource::<PerkEntryTable>()
        .map_or(PerkEntryPointDialect::None, |table| table.dialect());
    if dialect == PerkEntryPointDialect::None {
        return base;
    }
    with_attacker_weapon(world, attacker, |weapon| {
        let subjects = [weapon, Some(target)];
        if dialect == PerkEntryPointDialect::Fallout3NV {
            compose_entry_point(world, attacker, fnv::CALCULATE_WEAPON_DAMAGE, base, &subjects)
        } else {
            let modded =
                compose_entry_point(world, attacker, skyrim::MOD_ATTACK_DAMAGE, base, &subjects);
            compose_entry_point(world, attacker, skyrim::CALCULATE_WEAPON_DAMAGE, modded, &subjects)
        }
    })
}

/// The Damage Threshold `target` holds against `attacker`'s strike once both
/// sides' perks have had their say (FNV, M47.2): `Modify Damage Threshold
/// (defender)` (56) on the target's perks — the GECK's "run when the player is
/// hit; modifies the DT of the player" — then `(attacker)` (58) on the
/// attacker's, "run when an enemy is hit; modifies the DT of the enemy". Both
/// hooks carry three tabs: the perk owner, then xEdit FNV's `epcWeaponTarget`,
/// 1 the weapon and 2 the target. The weapon on both is the attacker's
/// (PERK-SUB-002). Hook 58's target is the actor hit. Hook 56's owner is the actor
/// hit, so its target is the attacker: its only authored tab-2 conditions are
/// Stonewall's and Hit the Deck's `IsWeaponSkillType`, and their descriptions say
/// "against all Melee Weapons and Unarmed attacks" and "against any and all
/// Explosives", the attack and not the defender's own weapon.
/// Games without a table return `base`.
pub fn compose_damage_threshold(
    world: &World,
    attacker: EntityId,
    target: EntityId,
    base: f32,
) -> f32 {
    let dialect = world
        .try_resource::<PerkEntryTable>()
        .map_or(PerkEntryPointDialect::None, |table| table.dialect());
    if dialect != PerkEntryPointDialect::Fallout3NV {
        return base;
    }
    with_attacker_weapon(world, attacker, |weapon| {
        let defended = compose_entry_point(
            world,
            target,
            fnv::MODIFY_DAMAGE_THRESHOLD_DEFENDER,
            base,
            &[weapon, Some(attacker)],
        );
        compose_entry_point(
            world,
            attacker,
            fnv::MODIFY_DAMAGE_THRESHOLD_ATTACKER,
            defended,
            &[weapon, Some(target)],
        )
    })
}

/// `actor`'s carry capacity (M47.2 encumbrance): its CarryWeight `base`
/// through the Get Max Carry Weight entry point (FO3 / FNV 22, Skyrim 10).
pub fn compose_max_carry_weight(world: &World, actor: EntityId, base: f32) -> f32 {
    compose_calculation(world, actor, PerkCalculation::MaxCarryWeight, base, &[None, None])
}

/// Whether `actor` holds a perk that grants VATS Concentrated Fire (FO3 / FNV entry point 33,
/// `Has Concentrated Fire`): the hook is a flag, composed from 0 so a perk that Sets it to 1 makes
/// it true. Vanilla's `ConcentratedFire` authors a single unconditional Set 1.0 at rank 0 in both
/// masters. Games with no FO3/FNV dialect never have it (#6422).
pub fn has_concentrated_fire(world: &World, actor: EntityId) -> bool {
    compose_calculation(world, actor, PerkCalculation::ConcentratedFire, 0.0, &[None, None]) > 0.5
}

/// The action points a VATS sequence that killed `victim` restores for `player` (PERK-HOOK-004): hook 35 composed
/// from a base of 0, with the player's weapon and the slain target as its tabs. `0` without the hook or a perk.
pub fn compose_player_kill_ap_reward(world: &World, player: EntityId, victim: EntityId) -> f32 {
    with_attacker_weapon(world, player, |weapon| {
        compose_calculation(world, player, PerkCalculation::PlayerKillApReward, 0.0, &[weapon, Some(victim)])
    })
}

/// The leveled lists `killer`'s perks add to `victim` on its death (PERK-HOOK-006): each `Add Leveled List`
/// entry on the dialect's *Add Leveled List On Death* hook, at the rank held, whose condition tabs pass with the
/// victim as the Target subject (tab 1: xEdit FNV `epcTarget`). In perk order, one row per entry. Empty when the
/// world has no perk table or the dialect has no such hook.
pub fn compose_death_leveled_lists(world: &World, killer: EntityId, victim: EntityId) -> Vec<u32> {
    let (hook, held, bodies): (u8, Vec<(u32, u8)>, Vec<(u32, Vec<PerkBodyEntry>)>) = {
        let Some(table) = world.try_resource::<PerkEntryTable>() else {
            return Vec::new();
        };
        let Some(hook) = PerkCalculation::DeathLeveledList.hook(table.dialect()) else {
            return Vec::new();
        };
        let Some(perks) = world.get::<Perks>(killer) else {
            return Vec::new();
        };
        let held: Vec<(u32, u8)> = perks.entries.iter().map(|perk| (perk.perk_form_id, perk.rank)).collect();
        let bodies = held
            .iter()
            .filter_map(|(perk, _)| table.bodies(*perk).map(|entries| (*perk, entries.to_vec())))
            .collect();
        (hook, held, bodies)
    };
    let mut lists = Vec::new();
    for (perk, entries) in bodies {
        let rank = held.iter().find(|(held_perk, _)| *held_perk == perk).map_or(0, |(_, rank)| *rank);
        for entry in entries {
            let PerkBodyKind::LeveledList { entry_point, list } = entry.kind else {
                continue;
            };
            if entry_point == hook
                && entry_applies_at(entry.rank, rank)
                && tabs_pass(world, killer, &entry.tabs, &[Some(victim)])
            {
                lists.push(list);
            }
        }
    }
    lists
}

/// Install the [`PerkEntryTable`] for a load order. Returns the number of
/// perks that contribute value-math entries.
pub fn install_perk_entry_points(world: &mut World, index: &EsmIndex) -> usize {
    let table = PerkEntryTable::from_index(index);
    let count = table.len();
    world.insert_resource(table);
    world.insert_resource(WeaponSkillTable::from_index(index));
    count
}

/// The composition order of an entry: ascending priority, ties in the order
/// the actor's perks (`perk_order`) and then the record's entries
/// (`entry_order`) were authored. See the module docs on why ascending.
pub fn compose_order_key(priority: u8, perk_order: usize, entry_order: usize) -> (u8, usize, usize) {
    (priority, perk_order, entry_order)
}

/// An entry that applied during a composition — for the debug console and
/// tests.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedPerkEntry {
    pub perk: u32,
    pub entry_point: u8,
    pub op: PerkOp,
    /// The running value after this entry.
    pub value: f32,
}

/// Why a held entry on the composed hook did not apply (PERK-DX-003). The first two are the §3.2 classes of
/// an entry on a Consumed hook that is not **Fireable** at this call site; the rest are the ordinary reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The entry is authored for another rank than the one held (zero-based rank).
    WrongRank { authored: u8, held: u8 },
    /// §3.2 **Subject-gated**: a conditioned tab has no subject at this call site (`tab` is the `PRKC` tab).
    SubjectGated { tab: i8 },
    /// §3.2 **Unmodelled**: a condition function the M47.1 evaluator does not model (the CTDA function index).
    Unmodelled { function_index: u32 },
    /// Fireable, and its conditions evaluated false for this owner and these subjects.
    ConditionsNotMet,
}

/// A held entry on the composed hook that did not apply.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedPerkEntry {
    pub perk: u32,
    pub entry_point: u8,
    pub op: PerkOp,
    pub reason: SkipReason,
}

/// A composed value and the entries that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct PerkComposition {
    pub value: f32,
    pub applied: Vec<AppliedPerkEntry>,
    /// The owner's other entries on this hook, and why each did not apply.
    pub skipped: Vec<SkippedPerkEntry>,
}

/// Compose `base` with every entry `owner`'s perks contribute to `entry_point`.
///
/// `tab_subjects[k - 1]` is the entity the conditions of `PRKC` tab `k` run on
/// (`k >= 1`; tab 0 is `owner`). A tab with conditions and no subject fails.
/// Returns `base` unchanged when no [`PerkEntryTable`] is installed, the owner
/// holds no perks, or nothing applies.
pub fn compose_entry_point(
    world: &World,
    owner: EntityId,
    entry_point: u8,
    base: f32,
    tab_subjects: &[Option<EntityId>],
) -> f32 {
    compose_entry_point_traced(world, owner, entry_point, base, tab_subjects).value
}

/// [`compose_entry_point`] with the applied entries reported.
pub fn compose_entry_point_traced(
    world: &World,
    owner: EntityId,
    entry_point: u8,
    base: f32,
    tab_subjects: &[Option<EntityId>],
) -> PerkComposition {
    let mut composition = PerkComposition {
        value: base,
        applied: Vec::new(),
        skipped: Vec::new(),
    };
    // Snapshot everything the walk needs, then let every guard go: the
    // condition evaluator below takes arbitrary component and resource locks.
    let held: Vec<(u32, u8)> = match world.get::<Perks>(owner) {
        Some(perks) => perks
            .entries
            .iter()
            .map(|perk| (perk.perk_form_id, perk.rank))
            .collect(),
        None => return composition,
    };
    if held.is_empty() {
        return composition;
    }
    let mut due: Vec<((u8, usize, usize), u32, PerkEntryPointEntry)> = Vec::new();
    let mut wrong_rank: Vec<SkippedPerkEntry> = Vec::new();
    {
        let Some(table) = world.try_resource::<PerkEntryTable>() else {
            return composition;
        };
        for (perk_order, (perk, rank)) in held.iter().enumerate() {
            let Some(entries) = table.perks.get(perk) else {
                continue;
            };
            for (entry_order, entry) in entries.iter().enumerate() {
                // `Perks` ranks are one-based; `PRKE` ranks are zero-based.
                if entry.entry_point != entry_point {
                    continue;
                }
                if entry_applies_at(entry.rank, *rank) {
                    due.push((
                        compose_order_key(entry.priority, perk_order, entry_order),
                        *perk,
                        entry.clone(),
                    ));
                } else {
                    wrong_rank.push(SkippedPerkEntry {
                        perk: *perk,
                        entry_point,
                        op: entry.op.clone(),
                        reason: SkipReason::WrongRank { authored: entry.rank, held: *rank },
                    });
                }
            }
        }
    }
    due.sort_by_key(|(key, _, _)| *key);

    for (_, perk, entry) in due {
        match tabs_verdict(world, owner, &entry.tabs, tab_subjects) {
            TabVerdict::Pass => {}
            failed => {
                composition.skipped.push(SkippedPerkEntry {
                    perk,
                    entry_point,
                    op: entry.op,
                    reason: match failed {
                        TabVerdict::SubjectGated { tab } => SkipReason::SubjectGated { tab },
                        TabVerdict::Unmodelled { function_index } => SkipReason::Unmodelled { function_index },
                        _ => SkipReason::ConditionsNotMet,
                    },
                });
                continue;
            }
        }
        composition.value = apply_op(world, owner, &entry.op, composition.value);
        composition.applied.push(AppliedPerkEntry {
            perk,
            entry_point,
            op: entry.op,
            value: composition.value,
        });
    }
    composition.skipped.extend(wrong_rank);
    composition
}

/// How one entry's condition tabs came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabVerdict {
    Pass,
    /// A conditioned tab has no subject (§3.2 **Subject-gated**).
    SubjectGated { tab: i8 },
    /// A condition the evaluator cannot model (§3.2 **Unmodelled**).
    Unmodelled { function_index: u32 },
    /// Every subject is supplied and every condition modelled, and the conditions are false.
    Failed,
}

/// Every condition tab passes against the subject it runs on.
fn tabs_pass(
    world: &World,
    owner: EntityId,
    tabs: &[PerkConditionTab],
    tab_subjects: &[Option<EntityId>],
) -> bool {
    tabs_verdict(world, owner, tabs, tab_subjects) == TabVerdict::Pass
}

/// [`tabs_pass`] with the reason it did not: the first tab that fails decides.
fn tabs_verdict(
    world: &World,
    owner: EntityId,
    tabs: &[PerkConditionTab],
    tab_subjects: &[Option<EntityId>],
) -> TabVerdict {
    for tab in tabs {
        if tab.conditions.is_empty() {
            continue;
        }
        // A condition the evaluator cannot model reads as 0.0, which many
        // comparisons satisfy; an entry gated on one must not fire on it
        // (the same fail-closed rule the Story Manager walk applies, #5380).
        if let Some(condition) = tab.conditions.iter().find(|condition| !crate::condition::is_modeled(world, condition)) {
            return TabVerdict::Unmodelled { function_index: condition.function_index };
        }
        let subject = match usize::try_from(tab.tab) {
            Ok(0) => Some(owner),
            Ok(k) => tab_subjects.get(k - 1).copied().flatten(),
            Err(_) => None,
        };
        let Some(subject) = subject else {
            return TabVerdict::SubjectGated { tab: tab.tab };
        };
        let mut context = ConditionContext::for_subject(subject);
        context.target = Some(owner);
        if !evaluate(&tab.conditions, world, &context) {
            return TabVerdict::Failed;
        }
    }
    TabVerdict::Pass
}

fn av_value(world: &World, owner: EntityId, avif: Option<u32>) -> f32 {
    // An actor value with no AVIF in this load order contributes nothing.
    let Some(avif) = avif else { return 0.0 };
    world
        .get::<ActorValues>(owner)
        .map_or(0.0, |values| values.current(avif))
}

fn apply_op(world: &World, owner: EntityId, op: &PerkOp, value: f32) -> f32 {
    match *op {
        PerkOp::Set(x) => x,
        PerkOp::Subtract(x) => value - x,
        PerkOp::Add(x) => value + x,
        PerkOp::Multiply(x) => value * x,
        PerkOp::AddRange { min, max } => {
            // The RNG guard lives for the draw alone.
            let unit = match world.try_resource_mut::<StoryManagerRng>() {
                Some(mut rng) => (rng.next_u64() >> 40) as f32 / (1u64 << 24) as f32,
                None => 0.5,
            };
            value + min + (max - min) * unit
        }
        PerkOp::AddAvMult { avif, mult } => value + av_value(world, owner, avif) * mult,
        PerkOp::SetToAvMult { avif, mult } => av_value(world, owner, avif) * mult,
        PerkOp::MultiplyAvMult { avif, mult } => value * av_value(world, owner, avif) * mult,
        PerkOp::MultiplyOnePlusAvMult { avif, mult } => {
            value * (1.0 + av_value(world, owner, avif) * mult)
        }
        PerkOp::Absolute => value.abs(),
        PerkOp::NegativeAbsolute => -value.abs(),
    }
}

mod bodies;
mod catalog;
mod gates;
pub use catalog::{hook, hooks, Hook, HookState, UNCLASSIFIED};
pub use gates::{plan_take_perk, PerkRefusal, PerkTake};
pub use bodies::{perk_body_system, reconcile_perk_bodies, PerkBodyState};

#[cfg(test)]
mod tests;
