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
mod app_events;
mod app_frame;
mod app_step;
mod asset_provider;
mod bench;
mod bench_camera;
mod boot;
pub(crate) mod cell_loader;
mod cli_args;
mod combat;
mod commands;
mod components;
mod cornell;
mod debug_load;
mod env_translate;
mod fog;
mod game_profiles;
mod groundcover_translate;
mod helpers;
mod interaction;
mod inventory;
mod list_cells;
mod material_translate;
mod name_lookup;
mod npc_spawn;
mod ownership_sample;
mod parsed_nif_cache;
mod ragdoll;
mod render;
mod save_io;
mod scene;
mod scene_import_cache;
#[cfg(test)]
mod scheduler_access_tests;
mod settings_io;
mod sf_smoke;
mod streaming;
mod streaming_helpers;
mod systems;
mod ui_input;

use anyhow::Result;
use byroredux_core::console::CommandRegistry;
use byroredux_core::ecs::{Scheduler, World};
use byroredux_core::settings::{SettingChange, SettingValue, SettingsRegistry};
use byroredux_core::string::StringPool;
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::{RendererConfig, VulkanContext};
use byroredux_ui::UiManager;
use std::time::Instant;
use winit::event::WindowEvent;
use winit::window::{CursorGrabMode, Window};

use crate::components::InputState;

fn main() -> Result<()> {
    boot::run()
}

/// Nearest-rank frame-time distribution for deterministic bench output.
/// Sorting happens once at bench exit; the hot path only appends one `f64`
/// per rendered frame.
fn bench_frame_distribution(samples_ms: &[f64]) -> [f64; 3] {
    if samples_ms.is_empty() {
        return [0.0; 3];
    }
    let mut sorted = samples_ms.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        let rank = (fraction * sorted.len() as f64).ceil() as usize;
        sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
    };
    [percentile(0.50), percentile(0.95), sorted[sorted.len() - 1]]
}

#[cfg(test)]
mod bench_frame_distribution_tests {
    use super::bench_frame_distribution;

    #[test]
    fn nearest_rank_reports_median_tail_and_worst_frame() {
        let samples: Vec<f64> = (1..=20).rev().map(f64::from).collect();
        assert_eq!(bench_frame_distribution(&samples), [10.0, 19.0, 20.0]);
        assert_eq!(bench_frame_distribution(&[]), [0.0; 3]);
    }
}

