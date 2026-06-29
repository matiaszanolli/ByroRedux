//! CHARAL character components: level, perks, provenance.
//!
//! The structural per-actor state layered over the numeric
//! [`ActorValues`](crate::ecs::components::ActorValues) substrate. Sparse
//! storage — only actors carry them. (Defined here, with the rest of CHARAL,
//! mirroring how `AnimationPlayer` lives in the `animation` module.)

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// An actor's level and progress toward the next. Universal: Fallout drives
/// it with XP, TES with skill use — both still have a level + an accumulator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CharacterLevel {
    /// Current level.
    pub level: u16,
    /// Experience accumulated toward the **next** level (resets on level-up;
    /// compared against `LevelingModel::xp_to_next`). `u32` is ample — the
    /// per-level threshold never approaches `u32::MAX` even at FO4 extremes,
    /// and storing per-level progress (not cumulative) keeps it bounded.
    pub xp: u32,
}

impl Component for CharacterLevel {
    type Storage = SparseSetStorage<Self>;
}

/// One owned perk and its current rank. 8 bytes (the `u8` rank pads to the
/// `u32` FormID's alignment — unavoidable without bit-packing, and not worth
/// it for the handful of perks an actor holds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerkRank {
    pub perk_form_id: u32,
    pub rank: u8,
}

/// The perks an actor owns. Iterated by the perk entry-point modifier
/// pipeline, so a contiguous `Vec` (cache-friendly traversal) beats a map;
/// the occasional "owns perk X?" check is a linear scan over the few perks an
/// actor holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Perks {
    pub entries: Vec<PerkRank>,
}

impl Perks {
    /// Current rank of `perk_form_id`, or `0` if not owned.
    #[inline]
    pub fn rank(&self, perk_form_id: u32) -> u8 {
        self.entries
            .iter()
            .find(|p| p.perk_form_id == perk_form_id)
            .map_or(0, |p| p.rank)
    }

    /// Grant `perk_form_id` at `rank`, or raise an existing entry to it.
    /// Idempotent — sets the rank, never stacks duplicates.
    pub fn set_rank(&mut self, perk_form_id: u32, rank: u8) {
        if let Some(p) = self
            .entries
            .iter_mut()
            .find(|p| p.perk_form_id == perk_form_id)
        {
            p.rank = rank;
        } else {
            self.entries.push(PerkRank { perk_form_id, rank });
        }
    }

    /// Number of distinct perks owned.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when no perks are owned.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Component for Perks {
    type Storage = SparseSetStorage<Self>;
}

/// Where an actor's base stats came from — the inputs population consumed and
/// runtime leveling may reuse (TES class governs attribute multipliers; FNV
/// class tag-skills drive growth). `0` = absent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Background {
    pub race_form_id: u32,
    pub class_form_id: u32,
    // birthsign / traits (TES / Starfield) join here when those games land.
}

impl Component for Background {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perks_set_get_idempotent() {
        let mut p = Perks::default();
        assert_eq!(p.rank(0x100), 0, "unowned → 0");
        p.set_rank(0x100, 1);
        p.set_rank(0x200, 3);
        assert_eq!(p.rank(0x100), 1);
        assert_eq!(p.rank(0x200), 3);
        // Raising an existing perk replaces, doesn't duplicate.
        p.set_rank(0x100, 4);
        assert_eq!(p.rank(0x100), 4);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn character_level_and_background_are_copy_and_compact() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<CharacterLevel>();
        assert_copy::<Background>();
        assert_copy::<PerkRank>();
        assert!(std::mem::size_of::<CharacterLevel>() <= 8);
        assert_eq!(std::mem::size_of::<PerkRank>(), 8);
        assert_eq!(std::mem::size_of::<Background>(), 8);
    }

    #[test]
    fn defaults_are_empty() {
        assert_eq!(CharacterLevel::default(), CharacterLevel { level: 0, xp: 0 });
        assert!(Perks::default().is_empty());
        assert_eq!(Background::default().race_form_id, 0);
    }
}
