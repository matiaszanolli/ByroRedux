//! The bow's rules: how long a draw takes, how fast the arrow leaves, where it ends (player-archery PRD
//! `docs/prd/player-archery.md`, BOW-MOD-001 / -002, BOW-DRW-002 / -003, BOW-FLT-001 to -003, -005).
//!
//! **Provenance.** Everything here is documented, and the rows say where. Oblivion's draw timer, speed formula
//! and settings are the CS wiki's (*fArrowSpeedMult*, *fArrowBowTimerBase*, *fArrowBowTimerMult*,
//! *fArrowWeakSpeed*); Skyrim's draw time, tilt and range cap are UESP *Skyrim:Archery*. What no source gives is
//! not here: gravity (the straight arrow needs none, BOW-FLT-004), a partial-draw damage or speed for Skyrim
//! (Q4: an early release fires nothing), and an Oblivion arrow range, which the host supplies as a labelled
//! provisional constant. The owner ruled out decoding the executables for rules (2026-11-20).
//!
//! Pure data and functions: no ECS, no game branches. The per-game seam is the
//! [`CharacterRulesProfile`](super::CharacterRulesProfile) `bow` row (`Some` on Oblivion and Skyrim); the load
//! order's `GMST` records retune the model ([`BowModel::with_gmst`]), as [`SpellCastModel`](super::SpellCastModel)
//! does.
//!
//! ```text
//! Oblivion  draw fraction = min(1, fArrowBowTimerBase + fArrowBowTimerMult * draw time)
//!           arrow speed   = ammo speed * fArrowSpeedMult * (fArrowWeakSpeed + (1 - fArrowWeakSpeed) * fraction)
//! Skyrim    draw complete = 0.4 s + 1.66 s / (weapon speed * (1 + Quick Shot)) + 0.6 s   (Quick Shot: no perk yet, 1)
//!           arrow speed   = the arrow's PROJ speed, at a full draw only (an earlier release fires nothing, Q4)
//! ```

use crate::ecs::resource::Resource;

/// How an arrow's flight path is chosen: the `gameplay.arrow_trajectory` setting (BOW-SET-001) picks one, and
/// the rules row names each game's default (BOW-SET-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrowTrajectory {
    /// The arrow flies where the crosshair points.
    #[default]
    Straight,
    /// The arrow falls under gravity. **Not offered** until a documented gravity rule exists (BOW-FLT-004).
    Ballistic,
}

impl ArrowTrajectory {
    /// The setting's value string.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Straight => "straight",
            Self::Ballistic => "ballistic",
        }
    }

    /// The trajectory a setting value names, `None` for anything else.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "straight" => Some(Self::Straight),
            "ballistic" => Some(Self::Ballistic),
            _ => None,
        }
    }

    /// The trajectory the engine can fly today: ballistic resolves to straight (BOW-FLT-004).
    #[must_use]
    pub const fn effective(self) -> Self {
        Self::Straight
    }
}

/// How the draw turns into a fraction and when it may be released.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawRule {
    /// Oblivion: `min(1, base + mult * draw time)`, so a release at any time fires (the weak-speed floor of
    /// [`SpeedRule::Formula`] applies).
    Timer { base: f32, mult: f32 },
    /// Skyrim: the draw is complete after `nock + draw / (weapon speed * (1 + Quick Shot)) + rest`
    /// (UESP *Skyrim:Archery*: "full draw time = Nock + Draw + rest"). A release before
    /// that fires nothing (Q4), so the fraction is 0 until complete and 1 after.
    Duration { nock: f32, draw: f32, rest: f32 },
}

/// Where an arrow's speed comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeedRule {
    /// Oblivion: `ammo speed * speed_mult * (weak + (1 - weak) * draw fraction)`.
    Formula { speed_mult: f32, weak: f32 },
    /// Skyrim: the arrow's `PROJ` speed, at a full draw.
    Projectile,
}

/// The bow's per-game numbers (BOW-MOD-001). Installed as a resource where the profile's `bow` row is `Some`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BowModel {
    pub draw: DrawRule,
    pub speed: SpeedRule,
    /// The arrow's upward tilt in degrees, first person then third (Skyrim `f1PArrowTiltUpAngle` 2,
    /// `f3PArrowTiltUpAngle` 2.5; Oblivion documents none, 0: BOW-FLT-002).
    pub tilt_degrees: [f32; 2],
    /// The cap on an arrow's range in world units, `INFINITY` where the game documents none (Skyrim:
    /// `fVisibleNavmeshMoveDist` 4096, past which arrows "phase through targets without doing any damage").
    pub range_cap: f32,
    /// The trajectory the setting starts at (BOW-SET-002).
    pub default_trajectory: ArrowTrajectory,
}