/// Derive `(deep_color.xyz, depth_below_surface)` from the active
/// camera's [`SubmersionState`]. Returns `[0, 0, 0, 0]` when the
/// camera is above water or no submersion data is available — the
/// composite shader treats `w == 0` as "underwater FX disabled".
struct App {
    window: Option<Window>,
    renderer: Option<VulkanContext>,
    renderer_config: RendererConfig,
    world: World,
    scheduler: Scheduler,
    last_frame: Instant,
    ui_manager: Option<UiManager>,
    /// Window-system state used to translate events into Scaleform space.
    ui_input_state: ui_input::UiInputState,
    /// Texture handle for the UI overlay (registered in the texture registry).
    ui_texture_handle: Option<u32>,
    /// Host methods already reported by the per-frame Scaleform drain, so a
    /// menu that calls an unimplemented method every frame reports it once
    /// rather than once per frame (#2714). Bounded by
    /// `byroredux_ui::MAX_DISTINCT_HOST_METHOD_NAMES` (#2964) — this mirrors
    /// the bridge's own movie-keyed diagnostic sets, so it needs the same
    /// cap for the same reason: an unbounded-by-construction set keyed by a
    /// string untrusted ActionScript content chooses.
    ui_reported_host_methods: std::collections::HashSet<String>,
    /// One-shot latch for the `ui_reported_host_methods` cap warning.
    ui_reported_host_methods_capped: bool,
    /// Reusable per-frame draw command buffer (cleared each frame, allocation retained).
    draw_commands: Vec<DrawCommand>,
    /// Reusable per-frame water draw command buffer. Built alongside
    /// `draw_commands` from `WaterPlane` ECS entities; routed through
    /// the renderer's dedicated water pipeline.
    water_commands: Vec<byroredux_renderer::vulkan::water::WaterDrawCommand>,
    /// Reusable per-frame light buffer (cleared each frame, allocation retained).
    gpu_lights: Vec<byroredux_renderer::GpuLight>,
    /// Reusable per-frame analytic local fog primitives.
    gpu_fog_volumes: Vec<byroredux_renderer::GpuFogVolume>,
    /// #2172 / PERF-D1-02 — decorate-sort scratch for `collect_lights`'
    /// GI-priority ordering. Held here for the same reason `gpu_lights`
    /// is: the buffer is rebuilt from scratch every frame, so only the
    /// allocation is worth carrying over.
    light_sort_scratch: Vec<(f32, byroredux_renderer::GpuLight)>,
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
    /// command emission. Retained across frames so the map's bucket
    /// allocation persists — see #253.
    ///
    /// `FxHashMap`, not std (#2923): `collect_static_mesh_draws` probes
    /// this once for **every** mesh entity in the draw-emit loop, not
    /// just the skinned ones, so most probes are misses on a map keyed
    /// by a small integer — the shape #1368 / #2174 already moved off
    /// SipHash-1-3 elsewhere on the render hot path.
    skin_offsets: rustc_hash::FxHashMap<byroredux_core::ecs::EntityId, u32>,
    /// R1 — per-frame deduplicated material table. Cleared at the
    /// top of `build_render_data`, populated as DrawCommands are
    /// emitted, uploaded as an SSBO before draw. Phase 2 builds it
    /// in lockstep with the legacy per-instance fields; Phases 3–6
    /// migrate shader reads onto it and drop the redundant copies.
    /// Retained on `App` so the `Vec`/`HashMap` allocations persist
    /// across frames — same scratch-buffer pattern as the others
    /// above (#243 / #253 / #509).
    material_table: byroredux_renderer::MaterialTable,
    /// Named benchmark timing/camera contract. Present for every finite
    /// `--bench-frames` run, including legacy invocations resolved onto a
    /// canonical mode in `boot::run`.
    bench_mode: Option<crate::bench::BenchMode>,
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
    /// to drive the automated benchmark exit, and — via `--bench-camera` —
    /// to index the deterministic camera path.
    bench_frames_count: u32,
    /// Deterministic camera path driven during a `--bench-frames` run.
    /// `None` leaves the authored pose alone, which is the default and what
    /// every pre-existing bench does. See `bench_camera`.
    bench_camera: Option<crate::bench_camera::BenchCameraPath>,
    /// Camera pose the bench path starts from — captured on the first bench
    /// frame rather than read from the CLI, so a path composes with whatever
    /// the scene (or `--camera-pos`) authored.
    bench_camera_origin: Option<crate::bench_camera::CameraPose>,
    /// Pose selected by `step_bench_camera` for the current frame. Character
    /// camera sync also runs in the scheduler, so the post-scheduler phase
    /// reapplies this exact pose before streaming and rendering.
    bench_camera_applied_pose: Option<crate::bench_camera::CameraPose>,
    /// Distance in BU from the bench camera's seeded origin to the geometry it
    /// looks at, measured once by a forward ray cast on the first stepped
    /// frame with a populated collision world. `Orbit`'s radius and `Dolly`'s
    /// travel derive from it. `None` until measured — physics is still empty
    /// when `seed_bench_camera_origin` runs.
    bench_camera_subject_distance: Option<f32>,
    /// Logical path frame for `grid-cross`. Unlike rendered-frame count this
    /// pauses while a boundary transaction is active, making each measured
    /// handoff independent of GPU frame rate and preventing the benchmark
    /// itself from superseding correct-but-slower streaming work.
    bench_camera_path_frame: u32,
    /// Soak traversal awaiting an ownership sample (EX-08 / #2374).
    ///
    /// The camera reaching the origin is not by itself a settled moment: the
    /// logical clock pauses while a boundary transaction runs, so the frame a
    /// cycle completes on can land mid-apply. Sampling there reads a
    /// half-torn-down world and makes reachability counts (`meshes_in_use`)
    /// alternate between phases while actual residency (`meshes_live_slots`)
    /// stays flat — a false leak. The cycle index parks here until streaming
    /// reports no boundary in progress.
    pending_soak_cycle: Option<u32>,
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
    /// Whole `about_to_wait` CPU wall samples, one per rendered bench frame.
    /// Retained only for the explicit finite bench window so p50/p95/max can
    /// expose boundary stutter that an average FPS line hides.
    bench_cpu_frame_ms: Vec<f64>,
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
    /// the listener cleanly instead of detaching it. Bench-hold also
    /// reads the confirmed bound endpoint before advertising attachability.
    #[cfg(feature = "debug-server")]
    debug_server: Option<byroredux_debug_server::DebugServerHandle>,
    /// Debug-UI overlay state — egui context + winit input
    /// translator. `None` until `resumed` constructs the window;
    /// initialised once at boot and outlives the rest of the App.
    /// Forwarded every `WindowEvent` so egui can grab clicks /
    /// keypresses when the overlay is visible.
    debug_ui: Option<byroredux_debug_ui::DebugUiState>,
    /// Save/load feedback produced before the window/debug UI exists (notably
    /// `--load`). Flushed into the player toast + console at resume.
    pending_player_messages: Vec<String>,
    /// Latched flag: the Entities panel asked us to rebuild its
    /// list this frame. Cleared at the start of every frame; set
    /// true by Phase 4b's `PanelOutputs::refresh_entities`. Held
    /// here (not in `DebugUiState`) because the snapshot is built
    /// from `&self.world`, which `DebugUiState::run`'s closure
    /// can't reach.
    debug_ui_refresh_entities: bool,
    /// #1584 — persistent scratch sets for the per-frame `meshes_in_use` /
    /// `textures_in_use` dedup walk in `about_to_wait`. Hoisted off the hot
    /// path so it `clear()`+reuses them instead of allocating two fresh
    /// `HashSet`s every frame (zero-steady-state-alloc posture). Capacity
    /// stabilises at the live cell's unique-handle count.
    in_use_mesh_scratch: std::collections::HashSet<u32>,
    in_use_tex_scratch: std::collections::HashSet<u32>,
    /// Phase 9 — timestamp at the END of the previous
    /// RedrawRequested handler. Subtracting from `Instant::now()`
    /// at the START of the next RedrawRequested yields
    /// "between_frames_ms" in `CpuFrameTimings`. `None` until
    /// the first frame closes; the per-frame writer uses 0.0 on
    /// the first frame so the panel doesn't show garbage.
    last_redraw_end: Option<Instant>,
}

