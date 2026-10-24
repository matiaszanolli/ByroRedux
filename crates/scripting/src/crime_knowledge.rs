//! Who knows of which crime — the canonical record the legacy condition
//! functions and ObScript getters ask about, across games.
//!
//! The CS wiki: *GetCrime* "returns 1 if the criminal has committed the
//! specified crime type against the calling actor, AND the calling actor knows
//! about the crime"; *GetCrimeKnown* `<type> <criminal> <victim>` "returns 1 if
//! the calling actor knows about the specified crime". Both ask the same
//! question of one record: an [`CrimeEntry`] says `knower` knows that `criminal`
//! committed a `kind` crime against `victim`.
//!
//! Anything that sees or hears of a crime writes the record, whatever a game
//! then does about it (Oblivion fines it and sends guards; the later games
//! have their own consequences): the witnesses see it, and the guards a
//! witness calls learn of it. Everything is keyed by reference FormID (the
//! player is `0x14`), so it survives a cell unloading, and it is saved. Paying
//! the bounty or serving the sentence forgives the criminal: their entries go.

use std::collections::BTreeSet;

use byroredux_core::character::CrimeKind;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

/// One fact: `knower` knows `criminal` committed a `kind` crime against `victim`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct CrimeEntry {
    pub knower: u32,
    pub criminal: u32,
    pub kind: CrimeKind,
    /// The reference (or, for theft from an actor's property, the actor base)
    /// the crime was against; 0 when it was against no one in particular (a
    /// faction's property).
    pub victim: u32,
}

/// Every crime anyone knows of. See the module docs.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct CrimeKnowledge {
    entries: BTreeSet<CrimeEntry>,
}

impl Resource for CrimeKnowledge {}

impl CrimeKnowledge {
    pub fn record(&mut self, entry: CrimeEntry) {
        self.entries.insert(entry);
    }

    /// Whether `knower` knows of `criminal` committing `kind` (any crime when `None`)
    /// against `victim` (anyone when `None`).
    pub fn knows(&self, knower: u32, criminal: u32, kind: Option<CrimeKind>, victim: Option<u32>) -> bool {
        self.entries.iter().any(|entry| {
            entry.knower == knower
                && entry.criminal == criminal
                && kind.is_none_or(|kind| entry.kind == kind)
                && victim.is_none_or(|victim| entry.victim == victim)
        })
    }

    /// Whether `knower` knows of a `kind` crime by `criminal` committed against one of
    /// `victims` (the calling actor's own reference and base, for *GetCrime*).
    pub fn knows_against_any(&self, knower: u32, criminal: u32, kind: CrimeKind, victims: &[u32]) -> bool {
        victims.iter().any(|&victim| self.knows(knower, criminal, Some(kind), Some(victim)))
    }

    /// `criminal` is forgiven: nobody knows of their crimes any more.
    pub fn forgive(&mut self, criminal: u32) {
        self.entries.retain(|entry| entry.criminal != criminal);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// This subsystem's resource.
pub fn install(world: &mut World) {
    world.init_resource::<CrimeKnowledge>();
}

/// This subsystem's saved column. Called by the binary's `build_save_registry`.
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<CrimeKnowledge>("CrimeKnowledge");
}

/// Note that `knower` knows of a crime. A no-op for an entity with no reference FormID.
pub fn note_known(world: &World, knower: EntityId, criminal: u32, kind: CrimeKind, victim: u32) {
    let Some(knower) = crate::obscript_state::reference_form(world, knower) else { return };
    if let Some(mut knowledge) = world.try_resource_mut::<CrimeKnowledge>() {
        knowledge.record(CrimeEntry { knower, criminal, kind, victim });
    }
}

/// The player's crimes are forgiven (bounty paid or sentence served).
pub fn forgive_player(world: &World) {
    if let Some(mut knowledge) = world.try_resource_mut::<CrimeKnowledge>() {
        knowledge.forgive(crate::PLAYER_REFERENCE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUARD: u32 = 0xA1;
    const BOB: u32 = 0xB0;
    const JOE: u32 = 0xC0;

    #[test]
    fn a_crime_is_known_by_who_saw_it_and_matches_by_kind_and_victim() {
        let mut knowledge = CrimeKnowledge::default();
        knowledge.record(CrimeEntry { knower: GUARD, criminal: JOE, kind: CrimeKind::Murder, victim: BOB });
        assert!(knowledge.knows(GUARD, JOE, Some(CrimeKind::Murder), Some(BOB)));
        assert!(knowledge.knows(GUARD, JOE, None, None), "any crime, any victim");
        assert!(!knowledge.knows(GUARD, JOE, Some(CrimeKind::Steal), None), "another kind");
        assert!(!knowledge.knows(GUARD, JOE, None, Some(0xDD)), "another victim");
        assert!(!knowledge.knows(BOB, JOE, None, None), "someone who did not see it");
        assert!(!knowledge.knows(GUARD, 0x14, None, None), "another criminal");
    }

    #[test]
    fn get_crime_asks_about_a_crime_against_the_caller() {
        let mut knowledge = CrimeKnowledge::default();
        knowledge.record(CrimeEntry { knower: BOB, criminal: JOE, kind: CrimeKind::Attack, victim: BOB });
        assert!(knowledge.knows_against_any(BOB, JOE, CrimeKind::Attack, &[BOB, 0x99]));
        assert!(!knowledge.knows_against_any(GUARD, JOE, CrimeKind::Attack, &[GUARD]), "the guard saw no crime against him");
    }

    #[test]
    fn forgiving_a_criminal_erases_what_is_known_of_them_alone() {
        let mut knowledge = CrimeKnowledge::default();
        knowledge.record(CrimeEntry { knower: GUARD, criminal: JOE, kind: CrimeKind::Steal, victim: 0 });
        knowledge.record(CrimeEntry { knower: GUARD, criminal: 0x14, kind: CrimeKind::Steal, victim: 0 });
        knowledge.forgive(0x14);
        assert!(knowledge.knows(GUARD, JOE, None, None) && !knowledge.knows(GUARD, 0x14, None, None));
        assert_eq!(knowledge.len(), 1);
    }

    #[cfg(feature = "save")]
    #[test]
    fn the_record_survives_a_json_save() {
        let mut knowledge = CrimeKnowledge::default();
        knowledge.record(CrimeEntry { knower: GUARD, criminal: 0x14, kind: CrimeKind::Murder, victim: BOB });
        let json = serde_json::to_value(&knowledge).expect("serializes");
        assert_eq!(serde_json::from_value::<CrimeKnowledge>(json).unwrap(), knowledge);
    }
}
