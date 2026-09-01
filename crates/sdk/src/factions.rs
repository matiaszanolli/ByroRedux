//! Portable callback-local faction membership summaries.

use thiserror::Error;

use crate::identity::FormRef;

/// Maximum faction memberships exposed for one actor in one callback.
pub const MAX_FACTIONS_PER_ENTITY: usize = 256;

/// One actor's rank in one portable FACT record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactionMembership {
    faction: FormRef,
    rank: i8,
}

impl FactionMembership {
    pub fn new(faction: FormRef, rank: i8) -> Result<Self, FactionError> {
        if faction.local() == 0 {
            return Err(FactionError::NullFaction);
        }
        Ok(Self { faction, rank })
    }

    pub const fn faction(&self) -> FormRef {
        self.faction
    }

    pub const fn rank(&self) -> i8 {
        self.rank
    }
}

/// Deterministically ordered membership projection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactionSnapshot {
    memberships: Vec<FactionMembership>,
    truncated: bool,
}

impl FactionSnapshot {
    pub fn new(memberships: Vec<FactionMembership>, truncated: bool) -> Result<Self, FactionError> {
        if memberships.len() > MAX_FACTIONS_PER_ENTITY {
            return Err(FactionError::MembershipBudgetExceeded {
                maximum: MAX_FACTIONS_PER_ENTITY,
            });
        }
        if memberships
            .windows(2)
            .any(|pair| pair[0].faction() >= pair[1].faction())
        {
            return Err(FactionError::MembershipsNotStrictlySorted);
        }
        Ok(Self {
            memberships,
            truncated,
        })
    }

    pub fn memberships(&self) -> &[FactionMembership] {
        &self.memberships
    }

    pub fn rank(&self, faction: FormRef) -> Option<i8> {
        self.memberships
            .binary_search_by_key(&faction, FactionMembership::faction)
            .ok()
            .map(|index| self.memberships[index].rank())
    }

    /// True when unresolved or over-budget memberships were omitted.
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FactionError {
    #[error("faction form identity reserves local zero")]
    NullFaction,
    #[error("faction snapshot exceeds the per-entity limit of {maximum}")]
    MembershipBudgetExceeded { maximum: usize },
    #[error("faction memberships must be unique and strictly sorted by portable form identity")]
    MembershipsNotStrictlySorted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faction_snapshots_are_sorted_portable_and_ranked() {
        let first = FactionMembership::new(FormRef::new([1; 16], 1), -1).unwrap();
        let second = FactionMembership::new(FormRef::new([2; 16], 1), 3).unwrap();
        let snapshot = FactionSnapshot::new(vec![first, second], true).unwrap();
        assert_eq!(snapshot.rank(first.faction()), Some(-1));
        assert_eq!(snapshot.rank(FormRef::new([9; 16], 1)), None);
        assert!(snapshot.truncated());
        assert_eq!(
            FactionSnapshot::new(vec![second, first], false),
            Err(FactionError::MembershipsNotStrictlySorted)
        );
    }
}