impl Resource for BowModel {}

impl BowModel {
    /// Oblivion's: the CS wiki's defaults (`fArrowBowTimerBase` 0.25, `fArrowBowTimerMult` 0.4,
    /// `fArrowSpeedMult` 1500, `fArrowWeakSpeed` 0.01). `Oblivion.esm` authors none of them, so these stand
    /// unless a plugin does.
    pub const OBLIVION: Self = Self {
        draw: DrawRule::Timer { base: 0.25, mult: 0.4 },
        speed: SpeedRule::Formula { speed_mult: 1500.0, weak: 0.01 },
        tilt_degrees: [0.0, 0.0],
        range_cap: f32::INFINITY,
        default_trajectory: ArrowTrajectory::Ballistic,
    };

    /// Skyrim's: UESP *Skyrim:Archery* (draw 1.66 s at weapon speed 1, nock about 0.4 s, tilt 2 and 2.5 degrees,
    /// the 4096 range cap).
    pub const SKYRIM: Self = Self {
        draw: DrawRule::Duration { nock: 0.4, draw: 1.66, rest: 0.6 },
        speed: SpeedRule::Projectile,
        tilt_degrees: [2.0, 2.5],
        range_cap: 4096.0,
        default_trajectory: ArrowTrajectory::Straight,
    };

