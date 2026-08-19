//! CHARAL character components: level, perks, provenance, faction reputation.
//!
//! The structural per-actor state layered over the numeric
//! [`ActorValues`](crate::ecs::components::ActorValues) substrate. Sparse
//! storage — only actors carry them. (Defined here, with the rest of CHARAL,
//! mirroring how `AnimationPlayer` lives in the `animation` module.)

use crate::character::reputation::{FactionRepThresholds, ReputationStanding, REPUTATION_AXIS_MAX};
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
    ///
    /// #2944 — `rank == 0` is a documented no-op, not a stored ghost entry:
    /// `rank()` already returns `0` for both "not owned" and "owned at rank
    /// 0", so inserting a zero-rank entry would be indistinguishable from
    /// not owning the perk while still costing a `Vec` slot. This method
    /// takes no `num_ranks` bound — callers that have the perk's declared
    /// max (from `PerkRecord::num_ranks`) should use [`Self::try_set_rank`]
    /// instead, which also rejects a rank past it.
    pub fn set_rank(&mut self, perk_form_id: u32, rank: u8) {
        if rank == 0 {
            return;
        }
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

    /// [`Self::set_rank`], rejecting a rank the perk doesn't have (#2944).
    ///
    /// `num_ranks` is the perk's own declared maximum
    /// (`PerkRecord::num_ranks`, parsed from `PERK`'s `DATA` sub-record).
    /// Rejects `rank == 0` (see [`Self::set_rank`]'s doc) and
    /// `rank > num_ranks` — a rank beyond the perk's ranks is a bug at the
    /// call site (a level-up path granting a rank the `PERK` record never
    /// defined), not something to silently clamp into range, per the
    /// gating checklist `docs/engine/charal-fo4-ruleset.md` § *Perk
    /// chart* asks `Perks` to validate against. Returns whether the rank
    /// was accepted.
    #[must_use]
    pub fn try_set_rank(&mut self, perk_form_id: u32, rank: u8, num_ranks: u8) -> bool {
        if rank == 0 || rank > num_ranks {
            return false;
        }
        self.set_rank(perk_form_id, rank);
        true
    }

    /// Revoke a perk if held. Returns `true` when removed. Mirrors
    /// [`crate::ecs::components::PerkList::remove`] (#2944) — the
    /// ECS-level "does this actor hold perk X" list has one; this
    /// rank-tracking component didn't.
    pub fn remove(&mut self, perk_form_id: u32) -> bool {
        if let Some(i) = self
            .entries
            .iter()
            .position(|p| p.perk_form_id == perk_form_id)
        {
            self.entries.remove(i);
            true
        } else {
            false
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
    pub repu_form_id: u32,
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
    fn find(&self, repu_form_id: u32) -> Option<&FactionStanding> {
        self.entries.iter().find(|f| f.repu_form_id == repu_form_id)
    }

    /// Mutable accessor that inserts a zeroed entry if the faction is unknown,
    /// so callers can accumulate without a prior `set`.
    fn entry_mut(&mut self, repu_form_id: u32) -> &mut FactionStanding {
        if let Some(i) = self
            .entries
            .iter()
            .position(|f| f.repu_form_id == repu_form_id)
        {
            &mut self.entries[i]
        } else {
            self.entries.push(FactionStanding {
                repu_form_id,
                fame: 0,
                infamy: 0,
            });
            self.entries.last_mut().unwrap()
        }
    }

    /// Accrued Fame with `repu_form_id` (`0` if unknown).
    #[inline]
    pub fn fame(&self, repu_form_id: u32) -> u16 {
        self.find(repu_form_id).map_or(0, |f| f.fame)
    }

    /// Accrued Infamy with `repu_form_id` (`0` if unknown).
    #[inline]
    pub fn infamy(&self, repu_form_id: u32) -> u16 {
        self.find(repu_form_id).map_or(0, |f| f.infamy)
    }

    /// Add Fame (monotonic — never decreases, clamped to the per-axis gameplay
    /// max [`REPUTATION_AXIS_MAX`] of 100). `points` is the already-resolved
    /// bump magnitude (see
    /// [`reputation_bump_points`](super::reputation::reputation_bump_points)).
    pub fn add_fame(&mut self, repu_form_id: u32, points: u16) {
        let e = self.entry_mut(repu_form_id);
        e.fame = e.fame.saturating_add(points).min(REPUTATION_AXIS_MAX);
    }

    /// Add Infamy (monotonic — never decreases, clamped to
    /// [`REPUTATION_AXIS_MAX`]).
    pub fn add_infamy(&mut self, repu_form_id: u32, points: u16) {
        let e = self.entry_mut(repu_form_id);
        e.infamy = e.infamy.saturating_add(points).min(REPUTATION_AXIS_MAX);
    }

    /// Zero both axes for a faction — the scripted-reset exception (NCR/Legion
    /// story beats, faction-armour disguise). No-op if the faction is unknown.
    pub fn reset(&mut self, repu_form_id: u32) {
        if let Some(f) = self
            .entries
            .iter_mut()
            .find(|f| f.repu_form_id == repu_form_id)
        {
            f.fame = 0;
            f.infamy = 0;
        }
    }

    /// The [`ReputationStanding`] with `repu_form_id` given that faction's
    /// thresholds — bridges the stored Fame/Infamy to the 4×4 classifier.
    #[inline]
    pub fn standing(
        &self,
        repu_form_id: u32,
        thresholds: &FactionRepThresholds,
    ) -> ReputationStanding {
        ReputationStanding::classify(
            u32::from(self.fame(repu_form_id)),
            u32::from(self.infamy(repu_form_id)),
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

    /// #2944 — `set_rank(id, 0)` must be a true no-op: no ghost entry for an
    /// unowned perk, and no silent revocation of an already-owned one either
    /// (that's what the new `remove` is for).
    #[test]
    fn set_rank_zero_is_a_no_op() {
        let mut p = Perks::default();
        p.set_rank(0x100, 0);
        assert!(
            p.is_empty(),
            "rank 0 on an unowned perk must not insert a ghost entry"
        );

        p.set_rank(0x200, 5);
        p.set_rank(0x200, 0);
        assert_eq!(
            p.rank(0x200),
            5,
            "rank 0 on an owned perk must not change its rank"
        );
        assert_eq!(p.len(), 1);
    }

    /// #2944 — the checked sibling enforces the perk's own declared max
    /// (`PerkRecord::num_ranks`) rather than accepting any `u8`.
    #[test]
    fn try_set_rank_rejects_zero_and_out_of_range() {
        let mut p = Perks::default();

        assert!(p.try_set_rank(0x100, 3, 5), "in range, accepted");
        assert_eq!(p.rank(0x100), 3);

        assert!(!p.try_set_rank(0x100, 6, 5), "past num_ranks, rejected");
        assert_eq!(p.rank(0x100), 3, "rejected write must not change state");

        assert!(!p.try_set_rank(0x100, 0, 5), "rank 0, rejected");
        assert_eq!(p.rank(0x100), 3);

        assert!(p.try_set_rank(0x100, 5, 5), "exactly num_ranks, accepted");
        assert_eq!(p.rank(0x100), 5);
    }

    /// #2944 — mirrors `PerkList::remove`'s shape: `true` iff something was
    /// actually removed.
    #[test]
    fn remove_reports_whether_held() {
        let mut p = Perks::default();
        assert!(!p.remove(0x100), "not owned, nothing to remove");

        p.set_rank(0x100, 2);
        assert!(p.remove(0x100));
        assert_eq!(p.rank(0x100), 0, "removed perk reads as unowned");
        assert!(p.is_empty());

        assert!(
            !p.remove(0x100),
            "removing twice reports no-op the second time"
        );
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
        assert_eq!(
            CharacterLevel::default(),
            CharacterLevel { level: 0, xp: 0 }
        );
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
        assert_eq!(
            rep.standing(F, &BOS),
            ReputationStanding::SmilingTroublemaker
        );

        // Monotonic: adding never lowers; clamps at the per-axis max of 100.
        rep.add_fame(F, u16::MAX);
        assert_eq!(rep.fame(F), 100);

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
