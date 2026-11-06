//! Scripted faction membership (M47.2).
//!
//! `AddToFaction`, `RemoveFromFaction` and `SetFactionRank` edit an actor's
//! live [`FactionRanks`]. That component is re-derived from the `NPC_` record
//! whenever the actor spawns (and for the player on every load), so a scripted
//! edit would vanish on the next cell load or save load. This ledger keeps the
//! edits — keyed by the actor's reference FormID (`0x14` for the player), the
//! identity the other scripted-reference ledgers use — and is replayed on top
//! of the re-derived membership.
//!
//! Semantics are the Creation Kit's *Actor Script* pages: `AddToFaction`
//! joins at rank 0 and does nothing for a member; `RemoveFromFaction` leaves;
//! `SetFactionRank` sets the rank, joining if needed.

use std::collections::HashMap;

use byroredux_core::ecs::components::FactionRanks;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

/// The player's reference FormID (`PlayerRef`), the ledger key for the player.
pub const PLAYER_REFERENCE: u32 = 0x14;

/// One scripted membership call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum FactionChange {
    /// `AddToFaction` — join at rank 0 unless already a member.
    Add,
    /// `RemoveFromFaction`.
    Remove,
    /// `SetFactionRank(rank)` — join if needed.
    SetRank(i8),
    /// `RemoveFromAllFactions` — leave every faction the actor is a member of
    /// (CK *RemoveFromAllFactions - Actor*). The call names no faction.
    RemoveAll,
}

impl FactionChange {
    /// Apply to `ranks`, returning the resulting membership (`None` = not a
    /// member).
    pub fn apply(self, ranks: &mut FactionRanks, faction: u32) -> Option<i8> {
        match self {
            Self::Add => {
                if !ranks.is_member(faction) {
                    ranks.set_rank(faction, 0);
                }
            }
            Self::Remove => {
                ranks.remove(faction);
            }
            Self::SetRank(rank) => ranks.set_rank(faction, rank),
            Self::RemoveAll => ranks.0.clear(),
        }
        ranks.rank(faction)
    }
}

/// The scripted membership of every actor a script touched: per reference
/// FormID, the faction and its resulting rank (`None` = removed). Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ScriptedFactionChanges {
    by_reference: HashMap<u32, Vec<(u32, Option<i8>)>>,
}

impl Resource for ScriptedFactionChanges {}

impl ScriptedFactionChanges {
    /// Record the resulting membership of `reference` in `faction`.
    pub fn record(&mut self, reference: u32, faction: u32, rank: Option<i8>) {
        let changes = self.by_reference.entry(reference).or_default();
        match changes.iter_mut().find(|(fid, _)| *fid == faction) {
            Some(entry) => entry.1 = rank,
            None => changes.push((faction, rank)),
        }
    }

    /// The recorded memberships of `reference`, in first-touch order.
    pub fn changes_for(&self, reference: u32) -> &[(u32, Option<i8>)] {
        self.by_reference
            .get(&reference)
            .map_or(&[][..], |changes| changes.as_slice())
    }

    /// Replay `reference`'s recorded memberships onto `ranks`.
    pub fn replay(&self, reference: u32, ranks: &mut FactionRanks) {
        for &(faction, rank) in self.changes_for(reference) {
            match rank {
                Some(rank) => ranks.set_rank(faction, rank),
                None => {
                    ranks.remove(faction);
                }
            }
        }
    }
}

/// Re-apply the ledger to a freshly (re)derived actor. Call after its
/// `FactionRanks` was stamped from the record — at reference spawn and after
/// the player's load-time reset. A no-op for an actor no script touched.
pub fn reapply_scripted_faction_changes(world: &World, entity: EntityId, reference: u32) {
    let changes: Vec<(u32, Option<i8>)> = match world.try_resource::<ScriptedFactionChanges>() {
        Some(ledger) => ledger.changes_for(reference).to_vec(),
        None => return,
    };
    if changes.is_empty() {
        return;
    }
    let Some(mut storage) = world.query_mut::<FactionRanks>() else {
        return;
    };
    if storage.get_mut(entity).is_none() {
        storage.insert(entity, FactionRanks::new());
    }
    let ranks = storage.get_mut(entity).expect("present or inserted above");
    let mut ledger = ScriptedFactionChanges::default();
    ledger.by_reference.insert(reference, changes);
    ledger.replay(reference, ranks);
}

/// #6705 — this subsystem's saved column(s). Called by the binary's `build_save_registry` in its
/// row order (the schema fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    // M47.2 — scripted AddToFaction / RemoveFromFaction / SetFactionRank,
    // replayed over the record-derived `FactionRanks` at spawn and load.
    registry.register_resource::<ScriptedFactionChanges>("ScriptedFactionChanges");
}

/// #6706 — this subsystem's world setup, owned here instead of the crate's `register` hub.
pub fn install(world: &mut byroredux_core::ecs::World) {
    // M47.2 — the scripted faction-membership ledger (saved; a restore
    // replaces this default).
    world.init_resource::<ScriptedFactionChanges>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_follow_the_creation_kit_semantics() {
        let mut ranks = FactionRanks::from_pairs([(1, 3)]);
        assert_eq!(FactionChange::Add.apply(&mut ranks, 1), Some(3), "a member keeps its rank");
        assert_eq!(FactionChange::Add.apply(&mut ranks, 2), Some(0), "joins at rank 0");
        assert_eq!(FactionChange::SetRank(-1).apply(&mut ranks, 4), Some(-1), "joins at the rank");
        assert_eq!(FactionChange::Remove.apply(&mut ranks, 1), None);
        let mut everyone = FactionRanks::from_pairs([(1, 3), (2, 0)]);
        assert_eq!(FactionChange::RemoveAll.apply(&mut everyone, 0), None);
        assert!(everyone.is_empty(), "every membership dropped");
        assert_eq!(ranks.0, vec![(2, 0), (4, -1)]);
    }

    #[test]
    fn the_ledger_keeps_the_last_state_per_faction_and_replays_it() {
        let mut ledger = ScriptedFactionChanges::default();
        ledger.record(0xA0, 1, Some(0));
        ledger.record(0xA0, 2, None);
        ledger.record(0xA0, 1, Some(2));
        assert_eq!(ledger.changes_for(0xA0), &[(1, Some(2)), (2, None)]);
        assert!(ledger.changes_for(0xB0).is_empty());

        // The record re-derived the actor in faction 2 and not in 1.
        let mut ranks = FactionRanks::from_pairs([(2, 1), (9, 0)]);
        ledger.replay(0xA0, &mut ranks);
        assert_eq!(ranks.0, vec![(9, 0), (1, 2)]);
    }

    #[test]
    fn reapply_inserts_ranks_for_an_actor_the_record_gave_none() {
        let mut world = World::new();
        world.register::<FactionRanks>();
        let actor = world.spawn();
        let mut ledger = ScriptedFactionChanges::default();
        ledger.record(0xA0, 5, Some(0));
        world.insert_resource(ledger);
        reapply_scripted_faction_changes(&world, actor, 0xA0);
        assert_eq!(world.get::<FactionRanks>(actor).unwrap().rank(5), Some(0));
        // An untouched reference is left alone.
        let other = world.spawn();
        reapply_scripted_faction_changes(&world, other, 0xB0);
        assert!(world.get::<FactionRanks>(other).is_none());
    }
}
