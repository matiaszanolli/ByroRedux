//! `--bench-capture-every N --bench-capture-dir DIR`: write a PNG of every Nth
//! rendered frame of a finite `--bench-frames` run (#6531 fire/smoke review).
//!
//! `--screenshot` only keeps the final frame, which cannot show flicker or
//! temporal instability. Screen-recording the engine is not a substitute (it
//! depends on the display server, and `x11grab` cannot see a Wayland session),
//! so the frames are taken from inside the renderer through the same
//! `ScreenshotBridge` the `--screenshot` path uses. Under
//! `--bench-mode renderer-stepped` the simulation clock and camera are pure
//! functions of the frame index, so the same command writes the same frames.
//!
//! This is a *capture* mode: every screenshot stalls the frame it lands on, so
//! the `bench:` timings of a run that uses it are not a baseline. Take timings
//! and frames in separate runs.
//!
//! This file holds only the argument contract and the pure frame schedule; the
//! bridge handshake lives next to `advance_screenshot_request` in
//! `app_events.rs`.

use std::path::PathBuf;

use crate::cli_args::parse_string_arg;

/// The bridge is a single slot and a capture takes a frame or two to read back,
/// so a request is only issued while none is in flight. A smaller stride would
/// silently drop due frames; refuse it instead.
pub(crate) const MIN_CAPTURE_STRIDE: u32 = 3;

/// A request issued within this many frames of the end of the run is skipped,
/// so the last in-flight capture has landed before the final `--screenshot`
/// claims the bridge.
pub(crate) const TAIL_GUARD_FRAMES: u32 = 3;

/// Frames to wait for one PNG before giving up on it (same budget as the
/// final `--screenshot`).
pub(crate) const CAPTURE_DEADLINE_FRAMES: u32 = 60;

#[derive(Debug)]
pub(crate) struct FrameSequenceCapture {
    pub every: u32,
    pub dir: PathBuf,
    /// Frame index of the capture currently in flight, and the ticks left to
    /// wait for it.
    pub pending: Option<(u32, u32)>,
    pub written: u32,
    /// Due frames that could not be requested because a capture was still in
    /// flight. Reported at the end of the run; non-zero means a gap.
    pub skipped: u32,
    /// Last frame index the schedule was consulted for. `about_to_wait` can tick
    /// more than once per rendered frame; a frame is acted on once.
    pub last_seen: Option<u32>,
}

impl FrameSequenceCapture {
    /// Parse the two flags. `Ok(None)` when neither is present.
    pub(crate) fn from_args(
        args: &[String],
        bench_frames: Option<u32>,
    ) -> Result<Option<Self>, String> {
        let every = parse_string_arg(args, "--bench-capture-every");
        let dir = parse_string_arg(args, "--bench-capture-dir");
        let (every, dir) = match (every, dir) {
            (None, None) => return Ok(None),
            (Some(e), Some(d)) => (e, d),
            _ => {
                return Err(
                    "--bench-capture-every and --bench-capture-dir must be given together"
                        .to_owned(),
                )
            }
        };
        if bench_frames.is_none() {
            return Err("--bench-capture-every requires --bench-frames N".to_owned());
        }
        let every: u32 = every
            .parse()
            .map_err(|_| format!("--bench-capture-every '{every}' is not a frame count"))?;
        if every < MIN_CAPTURE_STRIDE {
            return Err(format!(
                "--bench-capture-every must be at least {MIN_CAPTURE_STRIDE}: the capture \
                 bridge holds one frame at a time"
            ));
        }
        Ok(Some(Self {
            every,
            dir: PathBuf::from(dir),
            pending: None,
            written: 0,
            skipped: 0,
            last_seen: None,
        }))
    }

    /// Is rendered-frame index `frame` one the sequence wants, in a run of
    /// `target` frames? Frame 0 is the first rendered frame; the last
    /// `TAIL_GUARD_FRAMES` are left to the final `--screenshot`.
    pub(crate) fn frame_due(&self, frame: u32, target: u32) -> bool {
        frame % self.every == 0 && frame.saturating_add(TAIL_GUARD_FRAMES) < target
    }

    pub(crate) fn path_for(&self, frame: u32) -> PathBuf {
        self.dir.join(format!("frame_{frame:05}.png"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn absent_flags_mean_no_capture() {
        assert!(FrameSequenceCapture::from_args(&args(&["byroredux"]), Some(300))
            .unwrap()
            .is_none());
    }

    #[test]
    fn flags_must_come_as_a_pair_and_with_bench_frames() {
        let only_every = args(&["x", "--bench-capture-every", "10"]);
        assert!(FrameSequenceCapture::from_args(&only_every, Some(300)).is_err());
        let both = args(&["x", "--bench-capture-every", "10", "--bench-capture-dir", "o"]);
        assert!(FrameSequenceCapture::from_args(&both, None).is_err());
        assert!(FrameSequenceCapture::from_args(&both, Some(300)).unwrap().is_some());
    }

    #[test]
    fn stride_below_the_bridge_latency_is_refused() {
        let a = args(&["x", "--bench-capture-every", "2", "--bench-capture-dir", "o"]);
        assert!(FrameSequenceCapture::from_args(&a, Some(300)).is_err());
        let a = args(&["x", "--bench-capture-every", "0", "--bench-capture-dir", "o"]);
        assert!(FrameSequenceCapture::from_args(&a, Some(300)).is_err());
        let a = args(&["x", "--bench-capture-every", "three", "--bench-capture-dir", "o"]);
        assert!(FrameSequenceCapture::from_args(&a, Some(300)).is_err());
    }

    #[test]
    fn schedule_is_every_nth_frame_and_leaves_the_tail_alone() {
        let a = args(&["x", "--bench-capture-every", "10", "--bench-capture-dir", "o"]);
        let c = FrameSequenceCapture::from_args(&a, Some(30)).unwrap().unwrap();
        let due: Vec<u32> = (0..30).filter(|&f| c.frame_due(f, 30)).collect();
        // Frame 30 does not exist; 20 is 10 frames from the end so it is kept.
        assert_eq!(due, vec![0, 10, 20]);
        // A frame inside the tail guard is never requested.
        let c = FrameSequenceCapture::from_args(
            &args(&["x", "--bench-capture-every", "3", "--bench-capture-dir", "o"]),
            Some(10),
        )
        .unwrap()
        .unwrap();
        let due: Vec<u32> = (0..10).filter(|&f| c.frame_due(f, 10)).collect();
        assert_eq!(due, vec![0, 3, 6]);
    }

    #[test]
    fn file_names_sort_in_frame_order() {
        let a = args(&["x", "--bench-capture-every", "5", "--bench-capture-dir", "out"]);
        let c = FrameSequenceCapture::from_args(&a, Some(300)).unwrap().unwrap();
        assert_eq!(c.path_for(5), PathBuf::from("out/frame_00005.png"));
        assert!(c.path_for(5) < c.path_for(100));
    }
}
