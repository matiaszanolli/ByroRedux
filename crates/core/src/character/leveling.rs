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
//!   The level-up attribute bonuses are implemented by the TES leveling
//!   helpers (`docs/engine/charal.md` §5). Sourced: UESP
//!   *Oblivion:Leveling*.
//! * [`LevelingModel::SkillXp`] — **Skyrim**. A third shape, neither of the
//!   above: skill *use* grants XP (1 per skill rank, `xp_from_skill_rank`)
//!   against a character XP-to-next curve `xp_mult·L + xp_base`
//!   (`SKYRIM` = 25·L + 75), and each level-up grants a +10 Health/Magicka/
//!   Stamina pool pick plus a perk. Sourced in
//!   `docs/engine/charal-skyrim-ruleset.md`; the curve constants overlay the
//!   authored `fXPLevelUpBase` / `fXPLevelUpMult` GMSTs when present. The
//!   skill-rank coefficient is an engine rule, not an authored setting.
//!
//! Starfield is a documented **fourth** shape (skill points spent on
//! challenge-gated per-skill ranks) and is deliberately absent: its XP curve
//! and category-spend thresholds are unpublished research (`charal.md` §9),
//! so no `LevelingModel::STARFIELD` is buildable yet.

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
    /// Fallout — an XP-to-next curve `xp_a·L + xp_b`, a `level_cap` (`0` =
    /// uncapped), and a per-level [`LevelReward`]. The curve and cap overlay
    /// the authored `iXPBumpBase` / `iXPBase` / `iMaxCharacterLevel` GMSTs
    /// ([`LevelingModel::with_gmst`]), so a DLC master that authors a higher
    /// cap (`BrokenSteel.esm`) is honoured through the merged load order. New
    /// Vegas's add-ons author none — the executable adds their +5 each — so
    /// that raise is profile data applied after the overlay
    /// ([`LevelingModel::raise_cap`], #6227).
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
    /// UESP *Skyrim:Leveling*. Only the level curve is GMST-authored;
    /// `xp_per_skill_rank` is the engine's fixed rank-to-XP coefficient.
    SkillXp {
        xp_base: f32,
        xp_mult: f32,
        xp_per_skill_rank: f32,
        pool_pick_gain: f32,
        level_cap: u16,
    },
}

