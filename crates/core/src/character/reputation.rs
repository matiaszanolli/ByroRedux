//! Reputation family (CHARAL).
//!
//! Bethesda's social/standing axes — FO3/FNV **Karma**, FNV per-faction
//! **Reputation** (Fame/Infamy), FO4 per-companion **affinity** — all reduce
//! to the same shape: *one or two clamped/accumulating actor values + a band
//! classifier*, with the point grants and the band→effect wiring living in the
//! scripting/quest layer. This module owns only the **classifiers** and the
//! engine-supplied constants; it stores nothing per-actor (Karma is already an
//! [`ActorValues`](crate::ecs::components::ActorValues) entry; faction/affinity
//! storage lands with the player entity). See
//! `docs/engine/charal-fnv-fo3-ruleset.md` (Karma + Reputation sections).
//!
//! Three instances, one family:
//!
//! | Instance       | Scope          | Axes               | Classifier   |
//! |----------------|----------------|--------------------|--------------|
//! | Karma          | global         | 1 signed, clamped [-1000,1000] | 1-D band (5) |
//! | FNV Reputation | per-faction    | 2 (Fame, Infamy)               | 4×4 grid     |
//! | FO4 affinity   | per-companion  | 1 signed, clamped [-1000,1100] | 1-D band (7) |
//!
//! ## Efficiency
//!
//! Every type here is `Copy` and integer-only; the classifiers are branch
//! ladders or a `const` table index — no allocation, no float, no dispatch.

// ---------------------------------------------------------------------------
// Karma — FO3 == FNV (a single shared 1-axis band; the only delta is the
// cosmetic title strings, which are presentation data, not engine logic).
// ---------------------------------------------------------------------------

/// Karma's inclusive value bounds (`player.getav karma` ∈ `[-1000, +1000]`,
/// starting at `0`). Shared FO3/FNV.
pub const KARMA_MIN: i32 = -1000;
/// See [`KARMA_MIN`].
pub const KARMA_MAX: i32 = 1000;

/// Lower bound of the **Good** band (`+250 … +749`).
pub const KARMA_GOOD_MIN: i32 = 250;
/// Lower bound of the **Very Good** band (`+750 … +1000`).
pub const KARMA_VERY_GOOD_MIN: i32 = 750;
/// Lower bound of the **Neutral** band (`-249 … +249`). Below it is Evil — note
/// the asymmetry: Neutral bottoms at `-249`, so `-250` is already Evil.
pub const KARMA_NEUTRAL_MIN: i32 = -249;
/// Lower bound of the **Evil** band (`-749 … -250`); below it is Very Evil.
pub const KARMA_EVIL_MIN: i32 = -749;

/// The five Karma bands, ordered Very Evil → Very Good so comparisons (e.g.
/// "at least Good") work directly. `repr(i8)` keeps it a single byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(i8)]
pub enum KarmaBand {
    VeryEvil = -2,
    Evil = -1,
    #[default]
    Neutral = 0,
    Good = 1,
    VeryGood = 2,
}

impl KarmaBand {
    /// The English band name (HUD/Pip-Boy category — not the level-keyed title).
    pub const fn name(self) -> &'static str {
        match self {
            KarmaBand::VeryEvil => "Very Evil",
            KarmaBand::Evil => "Evil",
            KarmaBand::Neutral => "Neutral",
            KarmaBand::Good => "Good",
            KarmaBand::VeryGood => "Very Good",
        }
    }
}

/// Classify a raw Karma value into its band. Callers reading Karma from
/// [`ActorValues`] (an `f32`) round to `i32` first.
#[inline]
pub const fn karma_band(value: i32) -> KarmaBand {
    if value >= KARMA_VERY_GOOD_MIN {
        KarmaBand::VeryGood
    } else if value >= KARMA_GOOD_MIN {
        KarmaBand::Good
    } else if value >= KARMA_NEUTRAL_MIN {
        KarmaBand::Neutral
    } else if value >= KARMA_EVIL_MIN {
        KarmaBand::Evil
    } else {
        KarmaBand::VeryEvil
    }
}

/// Clamp a Karma value to `[KARMA_MIN, KARMA_MAX]` — Karma is the one
/// reputation axis that moves *both* ways and saturates at the bounds.
#[inline]
pub const fn clamp_karma(value: i32) -> i32 {
    if value < KARMA_MIN {
        KARMA_MIN
    } else if value > KARMA_MAX {
        KARMA_MAX
    } else {
        value
    }
}

