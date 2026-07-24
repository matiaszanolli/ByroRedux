//! Deterministic camera motion for benchmark and image-quality runs.
//!
//! Temporal reconstruction is only interesting when the camera moves: a
//! parked camera reprojects onto itself and every upscaler looks correct.
//! Every quality question worth asking — does history smear under a pan, do
//! disoccluded edges reconstruct, does a hard cut recover — needs the camera
//! driven along a repeatable path.
//!
//! The engine's only camera drivers are the fly camera (which needs mouse
//! capture) and the character controller (which needs a player rig), so a
//! headless `--bench-frames` run has no way to move the camera at all. This
//! module supplies one.
//!
//! ## Determinism
//!
//! Poses are a pure function of the **frame index**, never of wall-clock time
//! or delta-time. Two runs at different frame rates therefore produce the
//! same pose at the same frame number, which is what makes a captured frame
//! comparable across upscaler presets — the whole point of the exercise. This
//! is the same reasoning behind `BYROREDUX_FIXED_DT`, one level up: freeze
//! everything that is not the variable under test.

use byroredux_core::math::Vec3;
use std::str::FromStr;

/// One frame of camera motion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    pub position: Vec3,
    /// Unit forward vector. Always normalized by construction.
    pub forward: Vec3,
}

/// A repeatable camera path, selected by `--bench-camera`.
///
/// Each variant isolates one failure mode a temporal upscaler can have. They
/// are deliberately separate rather than one combined fly-through: when a
/// preset regresses, the failing path names the mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BenchCameraPath {
    /// The scene's authored pose, unchanged. The reference case, and the
    /// default when `--bench-camera` is absent.
    #[default]
    Static,
    /// Yaw sweep about the start position. Screen-space motion is near
    /// uniform and largely horizontal, which is the cleanest test of motion
    /// vector sign and scale: get the sign wrong and the whole frame smears.
    Pan,
    /// Orbit the look-at point at fixed radius. Combines rotation with
    /// translation, so foreground and background move at different screen
    /// rates — parallax, which is what exposes a wrong depth or a history
    /// that reprojects against the wrong surface.
    Orbit,
    /// Dolly straight ahead. Motion is radial from the screen centre and the
    /// frame edges continuously disocclude, which is the case a reactive mask
    /// cannot help with and only depth-based disocclusion detection covers.
    Dolly,
    /// Hold, then teleport once near the end of the run. Nothing about the
    /// new view is predictable from the old history, so this measures how
    /// fast reconstruction recovers from a discontinuity — and whether the
    /// engine's reset actually reached the upscaler.
    Cut,
}

impl BenchCameraPath {
    /// Fraction of the run spent before [`BenchCameraPath::Cut`] jumps.
    /// Leaves a short tail so the captured final frame is *recovering* from
    /// the cut rather than fully re-converged — a fully-settled frame would
    /// measure nothing.
    const CUT_AT: f32 = 0.8;

    /// Total yaw travelled by [`BenchCameraPath::Pan`] over the run.
    ///
    /// Deliberately modest. Temporal reconstruction operates on frames that
    /// overlap heavily — that is the regime where its artifacts (smearing,
    /// history rejection, edge shimmer) appear at all. A large sweep instead
    /// walks the subject off screen, and a frame of empty background measures
    /// nothing about the upscaler. 15° across the run is several hundred
    /// pixels of travel at a typical FOV while keeping the subject framed
    /// from the first frame to the last.
    const PAN_SWEEP_RADIANS: f32 = 15.0 * std::f32::consts::PI / 180.0;

    /// Arc travelled by [`BenchCameraPath::Orbit`]. Larger than the pan
    /// sweep because an orbit keeps the subject centred by construction, so
    /// the framing constraint is only that the camera stay in a sensible
    /// vantage rather than that the subject stay in view.
    const ORBIT_ARC_RADIANS: f32 = 20.0 * std::f32::consts::PI / 180.0;

    /// Fraction of the start distance-to-target that [`BenchCameraPath::Dolly`]
    /// closes over the run. Stops well short of the target so the camera
    /// cannot pass through the geometry it is looking at.
    const DOLLY_CLOSE_FRACTION: f32 = 0.35;