    /// The model with each setting the load order authors replacing its default. A non-finite value is ignored.
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        let authored = |name: &str, default: f32| gmst(name).filter(|value| value.is_finite()).unwrap_or(default);
        let draw = match self.draw {
            DrawRule::Timer { base, mult } => DrawRule::Timer {
                base: authored("fArrowBowTimerBase", base),
                mult: authored("fArrowBowTimerMult", mult),
            },
            duration @ DrawRule::Duration { .. } => duration,
        };
        let speed = match self.speed {
            SpeedRule::Formula { speed_mult, weak } => SpeedRule::Formula {
                speed_mult: authored("fArrowSpeedMult", speed_mult),
                weak: authored("fArrowWeakSpeed", weak),
            },
            SpeedRule::Projectile => SpeedRule::Projectile,
        };
        let tilt_degrees = if self.tilt_degrees == [0.0, 0.0] {
            self.tilt_degrees
        } else {
            [authored("f1PArrowTiltUpAngle", self.tilt_degrees[0]), authored("f3PArrowTiltUpAngle", self.tilt_degrees[1])]
        };
        let range_cap = if self.range_cap.is_finite() { authored("fVisibleNavmeshMoveDist", self.range_cap) } else { self.range_cap };
        Self { draw, speed, tilt_degrees, range_cap, ..self }
    }

    /// The seconds a draw has to be held before the arrow flies at full strength: Oblivion's timer reaches 1 at
    /// `(1 - base) / mult` (1.875 s with the defaults), Skyrim's is `nock + draw / weapon_speed + rest`. `weapon_speed`
    /// is the bow's authored speed, clamped to a small positive value so a record with none cannot divide by zero.
    #[must_use]
    pub fn full_draw_seconds(&self, weapon_speed: f32) -> f32 {
        match self.draw {
            DrawRule::Timer { base, mult } => {
                if mult > 0.0 {
                    ((1.0 - base) / mult).max(0.0)
                } else {
                    0.0
                }
            }
            DrawRule::Duration { nock, draw, rest } => nock + draw / weapon_speed.max(0.01) + rest,
        }
    }

    /// The draw fraction after `held` seconds, or `None` where the draw is not complete and the game's rule is to
    /// fire nothing (Skyrim, Q4). Oblivion always fires: its fraction is `min(1, base + mult * held)`.
    #[must_use]
    pub fn draw_fraction(&self, held: f32, weapon_speed: f32) -> Option<f32> {
        let held = held.max(0.0);
        match self.draw {
            DrawRule::Timer { base, mult } => Some((base + mult * held).clamp(0.0, 1.0)),
            DrawRule::Duration { .. } => (held >= self.full_draw_seconds(weapon_speed)).then_some(1.0),
        }
    }

    /// The arrow's speed in world units per second: `ammo_speed` is the `AMMO` record's, `projectile_speed` the
    /// `PROJ`'s (0 where the game has none), `fraction` the draw fraction.
    #[must_use]
    pub fn arrow_speed(&self, ammo_speed: f32, projectile_speed: f32, fraction: f32) -> f32 {
        match self.speed {
            SpeedRule::Formula { speed_mult, weak } => ammo_speed * speed_mult * (weak + (1.0 - weak) * fraction.clamp(0.0, 1.0)),
            SpeedRule::Projectile => projectile_speed,
        }
    }

    /// The arrow's range: the lesser of the record's range (`INFINITY` when it has none) and the game's cap.
    #[must_use]
    pub fn range(&self, record_range: f32) -> f32 {
        let record = if record_range > 0.0 { record_range } else { f32::INFINITY };
        record.min(self.range_cap)
    }

    /// The upward tilt in radians for a first-person (`true`) or third-person shot.
    #[must_use]
    pub fn tilt_radians(&self, first_person: bool) -> f32 {
        self.tilt_degrees[usize::from(!first_person)].to_radians()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 386, 683 and 1500 units per second for an arrow of speed 1.0 with the defaults (PRD §7.2 scenario 1).
    #[test]
    fn bow_speed_follows_the_draw_timer() {
        let model = BowModel::OBLIVION;
        for (held, expected) in [(0.0, 386.0), (0.5, 683.0), (1.875, 1500.0), (10.0, 1500.0)] {
            let fraction = model.draw_fraction(held, 1.0).expect("Oblivion always fires");
            let speed = model.arrow_speed(1.0, 0.0, fraction);
            assert!((speed - expected).abs() < 1.0, "held {held}: {speed} vs {expected}");
        }
        assert!((model.full_draw_seconds(1.0) - 1.875).abs() < 1e-4, "full draw at 1.875 s");
    }

    /// Skyrim's full draw is `0.4 + 1.66 / speed + 0.6` and a release before it fires nothing (Q4).
    #[test]
    fn skyrim_draw_time() {
        let model = BowModel::SKYRIM;
        let full = model.full_draw_seconds(1.0);
        assert!((full - 2.66).abs() < 1e-4, "0.4 + 1.66 + 0.6");
        assert_eq!(model.draw_fraction(full - 0.01, 1.0), None);
        assert_eq!(model.draw_fraction(full, 1.0), Some(1.0));
        // A heavier bow (lower speed) takes longer; a zero speed record cannot divide by zero.
        assert!(model.full_draw_seconds(0.5) > full);
        assert!(model.full_draw_seconds(0.0).is_finite());
        // The arrow leaves at the PROJ speed, not scaled by anything.
        assert_eq!(model.arrow_speed(0.0, 3600.0, 1.0), 3600.0);
    }

    #[test]
    fn range_is_the_lesser_of_the_record_and_the_cap() {
        assert_eq!(BowModel::SKYRIM.range(5000.0), 4096.0);
        assert_eq!(BowModel::SKYRIM.range(3000.0), 3000.0);
        assert_eq!(BowModel::SKYRIM.range(0.0), 4096.0);
        assert!(BowModel::OBLIVION.range(0.0).is_infinite());
    }

    #[test]
    fn the_load_order_retunes_the_model_and_a_bad_value_is_ignored() {
        let tuned = BowModel::OBLIVION.with_gmst(|name| match name {
            "fArrowBowTimerBase" => Some(0.5),
            "fArrowSpeedMult" => Some(f32::NAN),
            _ => None,
        });
        assert_eq!(tuned.draw, DrawRule::Timer { base: 0.5, mult: 0.4 });
        assert_eq!(tuned.speed, BowModel::OBLIVION.speed, "a non-finite setting keeps the default");
        let skyrim = BowModel::SKYRIM.with_gmst(|name| (name == "f1PArrowTiltUpAngle").then_some(3.0));
        assert_eq!(skyrim.tilt_degrees, [3.0, 2.5]);
        assert_eq!(BowModel::OBLIVION.with_gmst(|_| Some(9.0)).tilt_degrees, [0.0, 0.0], "Oblivion has no tilt to retune");
    }

    #[test]
    fn ballistic_resolves_to_straight_until_a_gravity_rule_exists() {
        assert_eq!(ArrowTrajectory::Ballistic.effective(), ArrowTrajectory::Straight);
        assert_eq!(ArrowTrajectory::from_id("ballistic"), Some(ArrowTrajectory::Ballistic));
        assert_eq!(ArrowTrajectory::from_id("arc"), None);
        assert_eq!(BowModel::OBLIVION.default_trajectory, ArrowTrajectory::Ballistic);
        assert_eq!(BowModel::SKYRIM.default_trajectory, ArrowTrajectory::Straight);
    }
}
