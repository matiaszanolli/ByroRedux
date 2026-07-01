//! Per-game leveling model (CHARAL).
//!
//! Bethesda's lineage advances characters two fundamentally different ways, so
//! [`LevelingModel`] is an **enum** over those shapes (the seam is which shape,
//! plus its data):
//!
//! * [`LevelingModel::XpCurve`] — **Fallout**. All three FO games share one
//!   XP-to-next shape `xp_a·L + xp_b`, so a game reduces to two curve numbers,
//!   a cap, and a [`LevelReward`] (the FO seam: FO4 spends a point on **+1
//!   SPECIAL or a perk**, FO3/FNV grant **skill points + a perk on a
//!   cadence**). Sourced in `docs/engine/charal-fo4-ruleset.md` +
//!   `charal-fnv-fo3-ruleset.md`.
//! * [`LevelingModel::SkillUse`] — **classic Elder Scrolls** (Morrowind /
//!   Oblivion). No XP: a level becomes available once a fixed number of
//!   increases accumulate in the character's *major* skills (Oblivion: 10).
//!   The level-up attribute bonuses are the deferred leveling-efficiency
//!   mechanic (`docs/engine/charal.md` §5), not modelled here. Sourced: UESP
//!   *Oblivion:Leveling*.
//!
//! Skyrim's per-skill-XP model (`SkillXp`) is a future third variant.

/// What a single level-up grants in the [`LevelingModel::XpCurve`] (Fallout)
/// model — the per-game progression seam.
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

/// A game's leveling rules. `Copy` and tiny — held inline by
/// [`super::CharacterRuleset`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LevelingModel {
    /// Fallout — an XP-to-next curve `xp_a·L + xp_b`, a hard `level_cap`
    /// (`0` = uncapped; add-ons raise it), and a per-level [`LevelReward`].
    XpCurve {
        xp_a: f32,
        xp_b: f32,
        level_cap: u16,
        reward: LevelReward,
    },
    /// Classic Elder Scrolls — a level unlocks once
    /// `major_skill_ups_per_level` increases land in the character's major
    /// skills (Oblivion: 10). `level_cap` `0` = uncapped (Oblivion has no hard
    /// engine cap; level is bounded by attribute maxing).
    SkillUse {
        major_skill_ups_per_level: u8,
        level_cap: u16,
    },
    /// Skyrim — a character-XP system fed by skill advancement. Raising a
    /// skill to rank `R` grants `R · xp_per_skill_rank` character XP; a level
    /// needs `xp_mult · level + xp_base` XP. Each level grants a perk and
    /// `pool_pick_gain` points into a chosen Health / Magicka / Stamina pool.
    /// `level_cap` `0` = uncapped (bounded by skills reaching 100). Sourced:
    /// UESP *Skyrim:Leveling* (`fXPLevelUpBase`/`fXPLevelUpMult`/
    /// `fXPPerSkillRank`).
    SkillXp {
        xp_base: f32,
        xp_mult: f32,
        xp_per_skill_rank: f32,
        pool_pick_gain: f32,
        level_cap: u16,
    },
}

impl LevelingModel {
    /// FO4 — `75·L + 125`, +1 SPECIAL or a perk per level, no hard cap.
    pub const FO4: Self = Self::XpCurve {
        xp_a: 75.0,
        xp_b: 125.0,
        level_cap: 0,
        reward: LevelReward::SpecialOrPerk,
    };

