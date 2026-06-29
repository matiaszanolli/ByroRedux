//! Per-game leveling model (CHARAL).
//!
//! All three Fallout games share one XP-to-next-level shape — `a·L + b` —
//! so a game's leveling rules reduce to four numbers (`a`, `b`, cap) plus a
//! [`LevelReward`] variant. The reward is the real per-game seam: FO4 spends
//! a single point on **+1 SPECIAL or a perk**, while FO3/FNV grant
//! **skill points + a perk on a cadence** and leave SPECIAL fixed. Sourced
//! in `docs/engine/charal-fo4-ruleset.md` + `charal-fnv-fo3-ruleset.md`.

/// What a single level-up grants — the per-game progression seam.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LevelReward {
    /// FO4 / FO76: one point per level, spent on **+1 SPECIAL or one perk
    /// rank** (same pool). No skills.
    #[default]
    SpecialOrPerk,
    /// FO3 / FNV: `base + int_mult·INT` **skill points** to distribute, plus
    /// a perk every `perk_cadence` levels. SPECIAL is fixed after chargen.
    SkillPoints {
        base: f32,
        int_mult: f32,
        perk_cadence: u8,
    },
}

/// A game's leveling rules: the linear XP curve, the hard cap, and the
/// per-level reward. `Copy` and tiny — held inline by
/// [`super::CharacterRuleset`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LevelingModel {
    /// XP to advance `L → L+1` = `xp_a·L + xp_b`.
    pub xp_a: f32,
    pub xp_b: f32,
    /// Base-game hard level cap (`0` = uncapped, e.g. FO4). Add-ons raise it
    /// (FO3 20→30, FNV 30→50); the loader bumps this when DLC is present.
    pub level_cap: u16,
    /// Per-level reward.
    pub reward: LevelReward,
}

impl LevelingModel {
    /// FO4 — `75·L + 125`, +1 SPECIAL or a perk per level, no hard cap.
    pub const FO4: Self = Self {
        xp_a: 75.0,
        xp_b: 125.0,
        level_cap: 0,
        reward: LevelReward::SpecialOrPerk,
    };

    /// FO3 — `150·L + 50`, `10 + INT` skill points + a perk every level,
    /// cap 20 (30 with *Broken Steel*).
    pub const FO3: Self = Self {
        xp_a: 150.0,
        xp_b: 50.0,
        level_cap: 20,
        reward: LevelReward::SkillPoints {
            base: 10.0,
            int_mult: 1.0,
            perk_cadence: 1,
        },
    };

    /// FNV — same curve as FO3, `10 + INT/2` skill points (0.5 carries on
    /// odd INT) + a perk every other level, cap 30 (50 with all add-ons).
    pub const FNV: Self = Self {
        xp_a: 150.0,
        xp_b: 50.0,
        level_cap: 30,
        reward: LevelReward::SkillPoints {
            base: 10.0,
            int_mult: 0.5,
            perk_cadence: 2,
        },
    };

    /// XP required to advance from `level` to `level + 1` (`xp_a·L + xp_b`).
    #[inline]
    pub fn xp_to_next(&self, level: u16) -> f32 {
        self.xp_a * f32::from(level) + self.xp_b
    }

    /// Skill points granted at a level-up for the given Intelligence, or
    /// `None` when the game awards none (FO4). The 0.5 carry for odd INT in
    /// FNV is the caller's concern (accumulate the fractional part across
    /// levels); this returns the exact per-level amount.
    #[inline]
    pub fn skill_points(&self, intelligence: u8) -> Option<f32> {
        match self.reward {
            LevelReward::SkillPoints { base, int_mult, .. } => {
                Some(base + int_mult * f32::from(intelligence))
            }
            LevelReward::SpecialOrPerk => None,
        }
    }

    /// Whether reaching `level` (a level-up, so `level >= 2`) offers a perk.
    /// FO4 offers the SPECIAL-or-perk choice at every level. The cadence
    /// phase (which exact levels) is the simple modulo; refine per game if a
    /// citing pass pins an offset.
    #[inline]
    pub fn grants_perk_at(&self, level: u16) -> bool {
        match self.reward {
            LevelReward::SpecialOrPerk => true,
            LevelReward::SkillPoints { perk_cadence, .. } => {
                perk_cadence != 0 && level % u16::from(perk_cadence) == 0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_curves_match_wiki_tables() {
        // FO4: 200 at L1, 875 at L10, 1775 at L22.
        assert_eq!(LevelingModel::FO4.xp_to_next(1), 200.0);
        assert_eq!(LevelingModel::FO4.xp_to_next(10), 875.0);
        assert_eq!(LevelingModel::FO4.xp_to_next(22), 1775.0);
        // FO3/FNV: 200 at L1, 350 at L2, 800 at L5.
        assert_eq!(LevelingModel::FO3.xp_to_next(1), 200.0);
        assert_eq!(LevelingModel::FNV.xp_to_next(2), 350.0);
        assert_eq!(LevelingModel::FO3.xp_to_next(5), 800.0);
    }

    #[test]
    fn skill_points_match_skill_rate() {
        // FO3: 10 + INT → INT 10 = 20, INT 1 = 11.
        assert_eq!(LevelingModel::FO3.skill_points(10), Some(20.0));
        assert_eq!(LevelingModel::FO3.skill_points(1), Some(11.0));
        // FNV: 10 + INT/2 → INT 10 = 15, INT 9 = 14.5 (carry is caller's job).
        assert_eq!(LevelingModel::FNV.skill_points(10), Some(15.0));
        assert_eq!(LevelingModel::FNV.skill_points(9), Some(14.5));
        // FO4 grants no skill points.
        assert_eq!(LevelingModel::FO4.skill_points(10), None);
    }

    #[test]
    fn perk_cadence_per_game() {
        // FO3 — every level; FNV — every other (even) level; FO4 — always.
        assert!(LevelingModel::FO3.grants_perk_at(3));
        assert!(LevelingModel::FNV.grants_perk_at(4));
        assert!(!LevelingModel::FNV.grants_perk_at(3));
        assert!(LevelingModel::FO4.grants_perk_at(7));
    }

    #[test]
    fn leveling_model_is_copy_and_small() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<LevelingModel>();
        // 2 f32 + u16 + (enum: tag + 2 f32 + u8) — comfortably under a cache line.
        assert!(std::mem::size_of::<LevelingModel>() <= 24);
    }
}