impl Drop for App {
    /// Release the ECS clone of the GPU allocator BEFORE `VulkanContext`
    /// is dropped, on *every* teardown path — not just the
    /// `WindowEvent::CloseRequested` arm (#1477 / REN-D7-NEW-01).
    ///
    /// `App` declares `renderer` before `world`, so Rust's
    /// declaration-order field drop would otherwise run
    /// `VulkanContext::Drop` (which calls `Arc::try_unwrap` on the
    /// allocator) while `world` still holds the extra strong-count via
    /// `AllocatorResource` — re-arming the device+surface+instance leak
    /// path (#1406 / MEM-03) on any panic unwind or non-CloseRequested
    /// exit. Doing the removal here makes the ordering structural: this
    /// `drop()` body runs first, then the fields drop naturally with the
    /// resource already gone and `renderer` already taken.
    ///
    /// Idempotent with the `CloseRequested` handler — `remove_resource`
    /// and `Option::take` are both no-ops the second time.
    fn drop(&mut self) {
        // INVARIANT (REG-08 / #1640, #1477): remove the `AllocatorResource`
        // (the ECS allocator clone) BEFORE `renderer.take()` drops the
        // `VulkanContext`. Reversing these two lines re-arms the
        // allocator-outlives-context hazard (#1406) on panic-unwind.
        self.world
            .remove_resource::<byroredux_renderer::vulkan::allocator::AllocatorResource>();
        self.renderer.take();
    }
}