    /// Pose for `frame` of a `total_frames`-long run, starting from the
    /// scene's authored `origin` / `forward`.
    ///
    /// `total_frames` normalizes the path so a 60-frame and a 300-frame run
    /// trace the *same* path at different sampling densities. Without that,
    /// a longer run would pan further and the captured frames would not be
    /// comparable across bench lengths.
    ///
    /// Clamps rather than wraps past the end: the final pose is held, so a
    /// capture taken one frame late still lands on the intended view.
    pub fn pose(self, frame: u32, total_frames: u32, origin: Vec3, forward: Vec3) -> CameraPose {
        let forward = normalize_or(forward, -Vec3::Z);
        let progress = if total_frames <= 1 {
            0.0
        } else {
            (frame as f32 / (total_frames - 1) as f32).clamp(0.0, 1.0)
        };

        match self {
            Self::Static => CameraPose {
                position: origin,
                forward,
            },
            Self::Pan => CameraPose {
                position: origin,
                forward: yaw_about_up(forward, Self::PAN_SWEEP_RADIANS * progress),
            },
            Self::Orbit => {
                // Orbit the point the camera already looks at, so the subject
                // stays framed while the viewpoint sweeps around it.
                let radius = origin.distance(Vec3::ZERO).max(1.0);
                let target = origin + forward * radius;
                let angle = Self::ORBIT_ARC_RADIANS * progress;
                let offset = yaw_about_up(origin - target, angle);
                let position = target + offset;
                CameraPose {
                    position,
                    forward: normalize_or(target - position, forward),
                }
            }
            Self::Dolly => {
                let radius = origin.distance(Vec3::ZERO).max(1.0);
                let travel = radius * Self::DOLLY_CLOSE_FRACTION * progress;
                CameraPose {
                    position: origin + forward * travel,
                    forward,
                }
            }
            Self::Cut => {
                if progress < Self::CUT_AT {
                    CameraPose {
                        position: origin,
                        forward,
                    }
                } else {
                    // Jump to the orbit path's far end rather than spinning
                    // in place. A large yaw would invalidate history just as
                    // thoroughly but usually lands on empty background, and a
                    // frame of background measures nothing about recovery.
                    // Reusing the orbit endpoint guarantees the post-cut view
                    // is a genuinely different vantage that still frames the
                    // subject.
                    Self::Orbit.pose(
                        total_frames.saturating_sub(1),
                        total_frames,
                        origin,
                        forward,
                    )
                }
            }
        }
    }

    /// Whether this path teleports at `frame`, i.e. the engine should treat
    /// it as a camera cut and reset temporal history. Only true on the exact
    /// transition frame — a reset every frame would disable reconstruction
    /// entirely and make the measurement meaningless.
    pub fn is_cut_frame(self, frame: u32, total_frames: u32) -> bool {
        if self != Self::Cut || total_frames <= 1 {
            return false;
        }
        let cut_frame = (Self::CUT_AT * (total_frames - 1) as f32).ceil() as u32;
        frame == cut_frame
    }

    /// Every path, in the order they are documented. Drives the CLI's
    /// error message and the harness matrix, so both stay complete by
    /// construction rather than by remembering to update a second list.
    pub const ALL: [Self; 5] = [Self::Static, Self::Pan, Self::Orbit, Self::Dolly, Self::Cut];
}

impl std::fmt::Display for BenchCameraPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Static => "static",
            Self::Pan => "pan",
            Self::Orbit => "orbit",
            Self::Dolly => "dolly",
            Self::Cut => "cut",
        })
    }
}

impl FromStr for BenchCameraPath {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "static" => Ok(Self::Static),
            "pan" => Ok(Self::Pan),
            "orbit" => Ok(Self::Orbit),
            "dolly" => Ok(Self::Dolly),
            "cut" => Ok(Self::Cut),
            other => {
                // Derive the expected list from `ALL` so a new variant cannot
                // be added without the error message learning about it.
                let names: Vec<String> = Self::ALL.iter().map(Self::to_string).collect();
                Err(format!(
                    "unknown --bench-camera '{other}'; expected one of: {}",
                    names.join(", ")
                ))
            }
        }
    }
}

