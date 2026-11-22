//! Oblivion's fall damage: what a landing costs the faller (#7056, Oblivion skill-use leveling; sourced in the CS wiki's
//! `fJumpFall*` setting pages and UESP *Oblivion:Acrobatics*).
//!
//! **Provenance.** Documented: the damage is `time spent falling * fJumpFallTimeMult` (80 a second), none at all below
//! `fJumpFallTimeMin` (0.2 s), never less than `fJumpFallTimeBase` (0) once it applies, times
//! `fJumpFallSkillBase + Acrobatics * fJumpFallSkillMult / 100` (1.25 and -0.01: "damage is multiplied by 1.25 at Acrobatics
//! of 0, and by 1.24 at Acrobatics of 100"; the CS wiki calls the skill term "probably bugged" and the engine keeps it as
//! shipped). The skill is not capped at 100. **Not documented, and declared:** what counts as *falling* ("a certain
//! threshold of vertical distance has to be passed before it's considered a fall", CS wiki *fJumpFallTimeBase*, no setting
//! named). [`FallTracker`] therefore counts the time the body spends below the height it left the ground at, so a jump
//! that lands where it began and a step off a curb cost nothing, and a drop of about 24 world units or more (0.2 s at
//! the engine's own gravity, which is not the original's) starts to hurt. The mounted rider term (`fJumpFallRiderMult`, 0)
//! is out: the engine has no mounts.
//!
//! Pure data and functions: no ECS, no game branches. The per-game seam is the
//! [`CharacterRulesProfile`](super::CharacterRulesProfile) `fall` row (`Some` on Oblivion); the load order's `GMST` records
//! retune the model ([`FallModel::with_gmst`]).
//!
//! ```text
//! base   = max(fJumpFallTimeBase, fall seconds * fJumpFallTimeMult)         (0 below fJumpFallTimeMin)
//! damage = base * (fJumpFallSkillBase + Acrobatics * fJumpFallSkillMult / 100)
//! ```

use crate::ecs::resource::Resource;

/// The fall-damage rules (installed as a resource where the profile's `fall` row is `Some`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FallModel {
    /// `fJumpFallTimeMult`: damage per second of falling.
    pub time_mult: f32,
    /// `fJumpFallTimeMin`: a fall shorter than this does no damage.
    pub time_min: f32,
    /// `fJumpFallTimeBase`: the least a damaging fall costs.
    pub time_base: f32,
    /// `fJumpFallSkillBase` and `fJumpFallSkillMult`: the Acrobatics term.
    pub skill_base: f32,
    pub skill_mult: f32,
}

impl Resource for FallModel {}

impl FallModel {
    /// Oblivion's: the CS wiki's defaults.
    pub const OBLIVION: Self = Self { time_mult: 80.0, time_min: 0.2, time_base: 0.0, skill_base: 1.25, skill_mult: -0.01 };

    /// The model with each setting the load order authors replacing its default. A non-finite value is ignored.
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        let authored = |name: &str, default: f32| gmst(name).filter(|value| value.is_finite()).unwrap_or(default);
        Self {
            time_mult: authored("fJumpFallTimeMult", self.time_mult),
            time_min: authored("fJumpFallTimeMin", self.time_min),
            time_base: authored("fJumpFallTimeBase", self.time_base),
            skill_base: authored("fJumpFallSkillBase", self.skill_base),
            skill_mult: authored("fJumpFallSkillMult", self.skill_mult),
        }
    }

    /// The Health a landing after `fall_seconds` of falling costs a faller with `acrobatics`, `0` for a fall too short to
    /// hurt. Never negative.
    #[must_use]
    pub fn damage(&self, fall_seconds: f32, acrobatics: f32) -> f32 {
        if fall_seconds > 0.0 && fall_seconds >= self.time_min {
            let base = (fall_seconds * self.time_mult).max(self.time_base);
            (base * (self.skill_base + acrobatics * self.skill_mult / 100.0)).max(0.0)
        } else {
            0.0
        }
    }
}

/// How far the body may move in one frame before it is a teleport, not a fall (world units): past it the tracker forgets
/// the fall. A save restore, a door arrival and a fast travel all hand the controller a body somewhere new.
pub const TELEPORT_DISTANCE: f32 = 400.0;

/// The longest frame a fall's clock counts, so one hitching frame cannot turn a short drop into a deadly one.
pub const MAX_FALL_STEP_SECONDS: f32 = 0.1;

/// Counts the time a body spends falling, frame by frame (see the module docs for what that means here). Transient: never
/// saved.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FallTracker {
    /// The height the body last stood at, `None` until it has stood somewhere this session (and again after a teleport or
    /// a swim, which end any fall).
    takeoff: Option<f32>,
    position: Option<[f32; 3]>,
    seconds: f32,
}

impl Resource for FallTracker {}

