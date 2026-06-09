//! ByroRedux — ECS-driven game loop with Vulkan rendering.

// Heap-allocation profiler (PERF-D2-NEW-03 / #1381). Behind the
// `dhat-heap` feature so default builds keep the system allocator and
// pay no override cost. When enabled, `dhat` becomes the global
// allocator and `main` starts a whole-run profiler that writes
// `dhat-heap.json` on exit.
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static DHAT_ALLOC: dhat::Alloc = dhat::Alloc;

mod anim_convert;
mod asset_provider;
mod cell_loader;
mod cli_args;
mod commands;
mod components;
mod cornell;
mod debug_load;
mod env_translate;
mod game_profiles;
mod helpers;
mod material_translate;
mod npc_spawn;
mod parsed_nif_cache;
mod render;
mod scene;
mod scene_import_cache;
mod sf_smoke;
mod streaming;
mod streaming_helpers;
mod systems;

use anyhow::Result;
use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::console::CommandRegistry;
use byroredux_core::ecs::{
    Access, ActiveCamera, Camera, DebugStats, DeltaTime, EngineConfig, MetricsSnapshot, Scheduler,
    ScratchTelemetry, SelectedRef, SkinCoverageStats, Stage, TotalTime, Transform, World,
};
use byroredux_core::string::StringPool;
use byroredux_platform::window::{self, WindowConfig};
use byroredux_renderer::vulkan::context::{DrawCommand, FrameInputs};
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::VulkanContext;
use byroredux_ui::UiManager;
use std::collections::HashMap;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::cli_args::{parse_string_arg, parse_vec3_arg};
use crate::commands::build_command_registry;
use crate::components::{CellRootIndex, FootstepConfig, InputState, NameIndex, SubtreeCache};
use crate::helpers::world_resource_set;
use crate::render::build_render_data;
use crate::streaming_helpers::{
    consume_streaming_payload, drain_streaming_state, SVGF_TAA_STREAMING_RECOVERY_FRAMES,
};
use crate::systems::{
    animate_lights_system, compute_underwater_params, footstep_system, log_stats_system,
    make_animation_system, make_billboard_system, make_transform_propagation_system,
    make_world_bound_propagation_system, metrics_sample_system, particle_system, spin_system,
    toggle_player_mode, weather_system, MetricsState,
};
use byroredux_core::ecs::SystemList;

/// Install a `tracing` subscriber for the cell-load span ladder
/// (#886 / INFRA-PERF-01). Default builds register a no-op subscriber
/// — `#[tracing::instrument]` macros expand to function-pointer-check
/// stubs that drop the span data, so there's no measurable overhead
/// on the hot path. With `--features tracing-tracy`, spans are piped
/// into the Tracy profiler instead, giving wall-clock visibility for
/// findings like #877 (BSA mutex contention), #879 (REFR mesh upload
/// fence-waits), #880 (NPC NIF re-parse), #881 (texture upload
/// budget), #882 (StringPool intern), #883 (multi-walk unload).
///
/// `env_logger` stays the canonical log path; tracing here is
/// strictly for span-based wall-clock profiling, not logging
/// duplication.
fn init_tracing() {
    // Default-build path: no fmt layer, no Tracy — `instrument` macros
    // become noop spans. Even the no-op `set_global_default` is
    // cheap; we skip it entirely so default builds are byte-identical
    // pre-this-commit at the subscriber level.
    #[cfg(feature = "tracing-tracy")]
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let registry = tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with(tracing_tracy::TracyLayer::default());
        registry.init();
        log::info!(
            "tracing-tracy: spans → Tracy profiler (run `tracy-capture -o run.tracy` \
             before launching to capture)"
        );
    }
}