// ---------------------------------------------------------------------------
// FNV Reputation — the 2-axis variant. Fame + Infamy accumulate independently
// and monotonically; the standing is a 4×4 classification of the (Fame range,
// Infamy range) pair.
// ---------------------------------------------------------------------------

/// Engine-supplied bump-magnitude table: an editor "bump type" `1..=5`
/// (Very Minor → Very Major) maps to a non-linear point gain. `player
/// .addreputation <faction> <0|1> <editor_int>`. Index `0` is unused. A single
/// shared constant — *not* per-faction.
pub const REPUTATION_BUMP_POINTS: [u8; 6] = [0, 1, 2, 4, 7, 12];

/// Points granted by an editor bump type (`1..=5`); `0` for out-of-range.
#[inline]
pub const fn reputation_bump_points(editor_int: u8) -> u8 {
    if (editor_int as usize) < REPUTATION_BUMP_POINTS.len() {
        REPUTATION_BUMP_POINTS[editor_int as usize]
    } else {
        0
    }
}

/// The per-axis gameplay maximum for Fame **and** Infamy. `addreputation`
/// "maxes out at its normal maximum value of 100" (fandom *Gamebryo console
/// commands*), matching the steepest vanilla Range-3 threshold (Caesar's
/// Legion = 100). Both axes saturate here in normal play.
pub const REPUTATION_AXIS_MAX: u16 = 100;

/// Per-faction Range-0→3 cut points for **one** Fame or Infamy axis (Range 0
/// is always `0`, so only the three positive thresholds are stored). Applied
/// independently to both axes of a faction. `u16` is ample — the largest
/// vanilla threshold is Caesar's Legion's `100`. 6 bytes, `Copy`.
///
/// These are AUTHORED data — vanilla FNV values live on the faction record;
/// the named constants below are reference/fallback values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactionRepThresholds {
    /// Minimum points for Range 1.
    pub r1: u16,
    /// Minimum points for Range 2.
    pub r2: u16,
    /// Minimum points for Range 3.
    pub r3: u16,
}

impl FactionRepThresholds {
    /// Construct from the three positive thresholds.
    pub const fn new(r1: u16, r2: u16, r3: u16) -> Self {
        Self { r1, r2, r3 }
    }

    /// The Range (`0..=3`) a point total falls into. `>=` against each
    /// threshold, matching the wiki's "minimum points required for each level."
    #[inline]
    pub const fn range(&self, points: u32) -> u8 {
        if points >= self.r3 as u32 {
            3
        } else if points >= self.r2 as u32 {
            2
        } else if points >= self.r1 as u32 {
            1
        } else {
            0
        }
    }
}

/// FNV vanilla per-faction reputation thresholds (reference values; the
/// authoritative source is the parsed faction record). Same array applies to
/// both the Fame and Infamy axes of the faction.
pub mod fnv_faction_thresholds {
    use super::FactionRepThresholds;

    pub const BOOMERS: FactionRepThresholds = FactionRepThresholds::new(8, 25, 50);
    pub const BROTHERHOOD_OF_STEEL: FactionRepThresholds = FactionRepThresholds::new(3, 10, 20);
    pub const CAESARS_LEGION: FactionRepThresholds = FactionRepThresholds::new(15, 50, 100);
    pub const FOLLOWERS_OF_THE_APOCALYPSE: FactionRepThresholds =
        FactionRepThresholds::new(8, 25, 50);
    pub const GREAT_KHANS: FactionRepThresholds = FactionRepThresholds::new(5, 15, 30);
    pub const POWDER_GANGERS: FactionRepThresholds = FactionRepThresholds::new(5, 15, 50);
    pub const NCR: FactionRepThresholds = FactionRepThresholds::new(12, 40, 80);
    pub const WHITE_GLOVE_SOCIETY: FactionRepThresholds = FactionRepThresholds::new(2, 5, 10);
    pub const FREESIDE: FactionRepThresholds = FactionRepThresholds::new(11, 35, 70);
    pub const GOODSPRINGS: FactionRepThresholds = FactionRepThresholds::new(3, 8, 15);
    pub const NOVAC: FactionRepThresholds = FactionRepThresholds::new(3, 10, 20);
    pub const PRIMM: FactionRepThresholds = FactionRepThresholds::new(5, 15, 30);
    pub const THE_STRIP: FactionRepThresholds = FactionRepThresholds::new(6, 20, 40);

