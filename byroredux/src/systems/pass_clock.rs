//! The "accumulate `dt`, run a pass every `period` seconds" clock the periodic gameplay
//! systems share (#6579): corpse discovery, attraction, mines, faction hostility and the
//! guard rush each re-spelled it, and each re-decided the details (whether a non-finite `dt`
//! is ignored, whether the pass sees how long it covers).

/// Seconds built up toward the next pass. `Default` is a clock that has just fired.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct PassClock {
    elapsed: f32,
}

impl PassClock {
    /// Advance by `dt`. Once `period` seconds have built up, restart and return the seconds
    /// this pass covers (at least `period`); otherwise `None`. A non-finite `dt` adds nothing,
    /// so one bad frame cannot fire a pass or leave the clock stuck at infinity; a negative one
    /// counts as zero.
    pub(crate) fn due(&mut self, dt: f32, period: f32) -> Option<f32> {
        if dt.is_finite() {
            self.elapsed += dt.max(0.0);
        }
        (self.elapsed >= period).then(|| std::mem::take(&mut self.elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pass_fires_once_the_period_builds_up_and_reports_what_it_covers() {
        let mut clock = PassClock::default();
        assert_eq!(clock.due(0.1, 0.25), None);
        assert_eq!(clock.due(0.1, 0.25), None);
        let covered = clock.due(0.1, 0.25).expect("0.3 s >= 0.25 s");
        assert!((covered - 0.3).abs() < 1e-6);
        assert_eq!(clock.due(0.1, 0.25), None, "the clock restarted at zero");
    }

    #[test]
    fn a_non_finite_or_negative_dt_adds_nothing() {
        let mut clock = PassClock::default();
        for dt in [f32::INFINITY, f32::NAN, f32::NEG_INFINITY, -1.0] {
            assert_eq!(clock.due(dt, 0.25), None, "{dt}");
        }
        assert_eq!(clock, PassClock::default());
    }
}