/// Rotate `v` about world up (+Y). The engine's cameras are yaw/pitch rigs
/// with no roll, so a world-up rotation is the correct notion of "turn" and
/// keeps the horizon level through the whole path.
fn yaw_about_up(v: Vec3, radians: f32) -> Vec3 {
    let (sin, cos) = radians.sin_cos();
    Vec3::new(v.x * cos + v.z * sin, v.y, -v.x * sin + v.z * cos)
}

/// Normalize, or fall back when the input is degenerate. A zero forward
/// vector would otherwise produce NaNs that propagate into the view matrix
/// and blank the frame — a failure that looks like a renderer bug.
fn normalize_or(v: Vec3, fallback: Vec3) -> Vec3 {
    if v.length_squared() > 1e-8 {
        v.normalize()
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: Vec3 = Vec3::new(0.0, 2.0, 10.0);
    const FORWARD: Vec3 = Vec3::new(0.0, 0.0, -1.0);
    const FRAMES: u32 = 60;

    fn pose(path: BenchCameraPath, frame: u32) -> CameraPose {
        path.pose(frame, FRAMES, ORIGIN, FORWARD)
    }

    /// The property the whole harness rests on: the same frame index yields
    /// the same pose, so two runs at different frame rates — or under
    /// different upscalers — are comparing the same view.
    #[test]
    fn every_path_is_a_pure_function_of_the_frame_index() {
        for path in BenchCameraPath::ALL {
            for frame in [0, 1, 17, FRAMES - 1] {
                assert_eq!(pose(path, frame), pose(path, frame), "{path} at {frame}");
            }
        }
    }

    /// A path normalized on `total_frames` traces the same shape at any
    /// bench length — otherwise a 300-frame run would pan five times as far
    /// as a 60-frame run and the two captures would not be comparable.
    #[test]
    fn path_shape_is_independent_of_bench_length() {
        for path in BenchCameraPath::ALL {
            let short = path.pose(59, 60, ORIGIN, FORWARD);
            let long = path.pose(299, 300, ORIGIN, FORWARD);
            assert!(
                short.position.distance(long.position) < 1e-3,
                "{path}: end position drifted with bench length"
            );
            assert!(
                short.forward.distance(long.forward) < 1e-3,
                "{path}: end forward drifted with bench length"
            );
        }
    }

    #[test]
    fn static_path_never_moves() {
        for frame in [0, 30, FRAMES - 1] {
            let p = pose(BenchCameraPath::Static, frame);
            assert_eq!(p.position, ORIGIN);
            assert_eq!(p.forward, FORWARD);
        }
    }

    /// Pan rotates without translating, and actually reaches its full sweep.
    #[test]
    fn pan_turns_in_place_through_the_full_sweep() {
        let start = pose(BenchCameraPath::Pan, 0);
        let end = pose(BenchCameraPath::Pan, FRAMES - 1);
        assert_eq!(start.position, ORIGIN);
        assert_eq!(end.position, ORIGIN);
        assert_eq!(start.forward, FORWARD);
        // Turned by the full sweep, and no further: a path that overshoots
        // walks the subject off screen and the capture measures nothing.
        let turned = start.forward.dot(end.forward).clamp(-1.0, 1.0).acos();
        assert!(
            (turned - BenchCameraPath::PAN_SWEEP_RADIANS).abs() < 1e-3,
            "panned {turned} rad, expected {}",
            BenchCameraPath::PAN_SWEEP_RADIANS
        );
    }

    /// Orbit keeps the subject framed: the point the camera looked at
    /// initially stays on the forward ray for the whole path.
    #[test]
    fn orbit_keeps_the_look_at_point_centred() {
        let radius = ORIGIN.distance(Vec3::ZERO).max(1.0);
        let target = ORIGIN + FORWARD * radius;
        for frame in [0, 20, 40, FRAMES - 1] {
            let p = pose(BenchCameraPath::Orbit, frame);
            let to_target = (target - p.position).normalize();
            assert!(
                to_target.dot(p.forward) > 0.999,
                "orbit lost the target at frame {frame}: dot {}",
                to_target.dot(p.forward)
            );
        }
    }

    /// Orbit must genuinely move the viewpoint — a rotation-only "orbit"
    /// would produce no parallax and silently duplicate the pan case.
    #[test]
    fn orbit_translates_the_viewpoint() {
        let start = pose(BenchCameraPath::Orbit, 0);
        let end = pose(BenchCameraPath::Orbit, FRAMES - 1);
        assert!(
            start.position.distance(end.position) > 1.0,
            "orbit barely moved: {} units",
            start.position.distance(end.position)
        );
    }

    /// Dolly closes on the subject without reaching or passing it.
    #[test]
    fn dolly_advances_along_forward_and_stops_short() {
        let start = pose(BenchCameraPath::Dolly, 0);
        let end = pose(BenchCameraPath::Dolly, FRAMES - 1);
        assert_eq!(start.position, ORIGIN);
        assert_eq!(end.forward, FORWARD);
        let travelled = start.position.distance(end.position);
        let radius = ORIGIN.distance(Vec3::ZERO);
        assert!(travelled > 0.0, "dolly did not advance");
        assert!(
            travelled < radius,
            "dolly overshot the subject: travelled {travelled} of {radius}"
        );
    }

    /// The cut holds one pose, then jumps once to the orbit endpoint — and
    /// reports exactly one cut frame, since resetting every frame would
    /// disable reconstruction rather than measure its recovery.
    #[test]
    fn cut_holds_then_jumps_once_to_a_framed_vantage() {
        let before = pose(BenchCameraPath::Cut, 0);
        let after = pose(BenchCameraPath::Cut, FRAMES - 1);
        assert_eq!(before.position, ORIGIN);
        assert_eq!(before.forward, FORWARD);

        // The post-cut pose is the orbit endpoint: a real viewpoint change,
        // not a spin onto empty background.
        let orbit_end = pose(BenchCameraPath::Orbit, FRAMES - 1);
        assert!(after.position.distance(orbit_end.position) < 1e-3);
        assert!(
            after.position.distance(before.position) > 1.0,
            "cut did not change the viewpoint"
        );

        let cut_frames: Vec<u32> = (0..FRAMES)
            .filter(|&f| BenchCameraPath::Cut.is_cut_frame(f, FRAMES))
            .collect();
        assert_eq!(cut_frames.len(), 1, "cut frames: {cut_frames:?}");
        let cut = cut_frames[0];
        assert_eq!(
            pose(BenchCameraPath::Cut, cut - 1).position,
            before.position
        );
        assert_eq!(pose(BenchCameraPath::Cut, cut).position, after.position);
    }

    /// Only the cut path reports cut frames; the continuous paths must never
    /// trigger a history reset, or they would measure nothing.
    #[test]
    fn continuous_paths_never_report_a_cut() {
        for path in BenchCameraPath::ALL {
            if path == BenchCameraPath::Cut {
                continue;
            }
            for frame in 0..FRAMES {
                assert!(!path.is_cut_frame(frame, FRAMES), "{path} reported a cut");
            }
        }
    }

    /// A degenerate forward must not produce NaNs — those propagate into the
    /// view matrix and blank the frame, which reads as a renderer bug.
    #[test]
    fn degenerate_forward_falls_back_instead_of_producing_nan() {
        for path in BenchCameraPath::ALL {
            let p = path.pose(30, FRAMES, ORIGIN, Vec3::ZERO);
            assert!(
                p.forward.is_finite() && p.position.is_finite(),
                "{path} produced a non-finite pose"
            );
            assert!(
                (p.forward.length() - 1.0).abs() < 1e-3,
                "{path} forward not unit"
            );
        }
    }

    #[test]
    fn path_names_round_trip() {
        for path in BenchCameraPath::ALL {
            assert_eq!(path.to_string().parse::<BenchCameraPath>().unwrap(), path);
        }
        assert!("spiral".parse::<BenchCameraPath>().is_err());
    }

    /// Single-frame runs are a legal degenerate case (`--bench-frames 1`) and
    /// must not divide by zero.
    #[test]
    fn single_frame_run_is_degenerate_but_safe() {
        for path in BenchCameraPath::ALL {
            let p = path.pose(0, 1, ORIGIN, FORWARD);
            assert_eq!(p.position, ORIGIN);
            assert!(!path.is_cut_frame(0, 1));
        }
    }
}
