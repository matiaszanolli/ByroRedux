//! `--linear-capture PATH [--linear-capture-frames N]`: on the bench-exit
//! frame, average the next N frames of composite's scene-linear output and
//! write a 32-bit-float RGB OpenEXR (M-LIGHT RLT-REF-007, M51 PTR-OUT-001).
//!
//! The M-LIGHT gates compare linear radiance, never the 8-bit tone-mapped
//! PNG `--screenshot` writes, and they average a stated number of frames so
//! a capture's noise floor is a property of that count. The renderer side
//! (`VulkanContext::request_linear_capture`) copies composite's scene image
//! before bloom; this file holds only the argument contract and the
//! per-tick drive `about_to_wait` calls after `advance_screenshot_request`.
//!
//! `--linear-capture-at F1,F2,…` instead writes one single-frame capture
//! during each listed bench frame, to PATH with `{frame}` replaced by the
//! frame index (M-LIGHT RLT-SHD-002 / -010: the temporal gates sample a
//! motion and the frames after it stops). The request is issued before that
//! frame's draw, so the capture shows the pose the bench path chose for it.
//! A listed frame that arrives while the previous capture is still in flight
//! is reported as skipped.
//!
//! `--linear-capture-tap post-taa` reads TAA's resolved output instead of
//! composite's (post-bloom, post-reconstruction; `final` view only), the
//! image the temporal gates measure. The default, `pre-bloom`, is the
//! unreconstructed scene-linear image every reference gate compares.
//!
//! Like `--bench-capture-every`, this is a capture mode: the copy stalls
//! nothing on the GPU, but the per-frame host readback is not free, so take
//! `bench:` timings in a separate run.

use std::path::PathBuf;

use crate::cli_args::parse_string_arg;

/// Ticks to wait past the last owed frame before giving up (the same
/// slack `--screenshot` allows).
const DEADLINE_SLACK_FRAMES: u32 = 60;

#[derive(Debug)]
pub(crate) struct LinearCaptureCli {
    pub path: PathBuf,
    pub frames: u32,
    /// `Some(ticks_left)` once the request is issued.
    pub deadline: Option<u32>,
    pub finished: bool,
    /// `--linear-capture-at`: bench frames to capture, ascending. Empty for
    /// the single bench-exit capture.
    pub at: Vec<u32>,
    /// Index into `at` of the next frame to capture.
    pub next: usize,
    /// The frame whose capture is in flight.
    pub in_flight: Option<u32>,
    /// `--linear-capture-tap`: where every capture of this run reads.
    pub tap: byroredux_renderer::vulkan::context::LinearCaptureTap,
}

/// What the caller does after one tick of [`LinearCaptureCli::advance`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LinearCaptureStep {
    /// Keep rendering frames; the capture is still owed.
    Pending,
    /// Landed, failed or timed out — the bench may exit.
    Done,
}

