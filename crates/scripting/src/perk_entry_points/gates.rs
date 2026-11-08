//! Taking a perk at the level-up menu (docs/prd/perk-entry-points.md §5.6, PERK-GATE).
//!
//! [`plan_take_perk`] answers "may the player take this perk now, and what does taking it do": it applies the
//! record's own gates — playable (PERK-GATE-003), minimum level (-002) and the level-up conditions (-001) —
//! and resolves a `NNAM` chain (-004). It changes nothing; the caller applies the plan. Script and console
//! grants (`AddPerk`, `Effect::ChangePerk`) never come through here (-005).
//!
//! **`NNAM`.** A perk that authors "Next Perk" is one rank of a chain whose records each carry the *total* at
//! that rank (`perk_body_census`: Fallout4.esm `IronFist01..05` multiply melee damage by 1.2 / 1.4 / 1.6 /
//! 1.8 / 2.0, Skyrim.esm `LightFingers00..80` add 20 / 40 / 60 / 80 / set 100), so taking the next rank
//! *replaces* the held record with the next one; keeping both would stack the totals. Skyrim's chains end at
//! a record with no `NNAM`. Fallout 4's are rings — the last rank's `NNAM` is the first record again (all
//! 300 links sit on a cycle) — and in 72 of the 79 rings the wrap is the one link that goes to a record of a
//! lower `DATA` level, which is how the last rank is recognised; the other seven rings have no level drop, so
//! a link to a record the player already holds is refused too, which bounds them.

use std::fmt;

use byroredux_core::character::{CharacterLevel, Perks};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

use super::{PerkEntryTable, PerkGateRecord};
use crate::condition::{first_failing_condition, function_of, is_modeled, ConditionContext};

/// Why the player may not take a perk.
#[derive(Debug, Clone, PartialEq)]
pub enum PerkRefusal {
    NoTable,
    UnknownPerk(u32),
    NotPlayable(u32),
    LevelTooLow { perk: u32, required: u8, level: u16 },
    /// A level-up condition failed, or is one the evaluator cannot read (`modelled == false`: the fail-closed
    /// rule, a missing value would otherwise read as 0.0 and satisfy `== 0`).
    Condition { perk: u32, index: u32, function: String, modelled: bool },
    AtMaxRank { perk: u32, max_rank: u8 },
    /// The chain has no further rank (`NNAM` back to a lower-level record, or already held).
    AtLastRank { perk: u32 },
}

impl fmt::Display for PerkRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTable => write!(f, "no PERK table is installed"),
            Self::UnknownPerk(perk) => write!(f, "{perk:08X} is not a known PERK"),
            Self::NotPlayable(perk) => write!(f, "perk {perk:08X} is not playable (it cannot be chosen at level-up)"),
            Self::LevelTooLow { perk, required, level } => {
                write!(f, "perk {perk:08X} needs level {required}, the player is level {level}")
            }
            Self::Condition { perk, index, function, modelled: true } => {
                write!(f, "perk {perk:08X} requires {function} (condition function {index}), which fails")
            }
            Self::Condition { perk, index, function, modelled: false } => write!(
                f,
                "perk {perk:08X} requires {function} (condition function {index}), which the evaluator does not model"
            ),
            Self::AtMaxRank { perk, max_rank } => {
                write!(f, "perk {perk:08X} is already at its maximum rank ({max_rank})")
            }
            Self::AtLastRank { perk } => write!(f, "perk {perk:08X} has no further rank"),
        }
    }
}

/// What taking a perk does: hold `perk` at `rank`, giving up `replaces` (a chain's previous rank).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerkTake {
    pub perk: u32,
    pub rank: u8,
    pub replaces: Option<u32>,
    /// The `Num Ranks` bound the rank was checked against.
    pub max_rank: u8,
}

/// The plan for the player taking `perk` at the level-up menu, or the first gate that refuses it.
pub fn plan_take_perk(world: &World, player: EntityId, perk: u32) -> Result<PerkTake, PerkRefusal> {
    // Everything read from the table is copied out before the evaluator takes its own locks.
    let (held_rank, record, chain_next, max_rank) = {
        let table = world.try_resource::<PerkEntryTable>().ok_or(PerkRefusal::NoTable)?;
        let record = table.gate(perk).cloned().ok_or(PerkRefusal::UnknownPerk(perk))?;
        let held_rank = world.get::<Perks>(player).map_or(0, |owned| owned.rank(perk));
        let chain_next = if held_rank > 0 && record.next_perk != 0 {
            let next = table.gate(record.next_perk).cloned().ok_or(PerkRefusal::UnknownPerk(record.next_perk))?;
            Some((record.next_perk, next, table.num_ranks(record.next_perk).unwrap_or(0)))
        } else {
            None
        };
        (held_rank, record, chain_next, table.num_ranks(perk).unwrap_or(0))
    };

    if let Some((next_perk, next, next_max)) = chain_next {
        let held_next = world.get::<Perks>(player).is_some_and(|owned| owned.rank(next_perk) > 0);
        if next.level < record.level || held_next {
            return Err(PerkRefusal::AtLastRank { perk });
        }
        check_record(world, player, next_perk, &next)?;
        return Ok(PerkTake { perk: next_perk, rank: 1, replaces: Some(perk), max_rank: next_max });
    }

    let rank = held_rank.saturating_add(1);
    if rank > max_rank {
        return Err(PerkRefusal::AtMaxRank { perk, max_rank });
    }
    check_record(world, player, perk, &record)?;
    Ok(PerkTake { perk, rank, replaces: None, max_rank })
}

/// PERK-GATE-003, -002, -001 in that order, for one record.
fn check_record(world: &World, player: EntityId, perk: u32, record: &PerkGateRecord) -> Result<(), PerkRefusal> {
    if !record.playable {
        return Err(PerkRefusal::NotPlayable(perk));
    }
    let level = world.get::<CharacterLevel>(player).map_or(0, |level| level.level);
    if u16::from(record.level) > level {
        return Err(PerkRefusal::LevelTooLow { perk, required: record.level, level });
    }
    // A condition the evaluator cannot read would default to 0.0 and could satisfy the comparison: refuse it
    // by name rather than grant on a guess.
    if let Some(condition) = record.conditions.iter().find(|condition| !is_modeled(world, condition)) {
        return Err(refusal_for(world, perk, condition.function_index, false));
    }
    let ctx = ConditionContext::for_subject(player);
    if let Some(at) = first_failing_condition(&record.conditions, world, &ctx) {
        return Err(refusal_for(world, perk, record.conditions[at].function_index, true));
    }
    Ok(())
}

fn refusal_for(world: &World, perk: u32, index: u32, modelled: bool) -> PerkRefusal {
    let function = match function_of(world, index) {
        crate::condition::ConditionFunction::Unknown(_) => format!("function {index}"),
        known => known.name().to_string(),
    };
    PerkRefusal::Condition { perk, index, function, modelled }
}