impl FallTracker {
    /// Advance one frame: the body is at `position` (x, y, z; y up), `grounded` says whether it stands on something,
    /// `swimming` whether it is in water, and `dt` is the frame. Returns the seconds the body fell for if this frame is a
    /// landing, else `None`. A fall that ends in water, a teleport and a body that has not yet stood anywhere end with
    /// `None`.
    pub fn step(&mut self, position: [f32; 3], grounded: bool, swimming: bool, dt: f32) -> Option<f32> {
        let moved = self.position.replace(position).map(|last| {
            let d = [position[0] - last[0], position[1] - last[1], position[2] - last[2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        });
        if moved.is_some_and(|distance| distance > TELEPORT_DISTANCE) {
            *self = Self { position: Some(position), ..Self::default() };
        }
        if swimming {
            self.takeoff = None;
            self.seconds = 0.0;
            return None;
        }
        if grounded {
            let landed = (self.seconds > 0.0).then_some(self.seconds);
            self.takeoff = Some(position[1]);
            self.seconds = 0.0;
            return landed;
        }
        if self.takeoff.is_some_and(|takeoff| position[1] < takeoff) {
            self.seconds += dt.clamp(0.0, MAX_FALL_STEP_SECONDS);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The documented formula: 80 a second, nothing under 0.2 s, 1.25 at Acrobatics 0 and 1.24 at 100.
    #[test]
    fn the_documented_damage() {
        let model = FallModel::OBLIVION;
        assert_eq!(model.damage(0.1, 0.0), 0.0, "under fJumpFallTimeMin");
        assert_eq!(model.damage(f32::NAN, 0.0), 0.0, "an unordered fall time must not deal damage");
        assert!((model.damage(model.time_min, 0.0) - 20.0).abs() < 1e-3, "the minimum is inclusive");
        assert!((model.damage(1.0, 0.0) - 100.0).abs() < 1e-3, "80 * 1.25");
        assert!((model.damage(1.0, 100.0) - 99.2).abs() < 1e-3, "80 * 1.24");
        assert!((model.damage(0.5, 50.0) - 40.0 * 1.245).abs() < 1e-3);
        assert!(model.damage(0.5, 1000.0) >= 0.0, "never negative");
        assert!((model.with_gmst(|name| (name == "fJumpFallTimeMult").then_some(10.0)).damage(1.0, 0.0) - 12.5).abs() < 1e-3);
    }

    /// Frame by frame at 60 fps: stand, step off a ledge and drop for 0.5 s, land.
    fn run(frames: &[([f32; 3], bool, bool)]) -> Vec<Option<f32>> {
        let mut tracker = FallTracker::default();
        frames.iter().map(|(position, grounded, swimming)| tracker.step(*position, *grounded, *swimming, 1.0 / 60.0)).collect()
    }

    #[test]
    fn a_drop_below_the_takeoff_height_counts_and_a_flat_jump_does_not() {
        let stand = ([0.0, 100.0, 0.0], true, false);
        let mut drop = vec![stand];
        drop.extend((1..=30).map(|n| ([0.0, 100.0 - n as f32 * 10.0, 0.0], false, false)));
        drop.push(([0.0, -200.0, 0.0], true, false));
        let landed = *run(&drop).last().unwrap();
        assert!((landed.unwrap() - 0.5).abs() < 1e-3, "30 frames below the takeoff: {landed:?}");
        // A jump up and back to the same height: the descent never goes below where it began.
        let mut jump = vec![stand];
        jump.extend([120.0, 140.0, 150.0, 140.0, 120.0, 100.0].map(|y| ([0.0, y, 0.0], false, false)));
        jump.push(stand);
        assert_eq!(*run(&jump).last().unwrap(), None, "a flat jump is no fall");
    }

    /// A curb: 16 units below takeoff for a handful of frames lands as a sub-threshold fall (the damage rule drops it).
    #[test]
    fn water_a_teleport_and_an_unarmed_body_end_a_fall() {
        let model = FallModel::OBLIVION;
        let curb = run(&[([0.0, 100.0, 0.0], true, false), ([0.0, 90.0, 0.0], false, false), ([0.0, 84.0, 0.0], false, false), ([0.0, 84.0, 0.0], true, false)]);
        assert_eq!(model.damage(curb.last().unwrap().unwrap(), 0.0), 0.0, "a curb costs nothing");
        // Falling into water is no landing.
        let wet = run(&[([0.0, 100.0, 0.0], true, false), ([0.0, 50.0, 0.0], false, false), ([0.0, 0.0, 0.0], false, true), ([0.0, 0.0, 0.0], true, false)]);
        assert_eq!(wet.last().unwrap(), &None);
        // A teleport mid-air forgets the fall: the body has not stood anywhere since.
        let moved = run(&[([0.0, 100.0, 0.0], true, false), ([0.0, 50.0, 0.0], false, false), ([5000.0, 800.0, 0.0], false, false), ([5000.0, 790.0, 0.0], false, false), ([5000.0, 790.0, 0.0], true, false)]);
        assert_eq!(moved.last().unwrap(), &None);
        // A body that has never stood (a spawn above the floor) lands without a fall.
        let spawn = run(&[([0.0, 100.0, 0.0], false, false), ([0.0, 60.0, 0.0], false, false), ([0.0, 0.0, 0.0], true, false)]);
        assert_eq!(spawn.last().unwrap(), &None);
    }

    /// One hitching frame is clamped, so a long frame while falling cannot count more than a tenth of a second.
    #[test]
    fn a_hitch_is_clamped() {
        let mut tracker = FallTracker::default();
        tracker.step([0.0, 100.0, 0.0], true, false, 1.0 / 60.0);
        tracker.step([0.0, 60.0, 0.0], false, false, 5.0);
        let landed = tracker.step([0.0, 0.0, 0.0], true, false, 1.0 / 60.0).unwrap();
        assert!((landed - MAX_FALL_STEP_SECONDS).abs() < 1e-6, "{landed}");
    }
}
