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

use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
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
    /// Travel east through three complete exterior cells. Unlike the visual-
    /// quality paths above, this is an engine-streaming workload: the fixed
    /// world-axis displacement guarantees three boundary crossings no matter
    /// where inside the starting cell the authored camera happens to be.
    GridCross,
    /// Hold, then teleport once near the end of the run. Nothing about the
    /// new view is predictable from the old history, so this measures how
    /// fast reconstruction recovers from a discontinuity — and whether the
    /// engine's reset actually reached the upscaler.
    Cut,
    /// Repeated out-and-back traversal for the EX-08 ownership soak (#2374).
    ///
    /// Where [`Self::GridCross`] travels one way and asks whether each
    /// boundary *settled*, this triangle-waves across the same boundaries N
    /// times and asks whether every owner came *back*. The reversal is the
    /// point: turning around mid-traversal is what exercises pending-worker
    /// cancellation, partial-apply cancellation, unload hysteresis, and
    /// stale-payload rejection, none of which a one-way path reaches at all.
    ///
    /// Deliberately shorter per leg than `GridCross`. Two cells is enough to
    /// cross a boundary and force the far cell to load, while keeping each
    /// cycle short enough that a useful cycle count fits in one run.
    GridSoak,
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

    /// Fallback subject distance, BU, when the caller could not measure one
    /// (no `PhysicsWorld`, or the camera looks at open sky).
    ///
    /// Room scale: 512 BU is ~7.3 m at the engine's 70 BU/m, i.e. the far wall
    /// of an ordinary Bethesda interior. Only reached when a real measurement
    /// is unavailable — `subject_distance` is otherwise a cast result, not a
    /// tuning constant.
    pub const FALLBACK_SUBJECT_DISTANCE_BU: f32 = 512.0;

    /// Three complete cells guarantees three boundary transitions from any
    /// finite starting X coordinate while leaving enough frames between them
    /// for the async worker and resumable main-thread apply to settle.
    const GRID_CROSS_DISTANCE_BU: f32 = 3.0 * EXTERIOR_CELL_UNITS;

    /// Out-leg length for [`Self::GridSoak`]. Two cells crosses at least one
    /// boundary from any starting X and forces the far cell to load; going
    /// further would buy no additional owner classes and cost cycles.
    const GRID_SOAK_DISTANCE_BU: f32 = 2.0 * EXTERIOR_CELL_UNITS;
    /// Out-and-back round trips per soak run.
    ///
    /// Must exceed `MIN_CYCLES_FOR_GROWTH` (4) by enough that the *recorded*
    /// cycles still support a growth verdict after the first round trip is
    /// consumed as the baseline: 6 traversals yield 5 recorded cycles.
    pub const GRID_SOAK_CYCLES: u32 = 6;
    /// Reserve the final 15% as a settle window, same rationale as
    /// `GRID_CROSS_MOVE_FRACTION` — the last return must finish unloading
    /// before the run ends or every correct implementation reports a surplus.
    const GRID_SOAK_MOVE_FRACTION: f32 = 0.85;
    /// Reserve the final 30% of the run as a settle window after crossing the
    /// third boundary. Without a tail, the final load would begin on the exit
    /// frame and every correct implementation would report it unfinished.
    const GRID_CROSS_MOVE_FRACTION: f32 = 0.70;

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
    ///
    /// `subject_distance` is how far ahead of `origin` the thing being looked
    /// at actually is, in BU — measured by the caller with a forward ray cast
    /// (see `App::measure_bench_subject_distance`), falling back to
    /// [`Self::FALLBACK_SUBJECT_DISTANCE_BU`]. Only `Orbit` and `Dolly` read
    /// it; the other paths ignore it entirely.
    ///
    /// Pre-fix both derived their radius as `origin.distance(Vec3::ZERO)` —
    /// the camera's distance from the **world origin**, which is not a subject
    /// distance but an artefact of where the cell happens to be placed. On
    /// FNV's `GSProspectorSaloonInterior` the camera spawns 3610 BU from the
    /// origin, so `Orbit` put its target 3610 BU ahead of a camera standing in
    /// a small saloon room and swung the viewpoint ~1100 BU sideways — clean
    /// out of the building. Measured on `4de5e78e`: `gpu_main_render` fell
    /// from 11.526 ms under `pan` to **0.010 ms** under `orbit` at an
    /// identical 1214 draws, i.e. the bench was timing an empty view. FO4's
    /// Dugout Inn showed the same signature. Cornell only escaped it by being
    /// authored at the origin.
    pub fn pose(
        self,
        frame: u32,
        total_frames: u32,
        origin: Vec3,
        forward: Vec3,
        subject_distance: f32,
    ) -> CameraPose {
        let forward = normalize_or(forward, -Vec3::Z);
        let subject_distance = if subject_distance.is_finite() && subject_distance > 0.0 {
            subject_distance
        } else {
            Self::FALLBACK_SUBJECT_DISTANCE_BU
        };
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
                let radius = subject_distance;
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
                // Same defect, same fix: the fraction must close on the real
                // subject, or a cell placed far from the world origin dollies
                // straight through it.
                let travel = subject_distance * Self::DOLLY_CLOSE_FRACTION * progress;
                CameraPose {
                    position: origin + forward * travel,
                    forward,
                }
            }
            Self::GridCross => {
                let travel_progress = (progress / Self::GRID_CROSS_MOVE_FRACTION).min(1.0);
                CameraPose {
                    position: origin + Vec3::X * (Self::GRID_CROSS_DISTANCE_BU * travel_progress),
                    forward,
                }
            }
            Self::GridSoak => {
                // Triangle wave: each cycle runs origin → +distance → origin.
                // At the exact end of the move window `saw` lands back on 0, so
                // the path finishes where it started and the final unload has
                // the settle tail to complete in.
                let travel_progress = (progress / Self::GRID_SOAK_MOVE_FRACTION).min(1.0);
                let t = travel_progress * Self::GRID_SOAK_CYCLES as f32;
                let saw = t - t.floor();
                let triangle = if saw < 0.5 {
                    saw * 2.0
                } else {
                    2.0 - saw * 2.0
                };
                CameraPose {
                    position: origin + Vec3::X * (Self::GRID_SOAK_DISTANCE_BU * triangle),
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
                        subject_distance,
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
    pub const ALL: [Self; 7] = [
        Self::Static,
        Self::Pan,
        Self::Orbit,
        Self::Dolly,
        Self::GridCross,
        Self::Cut,
        Self::GridSoak,
    ];

    /// Index of the out-and-back cycle that *completes* on `frame`, if any.
    ///
    /// Returns `Some(n)` only on the exact frame the camera returns to the
    /// origin for the `n`-th time (0-based), so the caller can sample
    /// ownership once per cycle rather than per frame. `None` on every other
    /// frame and on every non-soak path.
    ///
    /// The soak treats cycle 0's completion as its *baseline* — by then the
    /// one-time bootstrap allocations (worldspace weather textures, the
    /// fallback checkerboard, the reverb send track) have happened and been
    /// through one unload, so they sit inside the baseline instead of being
    /// reported as leaks.
    pub fn soak_cycle_completed(self, frame: u32, total_frames: u32) -> Option<u32> {
        if self != Self::GridSoak || total_frames <= 1 || frame == 0 {
            return None;
        }
        let cycle_at = |f: u32| -> u32 {
            let progress = (f as f32 / (total_frames - 1) as f32).clamp(0.0, 1.0);
            let travel = (progress / Self::GRID_SOAK_MOVE_FRACTION).min(1.0);
            (travel * Self::GRID_SOAK_CYCLES as f32).floor() as u32
        };
        let previous = cycle_at(frame - 1);
        let current = cycle_at(frame);
        (current > previous).then_some(previous)
    }
}

pub(crate) fn advance_grid_cross_frame(
    frame: u32,
    total_frames: u32,
    boundary_in_progress: bool,
) -> u32 {
    if boundary_in_progress {
        frame.min(total_frames)
    } else {
        frame.saturating_add(1).min(total_frames)
    }
}

pub(crate) fn grid_cross_complete(
    frame: u32,
    total_frames: u32,
    boundary_in_progress: bool,
) -> bool {
    frame >= total_frames && !boundary_in_progress
}

impl std::fmt::Display for BenchCameraPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Static => "static",
            Self::Pan => "pan",
            Self::Orbit => "orbit",
            Self::Dolly => "dolly",
            Self::GridCross => "grid-cross",
            Self::Cut => "cut",
            Self::GridSoak => "grid-soak",
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
            "grid-cross" => Ok(Self::GridCross),
            "cut" => Ok(Self::Cut),
            "grid-soak" => Ok(Self::GridSoak),
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

    /// Subject distance the unit tests orbit/dolly about. A plain
    /// stand-in for the forward-ray-cast measurement the engine makes at
    /// runtime — the paths must be correct for whatever distance they are
    /// handed, which is the property these tests check.
    const SUBJECT: f32 = 400.0;

    fn pose(path: BenchCameraPath, frame: u32) -> CameraPose {
        path.pose(frame, FRAMES, ORIGIN, FORWARD, SUBJECT)
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
            let short = path.pose(59, 60, ORIGIN, FORWARD, SUBJECT);
            let long = path.pose(299, 300, ORIGIN, FORWARD, SUBJECT);
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
        let target = ORIGIN + FORWARD * SUBJECT;
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

    /// The #2835 follow-up defect: `Orbit` and `Dolly` derived their radius
    /// from `origin.distance(Vec3::ZERO)`, so the path shape depended on where
    /// the *cell* happened to be authored relative to the world origin rather
    /// than on anything the camera could see.
    ///
    /// On FNV's `GSProspectorSaloonInterior` the camera spawns 3610 BU from
    /// the origin, which put the orbit target 3610 BU ahead of a camera in a
    /// small room and swung it out of the building: measured on `4de5e78e`,
    /// `gpu_main_render` collapsed from 11.526 ms under `pan` to 0.010 ms
    /// under `orbit` at an identical 1214 draws. Translating the whole scene
    /// must now leave the path's *shape* untouched.
    #[test]
    fn orbit_and_dolly_are_independent_of_distance_from_the_world_origin() {
        // Same local geometry, one placed at the origin and one 3610 BU away
        // — the real Prospector offset.
        let near = Vec3::new(0.0, 2.0, 10.0);
        let far = Vec3::new(536.0, 3560.0, -272.0);
        assert!(
            far.distance(Vec3::ZERO) > 3600.0,
            "fixture must be off-origin"
        );

        for path in [BenchCameraPath::Orbit, BenchCameraPath::Dolly] {
            for frame in [0, 20, FRAMES - 1] {
                let a = path.pose(frame, FRAMES, near, FORWARD, SUBJECT);
                let b = path.pose(frame, FRAMES, far, FORWARD, SUBJECT);
                // Displacement from each path's own origin must match.
                let da = a.position - near;
                let db = b.position - far;
                assert!(
                    da.distance(db) < 1e-2,
                    "{path} at frame {frame}: displacement differs with world \
                     placement ({da:?} vs {db:?}) — the radius is leaking the \
                     cell's offset from the world origin again"
                );
                assert!(
                    a.forward.distance(b.forward) < 1e-4,
                    "{path}: forward differs"
                );
            }
        }
    }

    /// The orbit radius must be the *supplied* subject distance, so a measured
    /// cast result actually reaches the path.
    #[test]
    fn orbit_radius_tracks_the_supplied_subject_distance() {
        for subject in [50.0_f32, 400.0, 5000.0] {
            let start = BenchCameraPath::Orbit.pose(0, FRAMES, ORIGIN, FORWARD, subject);
            let target = ORIGIN + FORWARD * subject;
            assert!(
                (start.position.distance(target) - subject).abs() < 1e-2,
                "subject {subject}: orbit radius is {}",
                start.position.distance(target)
            );
        }
    }

    /// A degenerate measurement must not produce a degenerate path: a miss,
    /// a zero, or a NaN resolves to the documented room-scale fallback rather
    /// than collapsing the orbit onto the camera.
    #[test]
    fn degenerate_subject_distance_falls_back_to_room_scale() {
        for bad in [0.0_f32, -10.0, f32::NAN, f32::INFINITY] {
            let p = BenchCameraPath::Orbit.pose(FRAMES - 1, FRAMES, ORIGIN, FORWARD, bad);
            let expected = BenchCameraPath::Orbit.pose(
                FRAMES - 1,
                FRAMES,
                ORIGIN,
                FORWARD,
                BenchCameraPath::FALLBACK_SUBJECT_DISTANCE_BU,
            );
            assert!(
                p.position.distance(expected.position) < 1e-3,
                "subject {bad} must resolve to the fallback"
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
        assert!(travelled > 0.0, "dolly did not advance");
        assert!(
            travelled < SUBJECT,
            "dolly overshot the subject: travelled {travelled} of {SUBJECT}"
        );
        assert!(
            (travelled - SUBJECT * BenchCameraPath::DOLLY_CLOSE_FRACTION).abs() < 1e-3,
            "dolly must close the documented fraction of the SUBJECT distance, \
             not of the camera's distance from the world origin (#2835 follow-up)"
        );
    }

    /// The streaming path is world-grid-relative, not view-relative: it must
    /// cross exactly three eastward cell boundaries even when the camera is
    /// looking elsewhere.
    #[test]
    fn grid_cross_traverses_three_complete_exterior_cells() {
        let origin = Vec3::new(EXTERIOR_CELL_UNITS * 0.37, 42.0, -500.0);
        let start = BenchCameraPath::GridCross.pose(0, FRAMES, origin, Vec3::X, SUBJECT);
        let end = BenchCameraPath::GridCross.pose(FRAMES - 1, FRAMES, origin, Vec3::X, SUBJECT);
        let settle = BenchCameraPath::GridCross.pose(
            (FRAMES as f32 * BenchCameraPath::GRID_CROSS_MOVE_FRACTION).ceil() as u32,
            FRAMES,
            origin,
            Vec3::X,
            SUBJECT,
        );
        assert_eq!(start.position, origin);
        assert_eq!(end.position.y, origin.y);
        assert_eq!(end.position.z, origin.z);
        assert!((end.position.x - origin.x - 3.0 * EXTERIOR_CELL_UNITS).abs() < 1e-3);
        assert_eq!(settle.position, end.position);
        assert_eq!(end.forward, Vec3::X);

        let start_grid_x = (start.position.x / EXTERIOR_CELL_UNITS).floor() as i32;
        let end_grid_x = (end.position.x / EXTERIOR_CELL_UNITS).floor() as i32;
        assert_eq!(end_grid_x - start_grid_x, 3);
    }

    #[test]
    fn grid_cross_logical_frame_pauses_until_boundary_settles() {
        assert_eq!(advance_grid_cross_frame(40, 100, true), 40);
        assert_eq!(advance_grid_cross_frame(40, 100, false), 41);
        assert_eq!(advance_grid_cross_frame(100, 100, false), 100);
        assert!(!grid_cross_complete(100, 100, true));
        assert!(grid_cross_complete(100, 100, false));
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
            let p = path.pose(30, FRAMES, ORIGIN, Vec3::ZERO, SUBJECT);
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
            let p = path.pose(0, 1, ORIGIN, FORWARD, SUBJECT);
            assert_eq!(p.position, ORIGIN);
            assert!(!path.is_cut_frame(0, 1));
        }
    }

    // ── grid-soak (EX-08 ownership soak, #2374) ─────────────────

    const SOAK_FRAMES: u32 = 900;

    #[test]
    fn grid_soak_starts_and_ends_at_the_origin() {
        // The gate compares the end state against a baseline taken at the same
        // position. If the path finished mid-traversal, the far cell would
        // still be resident and every run would report a false leak.
        let start = BenchCameraPath::GridSoak.pose(0, SOAK_FRAMES, ORIGIN, Vec3::X, SUBJECT);
        let end =
            BenchCameraPath::GridSoak.pose(SOAK_FRAMES - 1, SOAK_FRAMES, ORIGIN, Vec3::X, SUBJECT);
        assert_eq!(start.position, ORIGIN);
        assert!(
            (end.position - ORIGIN).length() < 1e-2,
            "soak ended at {:?}, not the origin",
            end.position
        );
    }

    #[test]
    fn grid_soak_reaches_a_full_cell_crossing_on_the_out_leg() {
        // Sample the whole run and take the extreme; the peak must clear one
        // cell or the traversal never crosses a boundary and the soak measures
        // nothing at all.
        let peak = (0..SOAK_FRAMES)
            .map(|f| {
                BenchCameraPath::GridSoak
                    .pose(f, SOAK_FRAMES, ORIGIN, Vec3::X, SUBJECT)
                    .position
                    .x
            })
            .fold(f32::MIN, f32::max);
        assert!(
            peak > EXTERIOR_CELL_UNITS,
            "soak peak {peak} did not clear one cell ({EXTERIOR_CELL_UNITS})"
        );
    }

    #[test]
    fn grid_soak_completes_exactly_the_configured_cycle_count() {
        let completions: Vec<u32> = (0..SOAK_FRAMES)
            .filter_map(|f| BenchCameraPath::GridSoak.soak_cycle_completed(f, SOAK_FRAMES))
            .collect();
        // Consecutive and 0-based: a skipped or repeated index would silently
        // drop a cycle from the growth series.
        let expected: Vec<u32> = (0..BenchCameraPath::GRID_SOAK_CYCLES).collect();
        assert_eq!(completions, expected);
    }

    #[test]
    fn grid_soak_records_enough_cycles_for_a_growth_verdict() {
        // Cycle 0 is consumed as the baseline, so the *recorded* count is one
        // less than the traversal count. It must still clear the growth
        // threshold or `evaluate()` can never return a MonotonicGrowth finding
        // and half the EX-08 gate is dead code.
        let recorded = BenchCameraPath::GRID_SOAK_CYCLES as usize - 1;
        assert!(
            recorded >= byroredux_core::ecs::resources::ownership::MIN_CYCLES_FOR_GROWTH,
            "{recorded} recorded cycles is below the growth threshold"
        );
    }

    #[test]
    fn soak_cycle_detection_is_inert_on_every_other_path() {
        for path in BenchCameraPath::ALL {
            if path == BenchCameraPath::GridSoak {
                continue;
            }
            for frame in 0..120 {
                assert_eq!(
                    path.soak_cycle_completed(frame, 120),
                    None,
                    "{path} reported a soak cycle"
                );
            }
        }
    }

    #[test]
    fn grid_soak_returns_to_origin_between_cycles() {
        // Each completion frame must actually be at the origin — the cycle
        // index and the geometry have to agree, or ownership gets sampled
        // mid-traversal with the far cell still loaded.
        for frame in 0..SOAK_FRAMES {
            if BenchCameraPath::GridSoak
                .soak_cycle_completed(frame, SOAK_FRAMES)
                .is_none()
            {
                continue;
            }
            let pose = BenchCameraPath::GridSoak.pose(frame, SOAK_FRAMES, ORIGIN, Vec3::X, SUBJECT);
            let drift = (pose.position - ORIGIN).length();
            assert!(
                drift < 0.06 * EXTERIOR_CELL_UNITS,
                "cycle boundary at frame {frame} sits {drift} from the origin"
            );
        }
    }

    #[test]
    fn grid_soak_parses_and_displays_round_trip() {
        assert_eq!(
            "grid-soak".parse::<BenchCameraPath>(),
            Ok(BenchCameraPath::GridSoak)
        );
        assert_eq!(BenchCameraPath::GridSoak.to_string(), "grid-soak");
        // `ALL` drives the CLI error text; a variant missing from it would be
        // unlisted and effectively undiscoverable.
        assert!(BenchCameraPath::ALL.contains(&BenchCameraPath::GridSoak));
    }
}
