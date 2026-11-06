//! One-off output and camera-override state carried by [`crate::App`] (#6696).
//!
//! The `--screenshot` request latch and deadline, `--linear-capture`, the
//! Mitsuba and material-dump exports, and the `--camera-pos` /
//! `--camera-forward` overrides. One `capture` field on `App` replaces nine
//! loose ones. Per-frame bench capture (`--bench-capture-every`) stays with
//! [`crate::bench_state::BenchState`]. `Default` is the construction contract:
//! nothing requested, no override.

/// One-off output and camera-override state. See the module docs.
#[derive(Default)]
pub(crate) struct CaptureState {
    /// When set, request a screenshot on the bench-exit frame and
    /// write the PNG to this path before quitting. Requires
    /// `bench.frames_target` to be set (otherwise there is no
    /// deterministic capture frame). See `--screenshot`.
    pub(crate) screenshot_path: Option<String>,
    /// `--linear-capture PATH [--linear-capture-frames N]`: on the
    /// bench-exit frame, average the next N frames of composite's
    /// scene-linear output (before bloom, exposure, grading and tone
    /// mapping) and write a 32-bit-float RGB OpenEXR to PATH (M-LIGHT
    /// RLT-REF-007, M51 PTR-OUT-001). The presented PNG of `--screenshot`
    /// is a separate artefact and unchanged.
    pub(crate) linear_capture: Option<crate::linear_capture::LinearCaptureCli>,
    /// `--export-mitsuba DIR` (M51 PTR-VAL-004): write the Cornell reference
    /// scene to a Mitsuba 3 directory after the first rendered frame.
    pub(crate) export_mitsuba_dir: Option<std::path::PathBuf>,
    /// `--export-mitsuba-frame F`: export after bench frame F is drawn
    /// rather than the first (M-LIGHT RLT-EXP-001: a mid-animation pose).
    pub(crate) export_mitsuba_frame: u32,
    /// `--material-dump PATH` (M-LIGHT A9, RLT-IBL-008): the loaded scene's
    /// material classes, written after the first rendered frame.
    pub(crate) material_dump_path: Option<std::path::PathBuf>,
    /// Optional override for the computed initial camera position —
    /// `--camera-pos x,y,z`. Applied during scene setup before the
    /// first frame. None = use the default auto-frame-scene placement.
    pub(crate) camera_pos_override: Option<(f32, f32, f32)>,
    /// Optional override for the initial camera forward vector —
    /// `--camera-forward x,y,z`. Will be normalized at scene setup.
    /// Requires `camera_pos_override` to have meaning.
    pub(crate) camera_forward_override: Option<(f32, f32, f32)>,
    /// Set once the bench-exit path fires the screenshot request.
    /// Prevents re-requesting on every frame while we pump the
    /// capture / encode pipeline.
    pub(crate) screenshot_requested: bool,
    /// Remaining frames to wait for the PNG result before giving up.
    /// Decremented each AboutToWait pass while the result slot is
    /// empty.
    pub(crate) screenshot_deadline_frames: u32,
}