impl LevelingModel {
    /// Overlay authored GMST values on the engine-supplied fallback model.
    /// Missing settings retain the sourced constants, keeping older masters
    /// and synthetic fixtures deterministic while allowing mods to retune the
    /// parsed curve.
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        match self {
            Self::SkillXp {
                xp_base,
                xp_mult,
                xp_per_skill_rank,
                pool_pick_gain,
                level_cap,
            } => Self::SkillXp {
                xp_base: gmst("fXPLevelUpBase").unwrap_or(xp_base),
                xp_mult: gmst("fXPLevelUpMult").unwrap_or(xp_mult),
                xp_per_skill_rank,
                pool_pick_gain,
                level_cap,
            },
            // #5915 — Fallout's curve and cap are authored too. GECK
            // *Experience Settings*: XP needed for level n is
            // `(n−1)·((n−2)/2·iXPBumpBase + iXPBase)`, and differencing gives
            // `xp_to_next(L) = iXPBumpBase·L + (iXPBase − iXPBumpBase)`. A
            // setting the load order does not author keeps the sourced
            // constant (all three games' constants satisfy
            // `xp_a + xp_b = iXPBase`, so the implied base is `xp_a + xp_b`),
            // and `iMaxCharacterLevel` replaces the cap only when authored —
            // `BrokenSteel.esm` raises FO3's 20 to 30.
            Self::XpCurve {
                xp_a,
                xp_b,
                level_cap,
                reward,
            } => {
                let bump = gmst("iXPBumpBase").unwrap_or(xp_a);
                let base = gmst("iXPBase").unwrap_or(xp_a + xp_b);
                let authored_cap = gmst("iMaxCharacterLevel")
                    .filter(|cap| cap.is_finite() && *cap >= 1.0)
                    .map(|cap| cap.min(f32::from(u16::MAX)) as u16);
                Self::XpCurve {
                    xp_a: bump,
                    xp_b: base - bump,
                    level_cap: authored_cap.unwrap_or(level_cap),
                    reward,
                }
            }
            other => other,
        }
    }

    /// #6227 — add `levels` to a capped model's cap: the raises an executable
    /// applies for a loaded plugin rather than a master authoring them. An
    /// uncapped model (`0`) stays uncapped, and a model with no cap concept
    /// is unchanged.
    #[must_use]
    pub fn raise_cap(self, levels: u16) -> Self {
        match self {
            Self::XpCurve { xp_a, xp_b, level_cap, reward } if level_cap > 0 => Self::XpCurve {
                xp_a,
                xp_b,
                level_cap: level_cap.saturating_add(levels),
                reward,
            },
            other => other,
        }
    }

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
    /// odd INT) + a perk every other level, cap 30 (50 with all four story
    /// add-ons — applied by `CharacterRulesProfile::level_cap_raise`, #6227).
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

    /// What reaching `new_level` grants under the Fallout rewards: for FO3 /
    /// FNV [`LevelReward::SkillPoints`], `base + int_mult·INT` skill points
    /// (before the Adjust Gained Skill Points perk hook) and whether a perk
    /// comes with it (every `perk_cadence` levels); for FO4
    /// [`LevelReward::SpecialOrPerk`], no skill points and the level's one
    /// SPECIAL-or-perk point. `None` for the Elder Scrolls models.
    pub fn skill_point_reward(&self, new_level: u16, intelligence: f32) -> Option<(f32, bool)> {
        match *self {
            Self::XpCurve {
                reward:
                    LevelReward::SkillPoints {
                        base,
                        int_mult,
                        perk_cadence,
                    },
                ..
            } => Some((
                base + int_mult * intelligence,
                perk_cadence > 0 && new_level.is_multiple_of(u16::from(perk_cadence)),
            )),
            Self::XpCurve {
                reward: LevelReward::SpecialOrPerk,
                ..
            } => Some((0.0, true)),
            _ => None,
        }
    }

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

    /// Roll `progress` through every level its accumulated XP pays for, up to
    /// the cap, and return the levels reached in order (#5918).
    ///
    /// A level costs [`Self::xp_to_next`] rounded **up** to whole XP (progress
    /// is integral) and at least 1: a mod-authored curve with `0 < cost < 1`
    /// used to truncate to a zero-XP level that never drained `xp`, spinning
    /// forever, and a fractional cost silently dropped its remainder every
    /// level. The roll also stops at `u16::MAX`, where `level` saturates, so no
    /// curve can loop unboundedly. Models without an XP-to-next (classic TES
    /// skill-use) roll nothing.
    pub fn roll_levels(&self, progress: &mut super::CharacterLevel) -> Vec<u16> {
        let mut reached = Vec::new();
        loop {
            let to_next = self.xp_to_next(progress.level);
            let capped = self.level_cap() > 0 && progress.level >= self.level_cap();
            if capped || progress.level == u16::MAX || to_next.is_nan() || to_next <= 0.0 {
                break;
            }
            // `ceil` of a huge/infinite curve saturates in the cast: such a
            // level is simply never reached.
            let cost = (to_next.ceil() as u32).max(1);
            if progress.xp < cost {
                break;
            }
            progress.xp -= cost;
            progress.level += 1;
            reached.push(progress.level);
        }
        reached
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

    /// Skill-point portion of [`Self::skill_point_reward`] for the given
    /// Intelligence. FO4 returns `Some(0.0)` because its level reward is a
    /// SPECIAL-or-perk choice; skill-use games return `None`.
    #[inline]
    pub fn skill_points(&self, intelligence: u8) -> Option<f32> {
        self.skill_point_reward(0, f32::from(intelligence))
            .map(|(points, _)| points)
    }

    /// Whether reaching `level` offers a perk, using the perk flag from
    /// [`Self::skill_point_reward`]. Skyrim's `SkillXp` model is the one
    /// exception: its level reward is outside the Fallout helper, but it
    /// grants a perk at every level. Classic-TES skill-use games grant none.
    #[inline]
    pub fn grants_perk_at(&self, level: u16) -> bool {
        self.skill_point_reward(level, 0.0)
            .map(|(_, perk)| perk)
            .unwrap_or(matches!(self, Self::SkillXp { .. }))
    }

    /// The hard level cap (`0` = uncapped): the sourced base-game constant,
    /// or the load order's authored `iMaxCharacterLevel` where
    /// [`Self::with_gmst`] overlaid one.
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

    /// M47.2 — Fallout wiki *Skill point*: FO3 `10 + INT` with a perk every
    /// level, FNV `10 + INT/2` with a perk every other level; the FNV half
    /// point carries ("with 9 Intelligence, the first level up nets 14
    /// points. The next level up will net 15 points").
    #[test]
    fn skill_point_rewards_follow_each_fallout() {
        use crate::character::LevelUpPoints;
        assert_eq!(LevelingModel::FO3.skill_point_reward(2, 7.0), Some((17.0, true)));
        assert_eq!(LevelingModel::FNV.skill_point_reward(2, 9.0), Some((14.5, true)));
        assert_eq!(LevelingModel::FNV.skill_point_reward(3, 9.0), Some((14.5, false)));
        assert_eq!(LevelingModel::SKYRIM.skill_point_reward(2, 9.0), None);
        assert_eq!(LevelingModel::FO4.skill_point_reward(2, 9.0), Some((0.0, true)));
        let mut banked = LevelUpPoints::default();
        banked.grant_skill_points(14.5);
        assert_eq!(banked.skill_points, 14);
        banked.grant_skill_points(14.5);
        assert_eq!(banked.skill_points, 29, "14 then 15");
    }

    /// #5918 — a fractional or sub-1 authored cost must neither spin nor
    /// leak XP; the roll is bounded for any curve.
    #[test]
    fn roll_levels_is_bounded_and_exact_for_odd_curves() {
        use crate::character::CharacterLevel;
        let curve = |xp_base: f32, xp_mult: f32| LevelingModel::SkillXp {
            xp_base,
            xp_mult,
            xp_per_skill_rank: 1.0,
            pool_pick_gain: 10.0,
            level_cap: 0,
        };

        // 0 < cost < 1 used to truncate to 0 and loop forever. Each level now
        // costs 1 XP, so 5 XP is exactly 5 levels.
        let mut progress = CharacterLevel { level: 1, xp: 5 };
        assert_eq!(curve(0.5, 0.0).roll_levels(&mut progress), [2, 3, 4, 5, 6]);
        assert_eq!(progress, CharacterLevel { level: 6, xp: 0 });

        // A fractional cost rounds up and charges the whole XP it needs.
        let mut progress = CharacterLevel { level: 1, xp: 205 };
        assert_eq!(curve(100.5, 0.0).roll_levels(&mut progress), [2, 3]);
        assert_eq!(progress.xp, 205 - 2 * 101);

        // Even a free curve stops where the level saturates.
        let mut progress = CharacterLevel { level: u16::MAX - 2, xp: u32::MAX };
        assert_eq!(curve(0.5, 0.0).roll_levels(&mut progress), [u16::MAX - 1, u16::MAX]);

        // Vanilla Skyrim (25·L + 75) and a Fallout cap are unchanged.
        let mut progress = CharacterLevel { level: 1, xp: 100 };
        assert_eq!(LevelingModel::SKYRIM.roll_levels(&mut progress), [2]);
        assert_eq!(progress.xp, 0);
        let mut progress = CharacterLevel { level: 20, xp: 100_000 };
        assert!(LevelingModel::FO3.roll_levels(&mut progress).is_empty(), "FO3 caps at 20");
        // A model with no XP-to-next rolls nothing.
        let mut progress = CharacterLevel { level: 3, xp: 999 };
        assert!(LevelingModel::OBLIVION.roll_levels(&mut progress).is_empty());
    }

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
        // FO4's combined reward has no skill points; its points component is 0.
        assert_eq!(LevelingModel::FO4.skill_points(10), Some(0.0));
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

    /// #6227 — a raise lifts a capped curve and leaves an uncapped or
    /// skill-driven model alone.
    #[test]
    fn raise_cap_lifts_only_a_capped_curve() {
        assert_eq!(LevelingModel::FNV.raise_cap(20).level_cap(), 50);
        assert_eq!(LevelingModel::FO4.raise_cap(20).level_cap(), 0);
        assert_eq!(LevelingModel::SKYRIM.raise_cap(20), LevelingModel::SKYRIM);
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
    fn skyrim_gmst_overlay_reads_only_authored_curve_settings() {
        let requested = std::cell::RefCell::new(Vec::new());
        let model = LevelingModel::SKYRIM.with_gmst(|name| {
            requested.borrow_mut().push(name.to_owned());
            match name {
                "fXPLevelUpBase" => Some(80.0),
                "fXPLevelUpMult" => Some(30.0),
                _ => None,
            }
        });
        assert_eq!(model.xp_to_next(10), 380.0);
        assert_eq!(model.xp_from_skill_rank(20), Some(20.0));
        assert_eq!(model.pool_pick_gain(), Some(10.0));
        assert_eq!(requested.into_inner(), ["fXPLevelUpBase", "fXPLevelUpMult"]);
    }

    /// #5915 — the Fallout curve and cap overlay their authored GMSTs, and a
    /// setting the load order does not author leaves the sourced constants.
    #[test]
    fn fallout_gmst_overlay_reads_curve_and_cap() {
        let requested = std::cell::RefCell::new(Vec::new());
        // `FalloutNV.esm`: iMaxCharacterLevel 30, iXPBumpBase 150, no iXPBase.
        let fnv = LevelingModel::FNV.with_gmst(|name| {
            requested.borrow_mut().push(name.to_owned());
            match name {
                "iMaxCharacterLevel" => Some(30.0),
                "iXPBumpBase" => Some(150.0),
                _ => None,
            }
        });
        assert_eq!(fnv, LevelingModel::FNV, "authored defaults reproduce the sourced model");
        assert_eq!(
            requested.into_inner(),
            ["iXPBumpBase", "iXPBase", "iMaxCharacterLevel"],
            "only the three curve/cap settings are read"
        );

        // `Fallout3.esm` authors the bump and no cap (exe default 20) ...
        let fo3 = LevelingModel::FO3.with_gmst(|name| (name == "iXPBumpBase").then_some(150.0));
        assert_eq!(fo3, LevelingModel::FO3);
        assert_eq!(fo3.level_cap(), 20);
        // ... and `BrokenSteel.esm` raises it to 30.
        let broken_steel =
            LevelingModel::FO3.with_gmst(|name| (name == "iMaxCharacterLevel").then_some(30.0));
        assert_eq!(broken_steel.level_cap(), 30);
        assert_eq!(broken_steel.xp_to_next(1), 200.0, "the curve is untouched");

        // `Fallout4.esm` authors iXPBase 200 only: 75·L + (200 − 75) = 75·L + 125.
        let fo4 = LevelingModel::FO4.with_gmst(|name| (name == "iXPBase").then_some(200.0));
        assert_eq!(fo4, LevelingModel::FO4);

        // A retuned curve: bump 100, base 300 → 100·L + 200.
        let tuned = LevelingModel::FNV.with_gmst(|name| match name {
            "iXPBumpBase" => Some(100.0),
            "iXPBase" => Some(300.0),
            _ => None,
        });
        assert_eq!(tuned.xp_to_next(1), 300.0);
        assert_eq!(tuned.xp_to_next(5), 700.0);
        // Only the bump retuned: base stays the sourced 200, so 120·L + 80.
        let bump_only = LevelingModel::FNV.with_gmst(|name| (name == "iXPBumpBase").then_some(120.0));
        assert_eq!(bump_only.xp_to_next(1), 200.0);
        assert_eq!(bump_only.xp_to_next(2), 320.0);

        // A nonsense cap never replaces the sourced one.
        for bad in [0.0, -5.0, f32::NAN] {
            let kept = LevelingModel::FNV.with_gmst(|name| (name == "iMaxCharacterLevel").then_some(bad));
            assert_eq!(kept.level_cap(), 30, "{bad}");
        }
        // The Fallout reward is carried through.
        assert_eq!(fnv.skill_point_reward(2, 9.0), LevelingModel::FNV.skill_point_reward(2, 9.0));
    }

    #[test]
    fn leveling_model_is_copy_and_small() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<LevelingModel>();
        // Enum tag + widest variant (XpCurve) — stays within half a cache line.
        assert!(std::mem::size_of::<LevelingModel>() <= 32);
    }
}
