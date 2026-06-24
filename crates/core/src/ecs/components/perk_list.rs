//! Per-actor perk list component.
//!
//! Attach [`PerkList`] to any actor entity that can hold perks. It is the
//! ECS surface the perk system (`PERK` records, perk-grant/revoke) writes to
//! and the M47.1 `HasPerk` condition function reads from (#1667).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::form_id::FormId;

/// The perks an actor currently holds.
///
/// Each entry is a runtime [`FormId`] — resolve it through
/// [`FormIdPool`](crate::form_id::FormIdPool) to get the stable
/// [`FormIdPair`](crate::form_id::FormIdPair), exactly like
/// [`FormIdComponent`](crate::ecs::components::FormIdComponent). Sparse
/// because only actors carry perks (most entities never do).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PerkList(pub Vec<FormId>);

impl PerkList {
    /// An empty perk list.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Build a perk list from an iterator of [`FormId`]s.
    pub fn from_perks(perks: impl IntoIterator<Item = FormId>) -> Self {
        Self(perks.into_iter().collect())
    }

    /// Does the actor hold the given perk?
    pub fn contains(&self, perk: FormId) -> bool {
        self.0.contains(&perk)
    }

    /// Grant a perk if not already held. Returns `true` when added.
    pub fn add(&mut self, perk: FormId) -> bool {
        if self.0.contains(&perk) {
            false
        } else {
            self.0.push(perk);
            true
        }
    }

    /// Revoke a perk if held. Returns `true` when removed.
    pub fn remove(&mut self, perk: FormId) -> bool {
        if let Some(i) = self.0.iter().position(|&p| p == perk) {
            self.0.remove(i);
            true
        } else {
            false
        }
    }

    /// Number of perks held.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` when the actor holds no perks.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Component for PerkList {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    fn perk(pool: &mut FormIdPool, local: u32) -> FormId {
        pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(local),
        })
    }

    #[test]
    fn add_is_idempotent_and_contains_tracks_membership() {
        let mut pool = FormIdPool::new();
        let a = perk(&mut pool, 0x58F80); // Adept Destruction
        let b = perk(&mut pool, 0x58F81);

        let mut perks = PerkList::new();
        assert!(perks.is_empty());
        assert!(perks.add(a));
        assert!(!perks.add(a), "re-granting the same perk is a no-op");
        assert!(perks.contains(a));
        assert!(!perks.contains(b));
        assert_eq!(perks.len(), 1);
    }

    #[test]
    fn remove_reports_whether_held() {
        let mut pool = FormIdPool::new();
        let a = perk(&mut pool, 0x58F80);
        let b = perk(&mut pool, 0x58F81);

        let mut perks = PerkList::from_perks([a]);
        assert!(perks.remove(a));
        assert!(!perks.remove(a), "removing a missing perk returns false");
        assert!(!perks.remove(b));
        assert!(perks.is_empty());
    }
}