fn main() -> Result<()> {
    // Whole-run heap profiler (PERF-D2-NEW-03 / #1381). Held for the
    // lifetime of `main`; on drop (process exit) `dhat` writes
    // `dhat-heap.json` to the CWD. No-op unless built with
    // `--features dhat-heap`.
    #[cfg(feature = "dhat-heap")]
    let _dhat_profiler = dhat::Profiler::new_heap();

    // Phase 20 — expand `--game <key>` into the full set of
    // `--esm` / `--bsa` / `--textures-bsa` / `--materials-ba2`
    // args BEFORE anything else reads argv. User-supplied flags
    // appear first in the resulting Vec and win
    // first-occurrence-wins for unique flags like `--esm`;
    // additive flags (`--bsa` / `--textures-bsa`) get both the
    // user's archives AND the profile's defaults.
    //
    // Phase 20.1 — seed the expanded args into the process-wide
    // `effective_args()` singleton so downstream readers
    // (scene.rs, nif_loader, debug_load, transition rebuilds)
    // see the post-expansion list instead of re-reading raw
    // `std::env::args()` and losing the synthesized flags.
    let args: Vec<String> = expand_game_profile_args(std::env::args().collect());
    crate::cli_args::set_effective_args(args.clone());
    let debug_mode = args.iter().any(|a| a == "--debug");

    // --bench-frames N: run N frames, emit a single `bench:` summary
    // line to stdout, then exit. Intended for CI perf tracking and for
    // reproducing the ROADMAP's sweetroll FPS claim without interactive
    // window-title reading. See #366.
    let bench_frames = args
        .iter()
        .position(|a| a == "--bench-frames")
        .and_then(|i| args.get(i + 1).and_then(|v| v.parse::<u32>().ok()));

    // --screenshot PATH: when set, request a screenshot on the bench
    // exit frame (requires --bench-frames) and write to PATH before
    // quitting. No-op without --bench-frames. Used for offline
    // rendering diagnostics / visual regression baselines.
    let screenshot_path = parse_string_arg(&args, "--screenshot");

    // --bench-hold: after `--bench-frames N` emits its summary, keep
    // the engine running (rendering, debug server reachable) instead
    // of exiting. The bench summary still prints exactly once, at the
    // target frame. Used by audit / triage workflows that need
    // `byro-dbg` to connect post-bench and run console commands like
    // `tex.missing` / `tex.loaded` against the loaded scene — pre-flag
    // the binary exited too quickly for a TCP client to attach. No-op
    // without `--bench-frames`. See FNV-D5 audit (`docs/audits/
    // AUDIT_FNV_2026-05-08.md` § Coverage gaps).
    let bench_hold = args.iter().any(|a| a == "--bench-hold");

    // --camera-pos x,y,z + --camera-forward x,y,z — override the
    // auto-computed initial camera pose. Useful for capturing specific
    // framing in bench mode without needing interactive WASD input.
    // Both args are `,`-separated floats. Pass one or both; missing
    // forward defaults to `-Z` toward the origin.
    let camera_pos = parse_vec3_arg(&args, "--camera-pos");
    let camera_forward = parse_vec3_arg(&args, "--camera-forward");

    // --rotation-mode 0..=3 — diagnostic switch for the REFR
    // Euler→Y-up conversion. See `cell_loader::euler_zup_to_quat_yup_refr`
    // doc for what each mode means. Used to triage the "large statics
    // misplaced + 90° rotated" symptom by screenshotting each candidate
    // on a known-good cell. Defaults to 0 (current shipping behavior).
    if let Some(idx) = args.iter().position(|a| a == "--rotation-mode") {
        if let Some(mode) = args.get(idx + 1).and_then(|v| v.parse::<u8>().ok()) {
            cell_loader::set_refr_rotation_mode_diag(mode.min(3));
            log::info!("--rotation-mode {} active", mode.min(3));
        }
    }

    // Set up logging. --debug forces debug level.
    if debug_mode {
        std::env::set_var(
            "RUST_LOG",
            std::env::var("RUST_LOG").unwrap_or("debug".into()),
        );
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    init_tracing();

    log::info!("ByroRedux starting");
    log::info!("{}", byroredux_cxx_bridge::ffi::native_hello());

    // --sf-smoke <CELL_EDID>: Starfield ESM resolve-rate smoke test.
    // Headless planning-phase deliverable that walks the ESM under the
    // current `GameKind` dispatch, picks one named interior cell, and
    // reports the % of REFRs whose base_form_id resolves to a known
    // StaticObject. Gate question for ROADMAP Milestone B (Starfield
    // interior renders). See #763 / SF-D6-04. Requires `--esm <PATH>`.
    // Logger is the global one initialised at line 152 above; the smoke
    // is a no-window, no-engine path that prints to stdout and exits.
    if let Some(idx) = args.iter().position(|a| a == "--sf-smoke") {
        let cell_edid = args
            .get(idx + 1)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("--sf-smoke requires a cell EDID argument"))?;
        let esm_path = parse_string_arg(&args, "--esm").ok_or_else(|| {
            anyhow::anyhow!("--sf-smoke requires --esm <PATH> to specify the ESM")
        })?;
        return sf_smoke::run(std::path::Path::new(&esm_path), &cell_edid);
    }

    // Headless --cmd mode: execute command and exit without creating a window.
    if let Some(cmd_idx) = args.iter().position(|a| a == "--cmd") {
        // #637 / FNV-D5-03 — the headless path builds an empty World
        // with only `DebugStats::default()` and the command registry, so
        // every cell-aware command (`stats`, `entities`, `tex.missing`,
        // `light.dump`, etc.) returns zeros regardless of which
        // `--esm` / `--bsa` flags were also passed. Reject the
        // combination with a clear error rather than silently producing
        // misleading output that a CI baseline check would otherwise
        // accept as valid.
        //
        // Audit Option (b) — wiring the cell loader into this path so
        // cell-aware stats work without a window — is the long-term
        // unblock for CI regression checks, but is substantially more
        // scope than this LOW-severity bundle covers. Filed as a
        // follow-up if/when CI starts asserting on baselines.
        let conflicting: Vec<&str> = [
            "--esm",
            "--bsa",
            "--textures-bsa",
            "--master",
            "--grid",
            "--cell",
            "--wrld",
            "--radius",
            "--mesh",
            "--tree",
            "--kf",
        ]
        .iter()
        .copied()
        .filter(|flag| args.iter().any(|a| a == flag))
        .collect();
        if !conflicting.is_empty() {
            eprintln!(
                "error: --cmd is headless and cannot resolve a cell-aware scene. \
                 Conflicting flag(s) passed: {}\n\
                 Use a live engine session (omit --cmd, then attach with byro-dbg) \
                 for cell-aware queries. See #637 / FNV-D5-03.",
                conflicting.join(", "),
            );
            return Err(anyhow::anyhow!(
                "--cmd cannot coexist with cell/asset-loading flags"
            ));
        }
        let input = args.get(cmd_idx + 1).map(|s| s.as_str()).unwrap_or("help");
        let mut world = World::new();
        world.insert_resource(DebugStats::default());
        world.insert_resource(EngineConfig {
            debug_logging: true,
            ..Default::default()
        });
        let registry = build_command_registry();
        world.insert_resource(SystemList(Vec::new()));
        world.insert_resource(registry);
        let reg = world.resource::<CommandRegistry>();
        let output = reg.execute(&world, input);
        drop(reg);
        for line in &output.lines {
            println!("{}", line);
        }
        return Ok(());
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(debug_mode, &args);
    app.bench_frames_target = bench_frames;
    app.bench_hold = bench_hold;
    app.screenshot_path = screenshot_path;
    app.camera_pos_override = camera_pos;
    app.camera_forward_override = camera_forward;
    event_loop.run_app(&mut app)?;

    Ok(())
}

/// Derive `(deep_color.xyz, depth_below_surface)` from the active
/// camera's [`SubmersionState`]. Returns `[0, 0, 0, 0]` when the
/// camera is above water or no submersion data is available — the
/// composite shader treats `w == 0` as "underwater FX disabled".
struct App {
    window: Option<Window>,
    renderer: Option<VulkanContext>,
    world: World,
    scheduler: Scheduler,
    last_frame: Instant,
    ui_manager: Option<UiManager>,
    /// Texture handle for the UI overlay (registered in the texture registry).
    ui_texture_handle: Option<u32>,
    /// Reusable per-frame draw command buffer (cleared each frame, allocation retained).
    draw_commands: Vec<DrawCommand>,
    /// Reusable per-frame water draw command buffer. Built alongside
    /// `draw_commands` from `WaterPlane` ECS entities; routed through
    /// the renderer's dedicated water pipeline.
    water_commands: Vec<byroredux_renderer::vulkan::water::WaterDrawCommand>,
    /// Reusable per-frame light buffer (cleared each frame, allocation retained).
    gpu_lights: Vec<byroredux_renderer::GpuLight>,
    /// M29.5/M29.6 — reusable per-frame bone-world matrices (column-
    /// major mat4 entries; slot 0 always identity). Sparse layout
    /// indexed by `skin_slot_id × MAX_BONES_PER_MESH`; the renderer
    /// uploads it each frame via `upload_bone_worlds`. The GPU
    /// `skin_palette.comp` does the per-slot `bone_world ×
    /// bind_inverses` multiply and writes the palette SSBO consumed
    /// by `skin_vertices.comp` + `triangle.vert` inline-skinning.
    bone_world: Vec<[[f32; 4]; 4]>,
    /// M29.6 — per-entity persistent slot pool for the bone-palette
    /// SSBOs. Stable slot IDs across frames so the persistent
    /// `bind_inverses` SSBO (uploaded once at first-sight) stays in
    /// lockstep with the per-frame `bone_world` writes.
    skin_slot_pool: byroredux_core::ecs::resources::SkinSlotPool,
    /// Reusable per-frame entity → bone-offset map. Populated by the
    /// skinned-mesh pass in `build_render_data` and read during draw
    /// command emission. Retained across frames so the HashMap's bucket
    /// allocation persists — see #253.
    skin_offsets: HashMap<byroredux_core::ecs::EntityId, u32>,
    /// R1 — per-frame deduplicated material table. Cleared at the
    /// top of `build_render_data`, populated as DrawCommands are
    /// emitted, uploaded as an SSBO before draw. Phase 2 builds it
    /// in lockstep with the legacy per-instance fields; Phases 3–6
    /// migrate shader reads onto it and drop the redundant copies.
    /// Retained on `App` so the `Vec`/`HashMap` allocations persist
    /// across frames — same scratch-buffer pattern as the others
    /// above (#243 / #253 / #509).
    material_table: byroredux_renderer::MaterialTable,
    /// When `Some(N)`, run exactly N frames then print a `bench:` line
    /// to stdout and exit. See `--bench-frames` in main() and #366.
    bench_frames_target: Option<u32>,
    /// When `true` and `bench_frames_target` is set, the engine keeps
    /// running after the bench summary lands instead of exiting —
    /// gives `byro-dbg` a window to attach and drive console commands
    /// against the loaded scene. Set via `--bench-hold`. Surfaced by
    /// the FNV-D5 audit's coverage gap (`docs/audits/
    /// AUDIT_FNV_2026-05-08.md`).
    bench_hold: bool,
    /// Set once the bench summary has been printed so the per-tick
    /// re-entry into the bench-exit branch under `--bench-hold` skips
    /// the print + screenshot path on every subsequent tick. Without
    /// this guard `--bench-hold` would dump the summary line on every
    /// `about_to_wait` and the screenshot path would re-fire forever.
    bench_summary_printed: bool,
    /// Frames rendered since startup. Paired with `bench_frames_target`
    /// to drive the automated benchmark exit.
    bench_frames_count: u32,
    /// Wall-clock start of the bench window (set on first bench frame).
    /// Used to compute real elapsed time independent of the rolling stats
    /// window, which measures per-AboutToWait dt and can miss CPU phases.
    bench_start: Option<Instant>,
    /// Accumulated nanoseconds spent in scheduler.run() during the bench.
    bench_systems_ns: u64,
    /// Number of about_to_wait ticks recorded during the bench window
    /// (distinct from bench_frames_count which counts render frames).
    bench_systems_ticks: u64,
    /// Accumulated nanoseconds in build_render_data() alone.
    bench_build_render_ns: u64,
    /// Accumulated nanoseconds in UI tick + render + texture upload.
    bench_ui_ns: u64,
    /// Accumulated nanoseconds spent in draw_frame() alone.
    bench_render_ns: u64,
    /// Per-phase draw_frame breakdown accumulated over the bench window.
    bench_frame_timings: byroredux_renderer::FrameTimings,
    /// When set, request a screenshot on the bench-exit frame and
    /// write the PNG to this path before quitting. Requires
    /// `bench_frames_target` to be set (otherwise there is no
    /// deterministic capture frame). See `--screenshot`.
    screenshot_path: Option<String>,
    /// Optional override for the computed initial camera position —
    /// `--camera-pos x,y,z`. Applied during scene setup before the
    /// first frame. None = use the default auto-frame-scene placement.
    camera_pos_override: Option<(f32, f32, f32)>,
    /// Optional override for the initial camera forward vector —
    /// `--camera-forward x,y,z`. Will be normalized at scene setup.
    /// Requires `camera_pos_override` to have meaning.
    camera_forward_override: Option<(f32, f32, f32)>,
    /// Set once the bench-exit path fires the screenshot request.
    /// Prevents re-requesting on every frame while we pump the
    /// capture / encode pipeline.
    screenshot_requested: bool,
    /// Remaining frames to wait for the PNG result before giving up.
    /// Decremented each AboutToWait pass while the result slot is
    /// empty.
    screenshot_deadline_frames: u32,
    /// World cell streaming state (M40 Phase 1a). `None` outside
    /// `--esm + --grid` exterior mode. When `Some`, every
    /// `about_to_wait` tick reads `ActiveCamera` translation, diffs
    /// the loaded cell set against the player's current grid coords,
    /// and synchronously loads / unloads the deltas via the per-cell
    /// loader.
    streaming: Option<streaming::WorldStreamingState>,
    /// Debug server lifecycle owner (#855 / C6-NEW-02). Holding the
    /// handle keeps the TCP listener thread alive; the natural App::Drop
    /// fires the handle's Drop, which sets the shutdown flag and joins
    /// the listener cleanly instead of detaching it. Read-side never
    /// touches it — the field exists purely for its Drop side-effect.
    #[cfg(feature = "debug-server")]
    #[allow(dead_code)]
    debug_server: Option<byroredux_debug_server::DebugServerHandle>,
    /// Debug-UI overlay state — egui context + winit input
    /// translator. `None` until `resumed` constructs the window;
    /// initialised once at boot and outlives the rest of the App.
    /// Forwarded every `WindowEvent` so egui can grab clicks /
    /// keypresses when the overlay is visible.
    debug_ui: Option<byroredux_debug_ui::DebugUiState>,
    /// Latched flag: the Entities panel asked us to rebuild its
    /// list this frame. Cleared at the start of every frame; set
    /// true by Phase 4b's `PanelOutputs::refresh_entities`. Held
    /// here (not in `DebugUiState`) because the snapshot is built
    /// from `&self.world`, which `DebugUiState::run`'s closure
    /// can't reach.
    debug_ui_refresh_entities: bool,
    /// Phase 9 — timestamp at the END of the previous
    /// RedrawRequested handler. Subtracting from `Instant::now()`
    /// at the START of the next RedrawRequested yields
    /// "between_frames_ms" in `CpuFrameTimings`. `None` until
    /// the first frame closes; the per-frame writer uses 0.0 on
    /// the first frame so the panel doesn't show garbage.
    last_redraw_end: Option<Instant>,
}

impl App {
    fn new(debug_mode: bool, args: &[String]) -> Self {
        let mut world = World::new();

        // Register built-in resources.
        world.insert_resource(DeltaTime(0.0));
        world.insert_resource(TotalTime(0.0));
        world.insert_resource(EngineConfig {
            debug_logging: debug_mode || cfg!(debug_assertions),
            ..Default::default()
        });
        world.insert_resource(DebugStats::default());
        world.insert_resource(ScratchTelemetry::default());
        world.insert_resource(SkinCoverageStats::default());
        // REND-#1451 — live attenuation tuning, read into the renderer
        // each frame and mutated by the `light.atten` console command.
        // Seed from the config `[defaults]` so a benched knee persists
        // across runs.
        let light_defaults = crate::game_profiles::load_launch_defaults();
        let mut light_tuning = crate::components::LightTuning::default();
        if let Some(knee) = light_defaults.light_atten_knee {
            light_tuning.knee_frac = knee.clamp(0.05, 1.0);
        }
        if let Some(legacy) = light_defaults.light_atten_legacy {
            light_tuning.legacy = legacy;
        }
        world.insert_resource(light_tuning);
        // CPU-side per-frame timings — fence_wait / submit_present /
        // etc. Filled by the binary's RedrawRequested handler after
        // each `draw_frame` from the renderer's `FrameTimings`
        // struct. Surfaces the "GPU stall hidden from per-pass
        // timestamps" diagnostic the debug UI's Metrics panel
        // exposed at Phase 7.
        world.insert_resource(byroredux_core::ecs::CpuFrameTimings::default());
        // Per-system wall-time list (Phase 11). Filled by
        // `Scheduler::run` at the end of each invocation, sorted
        // desc. The egui Metrics panel renders the top entries
        // so the operator can see which ECS system dominates
        // `atw_scheduler_ms`.
        world.insert_resource(byroredux_core::ecs::SchedulerSystemTimings::default());
        // Debug-UI sampler state + the aggregated snapshot. Snapshot is
        // empty until `metrics_sample_system` fires its first tick
        // (~500 ms in), at which point CPU / RAM / VRAM / GPU pass
        // times are filled and refreshed at 2 Hz.
        world.insert_resource(MetricsState::default());
        world.insert_resource(MetricsSnapshot::default());
        // Debug-UI load queue. Always present so the debug-server's
        // `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell`
        // handlers can push into it via `try_resource_mut` without
        // structurally inserting. Drained by `App::step_debug_loads`
        // between frames where `&mut World + &mut VulkanContext` are
        // both held.
        world.insert_resource(byroredux_core::ecs::PendingDebugLoadSlot::default());
        // Phase 5 — game profile registry. Loads
        // `assets/debug_profiles.toml` (engine-shipped defaults)
        // plus `~/.byroredux/profiles.toml` (per-user override).
        // Both files missing = empty registry, never an error.
        world.insert_resource(crate::game_profiles::load_default());
        world.insert_resource(SelectedRef::default());
        world.insert_resource(InputState::default());
        world.insert_resource(StringPool::new());
        // #1212 / D1-NEW-01 — FormIdPool is the intern table backing
        // `FormIdComponent` and `World::find_by_form_id`. Every
        // cell-loaded REFR's placement form-id interns through this
        // pool at spawn time so console (`prid <fid>`), debug-server,
        // and future Papyrus `ObjectReference` lookups all resolve.
        world.insert_resource(byroredux_core::form_id::FormIdPool::new());
        world.insert_resource(AnimationClipRegistry::new());
        world.insert_resource(NameIndex::new());
        world.insert_resource(SubtreeCache::new());
        world.insert_resource(CellRootIndex::new());
        world.insert_resource(byroredux_physics::PhysicsWorld::new());
        // M28.5 follow-up — engine-wide contact / KCC tunables. Owned
        // as a resource so a single edit propagates through every
        // collider spawn path (Path A NIF imports, character kinematic
        // capsule) and the KCC offset stays in lockstep. Defaults match
        // the pre-unification inline values.
        world.insert_resource(byroredux_physics::ContactConfig::default());
        // M44 Phase 1 — audio world. Init failure (no audio device,
        // CI, headless server) leaves the inner `AudioManager` as
        // `None` and every subsequent audio operation no-ops. Boot
        // never fails on a missing audio device.
        world.insert_resource(byroredux_audio::AudioWorld::new());
        // M44 Phase 3.5 — footstep config. `default_sound` is None
        // until/unless the cell loader (or a future asset-provider
        // hook) decodes a BSA-archived sound and stores it here.
        // Defaults are safe: `None` makes `footstep_system` no-op.
        world.insert_resource(FootstepConfig::default());
        // M44 Phase 3.5 / #932 — `footstep_system` reuses this Vec<Vec3>
        // scratch across frames instead of allocating a fresh one each
        // tick. Preallocated to capacity 32 to cover typical 5-10 NPC
        // walking case without re-growing.
        world.insert_resource(crate::components::FootstepScratch::default());
        // M44 Phase 3.5 — opportunistic footstep BSA load. When the
        // user passed `--sounds-bsa <path>`, decode the canonical
        // dirt-walk WAV and stash the `Arc<StaticSoundData>` in
        // `FootstepConfig.default_sound`. Silently no-op when the
        // flag is absent or the archive isn't openable.
        crate::asset_provider::try_load_default_footstep(&mut world, args);
        // Process-lifetime cache of parsed-and-imported NIF scenes.
        // Persists across cell transitions so repeat visits don't re-
        // parse every clutter mesh. See #381.
        world.insert_resource(crate::cell_loader::NifImportRegistry::new());

        // #880 / CELL-PERF-02 — companion cache for the hierarchical
        // scene-import path used by NPC spawn (`load_nif_bytes_with_
        // skeleton`). Pre-fix every NPC re-parsed the same skeleton +
        // body + hand NIFs from BSA bytes (~280 redundant parses /
        // Megaton load). Different output shape from `CachedNifImport`
        // — that one is the flat-import variant for REFR placements;
        // this one carries the hierarchical `ImportedScene` with its
        // `nodes: Vec<ImportedNode>` so the bone hierarchy spawns
        // correctly.
        world.insert_resource(crate::scene_import_cache::SceneImportCache::new());

        // Pre-register component storages that the physics sync system
        // queries on the first frame (before anything has been inserted).
        world.register::<byroredux_physics::RapierHandles>();
        // M44 Phase 3.5: pre-register footstep emitter storage so
        // `footstep_system`'s `query_mut::<FootstepEmitter>` returns
        // `Some` even before the first emitter is inserted (e.g. on
        // startup with no scene loaded).
        world.register::<crate::components::FootstepEmitter>();

        // Register scripting component storages.
        byroredux_scripting::register(&mut world);

        // M47.0 Phase 2 — build the script registry mapping SCPT
        // `editor_id` → spawner. Populated here by every demo module
        // exposing a `register_spawners` entry point; the cell
        // loader's per-REFR walk (Phase 3) will look up each REFR's
        // base record's `script_form_id` → SCPT → editor_id against
        // this registry. Inserted as a resource so it survives the
        // whole engine lifetime and stays modifiable by downstream
        // crates that ship additional script translations.
        let mut script_registry = byroredux_scripting::ScriptRegistry::new();
        byroredux_scripting::papyrus_demo::register_spawners(&mut script_registry);
        log::info!(
            "ScriptRegistry initialised with {} editor_id mappings",
            script_registry.len()
        );
        world.insert_resource(script_registry);

        // Build the system schedule — stages run sequentially, systems
        // within each stage run in parallel via rayon. All parallel
        // systems declare their access via `add_to_with_access`
        // (M27 migration complete); `undeclared_parallel_count()` is
        // asserted to be 0 after construction below. Exclusive systems
        // (add_exclusive) run serially after each stage's parallel
        // batch and do not participate in the conflict analyzer.
        let mut scheduler = Scheduler::new();
        // M27 Phase 3 — `fly_camera_system` and `character_controller_system`
        // are runtime-mutually-exclusive (each early-returns on the
        // wrong `PlayerMode`), so the scheduler's access analyzer
        // paired them up and surfaced a Transform + PhysicsWorld
        // WriteWrite conflict that's structurally impossible at
        // runtime. `player_controller_system` dispatches to one of
        // the two inner systems per frame; declared accesses are the
        // union of both inner systems' accesses.
        scheduler.add_to_with_access(
            Stage::Early,
            crate::systems::player_controller_system,
            Access::new()
                .reads_resource::<crate::systems::PlayerMode>()
                .reads_resource::<crate::systems::PlayerEntity>()
                .reads_resource::<ActiveCamera>()
                .reads_resource::<InputState>()
                .reads_resource::<byroredux_physics::PhysicsWorld>()
                .writes_resource::<byroredux_physics::PhysicsWorld>()
                .reads::<byroredux_physics::CharacterController>()
                .writes::<byroredux_physics::CharacterController>()
                .reads::<byroredux_physics::RapierHandles>()
                .reads::<Transform>()
                .writes::<Transform>(),
        );
        scheduler.add_to_with_access(
            Stage::Early,
            weather_system,
            Access::new()
                .reads_resource::<crate::components::WeatherDataRes>()
                .writes_resource::<crate::components::WeatherDataRes>()
                .reads_resource::<crate::components::WeatherTransitionRes>()
                .writes_resource::<crate::components::WeatherTransitionRes>()
                .writes_resource::<crate::components::GameTimeRes>()
                .reads_resource::<crate::components::CellLightingRes>()
                .writes_resource::<crate::components::CellLightingRes>()
                .writes_resource::<crate::components::SkyParamsRes>()
                .writes_resource::<crate::components::CloudSimState>(),
        );
        scheduler.add_to_with_access(
            Stage::Early,
            byroredux_scripting::timer_tick_system,
            Access::new()
                .writes::<byroredux_scripting::ScriptTimer>()
                .writes::<byroredux_scripting::TimerExpired>(),
        );
        // M47.0 Phase 1 — R5 papyrus_demo dispatchers. These are
        // event-driven (early-return when no ActivateEvent /
        // OnUpdateEvent / RecurringUpdate is present), so they
        // contribute zero work to frames without scripted activity.
        // Registered as exclusive in Update so they run serially
        // after the parallel batch — keeps M27's zero-conflict
        // report intact without forcing each demo to declare its
        // (component-heavy) access surface. Declared access lands
        // when the demos move to Stage::Script in a follow-up; for
        // now they ride the same "trivial gameplay system =
        // exclusive" lane as spin_system / footstep_system /
        // particle_system. See docs/engine/m47-0-design.md.
        //
        // Signature note: the 6 World-only demos predate engine
        // wiring (R5 was a prototype against unit tests that called
        // them directly). The blanket `impl<F: FnMut(&World, f32)>
        // System for F` requires the `dt` arg, so we adapt via inline
        // closures that drop the unused parameter. Renaming the demo
        // signatures themselves would break ~30 unit-test call sites
        // in papyrus_demo/tests.rs that intentionally call without
        // dt; closures are cheaper.
        // F7 (2026-05-27) — wrap each `&World`-only papyrus-demo
        // system as a local `fn` item so it gets a unique
        // `std::any::type_name`. The previous closures all shared the
        // auto-generated name `byroredux::App::new::{{closure}}`,
        // tripping the scheduler's `has_system(name)` duplicate-name
        // probe and emitting 5+ "duplicate exclusive system" warnings
        // per launch (cosmetic — the systems still all got added).
        // Local fn items each have their own unique type and unique
        // type-name path, so the duplicate check passes naturally.
        fn rumble_on_activate_dispatch(world: &World, _dt: f32) {
            byroredux_scripting::papyrus_demo::rumble_on_activate_system(world)
        }
        fn quest_advance_on_activate_dispatch(world: &World, _dt: f32) {
            byroredux_scripting::papyrus_demo::quest_advance::quest_advance_on_activate_system(
                world,
            )
        }
        fn dlc2_ttr4a_on_init_dispatch(world: &World, _dt: f32) {
            byroredux_scripting::papyrus_demo::dlc2_ttr4a::dlc2_ttr4a_on_init_system(world)
        }
        fn dlc2_ttr4a_on_update_dispatch(world: &World, _dt: f32) {
            byroredux_scripting::papyrus_demo::dlc2_ttr4a::dlc2_ttr4a_on_update_system(world)
        }
        fn mg07_on_load_dispatch(world: &World, _dt: f32) {
            byroredux_scripting::papyrus_demo::mg07_door::mg07_on_load_system(world)
        }
        fn mg07_on_activate_dispatch(world: &World, _dt: f32) {
            byroredux_scripting::papyrus_demo::mg07_door::mg07_on_activate_system(world)
        }
        scheduler.add_exclusive(Stage::Update, rumble_on_activate_dispatch);
        scheduler.add_exclusive(
            Stage::Update,
            byroredux_scripting::papyrus_demo::rumble_tick_system,
        );
        scheduler.add_exclusive(Stage::Update, quest_advance_on_activate_dispatch);
        scheduler.add_exclusive(Stage::Update, dlc2_ttr4a_on_init_dispatch);
        scheduler.add_exclusive(Stage::Update, dlc2_ttr4a_on_update_dispatch);
        scheduler.add_exclusive(Stage::Update, mg07_on_load_dispatch);
        scheduler.add_exclusive(Stage::Update, mg07_on_activate_dispatch);
        scheduler.add_exclusive(
            Stage::Update,
            byroredux_scripting::papyrus_demo::mg07_door::mg07_tick_system,
        );
        scheduler.add_to_with_access(
            Stage::Update,
            make_animation_system(),
            // M27 — animation_system writes the full set of animated-
            // channel storages (every channel a clip may target). The
            // declaration is the UNION across all paths; individual
            // frames touch a subset depending on which clips are
            // playing. See `byroredux/src/systems/animation.rs:305`.
            Access::new()
                .reads_resource::<byroredux_core::animation::AnimationClipRegistry>()
                .writes_resource::<byroredux_core::animation::AnimationClipRegistry>()
                .reads_resource::<crate::components::SubtreeCache>()
                .writes_resource::<crate::components::SubtreeCache>()
                .reads_resource::<crate::components::NameIndex>()
                .writes_resource::<crate::components::NameIndex>()
                .writes_resource::<byroredux_core::string::StringPool>()
                .reads::<byroredux_core::ecs::Name>()
                .writes::<Transform>()
                .writes::<byroredux_core::animation::RootMotionDelta>()
                .writes::<byroredux_core::ecs::AnimatedVisibility>()
                .writes::<byroredux_core::ecs::AnimatedDiffuseColor>()
                .writes::<byroredux_core::ecs::AnimatedEmissiveColor>()
                .writes::<byroredux_core::ecs::AnimatedAlpha>()
                .writes::<byroredux_core::ecs::AnimatedUvTransform>()
                .writes::<byroredux_core::ecs::AnimatedShaderFloat>()
                .writes::<byroredux_core::ecs::AnimatedMorphWeights>()
                .writes::<byroredux_core::ecs::LightSource>()
                .writes::<byroredux_core::animation::AnimationPlayer>()
                .writes::<byroredux_scripting::events::AnimationTextKeyEvents>()
                .writes::<byroredux_core::animation::AnimationStack>(),
        );
        // M27 Phase 3 — `spin_system` writes Transform on entities
        // tagged with `Spinning` (the demo cube). `animation_system`
        // also writes Transform on its own (disjoint) entity set.
        // They never touch the same entity, but the analyzer can't see
        // that — they pair as a WriteWrite Transform conflict. Moving
        // `spin_system` to exclusive sequences it after the Update
        // parallel batch and removes the conflict from the report
        // without changing observable behaviour. Cost: ~µs of lost
        // parallelism on the demo cube; negligible.
        scheduler.add_exclusive(Stage::Update, spin_system);
        // Phase 17 — procedural light flicker. Writes
        // LightSource.intensity + Transform.translation on entities
        // with a LightFlicker companion. Exclusive in Update so it
        // sequences AFTER the parallel batch (no Transform conflict
        // with animation_system / spin_system) but BEFORE
        // PostUpdate's transform propagation reads the result.
        scheduler.add_exclusive(Stage::Update, animate_lights_system);
        scheduler.add_to_with_access(
            Stage::PostUpdate,
            make_transform_propagation_system(),
            Access::new()
                .reads::<byroredux_core::ecs::Parent>()
                .reads::<byroredux_core::ecs::Children>()
                // WRITE (was read): the system drains the per-entity
                // Transform change-tracking dirty set, which needs &mut on
                // the storage. Local transforms are still only read.
                .writes::<Transform>()
                .writes::<byroredux_core::ecs::GlobalTransform>(),
        );
        // M44 Phase 3.5: footstep dispatch. Reads `GlobalTransform`
        // for the world-space spawn position, so it MUST run after
        // `make_transform_propagation_system()` — otherwise the
        // footstep lands at last-frame's pose. Pre-#848 this was
        // registered in `Stage::Update`, ahead of propagation; the
        // commit comment claimed "~3 cm of motion" stale but that
        // underestimated by ~100× for fly-cam boost (~3 game units
        // / frame at 60 FPS, audible spatial-pan offset on a
        // ~50-200-unit interior cell). Registered as exclusive so
        // it sequences AFTER the PostUpdate parallel batch — same
        // pattern as `particle_system` / `billboard_system` /
        // `world_bound_propagation_system` below. See #848.
        scheduler.add_exclusive(Stage::PostUpdate, footstep_system);
        // Particle simulation runs after transform propagation so emitter
        // entities have their final world-space spawn origin (#401).
        scheduler.add_exclusive(Stage::PostUpdate, particle_system);
        // PostUpdate ordering contract (#1375 invariant pin):
        //   1. transform_propagation — BFS GlobalTransform composition
        //   2. make_billboard_system  — overwrites billboard GT rotations
        //                              (camera-motion gated; see #1374)
        //   3. make_world_bound_propagation_system — drains GT dirty set,
        //      folds WorldBounds from the final per-frame transforms
        //
        // INVARIANT: no Stage::Late system may write GlobalTransform on
        // a LocalBound-bearing entity. If it does, that entity's
        // WorldBound will silently lag one frame because bound
        // propagation's GT drain fires before the Late write. Today
        // camera_follow_system + audio emitters write GT in Late but
        // carry no LocalBound — the lag is benign. Any future Late
        // system that writes GT on a bounded entity must either be
        // promoted to PostUpdate (before bounds) or accept one-frame
        // stale WorldBounds for that entity.
        scheduler.add_exclusive(Stage::PostUpdate, make_billboard_system());
        // Bound propagation runs last in PostUpdate so it sees final
        // world transforms (including billboard rotations). See #217.
        scheduler.add_exclusive(Stage::PostUpdate, make_world_bound_propagation_system());
        // Submersion detection runs in PostUpdate after bound
        // propagation so the camera's GlobalTransform is already
        // current for the frame. Reads `WaterPlane`/`WaterVolume` +
        // `GlobalTransform`, writes `SubmersionState` on the active
        // camera entity. Downstream consumers (audio low-pass send,
        // underwater composite tint) read the result later in the
        // frame.
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::submersion_system);
        scheduler.add_to_with_access(
            Stage::Physics,
            byroredux_physics::physics_sync_system,
            Access::new()
                .reads_resource::<byroredux_physics::PhysicsWorld>()
                .writes_resource::<byroredux_physics::PhysicsWorld>()
                .reads::<byroredux_core::ecs::components::CollisionShape>()
                .reads::<byroredux_core::ecs::components::RigidBodyData>()
                .reads::<byroredux_core::ecs::GlobalTransform>()
                .reads::<byroredux_physics::RapierHandles>()
                .writes::<byroredux_physics::RapierHandles>()
                .writes::<Transform>(),
        );
        // M28.5 — camera follow runs in Stage::Late, AFTER
        // `physics_sync_system` has settled the kinematic body's
        // post-step pose. Must run BEFORE `audio_system` /
        // `submersion_system` (both read camera GlobalTransform).
        // The character system writes both Transform and
        // GlobalTransform on the camera to bypass the missing
        // late-stage propagation pass.
        scheduler.add_to_with_access(
            Stage::Late,
            crate::systems::camera_follow_system,
            Access::new()
                .reads_resource::<crate::systems::PlayerEntity>()
                .reads_resource::<ActiveCamera>()
                .reads_resource::<InputState>()
                .reads::<byroredux_physics::CharacterController>()
                .reads::<byroredux_core::ecs::GlobalTransform>()
                .writes::<byroredux_core::ecs::GlobalTransform>()
                .reads::<Transform>()
                .writes::<Transform>(),
        );
        // M44 Phase 6 — cell-acoustics → reverb send (#846). Runs
        // before `audio_system` so any new spatial track constructed
        // this frame picks up the right send level. Already-playing
        // sounds keep their construction-time send (kira contract);
        // long-running ambients across interior/exterior transitions
        // are tracked separately in AUD-D5-NEW-06.
        scheduler.add_to_with_access(
            Stage::Late,
            crate::systems::reverb_zone_system,
            Access::new()
                .reads_resource::<crate::components::CellLightingRes>()
                .writes_resource::<byroredux_audio::AudioWorld>(),
        );
        // M44 Phase 1 — audio update runs in Stage::Late so it sees
        // final world transforms after propagation. The Phase 1 body
        // is a stub (see byroredux_audio::audio_system); future
        // phases (one-shot dispatch, listener pose sync, looping
        // emitter lifecycle) flesh it out without touching the
        // schedule wiring.
        //
        // M27 Phase 3 — registered as **exclusive** so it sequences
        // after the Late parallel batch. The ordering comment at
        // line 650-656 above ("MUST run BEFORE audio_system" /
        // "Must run BEFORE audio_system") encodes a real
        // dependency that the parallel batch can't guarantee on its
        // own; exclusive sequencing makes the dependency structural.
        // Side effect: removes two analyzer-visible conflicts
        // (camera_follow ↔ audio on GlobalTransform; reverb_zone ↔
        // audio on AudioWorld) — exclusive systems aren't paired
        // against anything in the access report.
        scheduler.add_exclusive(Stage::Late, byroredux_audio::audio_system);
        scheduler.add_to_with_access(
            Stage::Late,
            log_stats_system,
            Access::new()
                .reads_resource::<TotalTime>()
                .reads_resource::<DeltaTime>()
                .reads_resource::<DebugStats>(),
        );
        // Debug-UI metrics sampler — throttles itself to ~2 Hz, so the
        // per-frame cost is a single resource read + compare. On a
        // sample tick it walks sysinfo + the gpu-allocator block list
        // and writes the snapshot read by the protocol / TUI / egui
        // overlay.
        scheduler.add_to_with_access(
            Stage::Late,
            metrics_sample_system,
            Access::new()
                .reads_resource::<TotalTime>()
                .reads_resource::<SkinCoverageStats>()
                .reads_resource::<byroredux_renderer::vulkan::allocator::AllocatorResource>()
                .reads_resource::<byroredux_renderer::vulkan::allocator::GpuMemoryBudget>()
                .writes_resource::<MetricsState>()
                .writes_resource::<MetricsSnapshot>(),
        );
        scheduler.add_exclusive(Stage::Late, byroredux_scripting::event_cleanup_system);

        // #1394 — guard against silently re-introducing undeclared parallel
        // systems.  All parallel-batch entries must use add_to_with_access;
        // any future add_to() call will trip this in debug builds before
        // the Unknown row appears in the sys.accesses conflict report.
        let report_snapshot = scheduler.access_report();
        debug_assert_eq!(
            report_snapshot.undeclared_parallel_count(),
            0,
            "undeclared parallel system detected — use add_to_with_access instead of add_to"
        );

        // Store system names + console commands as resources.
        let system_names: Vec<String> = scheduler
            .system_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        world.insert_resource(SystemList(system_names));
        // R7: snapshot the per-stage access report once after the
        // schedule is built. Read by `sys.accesses` to surface
        // declared-access conflicts to the operator.
        world.insert_resource(byroredux_core::ecs::SchedulerAccessReport(report_snapshot));
        world.insert_resource(build_command_registry());

        // Start debug server (feature-gated, zero cost when disabled).
        // The returned handle's Drop signals shutdown + joins the
        // listener thread; stash it on App so natural teardown is tidy
        // (#855 / C6-NEW-02).
        #[cfg(feature = "debug-server")]
        let debug_server = {
            let debug_port: u16 = std::env::var("BYRO_DEBUG_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(9876);
            Some(byroredux_debug_server::start(&mut scheduler, debug_port))
        };

        Self {
            window: None,
            renderer: None,
            world,
            scheduler,
            last_frame: Instant::now(),
            ui_manager: None,
            ui_texture_handle: None,
            draw_commands: Vec::new(),
            water_commands: Vec::new(),
            gpu_lights: Vec::new(),
            bone_world: Vec::new(),
            // M29.6 — slot pool capacity. The persistent bind_inverses
            // SSBO is sized for MAX_TOTAL_BONES bones (196608 after the
            // #1284 step-2 bump, 12 MB target). Each pool slot occupies
            // MBPM (currently 144) bones, and slot 0 is reserved for
            // the global identity. So allocatable slot count =
            // (MAX_TOTAL_BONES / MBPM) - 1 = floor(196608 / 144) - 1 =
            // 1364. Allocating one slot beyond would push the palette
            // past the SSBO boundary.
            skin_slot_pool: byroredux_core::ecs::resources::SkinSlotPool::new(
                ((byroredux_renderer::vulkan::scene_buffer::MAX_TOTAL_BONES
                    / byroredux_core::ecs::components::MAX_BONES_PER_MESH)
                    - 1) as u32,
            ),
            skin_offsets: HashMap::new(),
            material_table: byroredux_renderer::MaterialTable::new(),
            bench_frames_target: None,
            bench_hold: false,
            bench_summary_printed: false,
            bench_frames_count: 0,
            bench_start: None,
            bench_systems_ns: 0,
            bench_systems_ticks: 0,
            bench_build_render_ns: 0,
            bench_ui_ns: 0,
            bench_render_ns: 0,
            bench_frame_timings: byroredux_renderer::FrameTimings::default(),
            screenshot_path: None,
            camera_pos_override: None,
            camera_forward_override: None,
            screenshot_requested: false,
            screenshot_deadline_frames: 0,
            streaming: None,
            #[cfg(feature = "debug-server")]
            debug_server,
            debug_ui: None,
            debug_ui_refresh_entities: false,
            last_redraw_end: None,
        }
    }

    /// Called once after the renderer is ready — delegates to scene::setup_scene.
    fn setup_scene(&mut self) {
        let ctx = self.renderer.as_mut().unwrap();
        scene::setup_scene(
            &mut self.world,
            ctx,
            &mut self.ui_manager,
            &mut self.ui_texture_handle,
            self.camera_pos_override,
            self.camera_forward_override,
            &mut self.streaming,
        );
    }

    /// World-streaming tick (M40 Phase 1b). Two-phase per frame:
    ///
    /// 1. **Drain ready payloads** from the worker thread. Each
    ///    payload was a cell pre-parsed off the main thread; we
    ///    finish the pool-bound import (string interning + BGSM
    ///    merge) into [`cell_loader::NifImportRegistry`], then call
    ///    [`cell_loader::load_one_exterior_cell`] which now hits
    ///    cache for every NIF in the cell (skipping the slow parse
    ///    path that pre-#Phase-1b ran on the main thread).
    /// 2. **Diff + dispatch** — read the camera position, compute
    ///    streaming deltas, send `LoadCellRequest`s for new cells,
    ///    and unload cells outside `radius_unload`.
    ///
    /// The drain runs every frame (worker payloads accumulate
    /// independent of player movement); the diff+dispatch is gated on
    /// the player crossing a cell boundary. A stale-load drop happens
    /// inside the drain when a payload's generation doesn't match
    /// `state.pending[(gx, gy)]` — the player may have moved out of
    /// range and back during the parse, invalidating the in-flight
    /// load.
    ///
    /// No-ops when `self.streaming` is `None` (interior cell or
    /// NIF-only modes).
    fn step_streaming(&mut self) {
        let Some(ctx) = self.renderer.as_mut() else {
            return;
        };
        if self.streaming.is_none() {
            return;
        }

        // ── 1. Drain ready payloads ─────────────────────────────────
        //
        // Pull payloads off the worker's channel one at a time. Each
        // is consumed via `consume_streaming_payload` (free function,
        // takes split-borrows of world/state/ctx — keeps the App
        // method signature borrow-checker friendly). Non-blocking via
        // `try_recv` — fall through immediately when no payload is
        // ready.
        loop {
            let payload_opt = self
                .streaming
                .as_mut()
                .and_then(|s| s.payload_rx.try_recv().ok());
            let Some(payload) = payload_opt else { break };

            consume_streaming_payload(
                &mut self.world,
                ctx,
                self.streaming.as_mut().unwrap(),
                payload,
            );
        }

        // ── 2. Diff + dispatch ──────────────────────────────────────
        let player_pos = {
            let Some(active) = self
                .world
                .try_resource::<byroredux_core::ecs::ActiveCamera>()
            else {
                return;
            };
            let cam_entity = active.0;
            let Some(tq) = self.world.query::<byroredux_core::ecs::Transform>() else {
                return;
            };
            let Some(tform) = tq.get(cam_entity) else {
                return;
            };
            tform.translation
        };
        let player_grid = streaming::world_pos_to_grid(player_pos.x, player_pos.z);
        let state = self.streaming.as_mut().unwrap();
        if state.last_player_grid == Some(player_grid) {
            return;
        }
        state.last_player_grid = Some(player_grid);
        log::info!(
            "Player crossed cell boundary → grid ({},{}) (world {:.0},{:.0},{:.0})",
            player_grid.0,
            player_grid.1,
            player_pos.x,
            player_pos.y,
            player_pos.z,
        );

        let deltas = streaming::compute_streaming_deltas(
            &state.loaded,
            player_grid,
            state.radius_load,
            state.radius_unload,
        );

        // Unload first to free GPU resources before kicking new loads —
        // cuts peak VRAM at the boundary crossing.
        let mut unloaded_any = false;
        for coord in deltas.to_unload {
            if let Some(slot) = state.loaded.remove(&coord) {
                cell_loader::unload_cell(&mut self.world, ctx, slot.cell_root);
                log::info!(
                    "Unloaded cell ({},{}) (root {})",
                    coord.0,
                    coord.1,
                    slot.cell_root
                );
                unloaded_any = true;
            }
            // If a load was in flight for this cell, leave the
            // pending entry; the drain step compares generation and
            // drops the stale payload when it eventually arrives.
        }
        // Cell unload despawns instances and forces a TLAS rebuild on
        // the next frame; the SVGF/TAA history is now stale for the
        // pixels those instances covered. Bump the recovery window so
        // ghosting is washed out in ~8 frames instead of 30+ at the
        // steady-state α. See #801 / STRM-N1.
        if unloaded_any {
            ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);
        }

        // Dispatch new loads — non-blocking send, worker picks them up
        // off-thread.
        //
        // Snapshot the NifImportRegistry's cached keys ONCE per
        // dispatch batch (i.e. per cell-crossing) so every request
        // shares the same view. Worker filters its model_paths
        // against this set so >95% of typical exterior statics
        // (rocks, roadways, junkpiles) skip BSA-extract + parse
        // entirely on cell crossings. See #862.
        let cached_keys = self
            .world
            .resource::<cell_loader::NifImportRegistry>()
            .snapshot_keys();
        for (gx, gy) in deltas.to_load {
            // Skip if a load is already in flight or the cell is
            // already loaded (the diff already filtered loaded, but a
            // duplicate compute_streaming_deltas call could happen
            // mid-frame).
            if state.pending.contains_key(&(gx, gy)) {
                continue;
            }
            let generation = state.next_generation;
            state.next_generation = state.next_generation.wrapping_add(1);
            state.pending.insert((gx, gy), generation);
            let req = streaming::LoadCellRequest {
                gx,
                gy,
                generation,
                wctx: state.wctx.clone(),
                tex_provider: state.tex_provider.clone(),
                cached_keys: cached_keys.clone(),
            };
            if state.send_request(req).is_err() {
                log::error!(
                    "Streaming worker channel closed; cell ({},{}) cannot be loaded",
                    gx,
                    gy
                );
                state.pending.remove(&(gx, gy));
            }
        }

        // ── 3. Stream the distant-terrain LOD ring (#1373) ──────────
        //
        // The player crossed a cell boundary (guarded by the early
        // return above), so the full-detail hole-out region moved with
        // them. Re-center the ring: spawn blocks entering the LOD radius,
        // unload those leaving, and regenerate boundary blocks whose hole
        // mask changed against the new full-detail region. Arcs are cloned
        // so `lod_blocks` can be borrowed mutably alongside `self.world`.
        let lod_tex = state.tex_provider.clone();
        let lod_wctx = state.wctx.clone();
        let lod_full_radius = state.radius_load;
        cell_loader::stream_lod_blocks(
            &mut self.world,
            ctx,
            lod_tex.as_ref(),
            lod_wctx.as_ref(),
            player_grid,
            lod_full_radius,
            &mut state.lod_blocks,
        );
        // Distant object LOD (Skyrim+/FO4 `.bto`) — no-op on other games.
        cell_loader::stream_object_lod_blocks(
            &mut self.world,
            ctx,
            lod_tex.as_ref(),
            lod_wctx.as_ref(),
            player_grid,
            lod_full_radius,
            &mut state.object_lod_blocks,
        );
    }

    /// Drain any queued debug-UI load ops and dispatch them to the
    /// existing loader primitives. Runs once per frame after
    /// `step_streaming` (so any in-flight streaming work settles
    /// first) and before `step_cell_transition` (so a queued debug
    /// cell load can't race with a `door.teleport`-driven transition
    /// that landed the same frame). No-op when the queue is empty,
    /// which is the steady-state case.
    fn step_debug_loads(&mut self) {
        let Some(ctx) = self.renderer.as_mut() else {
            return;
        };
        debug_load::execute_pending_debug_loads(&mut self.world, ctx, &mut self.streaming);
    }

    /// Drain any queued [`cell_loader::PendingCellTransition`] and
    /// dispatch the orchestrator. Runs once per frame after
    /// `step_streaming`. No-op on frames with no pending transition.
    ///
    /// Dispatches on the destination variant:
    ///
    /// * `Interior` — tear down any active exterior streaming state
    ///   (drain `state.loaded`, shutdown the worker thread), then
    ///   call `cell_loader::load_interior_cell` for the destination.
    /// * `Exterior` — tear down current interior (if any), tear down
    ///   existing streaming state, build a fresh `ExteriorWorldContext` +
    ///   `WorldStreamingState` for the destination worldspace,
    ///   stream initial radius, reposition camera.
    ///
    /// Provider construction is per-transition: rebuilding from CLI
    /// args matches the boot-time `scene::setup_scene` pattern. The
    /// cost is a few-hundred-ms BSA re-open per transition, acceptable
    /// for the single-trigger door flow.
    fn step_cell_transition(&mut self) {
        let Some(ctx) = self.renderer.as_mut() else {
            return;
        };
        let Some(pending) = cell_loader::take_pending_transition(&self.world) else {
            return;
        };

        let dest_label = cell_loader::log_transition_header(&pending);
        let args: Vec<String> = crate::cli_args::effective_args();

        // Default exterior-load radius — matches the CLI default (5 →
        // 11×11 grid). A future enhancement can plumb the boot-time
        // `--radius` through `LoadedPluginSet` to honor the operator's
        // chosen value across transitions.
        const DEFAULT_TRANSITION_RADIUS: i32 = 5;

        match pending.destination {
            cell_loader::TransitionDestination::Interior {
                editor_id,
                masters,
                esm_path,
            } => {
                // Exterior → Interior: drain the streaming state before
                // the interior load fires. Mirrors the CloseRequested
                // shutdown sequence: unload every loaded cell so its
                // BLAS / mesh / texture refs drain, flush deferred
                // destroys, then shutdown the worker with a bounded
                // timeout. The owned providers held by the streaming
                // state drop alongside the take().
                if self.streaming.is_some() {
                    drain_streaming_state(&mut self.world, ctx, &mut self.streaming);
                }
                let tex_provider = crate::asset_provider::build_texture_provider(&args);
                let mut mat_provider = crate::asset_provider::build_material_provider(&args);
                match cell_loader::load_interior_cell(
                    &mut self.world,
                    ctx,
                    &tex_provider,
                    Some(&mut mat_provider),
                    cell_loader::InteriorCellRequest {
                        editor_id: &editor_id,
                        masters: &masters,
                        esm_path: &esm_path,
                        dest_pos_zup: pending.destination_position_zup,
                        dest_rot_zup: pending.destination_rotation_zup,
                    },
                ) {
                    Ok(cam_pos) => {
                        log::info!(
                            "Cell transition applied: → {} at world ({:.1}, {:.1}, {:.1})",
                            dest_label,
                            cam_pos.x,
                            cam_pos.y,
                            cam_pos.z,
                        );
                        ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);
                    }
                    Err(e) => {
                        log::error!("Cell transition to {} FAILED: {}", dest_label, e);
                    }
                }
            }
            cell_loader::TransitionDestination::Exterior {
                worldspace,
                grid,
                masters,
                esm_path,
            } => {
                // 1. Tear down any active interior cell first — its
                // CurrentCellRoot would otherwise leak alongside the
                // new streaming state. No-op on the
                // Exterior→Exterior cross-worldspace path (no interior
                // was loaded).
                cell_loader::unload_current_interior(&mut self.world, ctx);

                // 2. Tear down any existing streaming state. Always
                // rebuild on exterior-destination transitions, even
                // intra-worldspace, so the orchestrator's failure
                // mode is uniform.
                if self.streaming.is_some() {
                    drain_streaming_state(&mut self.world, ctx, &mut self.streaming);
                }

                // 3. Build the fresh streaming context for the
                // destination worldspace + initial grid. `wrld_override`
                // pins the worldspace to what the reverse-lookup
                // returned so the heuristic search inside
                // `build_exterior_world_context` doesn't pick something
                // else.
                let tex_provider = crate::asset_provider::build_texture_provider(&args);
                let mat_provider = crate::asset_provider::build_material_provider(&args);
                match cell_loader::build_exterior_world_context(
                    &masters,
                    &esm_path,
                    grid.0,
                    grid.1,
                    DEFAULT_TRANSITION_RADIUS,
                    Some(&worldspace),
                ) {
                    Ok(wctx) => {
                        crate::scene::apply_worldspace_weather(
                            &mut self.world,
                            ctx,
                            &tex_provider,
                            &wctx,
                        );
                        let mut state = streaming::WorldStreamingState::new(
                            wctx,
                            tex_provider,
                            mat_provider,
                            DEFAULT_TRANSITION_RADIUS,
                        );
                        state.last_player_grid = Some(grid);
                        let _ = crate::scene::stream_initial_radius(
                            &mut self.world,
                            ctx,
                            &mut state,
                            grid.0,
                            grid.1,
                        );
                        self.streaming = Some(state);

                        // 4. Reposition the camera at the destination
                        // spawn point. `stream_initial_radius` returned
                        // a "load-centre" pose for the initial boot
                        // path, but here we want the XTEL-authored
                        // spawn, not the cell centre.
                        let dest_pos =
                            cell_loader::position_zup_to_yup(pending.destination_position_zup);
                        let dest_rot =
                            cell_loader::rotation_zup_to_yup_quat(pending.destination_rotation_zup);
                        cell_loader::reposition_camera(&mut self.world, dest_pos, dest_rot);

                        log::info!(
                            "Cell transition applied: → {} at world ({:.1}, {:.1}, {:.1})",
                            dest_label,
                            dest_pos.x,
                            dest_pos.y,
                            dest_pos.z,
                        );
                        ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);
                    }
                    Err(e) => {
                        log::error!(
                            "Cell transition to {} FAILED at exterior context build: {:#}",
                            dest_label,
                            e,
                        );
                    }
                }
            }
        }
    }
}