impl LinearCaptureCli {
    /// Parse the two flags. `Ok(None)` when `--linear-capture` is absent.
    pub(crate) fn from_args(
        args: &[String],
        bench_frames: Option<u32>,
    ) -> Result<Option<Self>, String> {
        let frames = parse_string_arg(args, "--linear-capture-frames");
        let at = parse_string_arg(args, "--linear-capture-at");
        let tap = parse_string_arg(args, "--linear-capture-tap");
        let Some(path) = parse_string_arg(args, "--linear-capture") else {
            if frames.is_some() || at.is_some() || tap.is_some() {
                return Err("--linear-capture-frames / -at / -tap require --linear-capture PATH".to_owned());
            }
            return Ok(None);
        };
        use byroredux_renderer::vulkan::context::LinearCaptureTap;
        let tap = match tap.as_deref() {
            None | Some("pre-bloom") => LinearCaptureTap::PreBloom,
            Some("post-taa") => LinearCaptureTap::PostTaa,
            Some(other) => {
                return Err(format!("--linear-capture-tap '{other}' is not pre-bloom or post-taa"))
            }
        };
        let at = match at {
            None => Vec::new(),
            Some(list) => {
                if !path.contains("{frame}") {
                    return Err("--linear-capture-at needs a PATH containing {frame}".to_owned());
                }
                let mut frames: Vec<u32> = list
                    .split(',')
                    .map(|f| {
                        f.trim()
                            .parse::<u32>()
                            .map_err(|_| format!("--linear-capture-at: '{f}' is not a frame index"))
                    })
                    .collect::<Result<_, _>>()?;
                frames.sort_unstable();
                frames.dedup();
                frames
            }
        };
        if bench_frames.is_none() {
            return Err("--linear-capture requires --bench-frames N".to_owned());
        }
        let frames = match frames {
            None => 1,
            Some(text) => match text.parse::<u32>() {
                Ok(n) if n > 0 => n,
                _ => {
                    return Err(format!(
                        "--linear-capture-frames '{text}' is not a positive frame count"
                    ))
                }
            },
        };
        Ok(Some(Self {
            path: PathBuf::from(path),
            frames,
            deadline: None,
            finished: false,
            at,
            next: 0,
            in_flight: None,
            tap,
        }))
    }

    fn path_for(&self, frame: u32) -> PathBuf {
        PathBuf::from(self.path.to_string_lossy().replace("{frame}", &frame.to_string()))
    }

    /// `--linear-capture-at`: called every frame before its draw with the
    /// bench frame about to be drawn. Collects a landed capture, then
    /// requests this frame's if it is listed; returns whether it did.
    pub(crate) fn advance_sequence(&mut self, ctx: &mut byroredux_renderer::VulkanContext, frame: u32) -> bool {
        if self.at.is_empty() {
            return false;
        }
        let mut requested = false;
        if let Some(captured) = self.in_flight {
            if let Some(outcome) = ctx.take_linear_capture_outcome() {
                report_outcome(outcome, Some(captured));
                self.in_flight = None;
            }
        }
        while self.next < self.at.len() && self.at[self.next] <= frame {
            let due = self.at[self.next];
            self.next += 1;
            if due == frame && self.in_flight.is_none() {
                ctx.request_linear_capture_at(self.path_for(frame), 1, self.tap);
                self.in_flight = Some(frame);
                requested = true;
            } else {
                eprintln!("linear-capture: frame {due} skipped (a capture was in flight)");
            }
        }
        requested
    }

    /// One `about_to_wait` tick after the bench target is reached: issue the
    /// request, then poll the renderer for its outcome.
    pub(crate) fn advance(
        &mut self,
        ctx: &mut byroredux_renderer::VulkanContext,
    ) -> LinearCaptureStep {
        if self.finished {
            return LinearCaptureStep::Done;
        }
        if !self.at.is_empty() {
            // The sequence is driven per frame by `advance_sequence`; the
            // bench waits for its last capture, then exits.
            let ticks_left = *self.deadline.get_or_insert(DEADLINE_SLACK_FRAMES);
            if let (Some(captured), Some(outcome)) = (self.in_flight, ctx.take_linear_capture_outcome()) {
                report_outcome(outcome, Some(captured));
                self.in_flight = None;
            }
            if (self.next >= self.at.len() && self.in_flight.is_none()) || ticks_left == 0 {
                for frame in &self.at[self.next..] {
                    eprintln!("linear-capture: frame {frame} never reached (the bench ended first)");
                }
                self.finished = true;
                return LinearCaptureStep::Done;
            }
            self.deadline = Some(ticks_left - 1);
            return LinearCaptureStep::Pending;
        }
        let Some(ticks_left) = self.deadline else {
            ctx.request_linear_capture_at(self.path.clone(), self.frames, self.tap);
            self.deadline = Some(self.frames + DEADLINE_SLACK_FRAMES);
            return LinearCaptureStep::Pending;
        };
        if let Some(outcome) = ctx.take_linear_capture_outcome() {
            report_outcome(outcome, None);
            self.finished = true;
            return LinearCaptureStep::Done;
        }
        if ticks_left == 0 {
            eprintln!("linear-capture: timed out waiting for {}", self.path.display());
            self.finished = true;
            return LinearCaptureStep::Done;
        }
        self.deadline = Some(ticks_left - 1);
        LinearCaptureStep::Pending
    }
}