    /// FO3 — `150·L + 50`, `10 + INT` skill points + a perk every level,
    /// cap 20 (30 with *Broken Steel*).
    pub const FO3: Self = Self::XpCurve {
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
    pub const FNV: Self = Self::XpCurve {
        xp_a: 150.0,
        xp_b: 50.0,
        level_cap: 30,
        reward: LevelReward::SkillPoints {
            base: 10.0,
            int_mult: 0.5,
            perk_cadence: 2,
        },
    };

    /// Oblivion — level up after 10 major-skill increases; no hard cap.
    pub const OBLIVION: Self = Self::SkillUse {
        major_skill_ups_per_level: 10,
        level_cap: 0,
    };

    /// Skyrim — `25·L + 75` XP per level, `1` character XP per skill rank
    /// gained, +10 to a chosen Health/Magicka/Stamina + a perk each level, no
    /// hard cap.
    pub const SKYRIM: Self = Self::SkillXp {
        xp_base: 75.0,
        xp_mult: 25.0,
        xp_per_skill_rank: 1.0,
        pool_pick_gain: 10.0,
        level_cap: 0,
    };

    /// XP required to advance from `level` to `level + 1`. Fallout:
    /// `xp_a·L + xp_b`; Skyrim: `xp_mult·L + xp_base`. `0.0` for the
    /// skill-use (classic TES) model, which has no XP-to-next.
    #[inline]
    pub fn xp_to_next(&self, level: u16) -> f32 {
        match self {
            Self::XpCurve { xp_a, xp_b, .. } => xp_a * f32::from(level) + xp_b,
            Self::SkillXp {
                xp_base, xp_mult, ..
            } => xp_mult * f32::from(level) + xp_base,
            Self::SkillUse { .. } => 0.0,
        }
    }

    /// Character XP awarded for raising a skill to rank `skill_level`
    /// (`skill_level · xp_per_skill_rank`). `Some` only for the Skyrim
    /// skill-XP model; `None` for Fallout (XP comes from kills/quests) and
    /// classic TES (no XP).
    #[inline]
    pub fn xp_from_skill_rank(&self, skill_level: u16) -> Option<f32> {
        match self {
            Self::SkillXp {
                xp_per_skill_rank, ..
            } => Some(f32::from(skill_level) * xp_per_skill_rank),
            _ => None,
        }
    }

    /// Points added to the chosen Health/Magicka/Stamina pool at each level-up
    /// (Skyrim: 10). `None` for models without the pool-pick reward.
    #[inline]
    pub fn pool_pick_gain(&self) -> Option<f32> {
        match self {
            Self::SkillXp { pool_pick_gain, .. } => Some(*pool_pick_gain),
            _ => None,
        }
    }

    /// Skill points granted at a level-up for the given Intelligence, or
    /// `None` when the game awards none (FO4, or any skill-use game). The 0.5
    /// carry for odd INT in FNV is the caller's concern (accumulate the
    /// fractional part across levels); this returns the exact per-level amount.
    #[inline]
    pub fn skill_points(&self, intelligence: u8) -> Option<f32> {
        match self {
            Self::XpCurve {
                reward: LevelReward::SkillPoints { base, int_mult, .. },
                ..
            } => Some(base + int_mult * f32::from(intelligence)),
            _ => None,
        }
    }

    /// Whether reaching `level` (a level-up, so `level >= 2`) offers a perk.
    /// FO4 offers the SPECIAL-or-perk choice at every level; FO3/FNV on their
    /// cadence; Skyrim grants a perk every level; classic-TES skill-use games
    /// have no perks. The cadence phase (which exact levels) is the simple
    /// modulo; refine per game if a citing pass pins an offset.
    #[inline]
    pub fn grants_perk_at(&self, level: u16) -> bool {
        match self {
            Self::XpCurve {
                reward: LevelReward::SpecialOrPerk,
                ..
            } => true,
            Self::XpCurve {
                reward: LevelReward::SkillPoints { perk_cadence, .. },
                ..
            } => *perk_cadence != 0 && level.is_multiple_of(u16::from(*perk_cadence)),
            Self::SkillXp { .. } => true,
            Self::SkillUse { .. } => false,
        }
    }

    /// The base-game hard level cap (`0` = uncapped). Add-ons raise it; the
    /// loader bumps it when DLC is present.
    #[inline]
    pub fn level_cap(&self) -> u16 {
        match self {
            Self::XpCurve { level_cap, .. }
            | Self::SkillUse { level_cap, .. }
            | Self::SkillXp { level_cap, .. } => *level_cap,
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
        // Oblivion (skill-use) has no XP curve.
        assert_eq!(LevelingModel::OBLIVION.xp_to_next(10), 0.0);
    }

    #[test]
    fn skill_points_match_skill_rate() {
        // FO3: 10 + INT → INT 10 = 20, INT 1 = 11.
        assert_eq!(LevelingModel::FO3.skill_points(10), Some(20.0));
        assert_eq!(LevelingModel::FO3.skill_points(1), Some(11.0));
        // FNV: 10 + INT/2 → INT 10 = 15, INT 9 = 14.5 (carry is caller's job).
        assert_eq!(LevelingModel::FNV.skill_points(10), Some(15.0));
        assert_eq!(LevelingModel::FNV.skill_points(9), Some(14.5));
        // FO4 and Oblivion grant no skill points.
        assert_eq!(LevelingModel::FO4.skill_points(10), None);
        assert_eq!(LevelingModel::OBLIVION.skill_points(10), None);
    }

    #[test]
    fn perk_cadence_per_game() {
        // FO3 — every level; FNV — every other (even) level; FO4 — always.
        assert!(LevelingModel::FO3.grants_perk_at(3));
        assert!(LevelingModel::FNV.grants_perk_at(4));
        assert!(!LevelingModel::FNV.grants_perk_at(3));
        assert!(LevelingModel::FO4.grants_perk_at(7));
        // Oblivion has no perks.
        assert!(!LevelingModel::OBLIVION.grants_perk_at(5));
    }

    #[test]
    fn oblivion_levels_by_ten_major_skill_ups() {
        match LevelingModel::OBLIVION {
            LevelingModel::SkillUse {
                major_skill_ups_per_level,
                level_cap,
            } => {
                assert_eq!(major_skill_ups_per_level, 10);
                assert_eq!(level_cap, 0);
            }
            _ => panic!("Oblivion should be a skill-use model"),
        }
    }

    #[test]
    fn level_caps_per_game() {
        assert_eq!(LevelingModel::FO3.level_cap(), 20);
        assert_eq!(LevelingModel::FNV.level_cap(), 30);
        assert_eq!(LevelingModel::FO4.level_cap(), 0);
        assert_eq!(LevelingModel::OBLIVION.level_cap(), 0);
        assert_eq!(LevelingModel::SKYRIM.level_cap(), 0);
    }

    #[test]
    fn skyrim_skill_xp_matches_uesp() {
        let sky = LevelingModel::SKYRIM;
        // XP to next level = 25·L + 75 → L1 = 100, L10 = 325.
        assert_eq!(sky.xp_to_next(1), 100.0);
        assert_eq!(sky.xp_to_next(10), 325.0);
        // Character XP from raising a skill to rank 50 = 50 · 1.
        assert_eq!(sky.xp_from_skill_rank(50), Some(50.0));
        // +10 to a chosen pool + a perk every level; no skill points.
        assert_eq!(sky.pool_pick_gain(), Some(10.0));
        assert!(sky.grants_perk_at(2));
        assert_eq!(sky.skill_points(10), None);
        // The skill-XP methods are Skyrim-only.
        assert_eq!(LevelingModel::FO3.xp_from_skill_rank(50), None);
        assert_eq!(LevelingModel::OBLIVION.pool_pick_gain(), None);
    }

    #[test]
    fn leveling_model_is_copy_and_small() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<LevelingModel>();
        // Enum tag + widest variant (XpCurve) — stays within half a cache line.
        assert!(std::mem::size_of::<LevelingModel>() <= 32);
    }
}
