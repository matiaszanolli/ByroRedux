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
//!   the engine cannot tell whether the weapon is a one-hander, and guessing
//!   "yes" would hand every perk bonus to every weapon.
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
    ActorValueRef, EsmIndex, PerkConditionTab, PerkEntryBody, PerkFunctionData,
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
    pub const MODIFY_DAMAGE_THRESHOLD_DEFENDER: u8 = 56;
    pub const MODIFY_DAMAGE_THRESHOLD_ATTACKER: u8 = 58;
}

/// Skyrim entry-point indices (`wbEntryPointsEnum`, xEdit `wbDefinitionsTES5`).
pub mod skyrim {
    pub const CALCULATE_WEAPON_DAMAGE: u8 = 0;
    pub const CALCULATE_MY_CRITICAL_HIT_CHANCE: u8 = 1;
    pub const CALCULATE_MY_CRITICAL_HIT_DAMAGE: u8 = 2;
    pub const GET_MAX_CARRY_WEIGHT: u8 = 10;
    pub const MOD_SNEAK_ATTACK_MULT: u8 = 18;
    pub const MOD_ATTACK_DAMAGE: u8 = 35;
    pub const MOD_INCOMING_DAMAGE: u8 = 36;
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

/// Perk entry-point entries and authored rank limits, keyed by PERK FormID.
/// `NOT_SAVED_BY_DESIGN`: re-derived from the load order every boot.
#[derive(Debug, Default, Clone)]
pub struct PerkEntryTable {
    dialect: PerkEntryPointDialect,
    perks: HashMap<u32, Arc<Vec<PerkEntryPointEntry>>>,
    rank_limits: HashMap<u32, u8>,
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
        let rank_limits = index
            .perks
            .iter()
            .map(|(&form_id, perk)| (form_id, perk.num_ranks))
            .collect();
        if dialect == PerkEntryPointDialect::None {
            return Self {
                dialect,
                perks: HashMap::new(),
                rank_limits,
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
            dialect,
            perks,
            rank_limits,
        }
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
        // 12-14 exist on Skyrim only (FNV's table ends at 9).
        (12, PerkFunctionData::ActorValueMult { actor_value, mult }) if dialect == PerkEntryPointDialect::Skyrim => {
            PerkOp::SetToAvMult {
                avif: av(*actor_value),
                mult: *mult,
            }
        }
        (13, PerkFunctionData::ActorValueMult { actor_value, mult }) if dialect == PerkEntryPointDialect::Skyrim => {
            PerkOp::MultiplyAvMult {
                avif: av(*actor_value),
                mult: *mult,
            }
        }
        (14, PerkFunctionData::ActorValueMult { actor_value, mult }) if dialect == PerkEntryPointDialect::Skyrim => {
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
    let subjects = [None, Some(target)];
    match dialect {
        PerkEntryPointDialect::Fallout3NV => {
            compose_entry_point(world, attacker, fnv::CALCULATE_WEAPON_DAMAGE, base, &subjects)
        }
        PerkEntryPointDialect::Skyrim => {
            let modded =
                compose_entry_point(world, attacker, skyrim::MOD_ATTACK_DAMAGE, base, &subjects);
            compose_entry_point(world, attacker, skyrim::CALCULATE_WEAPON_DAMAGE, modded, &subjects)
        }
        PerkEntryPointDialect::None => base,
    }
}

/// The Damage Threshold `target` holds against `attacker`'s strike once both
/// sides' perks have had their say (FNV, M47.2): `Modify Damage Threshold
/// (defender)` (56) on the target's perks — the GECK's "run when the player is
/// hit; modifies the DT of the player" — then `(attacker)` (58) on the
/// attacker's, "run when an enemy is hit; modifies the DT of the enemy". Both
/// hooks carry three tabs whose 1 and 2 are weapons (vanilla conditions there
/// are weapon-skill and weapon-list tests: Stonewall, Hit the Deck, Piercing
/// Strike), and weapons are not entities, so those tabs go unsupplied.
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
    let subjects = [None, None];
    let defended = compose_entry_point(
        world,
        target,
        fnv::MODIFY_DAMAGE_THRESHOLD_DEFENDER,
        base,
        &subjects,
    );
    compose_entry_point(
        world,
        attacker,
        fnv::MODIFY_DAMAGE_THRESHOLD_ATTACKER,
        defended,
        &subjects,
    )
}

/// `actor`'s carry capacity (M47.2 encumbrance): its CarryWeight `base`
/// through the Get Max Carry Weight entry point (FO3 / FNV 22, Skyrim 10).
pub fn compose_max_carry_weight(world: &World, actor: EntityId, base: f32) -> f32 {
    let hook = match world
        .try_resource::<PerkEntryTable>()
        .map_or(PerkEntryPointDialect::None, |table| table.dialect())
    {
        PerkEntryPointDialect::Fallout3NV => fnv::GET_MAX_CARRY_WEIGHT,
        PerkEntryPointDialect::Skyrim => skyrim::GET_MAX_CARRY_WEIGHT,
        PerkEntryPointDialect::None => return base,
    };
    compose_entry_point(world, actor, hook, base, &[None, None])
}

/// Install the [`PerkEntryTable`] for a load order. Returns the number of
/// perks that contribute value-math entries.
pub fn install_perk_entry_points(world: &mut World, index: &EsmIndex) -> usize {
    let table = PerkEntryTable::from_index(index);
    let count = table.len();
    world.insert_resource(table);
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

/// A composed value and the entries that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct PerkComposition {
    pub value: f32,
    pub applied: Vec<AppliedPerkEntry>,
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
                if entry.entry_point == entry_point && *rank != 0 && entry.rank == *rank - 1 {
                    due.push((
                        compose_order_key(entry.priority, perk_order, entry_order),
                        *perk,
                        entry.clone(),
                    ));
                }
            }
        }
    }
    due.sort_by_key(|(key, _, _)| *key);

    for (_, perk, entry) in due {
        if !tabs_pass(world, owner, &entry.tabs, tab_subjects) {
            continue;
        }
        composition.value = apply_op(world, owner, &entry.op, composition.value);
        composition.applied.push(AppliedPerkEntry {
            perk,
            entry_point,
            op: entry.op,
            value: composition.value,
        });
    }
    composition
}

/// Every condition tab passes against the subject it runs on.
fn tabs_pass(
    world: &World,
    owner: EntityId,
    tabs: &[PerkConditionTab],
    tab_subjects: &[Option<EntityId>],
) -> bool {
    tabs.iter().all(|tab| {
        if tab.conditions.is_empty() {
            return true;
        }
        // A condition the evaluator cannot model reads as 0.0, which many
        // comparisons satisfy; an entry gated on one must not fire on it
        // (the same fail-closed rule the Story Manager walk applies, #5380).
        let modeled = tab.conditions.iter().all(|condition| crate::condition::is_modeled(world, condition));
        if !modeled {
            return false;
        }
        let subject = match usize::try_from(tab.tab) {
            Ok(0) => Some(owner),
            Ok(k) => tab_subjects.get(k - 1).copied().flatten(),
            Err(_) => None,
        };
        let Some(subject) = subject else {
            return false;
        };
        let mut context = ConditionContext::for_subject(subject);
        context.target = Some(owner);
        evaluate(&tab.conditions, world, &context)
    })
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

#[cfg(test)]
mod tests;
