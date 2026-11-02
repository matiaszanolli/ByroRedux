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
        let Some(path) = parse_string_arg(args, "--linear-capture") else {
            if frames.is_some() {
                return Err("--linear-capture-frames requires --linear-capture PATH".to_owned());
            }
            return Ok(None);
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
        }))
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
        let Some(ticks_left) = self.deadline else {
            ctx.request_linear_capture(self.path.clone(), self.frames);
            self.deadline = Some(self.frames + DEADLINE_SLACK_FRAMES);
            return LinearCaptureStep::Pending;
        };
        if let Some(outcome) = ctx.take_linear_capture_outcome() {
            match outcome {
                Ok(report) => println!(
                    "linear-capture: wrote {} ({}x{}, frames={}, nonfinite={})",
                    report.path.display(),
                    report.width,
                    report.height,
                    report.frames,
                    report.nonfinite_samples
                ),
                Err(e) => eprintln!("linear-capture: failed: {e}"),
            }
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
}
