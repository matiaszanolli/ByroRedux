//! Render pacing for an overlay that stays up all session (#4717).
//!
//! [`SwfPlayer::render`](crate::SwfPlayer::render) is expensive: it submits
//! Ruffle's wgpu command list, then blocks on a full-target readback, a row
//! copy and an alpha un-multiply (measured 5–8 ms per pass at 1920×1080). Its
//! only gate used to be `dirty`, and `dirty` is OR'd every tick with Ruffle's
//! `needs_render`. Ruffle raises that after **every movie frame that runs**,
//! even for a movie parked on one frame with nothing on screen moving, so an
//! always-on HUD paid the whole pass at the movie's own frame rate (24–30 Hz)
//! for a picture that never changed. #2719 already made the *upload*
//! content-gated; this paces the *render and readback* in front of it.
//!
//! Ruffle exposes nothing finer than `needs_render` to tell a visible change
//! from a frame that merely ran (its `Stage::invalidated` is raised only by
//! ActionScript's `Stage.invalidate()`), so the pacer learns from the one
//! signal that is exact: whether the last readback differed from the one
//! before. Two rates follow from it:
//!
//! - **Active** — the picture changed on the last pass, or the host just
//!   invalidated it (a pushed value, an input event): passes at most every
//!   [`RenderPacing::active_interval`], the cadence cap.
//! - **Idle** — [`RenderPacing::idle_after`] consecutive passes came back
//!   byte-identical and the host has not touched the movie since: passes
//!   only every [`RenderPacing::idle_interval`]. That is a probe, not a
//!   freeze — a movie that starts animating on its own is noticed within one
//!   idle interval, after which it is active again.
//!
//! Time is the sum of the `dt`s handed to `tick`, not the wall clock, so the
//! policy is deterministic under test and follows the engine's own clock.

use std::time::Duration;

/// How often an always-on overlay may re-render and read back its target.
///
/// Opt-in per player ([`SwfPlayer::set_render_pacing`](crate::SwfPlayer::set_render_pacing)):
/// a modal menu keeps rendering on every dirty tick, because input response
/// there matters more than the cost of a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPacing {
    /// Minimum time between passes while the picture is changing or the host
    /// has just invalidated it. The cadence cap.
    pub active_interval: Duration,
    /// Minimum time between passes once the overlay is idle. Never shorter
    /// than `active_interval`.
    pub idle_interval: Duration,
    /// Consecutive byte-identical passes, with no host invalidation, before
    /// the overlay counts as idle. Enough to ride out a tween that lands on
    /// a few identical frames (8-bit alpha steps, an easing plateau) without
    /// dropping to the probe rate mid-animation.
    pub idle_after: u32,
}

/// The pacing state machine behind [`RenderPacing`]. Pure — no Ruffle, no
/// clock — so the policy is unit-testable without a GPU.
#[derive(Debug)]
pub(crate) struct RenderPacer {
    /// `None` = unpaced: every dirty tick renders (the pre-#4717 behaviour).
    pacing: Option<RenderPacing>,
    /// Ticked seconds since the last pass. Starts infinite so the first pass
    /// is never delayed.
    since_render: f64,
    /// Consecutive passes whose pixels matched the pass before.
    identical_streak: u32,
}

impl RenderPacer {
    pub(crate) fn new() -> Self {
        Self {
            pacing: None,
            since_render: f64::INFINITY,
            identical_streak: 0,
        }
    }

    pub(crate) fn set_pacing(&mut self, pacing: RenderPacing) {
        self.pacing = Some(pacing);
    }

    /// Account for `dt` seconds of ticked time. A non-finite or negative
    /// `dt` adds nothing.
    pub(crate) fn advance(&mut self, dt: f64) {
        if dt.is_finite() && dt > 0.0 {
            self.since_render += dt;
        }
    }

    /// The host touched the movie (input, a pushed value, a stage change):
    /// leave idle at once, so the change is rendered at the active cadence
    /// rather than at the next probe.
    pub(crate) fn wake(&mut self) {
        self.identical_streak = 0;
    }

    /// Whether a pass may run now.
    pub(crate) fn is_due(&self) -> bool {
        let Some(pacing) = self.pacing else {
            return true;
        };
        let interval = if self.identical_streak >= pacing.idle_after {
            pacing.idle_interval.max(pacing.active_interval)
        } else {
            pacing.active_interval
        };
        self.since_render >= interval.as_secs_f64()
    }

