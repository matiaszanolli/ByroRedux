//! Portable callback-local faction reputation state and semantic mutations.

use thiserror::Error;

use crate::identity::{EntityRef, FormRef};

/// Maximum reputation records exposed for one actor callback.
pub const MAX_REPUTATIONS_PER_ENTITY: usize = 256;
/// Canonical maximum of either fame or infamy axis.
pub const REPUTATION_AXIS_MAX: u16 = 100;

/// One fame/infamy pair keyed by its authored REPU record.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReputationEntry {
    reputation: FormRef,
    fame: u16,
    infamy: u16,
}

impl ReputationEntry {
    pub fn new(reputation: FormRef, fame: u16, infamy: u16) -> Result<Self, ReputationError> {
        if reputation.local() == 0 {
            return Err(ReputationError::NullForm);
        }
        if fame > REPUTATION_AXIS_MAX || infamy > REPUTATION_AXIS_MAX {
            return Err(ReputationError::AxisOutOfRange {
                maximum: REPUTATION_AXIS_MAX,
            });
        }
        Ok(Self {
            reputation,
            fame,
            infamy,
        })
    }

    pub const fn reputation(self) -> FormRef {
        self.reputation
    }

    pub const fn fame(self) -> u16 {
        self.fame
    }

    pub const fn infamy(self) -> u16 {
        self.infamy
    }
}

/// Deterministically ordered complete or explicitly truncated reputation state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReputationSnapshot {
    entries: Vec<ReputationEntry>,
    truncated: bool,
}

impl ReputationSnapshot {
    pub fn new(
        mut entries: Vec<ReputationEntry>,
        truncated: bool,
    ) -> Result<Self, ReputationError> {
        if entries.len() > MAX_REPUTATIONS_PER_ENTITY {
            return Err(ReputationError::EntryBudgetExceeded {
                maximum: MAX_REPUTATIONS_PER_ENTITY,
            });
        }
        entries.sort_by_key(|entry| entry.reputation);
        if entries
            .windows(2)
            .any(|pair| pair[0].reputation == pair[1].reputation)
        {
            return Err(ReputationError::DuplicateForm);
        }
        Ok(Self { entries, truncated })
    }

    pub fn entries(&self) -> &[ReputationEntry] {
        &self.entries
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn get(&self, reputation: FormRef) -> Option<ReputationEntry> {
        self.entries
            .binary_search_by_key(&reputation, |entry| entry.reputation)
            .ok()
            .map(|index| self.entries[index])
    }
}

/// Mutation supported by the engine's canonical reputation component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReputationOperation {
    AddFame,
    AddInfamy,
    Reset,
}

/// Deferred semantic reputation mutation for one callback-visible actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReputationCommand {
    entity: EntityRef,
    reputation: FormRef,
    operation: ReputationOperation,
    points: u16,
}

impl ReputationCommand {
    pub fn new(
        entity: EntityRef,
        reputation: FormRef,
        operation: ReputationOperation,
        points: u16,
    ) -> Result<Self, ReputationError> {
        if reputation.local() == 0 {
            return Err(ReputationError::NullForm);
        }
        if operation == ReputationOperation::Reset && points != 0 {
            return Err(ReputationError::ResetWithPoints);
        }
        Ok(Self {
            entity,
            reputation,
            operation,
            points,
        })
    }

    pub const fn entity(self) -> EntityRef {
        self.entity
    }

    pub const fn reputation(self) -> FormRef {
        self.reputation
    }

    pub const fn operation(self) -> ReputationOperation {
        self.operation
    }

    pub const fn points(self) -> u16 {
        self.points
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ReputationError {
    #[error("reputation form identity reserves local zero")]
    NullForm,
    #[error("reputation snapshot contains a duplicate REPU identity")]
    DuplicateForm,
    #[error("reputation axis exceeds the canonical maximum of {maximum}")]
    AxisOutOfRange { maximum: u16 },
    #[error("reputation snapshot exceeds the per-entity limit of {maximum}")]
    EntryBudgetExceeded { maximum: usize },
    #[error("a reputation reset must carry zero points")]
    ResetWithPoints,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(local: u32) -> FormRef {
        FormRef::new([3; 16], local)
    }

    #[test]
    fn snapshots_are_sorted_bounded_and_portable() {
        let snapshot = ReputationSnapshot::new(
            vec![
                ReputationEntry::new(form(2), 8, 5).unwrap(),
                ReputationEntry::new(form(1), 3, 1).unwrap(),
            ],
            true,
        )
        .unwrap();
        assert_eq!(snapshot.entries()[0].reputation(), form(1));
        assert_eq!(snapshot.get(form(2)).unwrap().fame(), 8);
        assert!(snapshot.truncated());
        assert!(matches!(
            ReputationSnapshot::new(
                vec![
                    ReputationEntry::new(form(1), 0, 0).unwrap(),
                    ReputationEntry::new(form(1), 1, 1).unwrap(),
                ],
                false,
            ),
            Err(ReputationError::DuplicateForm)
        ));
        assert!(matches!(
            ReputationEntry::new(form(3), REPUTATION_AXIS_MAX + 1, 0),
            Err(ReputationError::AxisOutOfRange { .. })
        ));
    }

    #[test]
    fn reset_commands_reject_meaningless_points() {
        let entity = EntityRef::new(1, 1).unwrap();
        assert!(ReputationCommand::new(entity, form(1), ReputationOperation::Reset, 1,).is_err());
    }
}
