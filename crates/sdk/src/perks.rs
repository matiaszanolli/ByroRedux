//! Portable callback-local actor perk summaries.

use thiserror::Error;

use crate::identity::FormRef;

/// Maximum ranked perks exposed for one actor in one callback.
pub const MAX_PERKS_PER_ENTITY: usize = 512;

/// One portable PERK identity and its non-zero owned rank.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PerkEntry {
    perk: FormRef,
    rank: u8,
}

impl PerkEntry {
    pub fn new(perk: FormRef, rank: u8) -> Result<Self, PerkError> {
        if perk.local() == 0 {
            return Err(PerkError::NullPerk);
        }
        if rank == 0 {
            return Err(PerkError::ZeroRank);
        }
        Ok(Self { perk, rank })
    }

    pub const fn perk(&self) -> FormRef {
        self.perk
    }

    pub const fn rank(&self) -> u8 {
        self.rank
    }
}

/// Deterministically ordered projection of an actor's owned perks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PerkSnapshot {
    entries: Vec<PerkEntry>,
    truncated: bool,
}

impl PerkSnapshot {
    pub fn new(entries: Vec<PerkEntry>, truncated: bool) -> Result<Self, PerkError> {
        if entries.len() > MAX_PERKS_PER_ENTITY {
            return Err(PerkError::PerkBudgetExceeded {
                maximum: MAX_PERKS_PER_ENTITY,
            });
        }
        if entries
            .windows(2)
            .any(|pair| pair[0].perk() >= pair[1].perk())
        {
            return Err(PerkError::PerksNotStrictlySorted);
        }
        Ok(Self { entries, truncated })
    }

    pub fn entries(&self) -> &[PerkEntry] {
        &self.entries
    }

    /// Return the owned rank, or `None` when the perk is not present.
    pub fn rank(&self, perk: FormRef) -> Option<u8> {
        self.entries
            .binary_search_by_key(&perk, PerkEntry::perk)
            .ok()
            .map(|index| self.entries[index].rank())
    }

    /// True when unresolved, invalid, duplicate, or over-budget entries were omitted.
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PerkError {
    #[error("perk form identity reserves local zero")]
    NullPerk,
    #[error("owned perk rank must be non-zero")]
    ZeroRank,
    #[error("perk snapshot exceeds the per-entity limit of {maximum}")]
    PerkBudgetExceeded { maximum: usize },
    #[error("perks must be unique and strictly sorted by portable form identity")]
    PerksNotStrictlySorted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perk_snapshots_are_portable_ranked_and_bounded() {
        let first = PerkEntry::new(FormRef::new([1; 16], 1), 1).unwrap();
        let second = PerkEntry::new(FormRef::new([2; 16], 1), 3).unwrap();
        let snapshot = PerkSnapshot::new(vec![first, second], true).unwrap();
        assert_eq!(snapshot.rank(first.perk()), Some(1));
        assert_eq!(snapshot.rank(FormRef::new([9; 16], 1)), None);
        assert!(snapshot.truncated());
        assert_eq!(
            PerkSnapshot::new(vec![second, first], false),
            Err(PerkError::PerksNotStrictlySorted)
        );
        assert_eq!(
            PerkEntry::new(FormRef::new([1; 16], 2), 0),
            Err(PerkError::ZeroRank)
        );
    }
}
