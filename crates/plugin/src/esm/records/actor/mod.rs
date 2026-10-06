//! Actor-related record parsers — NPC_, RACE, CLAS, FACT.
//!
//! NPC parsing pulls the essentials needed to spawn the NPC into the world:
//! base race/class form IDs, faction memberships, inventory list, and a
//! pointer to the head/body model. Every embedded FormID field on
//! `NpcRecord` is remapped to global load-order space at parse time
//! (`parse_npc`'s `remap` param), the same convention `parse_pack` /
//! `parse_qust` / `parse_perk` / `parse_avif` / `parse_dial` / `parse_info`
//! use — see #1996.

use crate::esm::reader::GameKind;

/// #4414 — an actor's aggression (xEdit `wbAggressionEnum`, FO3 onward).
/// The CK / GECK "AI Data" pages define what each attacks on sight:
/// Unaggressive initiates nothing, Aggressive attacks Enemies, Very
/// Aggressive Enemies and Neutrals, Frenzied anyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggression {
    Unaggressive,
    Aggressive,
    VeryAggressive,
    Frenzied,
}

/// #4414 — an actor's confidence (xEdit `wbConfidenceEnum`). Only
/// `Cowardly` changes combat start: "Cowardly actors NEVER engage in combat
/// under any circumstances" (CK / GECK "AI Data").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    Cowardly,
    Cautious,
    Average,
    Brave,
    Foolhardy,
}

/// #4414 — the `AIDT` combat-start inputs, canonical across games.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorAiData {
    pub aggression: Aggression,
    pub confidence: Confidence,
    /// With "Aggro Radius Behavior" set, the distance inside which a
    /// Neutral or Enemy is attacked at once. Skyrim/FO4 author it as the
    /// Attack radius; FO3/FNV author one Aggro Radius and the GECK says a
    /// target "within half the aggro radius" starts combat immediately.
    /// The warn behaviour the outer radii drive is not modelled.
    pub attack_radius: Option<f32>,
}

/// #4414 — `AIDT` per game (xEdit `wbAIDT`): aggression @0, confidence @1;
/// FO3/FNV aggro-radius behaviour bool @15 and radius s32 @16; Skyrim/FO4
/// flags @6 (bit 0 = aggro-radius behaviour), Attack radius u32 @16.
pub fn decode_ai_data(data: &[u8], game: GameKind) -> Option<ActorAiData> {
    if data.len() < 20 || matches!(game, GameKind::Oblivion | GameKind::Starfield) {
        return None;
    }
    let aggression = match data[0] {
        0 => Aggression::Unaggressive,
        1 => Aggression::Aggressive,
        2 => Aggression::VeryAggressive,
        3 => Aggression::Frenzied,
        _ => return None,
    };
    let confidence = match data[1] {
        0 => Confidence::Cowardly,
        1 => Confidence::Cautious,
        2 => Confidence::Average,
        3 => Confidence::Brave,
        4 => Confidence::Foolhardy,
        _ => return None,
    };
    let word = |offset: usize| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    let attack_radius = if game == GameKind::Fallout3NV {
        (data[15] != 0).then(|| word(16) as i32 as f32 / 2.0)
    } else {
        (data[6] & 1 != 0).then(|| word(16) as f32)
    }
    .filter(|radius| radius.is_finite() && *radius > 0.0);
    Some(ActorAiData {
        aggression,
        confidence,
        attack_radius,
    })
}

/// One faction the NPC belongs to, with their rank within it.
#[derive(Debug, Clone, Copy)]
pub struct FactionMembership {
    pub faction_form_id: u32,
    pub rank: i8,
}

/// One inventory entry on an NPC (`CNTO` sub-record).
#[derive(Debug, Clone, Copy)]
pub struct NpcInventoryEntry {
    pub item_form_id: u32,
    pub count: i32,
}

/// ACBS flag bit 7 — "PC Level Mult".
///
/// When set, [`NpcRecord::level`] is **not** an absolute level: it is a
/// fixed-point multiplier applied to the player's level, clamped to the
/// record's `calcMin..=calcMax` band. Confirmed against vanilla data
/// (#2955): on `FalloutNV.esm` this bit and `level > 100` select the exact
/// same 268 records, and the out-of-range values are exclusively round steps
/// (`500`, `750`, … `2000`) rather than plausible levels.
///
/// Anything reading `level` as a level — the CHARAL population path, leveled
/// list expansion, XP curves — must consult this first, or a generic raider
/// reads as level 1000 and draws the top tier of every leveled list.
pub const ACBS_PC_LEVEL_MULT: u32 = 0x0080;