    /// `(FalloutNV.esm base FormID, thresholds)` for every vanilla faction /
    /// settlement — reference/fallback keyed by canonical identity (the
    /// authoritative source remains the parsed faction record, load-order
    /// -resolved). FormIDs from fandom *Gamebryo console commands*. The
    /// threshold values reference the named constants above (single source).
    pub const BY_FORM_ID: [(u32, FactionRepThresholds); 13] = [
        (0x000F_FAE8, BOOMERS),
        (0x0011_E662, BROTHERHOOD_OF_STEEL),
        (0x000F_43DD, CAESARS_LEGION),
        (0x0012_4AD1, FOLLOWERS_OF_THE_APOCALYPSE),
        (0x0011_989B, GREAT_KHANS),
        (0x0015_58E6, POWDER_GANGERS),
        (0x000F_43DE, NCR),
        (0x0011_6F16, WHITE_GLOVE_SOCIETY),
        (0x0012_9A7A, FREESIDE),
        (0x0010_4C22, GOODSPRINGS),
        (0x0012_9A79, NOVAC),
        (0x000F_2406, PRIMM),
        (0x0011_8F61, THE_STRIP),
    ];

    /// Vanilla thresholds for a faction by its FalloutNV.esm base FormID.
    pub fn thresholds_for(form_id: u32) -> Option<FactionRepThresholds> {
        BY_FORM_ID
            .iter()
            .find(|(id, _)| *id == form_id)
            .map(|(_, t)| *t)
    }
}

// ---------------------------------------------------------------------------
// FO4 Affinity — the third reputation-family instance: per-companion, 1-axis,
// asymmetric bounds, threshold-banded like Karma but with its own scale and
// (unlike Karma) a fully specified accrual/decay formula straight from the
// decompiled `CompanionActorScript.psc` (`TryToModAffinity`), not just wiki
// prose. Source: fandom *Affinity*, 2026-07-03.
// ---------------------------------------------------------------------------

/// Affinity's inclusive value bounds. **Asymmetric**, unlike Karma: floors at
/// `-1000` (Hatred — permanent departure), caps at `+1100` (100 past the
/// `+1000` "Idolize" threshold, i.e. a fixed buffer above max band rather than
/// a round number).
pub const AFFINITY_MIN: i32 = -1000;
/// See [`AFFINITY_MIN`].
pub const AFFINITY_MAX: i32 = 1100;

/// The seven Affinity bands, ordered Hatred → Idolize so comparisons work
/// directly. `repr(i8)` keeps it one byte, mirroring [`KarmaBand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(i8)]
pub enum AffinityBand {
    Hatred = -3,
    Disdain = -2,
    #[default]
    Neutral = 0,
    Friend = 1,
    Admiration = 2,
    Confidant = 3,
    Idolize = 4,
}

impl AffinityBand {
    /// The wiki's relationship name for this band.
    pub const fn name(self) -> &'static str {
        match self {
            AffinityBand::Hatred => "Hatred",
            AffinityBand::Disdain => "Disdain",
            AffinityBand::Neutral => "Neutral",
            AffinityBand::Friend => "Friend",
            AffinityBand::Admiration => "Admiration",
            AffinityBand::Confidant => "Confidant",
            AffinityBand::Idolize => "Infatuation",
        }
    }
}

/// Classify a raw Affinity value into its band. Thresholds: `1000` Idolize,
/// `750` Confidant, `500` Admiration, `250` Friend, `0` Neutral, `-500`
/// Disdain, below that Hatred.
#[inline]
pub const fn affinity_band(value: i32) -> AffinityBand {
    if value >= 1000 {
        AffinityBand::Idolize
    } else if value >= 750 {
        AffinityBand::Confidant
    } else if value >= 500 {
        AffinityBand::Admiration
    } else if value >= 250 {
        AffinityBand::Friend
    } else if value >= 0 {
        AffinityBand::Neutral
    } else if value >= -500 {
        AffinityBand::Disdain
    } else {
        AffinityBand::Hatred
    }
}

