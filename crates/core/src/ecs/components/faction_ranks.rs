//! Per-actor faction membership component.
//!
//! Attach [`FactionRanks`] to any actor entity that belongs to factions. It
//! is the ECS surface the M47.1 `GetFactionRank` condition function reads
//! from (#1665), populated at NPC spawn from the `NPC_` record's `SNAM`
//! faction list.
//!
//! ## FormID space
//!
//! Faction FormIDs are copied verbatim from the `NpcRecord`, which remaps
//! them (along with every other embedded FormID field — `RNAM`/`CNAM`/
//! `SNAM`/… ) to global load-order space at parse time (`parse_npc`'s
//! `remap` param — see #1996). They land in the same space a remapped CTDA
//! `param_1` compares against on both single- and multi-plugin loads.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// The factions an actor belongs to, each with its rank.
///
/// Stored as `(faction_form_id, rank)` pairs — actors belong to only a
/// handful of factions, so a linear scan in [`FactionRanks::rank`] is cheaper
/// than a map. Sparse storage: most entities are not actors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactionRanks(pub Vec<(u32, i8)>);

impl FactionRanks {
    /// An empty membership list.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Build from `(faction_form_id, rank)` pairs.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (u32, i8)>) -> Self {
        Self(pairs.into_iter().collect())
    }

    /// The actor's rank in the given faction, or `None` when not a member.
    /// On the rare duplicate-faction authoring error, the first entry wins.
    pub fn rank(&self, faction_form_id: u32) -> Option<i8> {
        self.0
            .iter()
            .find(|(fid, _)| *fid == faction_form_id)
            .map(|(_, rank)| *rank)
    }

    /// Is the actor a member of the given faction (at any rank)?
    pub fn is_member(&self, faction_form_id: u32) -> bool {
        self.0.iter().any(|(fid, _)| *fid == faction_form_id)
    }

    /// Number of factions the actor belongs to.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` when the actor belongs to no factions.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Component for FactionRanks {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_finds_member_and_misses_non_member() {
        let f = FactionRanks::from_pairs([(0x0001_38B8, 2), (0x0001_5FFB, -1)]);
        assert_eq!(f.rank(0x0001_38B8), Some(2));
        assert_eq!(f.rank(0x0001_5FFB), Some(-1));
        assert_eq!(f.rank(0x0009_9999), None, "non-member → None");
        assert!(f.is_member(0x0001_38B8));
        assert!(!f.is_member(0x0009_9999));
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn empty_membership() {
        let f = FactionRanks::new();
        assert!(f.is_empty());
        assert_eq!(f.rank(0x1), None);
    }

    #[test]
    fn first_entry_wins_on_duplicate_faction() {
        let f = FactionRanks::from_pairs([(0x10, 3), (0x10, 7)]);
        assert_eq!(f.rank(0x10), Some(3));
    }
}