impl App {
    fn new(debug_mode: bool, args: &[String], mut renderer_config: RendererConfig) -> Self {
        // Three-phase construction (#1670) — see the helpers in `boot`.
        let mut world = boot::build_world(debug_mode, args);
        let mut scheduler = boot::build_scheduler();

        // Start debug server (feature-gated, zero cost when disabled).
        // The returned handle's Drop signals shutdown + joins the
        // listener thread; stash it on App so natural teardown is tidy
        // (#855 / C6-NEW-02).
        //
        // #1788 / CONC-D4-02 — this must run BEFORE
        // `install_runtime_registries` below: `debug_server::start` adds
        // `DebugDrainSystem` to the scheduler via `add_exclusive`, and
        // `install_runtime_registries` snapshots `SystemList` /
        // `SchedulerAccessReport` from the scheduler as it stands at
        // that point. Snapshotting first (the pre-fix order) silently
        // dropped the drain system from both the `systems` and
        // `sys.accesses` console command output on every debug-mode
        // launch — `debug_server::start`'s own doc comment already
        // states this precondition ("Call this after all systems have
        // been added to the scheduler").
        #[cfg(feature = "debug-server")]
        let debug_server = {
            let debug_port: u16 = std::env::var("BYRO_DEBUG_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(9876);
            match byroredux_debug_server::start(&mut scheduler, debug_port) {
                Ok(handle) => Some(handle),
                Err(error) => {
                    log::error!("Debug server failed to bind 127.0.0.1:{debug_port}: {error}");
                    None
                }
            }
        };

        boot::install_runtime_registries(&mut world, &scheduler);

        let mut pending_player_messages = Vec::new();
        // Queue startup restore after the save resources exist. The request
        // drains after renderer and scene setup, sharing the F9/menu path.
        if let Some(slot) = cli_args::parse_string_arg(args, "--load") {
            match slot.parse::<u32>() {
                Ok(slot) => {
                    let output = save_io::queue_load_slot(&world, slot);
                    if save_io::command_output_is_failure(&output) {
                        log::warn!("startup --load: {}", output.lines.join(" | "));
                    } else {
                        log::info!("startup --load: {}", output.lines.join(" | "));
                    }
                    pending_player_messages.extend(output.lines);
                }
                Err(_) => {
                    let message =
                        format!("Error: --load requires a numeric save slot, got '{slot}'");
                    log::error!("{message}");
                    pending_player_messages.push(message);
                }
            }
        }

        // Universal settings live in core and are presented by the on-screen
        // overlay. Subsystems can register additional entries here without
        // teaching the Settings tab about renderer/game-specific resources.
        let mut settings = SettingsRegistry::default();
        byroredux_debug_ui::register_builtin_settings(&mut settings)
            .expect("debug-UI built-in settings must be valid and unique");
        interaction::register_input_settings(&mut settings)
            .expect("input settings must be valid and unique");
        let settings_persistence = settings_io::SettingsPersistence::discover();
        settings_io::load(&mut settings, &settings_persistence);

        // Explicit command-line renderer selection wins for reproducible
        // benchmarks and diagnostics. Ordinary launches inherit the persisted
        // menu selection before VulkanContext is created, avoiding an
        // expensive first-frame upscaler rebuild.
        let explicit_upscaler = args
            .iter()
            .any(|arg| arg == "--upscaler" || arg == "--fsr-quality");
        if explicit_upscaler {
            let active_upscaler = renderer_config.upscaler.to_string();
            if let Err(error) = settings.set(
                byroredux_debug_ui::UPSCALER_SETTING_ID,
                SettingValue::Choice(active_upscaler.clone()),
            ) {
                log::warn!("could not seed the upscaler setting from '{active_upscaler}': {error}");
            }
        } else if let Some(SettingValue::Choice(spec)) = settings
            .get(byroredux_debug_ui::UPSCALER_SETTING_ID)
            .map(|entry| &entry.value)
        {
            match cli_args::parse_upscaler_spec(spec) {
                Ok(mode) => renderer_config.upscaler = mode,
                Err(error) => log::warn!("persisted upscaler '{spec}' is invalid: {error}"),
            }
        }
        world.insert_resource(settings);
        world.insert_resource(settings_persistence);
        interaction::sync_registered_settings(&world);

        Self {
            window: None,
            renderer: None,
            renderer_config,
            world,
            scheduler,
            last_frame: Instant::now(),
            ui_manager: None,
            ui_input_state: ui_input::UiInputState::default(),
            ui_texture_handle: None,
            ui_reported_host_methods: std::collections::HashSet::new(),
            ui_reported_host_methods_capped: false,
            draw_commands: Vec::new(),
            water_commands: Vec::new(),
            gpu_lights: Vec::new(),
            gpu_fog_volumes: Vec::new(),
            light_sort_scratch: Vec::new(),
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
            skin_offsets: rustc_hash::FxHashMap::default(),
            material_table: byroredux_renderer::MaterialTable::new(),
            bench_mode: None,
            bench_frames_target: None,
            bench_hold: false,
            bench_summary_printed: false,
            bench_frames_count: 0,
            bench_camera: None,
            bench_camera_origin: None,
            bench_camera_applied_pose: None,
            bench_camera_subject_distance: None,
            bench_camera_path_frame: 0,
            pending_soak_cycle: None,
            bench_start: None,
            bench_systems_ns: 0,
            bench_systems_ticks: 0,
            bench_build_render_ns: 0,
            bench_ui_ns: 0,
            bench_render_ns: 0,
            bench_frame_timings: byroredux_renderer::FrameTimings::default(),
            bench_cpu_frame_ms: Vec::new(),
            screenshot_path: None,
            camera_pos_override: None,
            camera_forward_override: None,
            screenshot_requested: false,
            screenshot_deadline_frames: 0,
            streaming: None,
            #[cfg(feature = "debug-server")]
            debug_server,
            debug_ui: None,
            pending_player_messages,
            debug_ui_refresh_entities: false,
            in_use_mesh_scratch: std::collections::HashSet::new(),
            in_use_tex_scratch: std::collections::HashSet::new(),
            last_redraw_end: None,
        }
    }

    fn debug_server_endpoint(&self) -> Option<String> {
        #[cfg(feature = "debug-server")]
        {
            self.debug_server
                .as_ref()
                .map(|server| server.local_addr().to_string())
        }
        #[cfg(not(feature = "debug-server"))]
        {
            None
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

    /// Route native window input to the focused Scaleform menu before world
    /// controls. F3 remains an engine-global debug-overlay binding.
    fn route_scaleform_window_event(&mut self, event: &WindowEvent) -> bool {
        // #2973 — refresh the cached modifier set and cursor position BEFORE
        // any early return. `dispatch_window_event` below is the only other
        // writer and it sits behind the focus gate, so without this a modifier
        // pressed or released while the menu is unfocused is never observed.
        // Cache only: nothing is dispatched to the menu from here.
        ui_input::cache_window_state(event, &mut self.ui_input_state);

        if ui_input::is_debug_overlay_key(event) {
            return false;
        }

        let window_size = self
            .window
            .as_ref()
            .map(Window::inner_size)
            .unwrap_or_default();
        let Some(ui_manager) = self.ui_manager.as_mut() else {
            return false;
        };
        if !ui_manager.has_input_focus() {
            return false;
        }

        let ui_size = (ui_manager.width, ui_manager.height);
        let dispatch = ui_input::dispatch_window_event(
            event,
            &mut self.ui_input_state,
            window_size,
            ui_size,
            |event| {
                // Focus ownership already guarantees modal capture. The
                // per-listener handled result is intentionally not used to
                // decide whether world input receives this event.
                ui_manager.handle_input(event);
            },
        );
        if let Some(is_in_stage) = dispatch.mouse_in_stage {
            ui_manager.set_mouse_in_stage(is_in_stage);
        }
        if dispatch.captured {
            self.release_world_input_for_ui();
        }
        dispatch.captured
    }

    /// Clear held gameplay input when a modal menu takes ownership so a key
    /// pressed before the transfer cannot leave the camera moving underneath
    /// the menu.
    fn release_world_input_for_ui(&mut self) {
        let had_mouse_capture = ui_input::release_world_input(&self.world);

        if had_mouse_capture {
            if let Some(window) = self.window.as_ref() {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                window.set_cursor_visible(true);
            }
        }
    }

    /// Return mouse look to gameplay after a native modal closes.
    fn capture_world_input(&mut self) {
        if self
            .ui_manager
            .as_ref()
            .is_some_and(UiManager::has_input_focus)
        {
            self.release_world_input_for_ui();
            return;
        }
        self.world.resource_mut::<InputState>().mouse_captured = true;
        if let Some(window) = self.window.as_ref() {
            let _ = window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked));
            window.set_cursor_visible(false);
        }
    }

    fn toggle_game_menu(&mut self) {
        let opened = self
            .debug_ui
            .as_mut()
            .is_some_and(byroredux_debug_ui::DebugUiState::toggle_game_menu);
        if opened {
            self.release_world_input_for_ui();
        } else {
            self.capture_world_input();
        }
    }

    fn open_inventory_menu(&mut self) {
        if let Some(ui) = self.debug_ui.as_mut() {
            ui.open_inventory_menu();
            self.release_world_input_for_ui();
        }
    }

    fn resume_from_game_menu(&mut self) {
        if let Some(ui) = self.debug_ui.as_mut() {
            ui.close_game_menu();
        }
        self.capture_world_input();
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

/// Phase 4b — build the per-frame [`PanelSnapshot`] the egui
/// overlay reads. Always populates `metrics`; the entity list is
/// rebuilt only when the Entities panel asked to refresh (avoids
/// walking the Name component query every frame for an overlay
/// that's hidden most of the time).
fn build_debug_ui_snapshot(
    world: &World,
    refresh_entities: bool,
    include_inventory: bool,
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

    let settings = world
        .try_resource::<SettingsRegistry>()
        .map(|registry| registry.entries().cloned().collect())
        .unwrap_or_default();

    byroredux_debug_ui::PanelSnapshot {
        interaction_prompt: build_interaction_prompt(world),
        show_crosshair: setting_bool(world, byroredux_debug_ui::SHOW_CROSSHAIR_SETTING_ID, true),
        show_prompts: setting_bool(world, byroredux_debug_ui::SHOW_PROMPTS_SETTING_ID, true),
        metrics,
        settings,
        inventory: include_inventory
            .then(|| inventory::snapshot(world))
            .flatten(),
        entities,
    }
}

fn build_interaction_prompt(world: &World) -> Option<byroredux_debug_ui::InteractionPrompt> {
    let verb = world
        .try_resource::<crate::interaction::InteractionState>()
        .and_then(|state| state.prompt_verb())?;
    let binding = world
        .try_resource::<crate::interaction::ActionBindings>()
        .map(|bindings| bindings.binding_label(crate::interaction::InputAction::Activate))
        .unwrap_or("E");
    Some(byroredux_debug_ui::InteractionPrompt { binding, verb })
}

fn surface_save_load_output(
    debug_ui: Option<&mut byroredux_debug_ui::DebugUiState>,
    context: &str,
    output: byroredux_core::console::CommandOutput,
) {
    let joined = output.lines.join(" | ");
    if save_io::command_output_is_failure(&output) {
        log::warn!("{context}: {joined}");
    } else {
        log::info!("{context}: {joined}");
    }
    if let Some(ui) = debug_ui {
        let mut lines = output.lines.into_iter();
        if let Some(first) = lines.next() {
            ui.push_player_message(first);
        }
        for line in lines {
            ui.push_console_line(line);
        }
    }
}

fn setting_bool(world: &World, id: &str, fallback: bool) -> bool {
    world
        .try_resource::<SettingsRegistry>()
        .and_then(|registry| registry.get(id).map(|entry| entry.value.clone()))
        .and_then(|value| match value {
            SettingValue::Bool(value) => Some(value),
            _ => None,
        })
        .unwrap_or(fallback)
}

/// Apply the [`PanelOutputs`] the overlay produced back to the world. Queued
/// loads use the debug server's `PendingDebugLoadSlot`, setting changes are
/// validated by the universal registry, and console expressions dispatch
/// through the shared `CommandRegistry`. Refresh requests latch for the next
/// snapshot. Console responses are appended to the overlay scrollback.
fn apply_debug_ui_outputs(
    world: &mut World,
    outputs: byroredux_debug_ui::PanelOutputs,
    refresh_entities_flag: &mut bool,
    debug_ui: Option<&mut byroredux_debug_ui::DebugUiState>,
) -> (bool, bool) {
    let mut debug_ui = debug_ui;
    let resume_game = outputs.resume_game;
    let quit_game = outputs.quit_game;
    if outputs.quicksave {
        let output = save_io::quicksave(world);
        surface_save_load_output(debug_ui.as_deref_mut(), "pause menu quicksave", output);
    }
    if outputs.quickload {
        let output = save_io::quickload_latest(world);
        surface_save_load_output(debug_ui.as_deref_mut(), "pause menu quickload", output);
    }
    let mut settings_changed = false;
    for action in outputs.inventory_actions {
        if inventory::apply_action(world, action) == inventory::MutationResult::Unavailable {
            log::warn!("native inventory action was unavailable for the current player/item");
        }
    }
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
    for change in outputs.setting_changes {
        let result = {
            let mut settings = world.resource_mut::<SettingsRegistry>();
            settings.set(&change.id, change.value.clone())
        };
        match result {
            Ok(changed) => {
                settings_changed |= changed;
                if let Some(ui) = debug_ui.as_deref_mut() {
                    ui.apply_setting_change(&change);
                }
                if let Some(companion) = interaction::apply_control_setting(world, &change) {
                    let companion_changed = world
                        .resource_mut::<SettingsRegistry>()
                        .set(&companion.id, companion.value)
                        .unwrap_or_else(|error| {
                            log::warn!("rejected companion binding change: {error}");
                            false
                        });
                    settings_changed |= companion_changed;
                }
                apply_camera_setting(world, &change);
                // The upscaler entry cannot be applied here: switching
                // rebuilds every render-resolution target and needs
                // `&mut VulkanContext`. Stage it for the frame boundary,
                // where `step_upscaler_switch` drains it.
                if change.id == byroredux_debug_ui::UPSCALER_SETTING_ID {
                    if let byroredux_core::settings::SettingValue::Choice(ref spec) = change.value {
                        world
                            .resource_mut::<byroredux_core::ecs::PendingUpscalerSwitch>()
                            .request(spec.clone());
                    }
                }
                log::info!(
                    "universal setting changed: {} = {:?}",
                    change.id,
                    change.value
                );
            }
            Err(error) => {
                log::warn!("rejected universal setting change: {error}");
            }
        }
    }
    if settings_changed {
        let persistence = world.resource::<settings_io::SettingsPersistence>().clone();
        let settings = world.resource::<SettingsRegistry>();
        settings_io::save(&settings, &persistence);
    }
    if outputs.console_evals.is_empty() {
        return (resume_game, quit_game);
    }
    // Collect responses first, then push into the overlay's
    // scrollback. Splitting the two phases keeps the `&World`
    // borrow CommandRegistry needs cleanly disjoint from the
    // `&mut DebugUiState` borrow `push_console_line` needs.
    let mut response_lines: Vec<String> = Vec::new();
    for expr in outputs.console_evals {
        // CONC-D3-04 / #1786 — `reg` stays held (read) across `execute`;
        // see the lock contract on `ConsoleCommand::execute`.
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
    (resume_game, quit_game)
}

fn apply_camera_setting(world: &World, change: &SettingChange) {
    if change.id != byroredux_debug_ui::FOV_SETTING_ID {
        return;
    }
    let SettingValue::Number(degrees) = &change.value else {
        return;
    };
    let Some(active) = world.try_resource::<byroredux_core::ecs::ActiveCamera>() else {
        return;
    };
    let entity = active.0;
    drop(active);
    if let Some(mut cameras) = world.query_mut::<byroredux_core::ecs::Camera>() {
        if let Some(camera) = cameras.get_mut(entity) {
            camera.fov_y = degrees.to_radians();
        }
    }
}

fn sync_camera_setting(world: &World) {
    let change = world
        .resource::<SettingsRegistry>()
        .get(byroredux_debug_ui::FOV_SETTING_ID)
        .map(|entry| SettingChange::new(&entry.id, entry.value.clone()));
    if let Some(change) = change {
        apply_camera_setting(world, &change);
    }
}