/// The actor's effective level for anything that treats level as a number.
///
/// #2955 — [`NpcRecord::level`] is overloaded on FO3/FNV: with the ACBS
/// [`ACBS_PC_LEVEL_MULT`] bit set it carries a fixed-point multiplier on the
/// *player's* level (vanilla values are round steps up to 2000), not an
/// absolute level. Feeding that raw into a leveled-list filter makes every
/// entry eligible, so the actor always draws the top tier; feeding it into an
/// XP curve asks for 150 050 XP instead of ~200.
///
/// The player-relative half is not modelled yet, so multiplier actors resolve
/// to their ACBS [`NpcRecord::calc_min`] — the record's own level floor, which
/// is game data rather than a derived guess. `calc_min` is `0` on records that
/// carry none, so the result is floored at 1: level 0 would make a leveled
/// list resolve to nothing at all, which trades over-levelled gear for no
/// gear.
///
/// The non-multiplier branch does NOT get that same floor: `0` is clamped only
/// up from negative (malformed/corrupt data), not up to `1`. Unlike
/// `calc_min`, a plain `level` of `0` is not a documented "record carries
/// none" sentinel — nothing distinguishes it from an authored `0`, so forcing
/// it to `1` would be inventing data the record never claimed to have. See
/// `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier`'s
/// `negative` case for the pin.
///
/// #3081 / #3171 — the SINGLE source of truth, and it lives here, beside the
/// record it reads, precisely because the two prior copies drifted. The first
/// (`byroredux/src/inventory.rs`, `09682c71`) had already diverged to
/// `.max(1)` on the non-multiplier branch before anyone noticed and was
/// deleted by #3081; the second (`actor_value_derive.rs`'s
/// `effective_npc_level`, `b434e4c0`) carried the *same* `.max(1)` divergence
/// #3081 had explicitly rejected, and outlived that fix because the
/// regression test only ever called the original. Both call sides now import
/// this one, and the pin calls it through this crate so a future copy has
/// something to fail against.
#[must_use]
pub fn effective_actor_level(npc: &NpcRecord) -> i16 {
    if npc.acbs_flags & ACBS_PC_LEVEL_MULT != 0 {
        npc.calc_min.max(1) as i16
    } else {
        npc.level.max(0)
    }
}

/// Byte length of a FO3 / FNV `CREA` `DATA` sub-record — see
/// [`CreatureStats`] for the field-by-field layout and its source.
pub const CREATURE_DATA_LEN: usize = 17;

/// FO3 / FNV `CREA` `DATA` — a creature's authored stat block.
///
/// Creature stats are **not** class-derived: `CREA` has no CLAS reference
/// at all (its `CNAM` names an `IPDS` or nothing — 0/1578 FNV and 0/533
/// FO3 records resolve one, #3383), so the `NPC_` auto-calc model does not
/// apply and this record *is* the source (#3390).
///
/// Layout from xEdit `Core/wbDefinitionsFNV.pas` `wbRecord(CREA, …)`
/// `wbStruct(DATA, …)`, byte-identical in `wbDefinitionsFO3.pas`:
///
/// ```text
/// {00} Type            u8   (0 Animal, 1 Mutated Animal, 2 Mutated Insect,
///                            3 Abomination, 4 Super Mutant, 5 Feral Ghoul,
///                            6 Robot, 7 Giant — wbCreatureTypeEnum)
/// {01} Combat Skill    u8
/// {02} Magic Skill     u8
/// {03} Stealth Skill   u8
/// {04} Health          i16
/// {06} (unused)        2 bytes
/// {08} Damage          i16
/// {10} Attributes      u8 × 7  (Strength, Perception, Endurance, Charisma,
///                               Intelligence, Agility, Luck)
/// ```
///
/// 17 bytes total. Verified against `FalloutNV.esm`
/// `VCrTier3GiantRadscorpionMedPers` (`00167EA7`), whose DATA reads
/// `02 41 32 32 96 00 00 00 3C 00 09 06 06 06 05 03 08` — Mutated Insect,
/// combat 65, health 150, damage 60, `S9 P6 E6 C6 I5 A3 L8`.
///
/// The `NPC_` `DATA` of the same games is a *different* struct (`i32` Base
/// Health + the same 7 attributes = 11 bytes, or 25 with the legacy unused
/// tail), which is why the parse arm keys on the exact 17-byte length.
///
/// [`CREATURE_DATA_LEN`] is the size the parse arm keys on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CreatureStats {
    /// `wbCreatureTypeEnum` discriminant. Retained verbatim — the engine
    /// has no creature-family taxonomy to translate it into yet.
    pub creature_type: u8,
    /// The three aggregate skill values that stand in for `NPC_`'s 13
    /// individual skills. Parsed because they are authored data; **not**
    /// derived into `ActorValues`, because FO3/FNV publish no `AVIF` these
    /// three map onto and inventing one would be a guess.
    pub combat_skill: u8,
    pub magic_skill: u8,
    pub stealth_skill: u8,
    /// Base Health, straight to the `Health` `AVIF`.
    pub health: i16,
    /// The creature's attack damage. Authored here rather than on a weapon
    /// (creatures fight unarmed); not an actor value in FO3/FNV. Reaches
    /// the spawned entity as `CreatureAttack`
    /// (`crates/core/src/ecs/components/creature_attack.rs`, #3762) — read
    /// there, not from this field, once a combat consumer exists.
    pub damage: i16,
    /// S-P-E-C-I-A-L, in `AttributeSet::FALLOUT` order.
    pub attributes: [u8; 7],
}

// #5093 — split by record (this file crossed 2120 prod_loc). Everything
// stays reachable at the historical `records::actor::*` paths through
// these glob re-exports; `tests.rs`'s `use super::*` sees the same set.
mod class;
mod faction;
mod npc;
mod race;

pub use class::*;
pub use faction::*;
pub use npc::*;
pub use race::*;

#[cfg(test)]
mod tests;