/// Clamp a raw Affinity value to `[AFFINITY_MIN, AFFINITY_MAX]`.
#[inline]
pub const fn clamp_affinity(value: i32) -> i32 {
    if value < AFFINITY_MIN {
        AFFINITY_MIN
    } else if value > AFFINITY_MAX {
        AFFINITY_MAX
    } else {
        value
    }
}

/// A companion's reaction to a player action/dialogue choice — the four
/// discrete deltas `TryToModAffinity` applies before the size scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AffinityReaction {
    Liked,
    Loved,
    Disliked,
    Hated,
}

impl AffinityReaction {
    /// The base delta before the [`AffinityReactionSize`] scalar.
    #[inline]
    const fn base_delta(self) -> f32 {
        match self {
            AffinityReaction::Liked => 15.0,
            AffinityReaction::Loved => 35.0,
            AffinityReaction::Disliked => -15.0,
            AffinityReaction::Hated => -35.0,
        }
    }
}

/// The `CA_Size_*` scalar `TryToModAffinity` multiplies a reaction by. Most
/// repeatable actions (weapon modding, lockpicking, …) are `Small`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AffinityReactionSize {
    Small,
    Normal,
    Large,
}

impl AffinityReactionSize {
    #[inline]
    const fn scalar(self) -> f32 {
        match self {
            AffinityReactionSize::Small => 0.5,
            AffinityReactionSize::Normal => 1.0,
            AffinityReactionSize::Large => 1.5,
        }
    }
}

/// The signed Affinity delta for one reaction at a given size — direct
/// transcription of `TryToModAffinity`.
#[inline]
pub fn affinity_reaction_delta(reaction: AffinityReaction, size: AffinityReactionSize) -> f32 {
    reaction.base_delta() * size.scalar()
}

/// The passive per-tick Affinity gain from a companion simply following the
/// player, awarded every in-game 10-minute period (gated on ≥1 XP earned
/// during it — combat-system concern, not modelled here). Self-limiting: the
/// bonus shrinks as Affinity rises, though it never reaches zero within
/// Affinity's actual `[-1000, 1100]` range (worst case, at the `1100` cap,
/// is still `+3.7`).
#[inline]
pub fn affinity_passive_gain(current_affinity: f32) -> f32 {
    40.0 - 0.033 * current_affinity
}

/// The overall sentiment of a [`ReputationStanding`] (the wiki's green / black
/// / red colouring): positive standings unlock goodwill, negative ones provoke
/// hostility, mixed ones sit in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ReputationSentiment {
    /// Red — disliked/hostile.
    Negative,
    /// Black — undecided / contradictory.
    #[default]
    Mixed,
    /// Green — liked/trusted.
    Positive,
}

/// The 16 FNV faction-standing titles, one per `(Fame range, Infamy range)`
/// cell of the 4×4 grid. The discriminant is `infamy * 4 + fame` so the grid
/// lookup is index arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReputationStanding {
    // Infamy 0
    Neutral = 0,
    Accepted = 1,
    Liked = 2,
    Idolized = 3,
    // Infamy 1
    Shunned = 4,
    Mixed = 5,
    SmilingTroublemaker = 6,
    GoodNaturedRascal = 7,
    // Infamy 2
    Hated = 8,
    SneeringPunk = 9,
    Unpredictable = 10,
    DarkHero = 11,
    // Infamy 3
    Vilified = 12,
    MercifulThug = 13,
    SoftHeartedDevil = 14,
    WildChild = 15,
}

/// The canonical 4×4 standing grid, `[infamy_range][fame_range]`.
const STANDING_GRID: [[ReputationStanding; 4]; 4] = {
    use ReputationStanding::*;
    [
        [Neutral, Accepted, Liked, Idolized],
        [Shunned, Mixed, SmilingTroublemaker, GoodNaturedRascal],
        [Hated, SneeringPunk, Unpredictable, DarkHero],
        [Vilified, MercifulThug, SoftHeartedDevil, WildChild],
    ]
};

impl ReputationStanding {
    /// The standing for a `(Fame range, Infamy range)` pair. Ranges are clamped
    /// to `0..=3` defensively.
    #[inline]
    pub const fn from_ranges(fame_range: u8, infamy_range: u8) -> Self {
        let f = if fame_range > 3 { 3 } else { fame_range };
        let i = if infamy_range > 3 { 3 } else { infamy_range };
        STANDING_GRID[i as usize][f as usize]
    }