fn report_outcome(
    outcome: Result<byroredux_renderer::vulkan::context::LinearCaptureReport, String>,
    frame: Option<u32>,
) {
    let at = frame.map(|f| format!(", bench_frame={f}")).unwrap_or_default();
    match outcome {
        Ok(report) => println!(
            "linear-capture: wrote {} ({}x{}, frames={}, nonfinite={}{at})",
            report.path.display(),
            report.width,
            report.height,
            report.frames,
            report.nonfinite_samples
        ),
        Err(e) => eprintln!("linear-capture: failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_path_and_frame_count() {
        let parsed = LinearCaptureCli::from_args(
            &args(&["--linear-capture", "out/a.exr", "--linear-capture-frames", "16"]),
            Some(64),
        )
        .unwrap()
        .unwrap();
        assert_eq!(parsed.path, PathBuf::from("out/a.exr"));
        assert_eq!(parsed.frames, 16);
    }

    #[test]
    fn frame_count_defaults_to_one() {
        let parsed = LinearCaptureCli::from_args(&args(&["--linear-capture", "a.exr"]), Some(8))
            .unwrap()
            .unwrap();
        assert_eq!(parsed.frames, 1);
    }

    #[test]
    fn rejects_missing_bench_frames_zero_count_and_orphan_count() {
        assert!(LinearCaptureCli::from_args(&args(&["--linear-capture", "a.exr"]), None).is_err());
        assert!(LinearCaptureCli::from_args(
            &args(&["--linear-capture", "a.exr", "--linear-capture-frames", "0"]),
            Some(8)
        )
        .is_err());
        assert!(
            LinearCaptureCli::from_args(&args(&["--linear-capture-frames", "4"]), Some(8)).is_err()
        );
        assert!(LinearCaptureCli::from_args(&args(&[]), Some(8)).unwrap().is_none());
    }

    #[test]
    fn capture_at_lists_sorted_frames_and_needs_a_frame_placeholder() {
        let parsed = LinearCaptureCli::from_args(
            &args(&["--linear-capture", "out/f_{frame}.exr", "--linear-capture-at", "16, 4,1,4"]),
            Some(64),
        )
        .unwrap()
        .unwrap();
        assert_eq!(parsed.at, vec![1, 4, 16]);
        assert_eq!(parsed.path_for(4), PathBuf::from("out/f_4.exr"));
        assert!(LinearCaptureCli::from_args(
            &args(&["--linear-capture", "out/f.exr", "--linear-capture-at", "4"]),
            Some(64)
        )
        .is_err());
        assert!(LinearCaptureCli::from_args(
            &args(&["--linear-capture", "f_{frame}.exr", "--linear-capture-at", "4,x"]),
            Some(64)
        )
        .is_err());
    }

    #[test]
    fn tap_defaults_to_pre_bloom_and_rejects_unknown_names() {
        use byroredux_renderer::vulkan::context::LinearCaptureTap;
        let parsed = LinearCaptureCli::from_args(&args(&["--linear-capture", "a.exr"]), Some(8)).unwrap().unwrap();
        assert_eq!(parsed.tap, LinearCaptureTap::PreBloom);
        let parsed = LinearCaptureCli::from_args(
            &args(&["--linear-capture", "a.exr", "--linear-capture-tap", "post-taa"]),
            Some(8),
        )
        .unwrap()
        .unwrap();
        assert_eq!(parsed.tap, LinearCaptureTap::PostTaa);
        assert!(LinearCaptureCli::from_args(
            &args(&["--linear-capture", "a.exr", "--linear-capture-tap", "after"]),
            Some(8)
        )
        .is_err());
    }
}
