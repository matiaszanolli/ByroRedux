//! Portable authored relationships between faction records.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::identity::FormRef;

/// Hard bound for the active load order's authored FACT-to-FACT edges.
pub const MAX_FACTION_RELATIONSHIPS: usize = 100_000;

/// Vanilla combat-reaction meanings. Unknown authored values remain available
/// through [`FactionRelationship::combat_reaction_raw`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatReaction {
    Neutral,
    Enemy,
    Ally,
    Friend,
}

impl CombatReaction {
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::Neutral),
            1 => Some(Self::Enemy),
            2 => Some(Self::Ally),
            3 => Some(Self::Friend),
            _ => None,
        }
    }
}

/// One directional authored relationship. The reverse edge is independent and
/// is absent unless the source content authored it explicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactionRelationship {
    source: FormRef,
    target: FormRef,
    modifier: i32,
    combat_reaction: u32,
}

impl FactionRelationship {
    pub fn new(
        source: FormRef,
        target: FormRef,
        modifier: i32,
        combat_reaction: u32,
    ) -> Result<Self, FactionRelationshipError> {
        if source.local() == 0 {
            return Err(FactionRelationshipError::NullSource);
        }
        if target.local() == 0 {
            return Err(FactionRelationshipError::NullTarget);
        }
        if !(-100..=100).contains(&modifier) {
            return Err(FactionRelationshipError::ModifierOutOfRange(modifier));
        }
        Ok(Self {
            source,
            target,
            modifier,
            combat_reaction,
        })
    }

    pub const fn source(self) -> FormRef {
        self.source
    }

    pub const fn target(self) -> FormRef {
        self.target
    }

    pub const fn modifier(self) -> i32 {
        self.modifier
    }

    pub const fn combat_reaction_raw(self) -> u32 {
        self.combat_reaction
    }

    pub const fn combat_reaction(self) -> Option<CombatReaction> {
        CombatReaction::from_raw(self.combat_reaction)
    }
}

/// Immutable, deterministic relationship snapshot for the active load order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactionRelationshipCatalog {
    entries: BTreeMap<(FormRef, FormRef), FactionRelationship>,
    truncated: bool,
}

impl FactionRelationshipCatalog {
    pub fn new(
        relationships: impl IntoIterator<Item = FactionRelationship>,
        truncated: bool,
    ) -> Result<Self, FactionRelationshipError> {
        let mut entries = BTreeMap::new();
        for relationship in relationships {
            if entries.len() >= MAX_FACTION_RELATIONSHIPS {
                return Err(FactionRelationshipError::RelationshipBudgetExceeded {
                    maximum: MAX_FACTION_RELATIONSHIPS,
                });
            }
            let key = (relationship.source(), relationship.target());
            if entries.insert(key, relationship).is_some() {
                return Err(FactionRelationshipError::DuplicateRelationship {
                    from_faction: key.0,
                    to_faction: key.1,
                });
            }
        }
        Ok(Self { entries, truncated })
    }

    pub fn relationship(&self, source: FormRef, target: FormRef) -> Option<FactionRelationship> {
        self.entries.get(&(source, target)).copied()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = FactionRelationship> + '_ {
        self.entries.values().copied()
    }

    /// True when invalid, unresolved, or over-budget authored edges were omitted.
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FactionRelationshipError {
    #[error("source faction identity reserves local zero")]
    NullSource,
    #[error("target faction identity reserves local zero")]
    NullTarget,
    #[error("faction relationship modifier {0} is outside -100..=100")]
    ModifierOutOfRange(i32),
    #[error("faction relationship count exceeds {maximum}")]
    RelationshipBudgetExceeded { maximum: usize },
    #[error("duplicate faction relationship from {from_faction:?} to {to_faction:?}")]
    DuplicateRelationship {
        from_faction: FormRef,
        to_faction: FormRef,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(source: u128, local: u32) -> FormRef {
        FormRef::new(source.to_be_bytes(), local)
    }

    #[test]
    fn relationships_are_directional_portable_and_preserve_unknown_reactions() {
        let source = form(1, 0x10);
        let target = form(2, 0x20);
        let reverse = FactionRelationship::new(target, source, -25, 1).unwrap();
        let forward = FactionRelationship::new(source, target, 40, 9).unwrap();
        let catalog = FactionRelationshipCatalog::new([reverse, forward], true).unwrap();

        assert_eq!(catalog.relationship(source, target), Some(forward));
        assert_eq!(catalog.relationship(target, source), Some(reverse));
        assert_eq!(forward.combat_reaction(), None);
        assert_eq!(forward.combat_reaction_raw(), 9);
        assert_eq!(reverse.combat_reaction(), Some(CombatReaction::Enemy));
        assert_eq!(catalog.iter().count(), 2);
        assert!(catalog.truncated());
    }

    #[test]
    fn invalid_or_duplicate_edges_are_rejected() {
        let source = form(1, 0x10);
        let target = form(2, 0x20);
        assert_eq!(
            FactionRelationship::new(source, target, 101, 0),
            Err(FactionRelationshipError::ModifierOutOfRange(101))
        );
        let edge = FactionRelationship::new(source, target, 0, 0).unwrap();
        assert!(matches!(
            FactionRelationshipCatalog::new([edge, edge], false),
            Err(FactionRelationshipError::DuplicateRelationship { .. })
        ));
    }
}
