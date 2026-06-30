//! CHARAL character components: level, perks, provenance, faction reputation.
//!
//! The structural per-actor state layered over the numeric
//! [`ActorValues`](crate::ecs::components::ActorValues) substrate. Sparse
//! storage — only actors carry them. (Defined here, with the rest of CHARAL,
//! mirroring how `AnimationPlayer` lives in the `animation` module.)

use crate::character::reputation::{FactionRepThresholds, ReputationStanding};
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

/// One faction's accrued Fame/Infamy — the storage cell the
/// [`ReputationStanding`] classifier reads. Both axes are **monotonic** (FNV
/// reputation never drops; scripted resets zero them via [`FactionReputation
/// ::reset`]). 8 bytes (`u32` FormID + two `u16`); the vanilla maximum
/// threshold is 100, so `u16` is ample headroom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactionStanding {
    pub faction_form_id: u32,
    pub fame: u16,
    pub infamy: u16,
}

/// An actor's per-faction reputation (the [`reputation`](super::reputation)
/// family's storage half). Player-scoped in practice — NPCs don't accrue it —
/// but a component so it rides the same ECS / save machinery as the rest of
/// CHARAL. A contiguous `Vec`: an actor knows a handful of factions, so a
/// linear scan beats a map and stays cache-friendly. Karma needs no analog —
/// it is already an [`ActorValues`](crate::ecs::components::ActorValues) entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactionReputation {
    pub entries: Vec<FactionStanding>,
}

impl FactionReputation {
    #[inline]
    fn find(&self, faction_form_id: u32) -> Option<&FactionStanding> {
        self.entries
            .iter()
            .find(|f| f.faction_form_id == faction_form_id)
    }

    /// Mutable accessor that inserts a zeroed entry if the faction is unknown,
    /// so callers can accumulate without a prior `set`.
    fn entry_mut(&mut self, faction_form_id: u32) -> &mut FactionStanding {
        if let Some(i) = self
            .entries
            .iter()
            .position(|f| f.faction_form_id == faction_form_id)
        {
            &mut self.entries[i]
        } else {
            self.entries.push(FactionStanding {
                faction_form_id,
                fame: 0,
                infamy: 0,
            });
            self.entries.last_mut().unwrap()
        }
    }

    /// Accrued Fame with `faction_form_id` (`0` if unknown).
    #[inline]
    pub fn fame(&self, faction_form_id: u32) -> u16 {
        self.find(faction_form_id).map_or(0, |f| f.fame)
    }

    /// Accrued Infamy with `faction_form_id` (`0` if unknown).
    #[inline]
    pub fn infamy(&self, faction_form_id: u32) -> u16 {
        self.find(faction_form_id).map_or(0, |f| f.infamy)
    }

    /// Add Fame (monotonic — saturating, never decreases). `points` is the
    /// already-resolved bump magnitude (see
    /// [`reputation_bump_points`](super::reputation::reputation_bump_points)).
    pub fn add_fame(&mut self, faction_form_id: u32, points: u16) {
        let e = self.entry_mut(faction_form_id);
        e.fame = e.fame.saturating_add(points);
    }

    /// Add Infamy (monotonic — saturating, never decreases).
    pub fn add_infamy(&mut self, faction_form_id: u32, points: u16) {
        let e = self.entry_mut(faction_form_id);
        e.infamy = e.infamy.saturating_add(points);
    }

    /// Zero both axes for a faction — the scripted-reset exception (NCR/Legion
    /// story beats, faction-armour disguise). No-op if the faction is unknown.
    pub fn reset(&mut self, faction_form_id: u32) {
        if let Some(f) = self
            .entries
            .iter_mut()
            .find(|f| f.faction_form_id == faction_form_id)
        {
            f.fame = 0;
            f.infamy = 0;
        }
    }

    /// The [`ReputationStanding`] with `faction_form_id` given that faction's
    /// thresholds — bridges the stored Fame/Infamy to the 4×4 classifier.
    #[inline]
    pub fn standing(
        &self,
        faction_form_id: u32,
        thresholds: &FactionRepThresholds,
    ) -> ReputationStanding {
        ReputationStanding::classify(
            u32::from(self.fame(faction_form_id)),
            u32::from(self.infamy(faction_form_id)),
            thresholds,
        )
    }
}

impl Component for FactionReputation {
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
        assert!(FactionReputation::default().entries.is_empty());
    }

    #[test]
    fn faction_reputation_accumulates_monotonically_and_classifies() {
        use crate::character::reputation::fnv_faction_thresholds::BROTHERHOOD_OF_STEEL as BOS;
        const F: u32 = 0x1B2A4; // a stand-in faction FormID

        let mut rep = FactionReputation::default();
        assert_eq!(rep.fame(F), 0, "unknown faction reads 0");
        assert_eq!(rep.standing(F, &BOS), ReputationStanding::Neutral);

        // Accrue Fame to Range 2 (BoS r2 = 10) and Infamy to Range 1 (r1 = 3).
        rep.add_fame(F, 7);
        rep.add_fame(F, 5); // 12 total → Range 2
        rep.add_infamy(F, 4); // Range 1
        assert_eq!(rep.fame(F), 12);
        assert_eq!(rep.infamy(F), 4);
        // (Fame 2, Infamy 1) → Smiling Troublemaker.
        assert_eq!(rep.standing(F, &BOS), ReputationStanding::SmilingTroublemaker);

        // Monotonic: adding never lowers; saturating at u16::MAX.
        rep.add_fame(F, u16::MAX);
        assert_eq!(rep.fame(F), u16::MAX);

        // Scripted reset zeroes both axes → back to Neutral.
        rep.reset(F);
        assert_eq!(rep.fame(F), 0);
        assert_eq!(rep.infamy(F), 0);
        assert_eq!(rep.standing(F, &BOS), ReputationStanding::Neutral);
    }

    #[test]
    fn faction_standing_is_compact() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<FactionStanding>();
        assert_eq!(std::mem::size_of::<FactionStanding>(), 8);
    }
}