    /// The standing for raw Fame/Infamy point totals against a faction's
    /// thresholds — the full Karma-analog classify for the 2-axis case.
    #[inline]
    pub const fn classify(fame: u32, infamy: u32, t: &FactionRepThresholds) -> Self {
        Self::from_ranges(t.range(fame), t.range(infamy))
    }

    /// Positive / Mixed / Negative bucket (the grid's green/black/red colour).
    pub const fn sentiment(self) -> ReputationSentiment {
        use ReputationStanding::*;
        match self {
            Accepted | Liked | Idolized | SmilingTroublemaker | GoodNaturedRascal => {
                ReputationSentiment::Positive
            }
            Shunned | Hated | SneeringPunk | Vilified | MercifulThug => {
                ReputationSentiment::Negative
            }
            Neutral | Mixed | Unpredictable | DarkHero | SoftHeartedDevil | WildChild => {
                ReputationSentiment::Mixed
            }
        }
    }

    /// The display title.
    pub const fn name(self) -> &'static str {
        use ReputationStanding::*;
        match self {
            Neutral => "Neutral",
            Accepted => "Accepted",
            Liked => "Liked",
            Idolized => "Idolized",
            Shunned => "Shunned",
            Mixed => "Mixed",
            SmilingTroublemaker => "Smiling Troublemaker",
            GoodNaturedRascal => "Good-Natured Rascal",
            Hated => "Hated",
            SneeringPunk => "Sneering Punk",
            Unpredictable => "Unpredictable",
            DarkHero => "Dark Hero",
            Vilified => "Vilified",
            MercifulThug => "Merciful Thug",
            SoftHeartedDevil => "Soft-Hearted Devil",
            WildChild => "Wild Child",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fnv_faction_thresholds as fac;
    use super::*;

    #[test]
    fn karma_bands_at_exact_boundaries() {
        // Cut points from the FO3/FNV pages, including the −249/−250 asymmetry.
        assert_eq!(karma_band(1000), KarmaBand::VeryGood);
        assert_eq!(karma_band(750), KarmaBand::VeryGood);
        assert_eq!(karma_band(749), KarmaBand::Good);
        assert_eq!(karma_band(250), KarmaBand::Good);
        assert_eq!(karma_band(249), KarmaBand::Neutral);
        assert_eq!(karma_band(0), KarmaBand::Neutral);
        assert_eq!(karma_band(-249), KarmaBand::Neutral);
        assert_eq!(karma_band(-250), KarmaBand::Evil);
        assert_eq!(karma_band(-749), KarmaBand::Evil);
        assert_eq!(karma_band(-750), KarmaBand::VeryEvil);
        assert_eq!(karma_band(-1000), KarmaBand::VeryEvil);
    }

    #[test]
    fn karma_band_is_ordered_and_one_byte() {
        assert!(KarmaBand::Good > KarmaBand::Neutral);
        assert!(KarmaBand::VeryEvil < KarmaBand::Evil);
        assert_eq!(std::mem::size_of::<KarmaBand>(), 1);
        assert_eq!(KarmaBand::default(), KarmaBand::Neutral);
    }

    #[test]
    fn karma_clamps_to_bounds() {
        assert_eq!(clamp_karma(5000), 1000);
        assert_eq!(clamp_karma(-5000), -1000);
        assert_eq!(clamp_karma(123), 123);
    }

    #[test]
    fn bump_table_matches_wiki() {
        // Very Minor … Very Major = 1, 2, 4, 7, 12.
        assert_eq!(reputation_bump_points(1), 1);
        assert_eq!(reputation_bump_points(2), 2);
        assert_eq!(reputation_bump_points(3), 4);
        assert_eq!(reputation_bump_points(4), 7);
        assert_eq!(reputation_bump_points(5), 12);
        // The page's worked example: `addreputation … 5` adds 12.
        assert_eq!(reputation_bump_points(5), 12);
        // Out of range degrades to 0.
        assert_eq!(reputation_bump_points(0), 0);
        assert_eq!(reputation_bump_points(6), 0);
    }

    #[test]
    fn faction_ranges_use_inclusive_minimums() {
        // Brotherhood of Steel: 0 / 3 / 10 / 20.
        let bos = fac::BROTHERHOOD_OF_STEEL;
        assert_eq!(bos.range(0), 0);
        assert_eq!(bos.range(2), 0);
        assert_eq!(bos.range(3), 1, "exactly r1 reaches Range 1");
        assert_eq!(bos.range(9), 1);
        assert_eq!(bos.range(10), 2);
        assert_eq!(bos.range(19), 2);
        assert_eq!(bos.range(20), 3);
        assert_eq!(bos.range(999), 3, "saturates at Range 3");
        // Caesar's Legion is the steepest: 15 / 50 / 100.
        assert_eq!(fac::CAESARS_LEGION.range(99), 2);
        assert_eq!(fac::CAESARS_LEGION.range(100), 3);
    }

    #[test]
    fn standing_grid_corners_and_diagonal() {
        use ReputationStanding::*;
        // Corners of the 4×4 grid (fame, infamy).
        assert_eq!(ReputationStanding::from_ranges(0, 0), Neutral);
        assert_eq!(ReputationStanding::from_ranges(3, 0), Idolized);
        assert_eq!(ReputationStanding::from_ranges(0, 3), Vilified);
        assert_eq!(ReputationStanding::from_ranges(3, 3), WildChild);
        // The "Mixed" diagonal — the cell a single signed axis (Karma) cannot
        // express, which is the whole reason Reputation needs two axes.
        assert_eq!(ReputationStanding::from_ranges(1, 1), Mixed);
        assert_eq!(ReputationStanding::from_ranges(2, 2), Unpredictable);
        // A representative off-diagonal each side.
        assert_eq!(ReputationStanding::from_ranges(2, 1), SmilingTroublemaker);
        assert_eq!(ReputationStanding::from_ranges(1, 2), SneeringPunk);
        // Range clamp is defensive.
        assert_eq!(ReputationStanding::from_ranges(9, 9), WildChild);
    }

    #[test]
    fn standing_sentiment_matches_grid_colours() {
        use ReputationStanding::*;
        // Green (positive).
        for s in [Accepted, Liked, Idolized, SmilingTroublemaker, GoodNaturedRascal] {
            assert_eq!(s.sentiment(), ReputationSentiment::Positive, "{}", s.name());
        }
        // Red (negative).
        for s in [Shunned, Hated, SneeringPunk, Vilified, MercifulThug] {
            assert_eq!(s.sentiment(), ReputationSentiment::Negative, "{}", s.name());
        }
        // Black (mixed).
        for s in [Neutral, Mixed, Unpredictable, DarkHero, SoftHeartedDevil, WildChild] {
            assert_eq!(s.sentiment(), ReputationSentiment::Mixed, "{}", s.name());
        }
    }

    #[test]
    fn faction_thresholds_resolve_by_form_id() {
        // The 13 vanilla factions are keyed by their FalloutNV.esm FormID, and
        // each maps to its named threshold constant (single source).
        assert_eq!(fac::BY_FORM_ID.len(), 13);
        assert_eq!(
            fac::thresholds_for(0x000F_43DD), // Caesar's Legion
            Some(fac::CAESARS_LEGION)
        );
        assert_eq!(
            fac::thresholds_for(0x000F_FAE8), // Boomers
            Some(fac::BOOMERS)
        );
        assert_eq!(fac::thresholds_for(0xDEAD_BEEF), None, "unknown faction");
        // The per-axis cap matches the steepest vanilla Range-3 (Legion = 100).
        assert_eq!(REPUTATION_AXIS_MAX, 100);
        assert_eq!(fac::CAESARS_LEGION.r3, REPUTATION_AXIS_MAX);
    }

    #[test]
    fn classify_from_raw_points() {
        // BoS Fame 12 (Range 2), Infamy 4 (Range 1) → row 1, col 2 = Smiling
        // Troublemaker (good at heart, occasional troublemaker).
        let s = ReputationStanding::classify(12, 4, &fac::BROTHERHOOD_OF_STEEL);
        assert_eq!(s, ReputationStanding::SmilingTroublemaker);
        assert_eq!(s.sentiment(), ReputationSentiment::Positive);
        // High both → Wild Child (the unreachable-from terminal cell).
        assert_eq!(
            ReputationStanding::classify(50, 50, &fac::BROTHERHOOD_OF_STEEL),
            ReputationStanding::WildChild
        );
    }

    #[test]
    fn affinity_bands_at_exact_boundaries() {
        assert_eq!(affinity_band(1100), AffinityBand::Idolize);
        assert_eq!(affinity_band(1000), AffinityBand::Idolize);
        assert_eq!(affinity_band(999), AffinityBand::Confidant);
        assert_eq!(affinity_band(750), AffinityBand::Confidant);
        assert_eq!(affinity_band(749), AffinityBand::Admiration);
        assert_eq!(affinity_band(500), AffinityBand::Admiration);
        assert_eq!(affinity_band(499), AffinityBand::Friend);
        assert_eq!(affinity_band(250), AffinityBand::Friend);
        assert_eq!(affinity_band(249), AffinityBand::Neutral);
        assert_eq!(affinity_band(0), AffinityBand::Neutral);
        assert_eq!(affinity_band(-1), AffinityBand::Disdain);
        assert_eq!(affinity_band(-500), AffinityBand::Disdain);
        assert_eq!(affinity_band(-501), AffinityBand::Hatred);
        assert_eq!(affinity_band(-1000), AffinityBand::Hatred);
    }

    #[test]
    fn affinity_band_is_ordered_and_one_byte() {
        assert!(AffinityBand::Idolize > AffinityBand::Confidant);
        assert!(AffinityBand::Hatred < AffinityBand::Disdain);
        assert_eq!(std::mem::size_of::<AffinityBand>(), 1);
        assert_eq!(AffinityBand::default(), AffinityBand::Neutral);
    }

    #[test]
    fn affinity_clamps_to_its_asymmetric_bounds() {
        assert_eq!(clamp_affinity(5000), AFFINITY_MAX);
        assert_eq!(clamp_affinity(-5000), AFFINITY_MIN);
        assert_eq!(clamp_affinity(123), 123);
        assert_eq!(AFFINITY_MIN, -1000);
        assert_eq!(AFFINITY_MAX, 1100);
    }

    #[test]
    fn affinity_reaction_deltas_match_the_decompiled_script() {
        use AffinityReaction::*;
        use AffinityReactionSize::*;
        // Base (Normal-size) deltas.
        assert_eq!(affinity_reaction_delta(Liked, Normal), 15.0);
        assert_eq!(affinity_reaction_delta(Loved, Normal), 35.0);
        assert_eq!(affinity_reaction_delta(Disliked, Normal), -15.0);
        assert_eq!(affinity_reaction_delta(Hated, Normal), -35.0);
        // Small-size repeatable actions (weapon modding, lockpicking, …):
        // wiki-stated 7.5 for a like, 17.5 for a love.
        assert_eq!(affinity_reaction_delta(Liked, Small), 7.5);
        assert_eq!(affinity_reaction_delta(Loved, Small), 17.5);
        // Large scales the other direction.
        assert_eq!(affinity_reaction_delta(Liked, Large), 22.5);
        assert_eq!(affinity_reaction_delta(Hated, Large), -52.5);
    }

    #[test]
    fn affinity_passive_gain_matches_the_wiki_worked_examples() {
        // 40 − 0.033·affinity, checked at every value the source page gives.
        assert!((affinity_passive_gain(100.0) - 36.7).abs() < 1e-6);
        assert!((affinity_passive_gain(250.0) - 31.75).abs() < 1e-6);
        assert!((affinity_passive_gain(500.0) - 23.5).abs() < 1e-6);
        assert!((affinity_passive_gain(750.0) - 15.25).abs() < 1e-6);
        // The page rounds 7.33 down to "+7" for display; we keep the float.
        assert!((affinity_passive_gain(990.0) - 7.33).abs() < 1e-2);
        // Never goes negative within Affinity's actual range, even at the cap.
        assert!(affinity_passive_gain(AFFINITY_MAX as f32) > 0.0);
    }

    #[test]
    fn affinity_types_are_compact() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<AffinityBand>();
        assert_copy::<AffinityReaction>();
        assert_copy::<AffinityReactionSize>();
    }

    #[test]
    fn standing_types_are_one_byte_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<ReputationStanding>();
        assert_copy::<ReputationSentiment>();
        assert_copy::<FactionRepThresholds>();
        assert_eq!(std::mem::size_of::<ReputationStanding>(), 1);
        assert_eq!(std::mem::size_of::<ReputationSentiment>(), 1);
        assert_eq!(std::mem::size_of::<FactionRepThresholds>(), 6);
    }
}