// Tear down the active exterior streaming state: drain every loaded
// cell via `unload_cell`, flush the renderer's deferred-destroy
// queues, then shutdown the worker thread with a bounded timeout.
// Leaves `*streaming_slot = None` on return.
//
// Free function (not an `App` method) so the caller can split-borrow
// `&mut self.world` / `&mut self.streaming` / `&mut self.renderer`
// without aliasing — the `App` method form fights the borrow checker
// when `ctx` was already extracted from `self.renderer.as_mut()`.
//
// Pulled out of the `WindowEvent::CloseRequested` handler so
// transition flows can re-use the same teardown sequence — the
// shutdown ordering invariants (loaded cells before worker join
// before context drop) are identical at door transitions.

// Phase 14 — `render_one_frame` is an inherent method on `App` so it
// can be called from both the `WindowEvent::RedrawRequested` arm (now
// a no-op) and the `about_to_wait` tick. Trait impl reopens after the
// inherent block below.
impl App {
    /// Phase 14 — pulled out of the original `WindowEvent::RedrawRequested`
    /// arm so the game loop can call it directly from `about_to_wait`
    /// instead of routing through `request_redraw()` → wait for the
    /// compositor's frame-callback → `RedrawRequested`. On Wayland +
    /// winit 0.30, that round-trip gates the engine at the
    /// compositor's pace (~18 FPS in the observed Sleeping Giant Inn
    /// reading) regardless of `ControlFlow::Poll`. Drawing from
    /// `about_to_wait` bypasses the gate; combined with MAILBOX
    /// present mode the compositor still vsyncs presentation but
    /// the render loop runs uncapped.
    fn render_one_frame(&mut self, event_loop: &ActiveEventLoop) {
        // Phase 15 — bracket render_one_frame in three phases
        // so the egui Metrics panel can pin which one of
        // pre-draw / draw_frame call / post-draw is hiding the
        // ~30 ms we still can't see (Phase-14 surfaced
        // render_one_frame's total wall as ~47 ms while the
        // GPU + per-call CPU brackets sum to ~18 ms).
        let rof_pre_t0 = Instant::now();
        let mut rof_pre_draw_ns: u64 = 0;
        let mut rof_draw_call_ns: u64 = 0;
        // Phase 4 — populate the panel snapshot from the
        // World, run egui (gets `PanelOutputs` back), apply
        // those outputs, then stash the FullOutput +
        // egui::Context for the renderer to consume.
        //
        // #1376: build_debug_ui_snapshot deep-clones two BTreeMaps +
        // a Vec of Strings every frame. Gate on `visible` so the clone
        // is skipped when the overlay is hidden (boot default). The
        // `ui.run` path below already early-returns on `!visible` and
        // ignores the snapshot; returning a default here is safe.
        let snapshot = if self.debug_ui.as_ref().is_some_and(|ui| ui.visible) {
            build_debug_ui_snapshot(&self.world, self.debug_ui_refresh_entities)
        } else {
            byroredux_debug_ui::PanelSnapshot::default()
        };
        self.debug_ui_refresh_entities = false;

        let (egui_frame, outputs) =
            if let (Some(ref mut ui), Some(win)) = (self.debug_ui.as_mut(), self.window.as_ref()) {
                let outputs = ui.run(win, &snapshot);
                let frame = ui.take_output().map(|out| (ui.egui_ctx.clone(), out));
                (frame, outputs)
            } else {
                (None, byroredux_debug_ui::PanelOutputs::default())
            };

        apply_debug_ui_outputs(
            &mut self.world,
            outputs,
            &mut self.debug_ui_refresh_entities,
            self.debug_ui.as_mut(),
        );

        if let Some(ref mut ctx) = self.renderer {
            if let Some((egui_ctx, output)) = egui_frame {
                ctx.submit_egui_frame(egui_ctx, output);
            }
            let is_benching = self.bench_frames_target.is_some();

            let brd_t0 = Instant::now();
            let frame = build_render_data(
                &self.world,
                &mut self.draw_commands,
                &mut self.water_commands,
                &mut self.gpu_lights,
                &mut self.bone_world,
                &mut self.skin_offsets,
                &mut self.skin_slot_pool,
                &mut self.material_table,
                ctx.particle_quad_handle,
            );
            if is_benching {
                self.bench_build_render_ns += brd_t0.elapsed().as_nanos() as u64;
            }

            {
                let mut tlm = self.world.resource_mut::<ScratchTelemetry>();
                tlm.materials_unique = self.material_table.unique_user_count();
                tlm.materials_interned = self.material_table.interned_count();
                tlm.materials_overflow = self.material_table.overflow_count();
            }
            // #1428 — catch any frame where we silently degraded over-cap
            // materials to slot 0 before the Once-gated warn fires again.
            // Only fires in debug; the `mem` console command surfaces the
            // per-frame count in all builds.
            debug_assert_eq!(
                self.material_table.overflow_count(),
                0,
                "MaterialTable overflow: {} intern call(s) fell back to the \
                 neutral-default slot 0 (MAX_MATERIALS={cap}). Run `mem` to \
                 confirm; consider raising MAX_MATERIALS in \
                 scene_buffer/constants.rs if this cell genuinely needs it.",
                self.material_table.overflow_count(),
                cap = byroredux_renderer::MAX_MATERIALS,
            );

            if ctx.mesh_registry.is_geometry_dirty() {
                if let Err(e) = ctx.mesh_registry.rebuild_geometry_ssbo(
                    &ctx.device,
                    ctx.allocator.as_ref().unwrap(),
                    &ctx.graphics_queue,
                    ctx.transfer_pool,
                ) {
                    log::warn!("Failed to rebuild geometry SSBO: {e}");
                }
            }

            world_resource_set::<DebugStats>(&self.world, |s| {
                s.draw_command_count = self.draw_commands.len() as u32;
            });

            // Tick and render the UI overlay (Ruffle SWF player).
            let ui_t0 = Instant::now();
            let mut ui_tex = None;
            if let Some(ref mut ui) = self.ui_manager {
                let dt = self
                    .world
                    .try_resource::<DeltaTime>()
                    .map(|d| d.0 as f64)
                    .unwrap_or(1.0 / 60.0);
                let ui_w = ui.width;
                let ui_h = ui.height;
                ui.tick(dt);

                if let Some(pixels) = ui.render() {
                    if let Some(handle) = self.ui_texture_handle {
                        let allocator = ctx.allocator.as_ref().unwrap();
                        let upload_ctx = GpuUploadCtx {
                            device: &ctx.device,
                            allocator,
                            queue: &ctx.graphics_queue,
                            command_pool: ctx.transfer_pool,
                        };
                        if let Err(e) = ctx
                            .texture_registry
                            .update_rgba(upload_ctx, handle, ui_w, ui_h, pixels)
                        {
                            log::error!("UI texture update failed: {e:#}");
                        }
                        ui_tex = Some(handle);
                    }
                } else if self.ui_texture_handle.is_some() {
                    ui_tex = self.ui_texture_handle;
                }
            }
            if is_benching {
                self.bench_ui_ns += ui_t0.elapsed().as_nanos() as u64;
            }

            let is_interior = self
                .world
                .try_resource::<crate::components::CellLightingRes>()
                .is_some_and(|l| l.is_interior);
            let clear_color = if is_interior {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                byroredux_core::types::Color::CORNFLOWER_BLUE.as_array()
            };
            let render_t0 = Instant::now();
            let mut frame_timings = Some(byroredux_renderer::FrameTimings::default());
            let pending = self.skin_slot_pool.drain_pending(
                byroredux_renderer::vulkan::scene_buffer::MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME,
            );
            let pending_with_data: Vec<(u32, Vec<[[f32; 4]; 4]>)> = pending
                .into_iter()
                .filter_map(|(slot, entity)| {
                    self.world
                        .get::<byroredux_core::ecs::SkinnedMesh>(entity)
                        .map(|skin| {
                            let mut padded: Vec<[[f32; 4]; 4]> = skin
                                .bind_inverses
                                .iter()
                                .map(|m| m.to_cols_array_2d())
                                .collect();
                            padded.resize(
                                byroredux_core::ecs::components::MAX_BONES_PER_MESH,
                                [
                                    [1.0, 0.0, 0.0, 0.0],
                                    [0.0, 1.0, 0.0, 0.0],
                                    [0.0, 0.0, 1.0, 0.0],
                                    [0.0, 0.0, 0.0, 1.0],
                                ],
                            );
                            (slot, padded)
                        })
                })
                .collect();
            // Phase 15 — close pre-draw bracket, open draw-call.
            rof_pre_draw_ns = rof_pre_t0.elapsed().as_nanos() as u64;
            let rof_draw_call_t0 = Instant::now();
            let dof = byroredux_renderer::DofView {
                aperture: frame.aperture,
                focus_dist: frame.focus_dist,
                cam_right: frame.cam_right,
                cam_up: frame.cam_up,
                cam_forward: frame.cam_forward,
                proj_mat: frame.proj_mat,
            };
            // REND-#1451 — push live point/spot attenuation tuning
            // (LightTuning resource, mutated by the `light.atten`
            // console command) into the renderer so the controlled
            // bench can sweep the knee / A/B the legacy model without a
            // rebuild. Absent resource → renderer keeps its defaults.
            if let Some(lt) = self.world.try_resource::<crate::components::LightTuning>() {
                ctx.light_atten_knee = lt.knee_frac;
                ctx.light_atten_legacy = lt.legacy;
            }
            match ctx.draw_frame(FrameInputs {
                clear_color,
                view_proj: &frame.view_proj,
                draw_commands: &self.draw_commands,
                lights: &self.gpu_lights,
                bone_world: &self.bone_world,
                bind_inverse_pending_uploads: &pending_with_data,
                materials: self.material_table.materials(),
                camera_pos: frame.camera_pos,
                ambient_color: frame.ambient,
                fog_color: frame.fog_color,
                fog_near: frame.fog_near,
                fog_far: frame.fog_far,
                fog_clip: frame.fog_clip,
                fog_power: frame.fog_power,
                ui_texture_handle: ui_tex,
                sky_params: &frame.sky,
                dof,
                timings: frame_timings.as_mut(),
                water_commands: &self.water_commands,
                underwater: compute_underwater_params(&self.world),
                pose_dirty: self.skin_slot_pool.pose_dirty(),
            }) {
                Ok(needs_recreate) => {
                    let last_draw_stats = ctx.last_draw_call_stats;
                    world_resource_set::<DebugStats>(&self.world, |s| {
                        s.batch_count = last_draw_stats.batch_count;
                        s.indirect_call_count = last_draw_stats.indirect_call_count;
                    });
                    if let Some(ref ft) = frame_timings {
                        const NS_TO_MS: f32 = 1.0e-6;
                        let mut cpu_t = self
                            .world
                            .resource_mut::<byroredux_core::ecs::CpuFrameTimings>();
                        cpu_t.fence_wait_ms = ft.fence_wait_ns as f32 * NS_TO_MS;
                        cpu_t.tlas_build_ms = ft.tlas_build_ns as f32 * NS_TO_MS;
                        cpu_t.ssbo_build_ms = ft.ssbo_build_ns as f32 * NS_TO_MS;
                        cpu_t.cmd_record_ms = ft.cmd_record_ns as f32 * NS_TO_MS;
                        cpu_t.submit_present_ms = ft.submit_present_ns as f32 * NS_TO_MS;
                        cpu_t.acquire_ms = ft.acquire_ns as f32 * NS_TO_MS;
                        cpu_t.between_frames_ms = self
                            .last_redraw_end
                            .map(|t| t.elapsed().as_nanos() as f32 * NS_TO_MS)
                            .unwrap_or(0.0);
                    }
                    if is_benching {
                        self.bench_render_ns += render_t0.elapsed().as_nanos() as u64;
                        if let Some(ft) = frame_timings {
                            let b = &mut self.bench_frame_timings;
                            b.fence_wait_ns += ft.fence_wait_ns;
                            b.tlas_build_ns += ft.tlas_build_ns;
                            b.ssbo_build_ns += ft.ssbo_build_ns;
                            b.cmd_record_ns += ft.cmd_record_ns;
                            b.submit_present_ns += ft.submit_present_ns;
                        }
                        if self.bench_start.is_none() {
                            self.bench_start = Some(Instant::now());
                        }
                        self.bench_frames_count += 1;
                    }
                    if needs_recreate {
                        if let Some(ref win) = self.window {
                            let size = win.inner_size();
                            if size.width > 0 && size.height > 0 {
                                if let Err(e) = ctx.recreate_swapchain([size.width, size.height]) {
                                    log::error!("Swapchain recreate failed: {e:#}");
                                    event_loop.exit();
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Draw failed: {e:#}");
                    event_loop.exit();
                }
            }
            // Phase 15 — close draw-call bracket. Post-draw
            // includes the remaining work in this scope plus
            // the `last_redraw_end` stamp below.
            rof_draw_call_ns = rof_draw_call_t0.elapsed().as_nanos() as u64;
        }
        // Phase 9 — stamp end-of-frame. Next `render_one_frame`
        // call computes `now() - last_redraw_end` as
        // `between_frames_ms` (compositor wait + scheduler.run +
        // about_to_wait host work). Set unconditionally even if
        // the inner `if let Some(ref mut ctx)` branch was skipped,
        // so the metric remains continuous across renderer-down
        // frames.
        self.last_redraw_end = Some(Instant::now());
        // Phase 15 — close post-draw bracket and fold the
        // three-phase split into CpuFrameTimings. atw_post
        // surfaces render_one_frame's WALL total; this split
        // shows which phase inside it dominates.
        const NS_TO_MS: f32 = 1.0e-6;
        let rof_post_draw_ns = rof_pre_t0
            .elapsed()
            .as_nanos()
            .saturating_sub((rof_pre_draw_ns + rof_draw_call_ns) as u128)
            as u64;
        let mut cpu_t = self
            .world
            .resource_mut::<byroredux_core::ecs::CpuFrameTimings>();
        cpu_t.rof_pre_draw_ms = rof_pre_draw_ns as f32 * NS_TO_MS;
        cpu_t.rof_draw_call_ms = rof_draw_call_ns as f32 * NS_TO_MS;
        cpu_t.rof_post_draw_ms = rof_post_draw_ns as f32 * NS_TO_MS;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let config = WindowConfig::default();

        let win = match window::create_window(event_loop, &config) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create window: {e:#}");
                event_loop.exit();
                return;
            }
        };

        let size = win.inner_size();
        let (display, window_handle) = match window::raw_handles(&win) {
            Ok(h) => h,
            Err(e) => {
                log::error!("Failed to get raw handles: {e:#}");
                event_loop.exit();
                return;
            }
        };

        match VulkanContext::new(display, window_handle, [size.width, size.height]) {
            Ok(ctx) => {
                // Create screenshot bridge for debug server access.
                let ss_handle = ctx.screenshot_handle();
                self.world
                    .insert_resource(byroredux_core::ecs::ScreenshotBridge {
                        requested: ss_handle.requested,
                        result: ss_handle.result,
                        // #1006 — owner-tagged claim so the CLI
                        // `--screenshot` deadline loop and the
                        // debug-server `DebugRequest::Screenshot`
                        // can't race on a single result slot.
                        // Starts idle (SCREENSHOT_OWNER_NONE).
                        owner: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(
                            byroredux_core::ecs::resources::SCREENSHOT_OWNER_NONE,
                        )),
                    });

                // Expose the GPU allocator to the ECS so the
                // `mem.frag` console command can compute a live
                // fragmentation report on demand. Newtype wrapper
                // dodges the orphan rule on `Resource`. See #503.
                if let Some(ref alloc) = ctx.allocator {
                    self.world.insert_resource(
                        byroredux_renderer::vulkan::allocator::AllocatorResource(alloc.clone()),
                    );
                }

                // Cache the VRAM budget once — heap sizes are immutable
                // after device pick. Read by `metrics_sample_system` to
                // compute the `used / budget` ratio without a per-frame
                // `vkGetPhysicalDeviceMemoryProperties` round trip.
                self.world.insert_resource(
                    byroredux_renderer::vulkan::allocator::GpuMemoryBudget::sample(
                        &ctx.instance,
                        ctx.physical_device,
                    ),
                );

                // Phase 4 of the debug-UI plan — initialise the
                // egui overlay before the first frame.
                let mut ctx = ctx;
                if let Err(e) =
                    ctx.init_egui(byroredux_renderer::vulkan::sync::MAX_FRAMES_IN_FLIGHT)
                {
                    log::warn!("debug-UI overlay init failed: {e:#}");
                }
                let debug_ui_state = byroredux_debug_ui::DebugUiState::new(event_loop, &win);
                self.debug_ui = Some(debug_ui_state);

                self.renderer = Some(ctx);
                self.window = Some(win);
                self.last_frame = Instant::now();
                self.setup_scene();
                // M41.0 Phase 1b.x — Prime the scene's transform state
                // BEFORE the event loop starts.
                self.scheduler.run(&self.world, 0.0);
                self.renderer.as_ref().unwrap().log_memory_usage();
                log::info!("Engine ready — entering game loop");
            }
            Err(e) => {
                log::error!("Vulkan init failed: {e:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Debug-UI event forwarding — egui sees every WindowEvent
        // before the camera input layer. When the overlay is
        // visible AND egui claims to have consumed the event (e.g.
        // a click inside an egui window, a keypress targeting a
        // text field), the rest of the dispatch is skipped so the
        // fly camera doesn't move with the cursor that's busy
        // dragging an egui slider. CloseRequested + Resized always
        // run their normal handlers — egui doesn't care about
        // those.
        let egui_consumed = if let (Some(ref mut state), Some(win)) =
            (self.debug_ui.as_mut(), self.window.as_ref())
        {
            state.on_window_event(win, &event).consumed
        } else {
            false
        };
        if egui_consumed && !matches!(event, WindowEvent::CloseRequested | WindowEvent::Resized(_))
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested — shutting down");
                // M40 streaming cleanup: every per-cell streamed load owns
                // GPU resources (BLAS / mesh buffers / texture refcounts)
                // released only via `cell_loader::unload_cell`. Without
                // this sweep the allocator finds 1 dangling ref per
                // (BLAS + mesh + texture) per loaded cell at ctx
                // destruction time, triggers the
                // "leaking allocator to avoid use-after-free" path, and
                // SIGSEGVs as the orphaned device handles get reaped.
                if let (Some(ref mut state), Some(ref mut ctx)) =
                    (self.streaming.as_mut(), self.renderer.as_mut())
                {
                    let cells: Vec<_> = state.loaded.drain().collect();
                    log::info!(
                        "Streaming shutdown: unloading {} streamed cells before ctx destroy",
                        cells.len()
                    );
                    for ((_gx, _gy), slot) in cells {
                        cell_loader::unload_cell(&mut self.world, ctx, slot.cell_root);
                    }
                    // #732 / LIFE-H2 — `unload_cell` queues per-cell
                    // BLAS/mesh/texture into the renderer's deferred-
                    // destroy lists with a `MAX_FRAMES_IN_FLIGHT`
                    // countdown. The countdown only ticks inside
                    // `draw_frame`, but the window-close path goes
                    // straight from the unload sweep to `ctx Drop` with
                    // no intervening render frames. `Drop`'s in-block
                    // drain catches them eventually, but doing the
                    // drain explicitly here releases the per-queue
                    // entries' `Arc<Mutex<Allocator>>` clones before we
                    // even start tearing down the context — keeps the
                    // shutdown ordering visible at the call site rather
                    // than buried inside `VulkanContext::Drop`.
                    ctx.flush_pending_destroys();
                }
                // Drop the streaming state explicitly — joins the
                // worker thread cleanly before we tear down the GPU.
                // Without this, the worker could still be holding
                // `Arc<TextureProvider>` file handles that the
                // allocator's leak path would observe as outstanding
                // refs.
                //
                // #856 / C6-NEW-03 — pre-fix this was a bare
                // `self.streaming.take()` which detached the worker
                // (the `JoinHandle` was dropped on the same line via
                // `WorldStreamingState` Drop). `shutdown` drops
                // `request_tx` first, then joins with a 1-second
                // bound so a slow `BsaArchive::extract()` can't
                // block process teardown.
                if let Some(mut state) = self.streaming.take() {
                    state.shutdown(std::time::Duration::from_secs(1));
                    // `state` goes out of scope here. Drop runs but
                    // `shutdown` already took `worker` and `request_tx`
                    // so it short-circuits — the join is not repeated.
                }
                // Release the ECS clone of the GPU allocator before
                // dropping the renderer.  VulkanContext::Drop calls
                // Arc::try_unwrap on the allocator; if AllocatorResource
                // is still in the world it holds an extra strong-count
                // that makes try_unwrap fail, triggering the
                // device+surface+instance leak path (#1406 / MEM-03).
                self.world
                    .remove_resource::<byroredux_renderer::vulkan::allocator::AllocatorResource>();
                self.renderer.take();
                self.window.take();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut ctx) = self.renderer {
                    if size.width > 0 && size.height > 0 {
                        if let Err(e) = ctx.recreate_swapchain([size.width, size.height]) {
                            log::error!("Swapchain recreate failed: {e:#}");
                            event_loop.exit();
                        }
                        // Update camera aspect ratio.
                        if let Some(active) = self.world.try_resource::<ActiveCamera>() {
                            let cam_entity = active.0;
                            drop(active);
                            if let Some(mut q) = self.world.query_mut::<Camera>() {
                                if let Some(cam) = q.get_mut(cam_entity) {
                                    cam.aspect = size.width as f32 / size.height as f32;
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Phase 14 — render is now driven by `about_to_wait`,
                // not by the compositor's `RedrawRequested` event.
                // The OS still fires this on window expose / resize /
                // first paint, but we don't render here — the next
                // `about_to_wait` tick will do the work and present
                // the new frame. Keeping the arm empty (not removed)
                // so the match stays exhaustive against the existing
                // dummy match scope; the body is intentionally bare.
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let mut input = self.world.resource_mut::<InputState>();
                    match event.state {
                        ElementState::Pressed => {
                            // Escape toggles mouse capture.
                            if code == KeyCode::Escape && !event.repeat {
                                let captured = !input.mouse_captured;
                                input.mouse_captured = captured;
                                drop(input);
                                if let Some(ref win) = self.window {
                                    if captured {
                                        let _ =
                                            win.set_cursor_grab(CursorGrabMode::Confined).or_else(
                                                |_| win.set_cursor_grab(CursorGrabMode::Locked),
                                            );
                                        win.set_cursor_visible(false);
                                    } else {
                                        let _ = win.set_cursor_grab(CursorGrabMode::None);
                                        win.set_cursor_visible(true);
                                    }
                                }
                            } else if code == KeyCode::KeyF && !event.repeat {
                                // M28.5 follow-up — Walk ↔ Fly mode toggle.
                                // Temporary debug binding until an in-engine
                                // console (byro-dbg embed) is available. Models
                                // Bethesda's `tcl` (toggle collision) command:
                                // - Fly → Character: snap the character body to
                                //   the camera's current world position (so the
                                //   player "lands" wherever the freeflight cam
                                //   was looking from). The character_controller
                                //   then takes over from there.
                                // - Character → Fly: no-op on positions —
                                //   `camera_follow_system` had been writing the
                                //   active camera at `body_pos + eye_height`
                                //   anyway, so the fly cam takes over from the
                                //   same place. The character body stays alive
                                //   but `character_controller_system` early-
                                //   returns on FlyCam mode, so it freezes in
                                //   place until the user toggles back.
                                drop(input);
                                toggle_player_mode(&mut self.world);
                            } else if code == KeyCode::F3 && !event.repeat {
                                // Phase 4 of the debug-UI plan — F3
                                // toggles the egui overlay. Doesn't
                                // touch InputState so the camera
                                // input layer is uninterrupted (the
                                // overlay's own egui-winit handler
                                // is the source of truth for any
                                // mouse / keyboard egui needs).
                                drop(input);
                                if let Some(ref mut ui) = self.debug_ui {
                                    ui.toggle();
                                }
                            } else {
                                input.keys_held.insert(code);
                            }
                        }
                        ElementState::Released => {
                            input.keys_held.remove(&code);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            let mut input = self.world.resource_mut::<InputState>();
            if input.mouse_captured {
                let sensitivity = input.look_sensitivity;
                input.yaw -= delta.0 as f32 * sensitivity;
                input.pitch -= delta.1 as f32 * sensitivity;
                // Clamp pitch to avoid flipping.
                input.pitch = input.pitch.clamp(
                    -std::f32::consts::FRAC_PI_2 + 0.01,
                    std::f32::consts::FRAC_PI_2 - 0.01,
                );
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let atw_pre_t0 = Instant::now();
        let now = atw_pre_t0;
        // `BYROREDUX_FIXED_DT=<seconds>` overrides the wall-clock dt
        // with a fixed value so simulation state at frame N is
        // reproducible across runs. Used by the golden-frame
        // regression tests in `tests/golden_frames.rs` — set to `0`
        // to freeze animation entirely (camera, spin, TAA jitter
        // still advance per-frame because they're frame-counter
        // driven, not dt-driven), or `0.01666` to step at 60 Hz
        // for deterministic time-based sims. Has no effect when
        // unset; `last_frame` is still tracked for any consumer
        // that reads wall-clock elapsed elsewhere.
        let wall_dt = now.duration_since(self.last_frame).as_secs_f32();
        let dt = std::env::var("BYROREDUX_FIXED_DT")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(wall_dt);
        self.last_frame = now;

        // Update time resources.
        world_resource_set::<DeltaTime>(&self.world, |r| r.0 = dt);
        world_resource_set::<TotalTime>(&self.world, |r| r.0 += dt);

        // Update debug stats.
        //
        // #637 / FNV-D5-02 — `mesh_count` / `texture_count` are
        // registry-wide and don't drop on cell unload. The new
        // `meshes_in_use` / `textures_in_use` counts walk the ECS
        // `MeshHandle` / `TextureHandle` queries and dedupe non-zero
        // handles, so a regression that retains a registry entry past
        // the last live consumer shows up as `registry > in_use`. Done
        // in two scopes because the queries need an immutable world
        // borrow that can't coexist with `resource_mut::<DebugStats>`.
        let (meshes_in_use, textures_in_use) = {
            let mut mesh_set: std::collections::HashSet<u32> = std::collections::HashSet::new();
            if let Some(q) = self.world.query::<byroredux_core::ecs::MeshHandle>() {
                for (_, h) in q.iter() {
                    if h.0 != 0 {
                        mesh_set.insert(h.0);
                    }
                }
            }
            let mut tex_set: std::collections::HashSet<u32> = std::collections::HashSet::new();
            if let Some(q) = self.world.query::<byroredux_core::ecs::TextureHandle>() {
                for (_, h) in q.iter() {
                    if h.0 != 0 {
                        tex_set.insert(h.0);
                    }
                }
            }
            (mesh_set.len() as u32, tex_set.len() as u32)
        };
        {
            let mut stats = self.world.resource_mut::<DebugStats>();
            stats.push_frame_time(dt);
            stats.entity_count = self.world.next_entity_id();
            stats.meshes_in_use = meshes_in_use;
            stats.textures_in_use = textures_in_use;
            if let Some(ref ctx) = self.renderer {
                stats.mesh_count = ctx.mesh_registry.len() as u32;
                stats.texture_count = ctx.texture_registry.len() as u32;
            }
            // #1284 — mirror SkinSlotPool telemetry into DebugStats so
            // `log_stats_system` (ECS, no App access) can surface it.
            stats.skin_pool_live = self.skin_slot_pool.live_slot_count();
            stats.skin_pool_max = self.skin_slot_pool.max_slot();
            stats.skin_pool_overflow_attempts = self.skin_slot_pool.overflow_attempt_count();
        }

        // Refresh renderer-side scratch-Vec telemetry (R6). Reuses the
        // resource's `rows` Vec so this is amortized to ~zero allocs
        // after the first frame; capacity stabilises at the count of
        // declared scratches in `VulkanContext::fill_scratch_telemetry`.
        if let Some(ref ctx) = self.renderer {
            let mut tlm = self.world.resource_mut::<ScratchTelemetry>();
            ctx.fill_scratch_telemetry(&mut tlm.rows);
        }

        // Refresh skinned-BLAS coverage stats — captures last frame's
        // dispatches / first-sight / refit counters from the renderer
        // so `skin.coverage` reflects the just-drawn frame. Mirrors the
        // scratch-telemetry pattern; the `failed_entity_ids` Vec is
        // bounded to 16 entries inside `fill_skin_coverage_stats`.
        if let Some(ref ctx) = self.renderer {
            let mut cov = self.world.resource_mut::<SkinCoverageStats>();
            ctx.fill_skin_coverage_stats(&mut cov);
        }

        // End of pre-scheduler phase (Phase 10 bracket).
        let atw_pre_ns = atw_pre_t0.elapsed().as_nanos() as u64;

        // Run all systems.
        let systems_t0 = Instant::now();
        self.scheduler.run(&self.world, dt);
        let atw_scheduler_ns = systems_t0.elapsed().as_nanos() as u64;
        if self.bench_frames_target.is_some() && self.renderer.is_some() {
            self.bench_systems_ns += atw_scheduler_ns;
            self.bench_systems_ticks += 1;
        }

        // Post-scheduler phase starts here (Phase 10 bracket).
        let atw_post_t0 = Instant::now();

        // World cell streaming (M40 Phase 1a). Runs after the
        // scheduler so the scheduler-driven `fly_camera_system` has
        // already published the player's current Transform translation
        // for this frame. No-ops outside `--esm + --grid` exterior
        // mode and when the player hasn't crossed a boundary.
        self.step_streaming();

        // Debug-UI load queue (Phase 2 of the debug-UI plan). Drains
        // the `PendingDebugLoadSlot` populated by the debug-server's
        // `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell`
        // handlers. Sequenced BEFORE `step_cell_transition` so a
        // debug load that arrives the same frame as a queued
        // `door.teleport` doesn't trample the transition's mid-load
        // state.
        self.step_debug_loads();

        // Cell-transition dispatch (M40 Phase 2 Stage 3). Drains the
        // `PendingCellTransitionSlot` posted by `door.teleport`
        // (and future F-key activate) and dispatches the orchestrator.
        // No-op when the slot is `None` — the common per-frame case.
        self.step_cell_transition();

        // Update window title with stats (throttled: every 16 frames ≈ 4×/sec at 60fps).
        let config_debug = self.world.resource::<EngineConfig>().debug_logging;
        if config_debug {
            let stats = self.world.resource::<DebugStats>();
            if stats.frame_index().is_multiple_of(16) {
                if let Some(ref win) = self.window {
                    // #1258 — `{}/{}b/{}c draws` = input DrawCommands /
                    // post-merge batches / actual GPU calls.
                    win.set_title(&format!(
                        "ByroRedux | {:.0} FPS | {:.1}ms | {} entities | {} meshes | {} textures | {}/{}b/{}c draws",
                        stats.avg_fps(), stats.frame_time_ms,
                        stats.entity_count, stats.mesh_count, stats.texture_count,
                        stats.draw_command_count, stats.batch_count, stats.indirect_call_count,
                    ));
                }
            }
        }

        // Phase 14 — drive rendering directly from `about_to_wait`
        // instead of `win.request_redraw()` → wait for
        // compositor frame callback → `WindowEvent::RedrawRequested`.
        // On Wayland + winit 0.30 the indirection costs ~54 ms per
        // frame at the compositor's pace. Drawing here uncaps the
        // loop; MAILBOX present mode still vsyncs the actual
        // presentation but `between_frames` drops to the
        // GPU+CPU-bound minimum.
        if self.window.is_some() && self.renderer.is_some() {
            self.render_one_frame(event_loop);
        }

        // --bench-frames: once we've rendered the target number of
        // frames, emit a single machine-readable summary line and exit.
        // The renderer must be up (bench counts start after the first
        // real frame); a `--bench-frames N` that never renders (window
        // creation fails, etc.) does nothing here.
        if let Some(target) = self.bench_frames_target {
            if self.renderer.is_some() {
                // Guard: under `--bench-hold` we re-enter this branch on
                // every `about_to_wait` tick once the bench window has
                // closed; without the `bench_summary_printed` flag the
                // summary would dump per-tick and the screenshot path
                // would re-fire forever.
                if self.bench_frames_count >= target && !self.bench_summary_printed {
                    let stats = self.world.resource::<DebugStats>();
                    let elapsed_secs = self
                        .bench_start
                        .map(|t| t.elapsed().as_secs_f64())
                        .unwrap_or(1.0);
                    let wall_fps = self.bench_frames_count as f64 / elapsed_secs;
                    let wall_ms = elapsed_secs * 1000.0 / self.bench_frames_count as f64;
                    let n = self.bench_frames_count as f64;
                    let ticks_per_frame = self.bench_systems_ticks as f64 / n;
                    let systems_ms = if self.bench_systems_ticks > 0 {
                        self.bench_systems_ns as f64 / self.bench_systems_ticks as f64 / 1e6
                    } else {
                        0.0
                    };
                    let brd_ms = self.bench_build_render_ns as f64 / n / 1e6;
                    let ui_ms = self.bench_ui_ns as f64 / n / 1e6;
                    let draw_ms = self.bench_render_ns as f64 / n / 1e6;
                    let ft = &self.bench_frame_timings;
                    let fence_ms = ft.fence_wait_ns as f64 / n / 1e6;
                    let tlas_ms = ft.tlas_build_ns as f64 / n / 1e6;
                    let ssbo_ms = ft.ssbo_build_ns as f64 / n / 1e6;
                    let cmd_ms = ft.cmd_record_ns as f64 / n / 1e6;
                    let submit_ms = ft.submit_present_ns as f64 / n / 1e6;
                    let accounted = systems_ms * ticks_per_frame + brd_ms + ui_ms + draw_ms;
                    let unaccounted_ms = (wall_ms - accounted).max(0.0);
                    // #1194 — per-pass GPU timer snapshot. The
                    // SkinCoverageStats resource is filled at the end
                    // of every `draw_frame`; values here are from the
                    // last completed frame and represent one
                    // `MAX_FRAMES_IN_FLIGHT` cycle of pipeline lag.
                    // Reads 0.0 across the board when the driver
                    // lacks `timestampComputeAndGraphics` or no
                    // skinned/TAA work fired on the snapshot frame.
                    // Surfaces `gpu_skin_disp` / `gpu_blas_refit` /
                    // `gpu_taa` so PERF-DIM7-01/-02/-03 (#1195/#1196/
                    // #1197) can measure rather than guess.
                    let (gpu_skin_disp_ms, gpu_blas_refit_ms, gpu_taa_ms) = self
                        .world
                        .try_resource::<byroredux_core::ecs::SkinCoverageStats>()
                        .map(|s| {
                            (
                                s.gpu_skin_dispatch_ms,
                                s.gpu_skin_blas_refit_ms,
                                s.gpu_taa_ms,
                            )
                        })
                        .unwrap_or((0.0, 0.0, 0.0));
                    println!(
                        "bench: frames={} wall_fps={:.1} wall_ms={:.2} \
                         brd_ms={:.2} ui_ms={:.2} draw_ms={:.2} \
                         [fence={:.2} tlas={:.2} ssbo={:.2} cmd={:.2} submit={:.2}] \
                         [gpu_skin_disp={:.3} gpu_blas_refit={:.3} gpu_taa={:.3}] \
                         systems_ms={:.2} ticks_per_frame={:.1} unaccounted_ms={:.2} \
                         entities={} meshes={} textures={} draws={}/{}b/{}c",
                        self.bench_frames_count,
                        wall_fps,
                        wall_ms,
                        brd_ms,
                        ui_ms,
                        draw_ms,
                        fence_ms,
                        tlas_ms,
                        ssbo_ms,
                        cmd_ms,
                        submit_ms,
                        gpu_skin_disp_ms,
                        gpu_blas_refit_ms,
                        gpu_taa_ms,
                        systems_ms,
                        ticks_per_frame,
                        unaccounted_ms,
                        stats.entity_count,
                        stats.mesh_count,
                        stats.texture_count,
                        // #1258 — `draws=N/Mb/Kc` = N input DrawCommands
                        // / M post-merge batches / K actual GPU calls.
                        // Pre-fix this was a single `draws=N` that
                        // looked like a GPU call count but was actually
                        // the input. Format change preserves the
                        // existing first number for audit comparability.
                        stats.draw_command_count,
                        stats.batch_count,
                        stats.indirect_call_count,
                    );
                    drop(stats);

                    // --screenshot: queue a capture request and defer
                    // the event-loop exit until the PNG lands (or the
                    // frame-budget elapses). The screenshot flow takes
                    // 2+ frames: frame N kicks the staging copy, N+1
                    // encodes the PNG. We re-enter this branch up to
                    // SCREENSHOT_DEADLINE_FRAMES times before giving
                    // up.
                    if let Some(path) = self.screenshot_path.clone() {
                        if !self.screenshot_requested {
                            if let Some(bridge) = self
                                .world
                                .try_resource::<byroredux_core::ecs::ScreenshotBridge>()
                            {
                                // #1006 — claim ownership atomically.
                                // If the debug-server already holds
                                // the bridge (rare: byro-dbg attached
                                // before the CLI's first frame issues
                                // its screenshot command), bail with a
                                // clear error so the user knows the
                                // collision happened instead of silently
                                // racing for the result slot.
                                if !bridge
                                    .try_claim(byroredux_core::ecs::resources::SCREENSHOT_OWNER_CLI)
                                {
                                    eprintln!(
                                        "screenshot: bridge already claimed (debug-server owns it) — skipping CLI capture"
                                    );
                                    self.screenshot_path = None;
                                } else {
                                    drop(bridge);
                                    self.screenshot_requested = true;
                                    self.screenshot_deadline_frames = 60;
                                    return; // keep running frames
                                }
                            }
                        }

                        // Poll the result slot until the PNG arrives.
                        // Owner-gated take so a debug-server screenshot
                        // racing past the CLI claim can't steal our bytes.
                        let maybe_bytes = self
                            .world
                            .try_resource::<byroredux_core::ecs::ScreenshotBridge>()
                            .and_then(|b| {
                                b.take_result_for(
                                    byroredux_core::ecs::resources::SCREENSHOT_OWNER_CLI,
                                )
                            });
                        if let Some(bytes) = maybe_bytes {
                            match std::fs::write(&path, &bytes) {
                                Ok(()) => {
                                    println!("screenshot: wrote {} bytes to {}", bytes.len(), path)
                                }
                                Err(e) => eprintln!("screenshot: failed to write {}: {e}", path),
                            }
                        } else if self.screenshot_deadline_frames > 0 {
                            self.screenshot_deadline_frames -= 1;
                            return; // keep pumping
                        } else {
                            eprintln!("screenshot: timed out waiting for PNG result",);
                        }
                    }

                    // Latch the summary-once invariant for `--bench-hold`
                    // and only exit when the caller hasn't asked to hold
                    // the engine open. Under hold, the next about_to_wait
                    // ticks render normal frames + service the debug
                    // server (port 9876 by default) so `byro-dbg` can
                    // attach and run console commands against the loaded
                    // scene. See `--bench-hold` in main() and the FNV-D5
                    // audit's coverage gap.
                    self.bench_summary_printed = true;
                    if !self.bench_hold {
                        event_loop.exit();
                    } else {
                        eprintln!(
                            "bench-hold: engine held open — \
                             attach via `cargo run -p byro-dbg` \
                             (port {}). Ctrl+C / window close to exit.",
                            std::env::var("BYRO_DEBUG_PORT").unwrap_or_else(|_| "9876".to_string()),
                        );
                    }
                }
            }
        }

        // Phase 10 — write the about_to_wait phase timings into
        // `CpuFrameTimings` so the egui Metrics panel can show
        // where the 501 ms `between_frames` gap (Phase 9) is
        // actually spent inside this handler. Pre / scheduler /
        // post split lets the operator localize without
        // per-system instrumentation.
        const NS_TO_MS: f32 = 1.0e-6;
        let atw_post_ns = atw_post_t0.elapsed().as_nanos() as u64;
        let mut cpu_t = self
            .world
            .resource_mut::<byroredux_core::ecs::CpuFrameTimings>();
        cpu_t.atw_pre_ms = atw_pre_ns as f32 * NS_TO_MS;
        cpu_t.atw_scheduler_ms = atw_scheduler_ns as f32 * NS_TO_MS;
        cpu_t.atw_post_ms = atw_post_ns as f32 * NS_TO_MS;
    }
}

/// Phase 4b — build the per-frame [`PanelSnapshot`] the egui
/// overlay reads. Always populates `metrics`; the entity list is
/// rebuilt only when the Entities panel asked to refresh (avoids
/// walking the Name component query every frame for an overlay
/// that's hidden most of the time).
fn build_debug_ui_snapshot(
    world: &World,
    refresh_entities: bool,
) -> byroredux_debug_ui::PanelSnapshot {
    let metrics = world
        .try_resource::<byroredux_core::ecs::MetricsSnapshot>()
        .map(|m| byroredux_debug_ui::panels::MetricsSnapshotView {
            sampled_at_secs: m.sampled_at_secs,
            cpu_pct: m.cpu_pct,
            ram_used_mb: m.ram_used_mb,
            ram_total_mb: m.ram_total_mb,
            process_ram_mb: m.process_ram_mb,
            vram_used_mb: m.vram_used_mb,
            vram_reserved_mb: m.vram_reserved_mb,
            vram_budget_mb: m.vram_budget_mb,
            gpu_pass_ms: m.gpu_pass_ms.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            cpu_pass_ms: m.cpu_pass_ms.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            top_systems_ms: m.top_systems_ms.clone(),
        });

    let entities = if refresh_entities {
        // Resolve `Name` through the world's StringPool — `Name`
        // holds a `FixedString` symbol, not the resolved string.
        let mut out: Vec<(u32, String)> = Vec::new();
        if let (Some(q), Some(pool)) = (
            world.query::<byroredux_core::ecs::Name>(),
            world.try_resource::<StringPool>(),
        ) {
            for (id, name) in q.iter() {
                let resolved = pool
                    .resolve(name.0)
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                out.push((id, resolved));
            }
        }
        Some(out)
    } else {
        None
    };

    byroredux_debug_ui::PanelSnapshot { metrics, entities }
}

/// Phase 4b — apply the [`PanelOutputs`] the overlay produced back
/// to the world: queued loads go onto the same
/// `PendingDebugLoadSlot` the debug-server's `Load*` handlers
/// write, console expressions dispatch through the
/// `CommandRegistry`. The refresh flag latches for the next frame's
/// snapshot build. Each console eval's response lines are appended
/// to the overlay's scrollback so the operator sees the output
/// inline in the Console tab (without it the eval was a black hole —
/// the input echo showed but nothing came back).
fn apply_debug_ui_outputs(
    world: &mut World,
    outputs: byroredux_debug_ui::PanelOutputs,
    refresh_entities_flag: &mut bool,
    debug_ui: Option<&mut byroredux_debug_ui::DebugUiState>,
) {
    if outputs.refresh_entities {
        *refresh_entities_flag = true;
    }
    if !outputs.queued_loads.is_empty() {
        let mut slot = world.resource_mut::<byroredux_core::ecs::PendingDebugLoadSlot>();
        for load in outputs.queued_loads {
            match load {
                byroredux_debug_ui::QueuedLoad::Nif { path, label } => {
                    slot.push(byroredux_core::ecs::PendingDebugLoad::Nif { path, label });
                }
            }
        }
    }
    if outputs.console_evals.is_empty() {
        return;
    }
    // Collect responses first, then push into the overlay's
    // scrollback. Splitting the two phases keeps the `&World`
    // borrow CommandRegistry needs cleanly disjoint from the
    // `&mut DebugUiState` borrow `push_console_line` needs.
    let mut response_lines: Vec<String> = Vec::new();
    for expr in outputs.console_evals {
        if let Some(reg) = world.try_resource::<CommandRegistry>() {
            let output = reg.execute(world, &expr);
            log::info!("debug-ui console: {} → {}", expr, output.lines.join(" | "));
            response_lines.extend(output.lines);
        }
    }
    if let Some(ui) = debug_ui {
        for line in response_lines {
            ui.push_console_line(line);
        }
    }
}

/// Phase 20 — `--game <key>` CLI expansion. Looks up the named
/// profile in `assets/debug_profiles.toml` (+ per-user override at
/// `~/.byroredux/profiles.toml`), resolves `<games-root>/<subdir>`,
/// and appends synthetic `--esm` / `--bsa` / `--textures-bsa` /
/// `--materials-ba2` args to the input list. The user's own flags
/// stay at the FRONT — first-occurrence-wins for unique args means
/// a user `--esm Custom.esm` beats the profile default; additive
/// args (`--bsa`) take both. No-op when `--game` is absent.
///
/// `--games-root <path>` and `BYROREDUX_GAMES_ROOT` env var
/// override the default `/mnt/data/SteamLibrary/steamapps/common`.
/// Both stripped from the returned arg list — the rest of the
/// engine doesn't read them.
///
/// Missing profile / non-existent paths → log a warning and
/// return the input unchanged so the engine still boots (the
/// user can correct + retry without a re-build cycle).
fn expand_game_profile_args(mut args: Vec<String>) -> Vec<String> {
    use crate::cli_args::parse_string_arg;

    // Launch defaults from the `[defaults]` table (profiles.toml,
    // shipped + per-user override). Let an explicit `--game` win; fall
    // back to `[defaults].game` ONLY when no other content-loading flag
    // is present, so `--mesh foo.nif`, `--cmd`, an explicit `--esm`, or
    // a master-only run aren't hijacked into loading the default cell.
    let defaults = crate::game_profiles::load_launch_defaults();
    let has_other_load_flags = ["--esm", "--mesh", "--tree", "--kf", "--cmd", "--master"]
        .iter()
        .any(|f| args.iter().any(|a| a == f));
    let game_key = match parse_string_arg(&args, "--game") {
        Some(k) => k,
        None => match defaults.game.clone() {
            Some(k) if !has_other_load_flags => {
                eprintln!("[defaults] game = {k:?} (no --game / load flag given)");
                k
            }
            _ => return args,
        },
    };
    // `--games-root` CLI wins; else the config `[defaults].games_root`.
    let games_root_cli =
        parse_string_arg(&args, "--games-root").or_else(|| defaults.games_root.clone());

    // Strip the two new flags + their values from the returned
    // args; downstream code doesn't recognise them.
    args = strip_flag_and_value(args, "--game");
    args = strip_flag_and_value(args, "--games-root");

    // Load profile registry from disk. Same loader the App uses
    // at world init; safe to call before logging is initialised
    // (uses eprintln internally on failure).
    let registry = crate::game_profiles::load_default();
    let entry = match registry.get(&game_key) {
        Some(e) => e.clone(),
        None => {
            eprintln!(
                "--game {}: profile not found in `assets/debug_profiles.toml` \
                 or `~/.byroredux/profiles.toml`. Known keys: {:?}",
                game_key,
                registry.iter().map(|(k, _)| k).collect::<Vec<_>>()
            );
            return args;
        }
    };

    let games_root = crate::game_profiles::resolve_games_root(games_root_cli.as_deref());
    let data_dir = crate::game_profiles::resolve_profile_root(&entry, &games_root);

    if data_dir.as_os_str().is_empty() {
        eprintln!(
            "--game {}: profile carries neither `root` nor `subdir`; cannot resolve data dir",
            game_key,
        );
        return args;
    }
    if !data_dir.exists() {
        eprintln!(
            "--game {}: resolved data dir does not exist: {} \
             (set --games-root, BYROREDUX_GAMES_ROOT env var, or override \
              the profile's `root` in ~/.byroredux/profiles.toml)",
            game_key,
            data_dir.display(),
        );
        // Fall through anyway — let downstream loaders report
        // specific missing files for clearer diagnostics.
    }

    // Append profile-derived args. User's earlier --esm wins on
    // first-occurrence-wins; additive --bsa flags compose.
    eprintln!(
        "--game {} expanding from {} (data dir: {})",
        game_key,
        entry.name,
        data_dir.display(),
    );
    let join_arg =
        |archive: &str| -> String { data_dir.join(archive).to_string_lossy().into_owned() };

    args.push("--esm".to_string());
    args.push(join_arg(&entry.esm));
    for bsa in &entry.default_bsas {
        args.push("--bsa".to_string());
        args.push(join_arg(bsa));
    }
    for bsa in &entry.default_textures_bsas {
        args.push("--textures-bsa".to_string());
        args.push(join_arg(bsa));
    }
    for bsa in &entry.default_materials_bsas {
        args.push("--materials-ba2".to_string());
        args.push(join_arg(bsa));
    }

    // Default cell: inject `[defaults].cell` only when the profile was
    // resolved and no explicit location flag is present. A bare
    // `--game fnv` (or a no-arg default-game launch) then boots
    // straight into the configured cell.
    if let Some(cell) = &defaults.cell {
        let has_location = ["--cell", "--grid", "--wrld"]
            .iter()
            .any(|f| args.iter().any(|a| a == f));
        if !has_location {
            eprintln!("[defaults] cell = {cell:?} (no --cell / --grid / --wrld given)");
            args.push("--cell".to_string());
            args.push(cell.clone());
        }
    }
    args
}

/// Remove every `<flag> <value>` pair from the args list. Used
/// by `--game` expansion to strip the new flags before downstream
/// parsers see them.
fn strip_flag_and_value(args: Vec<String>, flag: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        if a == flag {
            // Also discard the next arg (the value).
            let _ = iter.next();
            continue;
        }
        out.push(a);
    }
    out
}
