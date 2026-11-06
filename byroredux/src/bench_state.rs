//! Bench-window state carried by [`crate::App`].
//!
//! One bench run touches ~two dozen fields — the CLI contract, the per-frame
//! timing accumulators, the deterministic camera path and the frame capture.
//! They live here so `App` declares one `bench` field instead (#6693).
//! Every field is bench-only and zero/`None` outside a finite bench run, so
//! `Default` is the construction contract.

use std::time::Instant;

/// Bench-window state. See the module docs.
#[derive(Default)]
pub(crate) struct BenchState {
    /// Named benchmark timing/camera contract. Present for every finite
    /// `--bench-frames` run, including legacy invocations resolved onto a
    /// canonical mode in `boot::run`.
    pub(crate) mode: Option<crate::bench::BenchMode>,
    /// When `Some(N)`, run exactly N frames then print a `bench:` line
    /// to stdout and exit. See `--bench-frames` in main() and #366.
    pub(crate) frames_target: Option<u32>,
    /// When `true` and `frames_target` is set, the engine keeps
    /// running after the bench summary lands instead of exiting —
    /// gives `byro-dbg` a window to attach and drive console commands
    /// against the loaded scene. Set via `--bench-hold`. Surfaced by
    /// the FNV-D5 audit's coverage gap (`docs/audits/
    /// AUDIT_FNV_2026-05-08.md`).
    pub(crate) hold: bool,
    /// Set once the bench summary has been printed so the per-tick
    /// re-entry into the bench-exit branch under `--bench-hold` skips
    /// the print + screenshot path on every subsequent tick. Without
    /// this guard `--bench-hold` would dump the summary line on every
    /// `about_to_wait` and the screenshot path would re-fire forever.
    pub(crate) summary_printed: bool,
    /// Frames rendered since startup. Paired with `frames_target`
    /// to drive the automated benchmark exit, and — via `--bench-camera` —
    /// to index the deterministic camera path.
    pub(crate) frames_count: u32,
    /// Deterministic camera path driven during a `--bench-frames` run.
    /// `None` leaves the authored pose alone, which is the default and what
    /// every pre-existing bench does. See `bench_camera`.
    pub(crate) camera: Option<crate::bench_camera::BenchCameraPath>,
    /// Path length and freeze frame shared by the camera path and every
    /// `BenchMover` (`--bench-camera-length`, `--bench-camera-freeze-at`, `--bench-camera-hold-at`).
    pub(crate) path: crate::bench_camera::BenchPathClock,
    /// Camera pose the bench path starts from — captured on the first bench
    /// frame rather than read from the CLI, so a path composes with whatever
    /// the scene (or `--camera-pos`) authored.
    pub(crate) camera_origin: Option<crate::bench_camera::CameraPose>,
    /// Pose selected by `step_bench_camera` for the current frame. Character
    /// camera sync also runs in the scheduler, so the post-scheduler phase
    /// reapplies this exact pose before streaming and rendering.
    pub(crate) camera_applied_pose: Option<crate::bench_camera::CameraPose>,
    /// Distance in BU from the bench camera's seeded origin to the geometry it
    /// looks at, measured once by a forward ray cast on the first stepped
    /// frame with a populated collision world. `Orbit`'s radius and `Dolly`'s
    /// travel derive from it. `None` until measured — physics is still empty
    /// when `seed_bench_camera_origin` runs.
    pub(crate) camera_subject_distance: Option<f32>,
    /// Logical path frame for `grid-cross`. Unlike rendered-frame count this
    /// pauses while a boundary transaction is active, making each measured
    /// handoff independent of GPU frame rate and preventing the benchmark
    /// itself from superseding correct-but-slower streaming work.
    pub(crate) camera_path_frame: u32,
    /// Soak traversal awaiting an ownership sample (EX-08 / #2374).
    ///
    /// The camera reaching the origin is not by itself a settled moment: the
    /// logical clock pauses while a boundary transaction runs, so the frame a
    /// cycle completes on can land mid-apply. Sampling there reads a
    /// half-torn-down world and makes reachability counts (`meshes_in_use`)
    /// alternate between phases while actual residency (`meshes_live_slots`)
    /// stays flat — a false leak. The cycle index parks here until streaming
    /// reports no boundary in progress.
    pub(crate) pending_soak_cycle: Option<u32>,
    /// Wall-clock start of the bench window (set on first bench frame).
    /// Used to compute real elapsed time independent of the rolling stats
    /// window, which measures per-AboutToWait dt and can miss CPU phases.
    pub(crate) start: Option<Instant>,
    /// Accumulated nanoseconds spent in scheduler.run() during the bench.
    pub(crate) systems_ns: u64,
    /// Number of about_to_wait ticks recorded during the bench window
    /// (distinct from frames_count which counts render frames).
    pub(crate) systems_ticks: u64,
    /// Accumulated nanoseconds in build_render_data() alone.
    pub(crate) build_render_ns: u64,
    /// Accumulated nanoseconds in UI tick + render + texture upload.
    pub(crate) ui_ns: u64,
    /// Accumulated nanoseconds spent in draw_frame() alone.
    pub(crate) render_ns: u64,
    /// Per-phase draw_frame breakdown accumulated over the bench window.
    pub(crate) frame_timings: byroredux_renderer::FrameTimings,
    /// Whole `about_to_wait` CPU wall samples, one per rendered bench frame.
    /// Retained only for the explicit finite bench window so p50/p95/max can
    /// expose boundary stutter that an average FPS line hides.
    pub(crate) cpu_frame_ms: Vec<f64>,
    /// M-LIGHT RLT-PERF-005 — adaptive RT tier histogram over the bench
    /// window past the warm-up, and the last frame sampled.
    pub(crate) rt_tier_hist: [u32; 4],
    pub(crate) rt_tier_sampled_frame: u32,
    /// `--bench-capture-every N --bench-capture-dir DIR`: PNG of every Nth
    /// rendered frame of a finite bench run (see `bench_capture`).
    pub(crate) capture: Option<crate::bench_capture::FrameSequenceCapture>,
}

#[cfg(test)]
mod tests {
    /// #6693 — bench state has one home. A new `bench_*` field on `App` would
    /// quietly rebuild the 22-field sprawl this struct replaced.
    #[test]
    fn app_declares_no_bench_fields_of_its_own() {
        let main_src = include_str!("main.rs");
        let header = "\nstruct App {";
        let start = main_src.find(header).expect("`struct App` not found") + header.len();
        let body = &main_src[start..];
        let body = &body[..body.find("\n}\n").expect("`struct App` is never closed")];
        assert!(
            body.contains("\n    bench: crate::bench_state::BenchState,"),
            "App must hold its bench state in the `bench` field"
        );
        let strays: Vec<&str> = body
            .lines()
            .map(str::trim_start)
            .filter(|l| !l.starts_with("//") && l.starts_with("bench_"))
            .collect();
        assert!(
            strays.is_empty(),
            "bench fields belong on BenchState, not App: {strays:?}"
        );
    }
}
