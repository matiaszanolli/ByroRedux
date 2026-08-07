//! Canonical game clock used by weather, AI schedules, console controls,
//! and save/load.

use byroredux_core::ecs::Resource;

pub(crate) const HOURS_PER_DAY: f32 = 24.0;
pub(crate) const DEFAULT_TIME_SCALE: f32 = 30.0;

/// Persistent game time.
///
/// `time_scale` is the Bethesda-style ratio of game seconds to real seconds:
/// `30.0` advances one game hour every two real minutes. All mutation goes
/// through the helpers below so the hour remains in `[0, 24)`, multi-day
/// advances cannot leave it out of range, and pause/resume retains the prior
/// non-zero rate.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct GameTimeRes {
    /// Number of complete game days elapsed since this runtime began.
    pub(crate) day: u64,
    /// Current hour (`0.0` = midnight, `6.0` = 6am, `12.0` = noon).
    pub(crate) hour: f32,
    /// Game seconds advanced per real second. `0.0` pauses the clock.
    pub(crate) time_scale: f32,
    /// Last positive scale, restored by [`Self::resume`].
    resume_time_scale: f32,
}

impl GameTimeRes {
    pub(crate) fn new(hour: f32, time_scale: f32) -> Self {
        let mut time = Self::default();
        time.set_hour(hour);
        time.set_time_scale(time_scale);
        time
    }

    pub(crate) fn frozen_at(hour: f32) -> Self {
        Self::new(hour, 0.0)
    }

    /// Set the clock without changing the current day.
    ///
    /// Returns `false` for non-finite input. Finite values wrap through the
    /// 24-hour clock, which keeps internal callers safe even if they pass a
    /// computed value outside the conventional range.
    pub(crate) fn set_hour(&mut self, hour: f32) -> bool {
        if !hour.is_finite() {
            return false;
        }
        self.hour = hour.rem_euclid(HOURS_PER_DAY);
        true
    }

    /// Advance by a non-negative number of game hours, carrying whole days.
    pub(crate) fn advance_hours(&mut self, hours: f32) -> bool {
        if !hours.is_finite() || hours < 0.0 {
            return false;
        }
        let total = self.hour + hours;
        let elapsed_days = (total / HOURS_PER_DAY).floor() as u64;
        self.day = self.day.saturating_add(elapsed_days);
        self.hour = total.rem_euclid(HOURS_PER_DAY);
        true
    }

    /// Advance from real elapsed seconds using the active time scale.
    pub(crate) fn tick(&mut self, real_seconds: f32) {
        if real_seconds.is_finite() && real_seconds > 0.0 && self.time_scale > 0.0 {
            self.advance_hours(real_seconds * self.time_scale / 3600.0);
        }
    }

    /// Set the game-seconds-per-real-second ratio.
    pub(crate) fn set_time_scale(&mut self, scale: f32) -> bool {
        if !scale.is_finite() || scale < 0.0 {
            return false;
        }
        self.time_scale = scale;
        if scale > 0.0 {
            self.resume_time_scale = scale;
        }
        true
    }

    pub(crate) fn pause(&mut self) {
        if self.time_scale > 0.0 {
            self.resume_time_scale = self.time_scale;
        }
        self.time_scale = 0.0;
    }

    pub(crate) fn resume(&mut self) {
        if self.time_scale == 0.0 {
            self.time_scale = if self.resume_time_scale > 0.0 {
                self.resume_time_scale
            } else {
                DEFAULT_TIME_SCALE
            };
        }
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.time_scale == 0.0
    }
}

impl Default for GameTimeRes {
    fn default() -> Self {
        Self {
            day: 0,
            hour: 10.0,
            time_scale: DEFAULT_TIME_SCALE,
            resume_time_scale: DEFAULT_TIME_SCALE,
        }
    }
}

impl Resource for GameTimeRes {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_carries_multiple_days_and_keeps_hour_canonical() {
        let mut time = GameTimeRes::default();
        time.set_hour(23.0);
        time.set_time_scale(3600.0);
        time.tick(50.0);
        assert_eq!(time.day, 3);
        assert!((time.hour - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pause_resume_restores_the_last_positive_scale() {
        let mut time = GameTimeRes::default();
        time.set_time_scale(120.0);
        time.pause();
        assert!(time.is_paused());
        time.resume();
        assert_eq!(time.time_scale, 120.0);
    }

    #[test]
    fn setters_reject_invalid_values_without_corrupting_the_clock() {
        let mut time = GameTimeRes::default();
        assert!(!time.set_hour(f32::NAN));
        assert!(!time.advance_hours(-1.0));
        assert!(!time.set_time_scale(f32::INFINITY));
        assert_eq!(time, GameTimeRes::default());
    }
}