    /// A pass ran; `changed` is whether its pixels differed from the pass
    /// before.
    pub(crate) fn note_render(&mut self, changed: bool) {
        self.since_render = 0.0;
        self.identical_streak = if changed {
            0
        } else {
            self.identical_streak.saturating_add(1)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f64 = 1.0 / 30.0;

    /// The always-on HUD shape: 33 ms cap, 250 ms probe, idle after 8.
    fn hud() -> RenderPacing {
        RenderPacing {
            active_interval: Duration::from_millis(33),
            idle_interval: Duration::from_millis(250),
            idle_after: 8,
        }
    }

    /// Drive `ticks` frames of a movie that "runs a frame" every tick (so
    /// Ruffle asks for a render each time) and report how many passes ran.
    /// `changes(pass)` says whether that pass's pixels differ from the last.
    fn passes(
        pacer: &mut RenderPacer,
        ticks: usize,
        mut changes: impl FnMut(usize) -> bool,
    ) -> usize {
        let mut passes = 0;
        for _ in 0..ticks {
            pacer.advance(FRAME);
            if pacer.is_due() {
                pacer.note_render(changes(passes));
                passes += 1;
            }
        }
        passes
    }

    /// An unpaced player renders on every dirty tick — the shape #4717
    /// measured (150 passes in 5 s at 30 Hz, none of them changing the
    /// picture). This is the contract a modal menu keeps.
    #[test]
    fn unpaced_renders_every_dirty_tick() {
        let mut pacer = RenderPacer::new();
        assert_eq!(passes(&mut pacer, 150, |_| false), 150);
    }

    /// #4717 — the regression pin: a static movie (its frames run, nothing
    /// pushed, no input, pixels never change) costs a handful of passes to
    /// prove it is static and then one per idle interval, not one per frame.
    #[test]
    fn a_static_movie_falls_to_the_idle_probe_rate() {
        let mut pacer = RenderPacer::new();
        pacer.set_pacing(hud());
        let count = passes(&mut pacer, 150, |_| false);
        // 8 active passes (one per tick at 30 Hz), then a probe every
        // 250 ms (7.5 ticks → every 8th) over the remaining 142 ticks.
        assert!(
            (8..=8 + 142 / 7 + 1).contains(&count),
            "5 s of a static 30 Hz movie ran {count} passes; unpaced it is 150"
        );
        assert!(count <= 30, "idle probing must be cheap, got {count}");
    }

    /// The cap also holds for a picture that keeps changing: a fast tick
    /// rate (a 240 Hz engine loop) renders at most once per active interval.
    #[test]
    fn a_changing_picture_is_capped_at_the_active_interval() {
        let mut pacer = RenderPacer::new();
        pacer.set_pacing(hud());
        let mut count = 0;
        for _ in 0..240 {
            pacer.advance(1.0 / 240.0);
            if pacer.is_due() {
                pacer.note_render(true);
                count += 1;
            }
        }
        // One second at a 33 ms cap: 30 passes, and never the 240 unpaced.
        assert!((29..=31).contains(&count), "got {count} passes in 1 s");
    }

    /// Going idle needs `idle_after` *consecutive* identical passes; one
    /// changed pass restarts the count.
    #[test]
    fn a_changed_pass_restarts_the_idle_countdown() {
        let mut pacer = RenderPacer::new();
        pacer.set_pacing(hud());
        // Seven identical passes, then a change, then seven more: never idle.
        let count = passes(&mut pacer, 15, |pass| pass == 7);
        assert_eq!(count, 15, "still active after every restart");
        // One more identical pass reaches the eighth in a row: now idle.
        assert_eq!(passes(&mut pacer, 1, |_| false), 1);
        pacer.advance(FRAME);
        assert!(!pacer.is_due(), "idle: the next tick is not a pass");
    }

    /// A host invalidation (input, a pushed value) leaves idle immediately —
    /// the change is rendered at the active cadence, not at the next probe.
    #[test]
    fn a_host_invalidation_wakes_an_idle_overlay() {
        let mut pacer = RenderPacer::new();
        pacer.set_pacing(hud());
        passes(&mut pacer, 40, |_| false);
        pacer.advance(FRAME);
        assert!(!pacer.is_due(), "precondition: idle");

        pacer.wake();
        assert!(
            pacer.is_due(),
            "a woken overlay renders at the 33 ms cadence, not the 250 ms probe"
        );
    }

    /// A movie that starts animating on its own is noticed within one idle
    /// interval, and then renders at the active cadence again.
    #[test]
    fn autonomous_animation_is_found_by_the_probe_and_resumes_full_rate() {
        let mut pacer = RenderPacer::new();
        pacer.set_pacing(hud());
        passes(&mut pacer, 60, |_| false);

        // From here every pass differs. Count the ticks to the first pass.
        let mut ticks_to_first = 0;
        loop {
            pacer.advance(FRAME);
            ticks_to_first += 1;
            if pacer.is_due() {
                pacer.note_render(true);
                break;
            }
            assert!(ticks_to_first < 20, "the probe never fired");
        }
        assert!(
            ticks_to_first as f64 * FRAME <= 0.250 + FRAME,
            "found after {ticks_to_first} ticks, more than one idle interval"
        );
        // Active again: the very next tick renders.
        pacer.advance(FRAME);
        assert!(pacer.is_due());
    }

    /// The first pass is never delayed, and a bad `dt` cannot wedge the
    /// pacer (NaN would otherwise poison the accumulator for good).
    #[test]
    fn the_first_pass_is_immediate_and_bad_dt_is_ignored() {
        let mut pacer = RenderPacer::new();
        pacer.set_pacing(hud());
        assert!(pacer.is_due(), "nothing has rendered yet");
        pacer.note_render(true);
        pacer.advance(f64::NAN);
        pacer.advance(-1.0);
        pacer.advance(f64::INFINITY);
        assert!(!pacer.is_due(), "no time passed, so no pass is due");
        pacer.advance(FRAME);
        assert!(pacer.is_due());
    }

    /// A misconfigured idle interval shorter than the active one cannot make
    /// idling *faster* than active.
    #[test]
    fn idle_is_never_faster_than_active() {
        let mut pacer = RenderPacer::new();
        pacer.set_pacing(RenderPacing {
            active_interval: Duration::from_millis(100),
            idle_interval: Duration::from_millis(10),
            idle_after: 1,
        });
        pacer.note_render(false);
        pacer.note_render(false);
        pacer.advance(0.05);
        assert!(!pacer.is_due());
    }
}
