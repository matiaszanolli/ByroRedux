//! ByroRedux — ECS-driven game loop with Vulkan rendering.

mod anim_convert;
mod asset_provider;
mod cell_loader;
mod commands;
mod components;
mod helpers;
mod npc_spawn;
mod render;
mod scene;
mod streaming;
mod systems;

use anyhow::Result;
use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::console::CommandRegistry;
use byroredux_core::ecs::{
    Access, ActiveCamera, Camera, DebugStats, DeltaTime, EngineConfig, Scheduler, ScratchTelemetry,
    Stage, TotalTime, Transform, World,
};
use byroredux_core::string::StringPool;
use byroredux_platform::window::{self, WindowConfig};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::VulkanContext;
use byroredux_ui::UiManager;
use std::collections::HashMap;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::commands::build_command_registry;
use crate::components::{InputState, NameIndex, Spinning, SubtreeCache};
use crate::helpers::world_resource_set;
use crate::render::build_render_data;
use crate::systems::{
    animation_system, billboard_system, fly_camera_system, log_stats_system,
    make_transform_propagation_system, make_world_bound_propagation_system, particle_system,
    spin_system, weather_system,
};
use byroredux_core::ecs::SystemList;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
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

    // --camera-pos x,y,z + --camera-forward x,y,z — override the
    // auto-computed initial camera pose. Useful for capturing specific
    // framing in bench mode without needing interactive WASD input.
    // Both args are `,`-separated floats. Pass one or both; missing
    // forward defaults to `-Z` toward the origin.
    let camera_pos = parse_vec3_arg(&args, "--camera-pos");
    let camera_forward = parse_vec3_arg(&args, "--camera-forward");

    // Set up logging. --debug forces debug level.
    if debug_mode {
        std::env::set_var(
            "RUST_LOG",
            std::env::var("RUST_LOG").unwrap_or("debug".into()),
        );
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("ByroRedux starting");
    log::info!("{}", byroredux_cxx_bridge::ffi::native_hello());

    // Headless --cmd mode: execute command and exit without creating a window.
    if let Some(cmd_idx) = args.iter().position(|a| a == "--cmd") {
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

    let mut app = App::new(debug_mode);
    app.bench_frames_target = bench_frames;
    app.screenshot_path = screenshot_path;
    app.camera_pos_override = camera_pos;
    app.camera_forward_override = camera_forward;
    event_loop.run_app(&mut app)?;

    Ok(())
}

fn parse_string_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

/// Parse `x,y,z` into a `(f32, f32, f32)` tuple — stored as a plain
/// triple here to avoid leaking the renderer's `Vec3` into main.rs.
fn parse_vec3_arg(args: &[String], flag: &str) -> Option<(f32, f32, f32)> {
    let s = parse_string_arg(args, flag)?;
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.as_slice() {
        [x, y, z] => Some((*x, *y, *z)),
        _ => {
            log::warn!("`{flag} {s}` could not be parsed as x,y,z floats; ignoring");
            None
        }
    }
}

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
    /// Reusable per-frame light buffer (cleared each frame, allocation retained).
    gpu_lights: Vec<byroredux_renderer::GpuLight>,
    /// Reusable per-frame bone palette (column-major mat4 entries; slot 0
    /// always identity). Walked by `build_render_data` for every
    /// SkinnedMesh entity and uploaded once per frame.
    bone_palette: Vec<[[f32; 4]; 4]>,
    /// Reusable per-frame entity → bone-offset map. Populated by the
    /// skinned-mesh pass in `build_render_data` and read during draw
    /// command emission. Retained across frames so the HashMap's bucket
    /// allocation persists — see #253.
    skin_offsets: HashMap<byroredux_core::ecs::EntityId, u32>,
    /// Per-SkinnedMesh scratch buffer — `build_render_data` clears it
    /// once per skinned entity and asks `SkinnedMesh::compute_palette_into`
    /// to refill. Hoisted onto the App struct so the Vec's capacity
    /// persists across frames rather than re-growing from zero each
    /// call. Typical capacity ~MAX_BONES_PER_MESH × 64 B ≈ 8 KB
    /// (small, but the fresh allocation at the top of every frame was
    /// observable in profilers). See #509 (PERF-2026-04-20 D6-L1) and
    /// #243 / #253 for the other scratch hoists this follows.
    palette_scratch: Vec<byroredux_core::math::Mat4>,
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
}

impl App {
    fn new(debug_mode: bool) -> Self {
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
        world.insert_resource(InputState::default());
        world.insert_resource(StringPool::new());
        world.insert_resource(AnimationClipRegistry::new());
        world.insert_resource(NameIndex::new());
        world.insert_resource(SubtreeCache::new());
        world.insert_resource(byroredux_physics::PhysicsWorld::new());
        // M44 Phase 1 — audio world. Init failure (no audio device,
        // CI, headless server) leaves the inner `AudioManager` as
        // `None` and every subsequent audio operation no-ops. Boot
        // never fails on a missing audio device.
        world.insert_resource(byroredux_audio::AudioWorld::new());
        // Process-lifetime cache of parsed-and-imported NIF scenes.
        // Persists across cell transitions so repeat visits don't re-
        // parse every clutter mesh. See #381.
        world.insert_resource(crate::cell_loader::NifImportRegistry::new());

        // Pre-register component storages that the physics sync system
        // queries on the first frame (before anything has been inserted).
        world.register::<byroredux_physics::RapierHandles>();

        // Register scripting component storages.
        byroredux_scripting::register(&mut world);

        // Build the system schedule — stages run sequentially, systems
        // within each stage run in parallel via rayon. Three systems
        // here demonstrate the R7 declared-access pattern via
        // `add_to_with_access`; the rest stay undeclared (closures and
        // function items can't impl `System::access`, so they appear
        // as "Unknown" in the conflict report until migrated). Drive
        // further migrations off the `sys.accesses` console output.
        let mut scheduler = Scheduler::new();
        scheduler.add_to_with_access(
            Stage::Early,
            fly_camera_system,
            Access::new()
                .reads_resource::<ActiveCamera>()
                .reads_resource::<InputState>()
                .reads::<byroredux_physics::RapierHandles>()
                .writes::<Transform>()
                .writes_resource::<byroredux_physics::PhysicsWorld>(),
        );
        scheduler.add_to(Stage::Early, weather_system);
        scheduler.add_to(Stage::Early, byroredux_scripting::timer_tick_system);
        scheduler.add_to(Stage::Update, animation_system);
        scheduler.add_to_with_access(
            Stage::Update,
            spin_system,
            Access::new().reads::<Spinning>().writes::<Transform>(),
        );
        scheduler.add_to(Stage::PostUpdate, make_transform_propagation_system());
        // Particle simulation runs after transform propagation so emitter
        // entities have their final world-space spawn origin (#401).
        scheduler.add_exclusive(Stage::PostUpdate, particle_system);
        // Billboards must run AFTER transform propagation so they can
        // overwrite the computed world rotation. Registered as exclusive
        // so the scheduler sequences it after the PostUpdate parallel
        // batch. See issue #225.
        scheduler.add_exclusive(Stage::PostUpdate, billboard_system);
        // Bound propagation runs last in PostUpdate so it sees final
        // world transforms (including billboard rotations). See #217.
        scheduler.add_exclusive(Stage::PostUpdate, make_world_bound_propagation_system());
        scheduler.add_to(Stage::Physics, byroredux_physics::physics_sync_system);
        // M44 Phase 1 — audio update runs in Stage::Late so it sees
        // final world transforms after propagation. The Phase 1 body
        // is a stub (see byroredux_audio::audio_system); future
        // phases (one-shot dispatch, listener pose sync, looping
        // emitter lifecycle) flesh it out without touching the
        // schedule wiring.
        scheduler.add_to(Stage::Late, byroredux_audio::audio_system);
        scheduler.add_to_with_access(
            Stage::Late,
            log_stats_system,
            Access::new()
                .reads_resource::<TotalTime>()
                .reads_resource::<DeltaTime>()
                .reads_resource::<DebugStats>(),
        );
        scheduler.add_exclusive(Stage::Late, byroredux_scripting::event_cleanup_system);

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
        world.insert_resource(byroredux_core::ecs::SchedulerAccessReport(
            scheduler.access_report(),
        ));
        world.insert_resource(build_command_registry());

        // Start debug server (feature-gated, zero cost when disabled).
        #[cfg(feature = "debug-server")]
        {
            let debug_port: u16 = std::env::var("BYRO_DEBUG_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(9876);
            byroredux_debug_server::start(&mut world, &mut scheduler, debug_port);
        }

        Self {
            window: None,
            renderer: None,
            world,
            scheduler,
            last_frame: Instant::now(),
            ui_manager: None,
            ui_texture_handle: None,
            draw_commands: Vec::new(),
            gpu_lights: Vec::new(),
            bone_palette: Vec::new(),
            skin_offsets: HashMap::new(),
            palette_scratch: Vec::new(),
            material_table: byroredux_renderer::MaterialTable::new(),
            bench_frames_target: None,
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
        for coord in deltas.to_unload {
            if let Some(slot) = state.loaded.remove(&coord) {
                cell_loader::unload_cell(&mut self.world, ctx, slot.cell_root);
                log::info!(
                    "Unloaded cell ({},{}) (root {})",
                    coord.0,
                    coord.1,
                    slot.cell_root
                );
            }
            // If a load was in flight for this cell, leave the
            // pending entry; the drain step compares generation and
            // drops the stale payload when it eventually arrives.
        }

        // Dispatch new loads — non-blocking send, worker picks them up
        // off-thread.
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
            };
            if state.request_tx.send(req).is_err() {
                log::error!(
                    "Streaming worker channel closed; cell ({},{}) cannot be loaded",
                    gx,
                    gy
                );
                state.pending.remove(&(gx, gy));
            }
        }
    }

}

/// Apply a single worker-pre-parsed [`streaming::LoadCellPayload`]:
/// stale-generation gate, finish-import every entry into the NIF
/// cache, then synchronously call
/// [`cell_loader::load_one_exterior_cell`] (which now hits cache for
/// every NIF — the slow parse path is skipped).
///
/// Free function (not an `App` method) so the caller can split-borrow
/// `&mut self.world` / `&mut self.streaming.as_mut().unwrap()` /
/// `&mut self.renderer.as_mut().unwrap()` without aliasing — `App`
/// method signatures take `&mut self` whole, which conflicts with the
/// drain loop's `&mut self.renderer` borrow.
fn consume_streaming_payload(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    state: &mut streaming::WorldStreamingState,
    payload: streaming::LoadCellPayload,
) {
    let coord = (payload.gx, payload.gy);
    // Stale-load gate via the testable `classify_payload` helper.
    match streaming::classify_payload(&state.pending, coord, payload.generation) {
        streaming::PayloadDecision::Apply => {
            state.pending.remove(&coord);
        }
        streaming::PayloadDecision::StaleNewerPending { .. }
        | streaming::PayloadDecision::StaleNoPending => {
            log::debug!(
                "Dropping stale streaming payload ({},{}) gen={}",
                payload.gx,
                payload.gy,
                payload.generation
            );
            return;
        }
    }

    // Finish-import every pre-parsed entry into the cache. Subsequent
    // load_one_exterior_cell calls now hit cache for every NIF.
    let wctx = state.wctx.clone();
    for (model_path, partial_opt) in payload.parsed {
        match partial_opt {
            Some(partial) => {
                cell_loader::finish_partial_import(
                    world,
                    Some(&mut state.mat_provider),
                    Some(state.tex_provider.as_ref()),
                    &model_path,
                    partial,
                );
            }
            None => {
                let cache_key = model_path.to_ascii_lowercase();
                let mut reg = world.resource_mut::<cell_loader::NifImportRegistry>();
                reg.insert(cache_key, None);
            }
        }
    }

    // Spawn pass — every NIF lookup hits cache (slow parse path skipped).
    match cell_loader::load_one_exterior_cell(
        wctx.as_ref(),
        payload.gx,
        payload.gy,
        world,
        ctx,
        state.tex_provider.as_ref(),
        Some(&mut state.mat_provider),
        None,
    ) {
        Ok(Some(info)) => {
            state.loaded.insert(
                coord,
                streaming::LoadedCell {
                    cell_root: info.cell_root,
                },
            );
        }
        Ok(None) => {
            // Worldspace hole — common at edges; pending entry already
            // cleared above.
        }
        Err(e) => {
            log::warn!(
                "Streaming cell ({},{}) spawn failed after pre-parse: {:#}",
                payload.gx,
                payload.gy,
                e
            );
        }
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

                self.renderer = Some(ctx);
                self.window = Some(win);
                self.last_frame = Instant::now();
                self.setup_scene();
                // M41.0 Phase 1b.x — Prime the scene's transform state
                // BEFORE the event loop starts. winit fires the initial
                // `WindowEvent::RedrawRequested` for the first paint
                // *before* the first `about_to_wait` tick, so without
                // this prime the renderer's first `build_render_data`
                // reads every freshly-spawned entity's `GlobalTransform`
                // at its `IDENTITY` default — for skinned meshes that
                // means the bone palette is computed as `IDENTITY ×
                // bind_inverse` and the body's vertices get yanked toward
                // world-origin, producing a one-frame stretched-cone
                // artifact that's brief but visible. Running the
                // scheduler once here drives the propagation system
                // through every spawned subtree (placement_root → skel /
                // body / head NIF chains) so frame 0 has the same valid
                // GTs every subsequent frame will. Sibling fix to the
                // explicit `GlobalTransform::new(ref_pos, ...)` in
                // `npc_spawn::spawn_npc_entity` — that pre-seeds the
                // placement root, this propagates the seed.
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
                // worker thread cleanly via the request_tx Drop chain
                // before we tear down the GPU. Without this, the
                // worker could still be holding `Arc<TextureProvider>`
                // file handles that the allocator's leak path would
                // observe as outstanding refs.
                self.streaming.take();
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
                if let Some(ref mut ctx) = self.renderer {
                    let is_benching = self.bench_frames_target.is_some();

                    let brd_t0 = Instant::now();
                    let (view_proj, camera_pos, ambient, fog_color, fog_near, fog_far, sky_params) =
                        build_render_data(
                            &self.world,
                            &mut self.draw_commands,
                            &mut self.gpu_lights,
                            &mut self.bone_palette,
                            &mut self.skin_offsets,
                            &mut self.palette_scratch,
                            &mut self.material_table,
                            ctx.particle_quad_handle,
                        );
                    if is_benching {
                        self.bench_build_render_ns += brd_t0.elapsed().as_nanos() as u64;
                    }

                    // #780 / PERF-N1 — snapshot R1 dedup metrics for
                    // the frame we just built so `ctx.scratch` can
                    // surface them. Cheap (two usize writes) and
                    // capturing here means the values reflect the
                    // exact state visible to the SSBO upload below.
                    {
                        let mut tlm = self.world.resource_mut::<ScratchTelemetry>();
                        tlm.materials_unique = self.material_table.len();
                        tlm.materials_interned = self.material_table.interned_count();
                    }

                    // Rebuild the global geometry SSBO if new meshes were
                    // loaded since the last build (cell transitions, late
                    // streaming). See #258.
                    if ctx.mesh_registry.is_geometry_dirty() {
                        if let Err(e) = ctx.mesh_registry.rebuild_geometry_ssbo(
                            &ctx.device,
                            ctx.allocator.as_ref().unwrap(),
                            &ctx.graphics_queue,
                            ctx.transfer_pool,
                            None, // TODO: thread StagingPool through frame loop (#242)
                        ) {
                            log::warn!("Failed to rebuild geometry SSBO: {e}");
                        }
                    }

                    // Record draw call count for diagnostics.
                    world_resource_set::<DebugStats>(&self.world, |s| {
                        s.draw_call_count = self.draw_commands.len() as u32;
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
                                if let Err(e) = ctx.texture_registry.update_rgba(
                                    &ctx.device,
                                    allocator,
                                    &ctx.graphics_queue,
                                    ctx.transfer_pool,
                                    handle,
                                    ui_w,
                                    ui_h,
                                    pixels,
                                ) {
                                    log::error!("UI texture update failed: {e:#}");
                                }
                                ui_tex = Some(handle);
                            }
                        } else if self.ui_texture_handle.is_some() {
                            // Not dirty, but still draw the last frame.
                            ui_tex = self.ui_texture_handle;
                        }
                    }
                    if is_benching {
                        self.bench_ui_ns += ui_t0.elapsed().as_nanos() as u64;
                    }

                    // Clear color — black for interior cells (any gap
                    // in wall geometry reveals this pixel), cornflower
                    // blue for no-cell/default mode so a raw engine
                    // launch still has the test-pattern backdrop.
                    // Exterior cells ignore this entirely: composite.frag
                    // replaces depth=far pixels with the sky gradient.
                    let is_interior = self
                        .world
                        .try_resource::<crate::components::CellLightingRes>()
                        .map_or(false, |l| l.is_interior);
                    let clear_color = if is_interior {
                        [0.0, 0.0, 0.0, 1.0]
                    } else {
                        byroredux_core::types::Color::CORNFLOWER_BLUE.as_array()
                    };
                    let render_t0 = Instant::now();
                    let mut frame_timings = if is_benching {
                        Some(byroredux_renderer::FrameTimings::default())
                    } else {
                        None
                    };
                    match ctx.draw_frame(
                        clear_color,
                        &view_proj,
                        &self.draw_commands,
                        &self.gpu_lights,
                        &self.bone_palette,
                        self.material_table.materials(),
                        camera_pos,
                        ambient,
                        fog_color,
                        fog_near,
                        fog_far,
                        ui_tex,
                        &sky_params,
                        frame_timings.as_mut(),
                    ) {
                        Ok(needs_recreate) => {
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
                                        if let Err(e) =
                                            ctx.recreate_swapchain([size.width, size.height])
                                        {
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
                }
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
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        // Update time resources.
        world_resource_set::<DeltaTime>(&self.world, |r| r.0 = dt);
        world_resource_set::<TotalTime>(&self.world, |r| r.0 += dt);

        // Update debug stats.
        {
            let mut stats = self.world.resource_mut::<DebugStats>();
            stats.push_frame_time(dt);
            stats.entity_count = self.world.next_entity_id();
            if let Some(ref ctx) = self.renderer {
                stats.mesh_count = ctx.mesh_registry.len() as u32;
                stats.texture_count = ctx.texture_registry.len() as u32;
            }
        }

        // Refresh renderer-side scratch-Vec telemetry (R6). Reuses the
        // resource's `rows` Vec so this is amortized to ~zero allocs
        // after the first frame; capacity stabilises at the count of
        // declared scratches in `VulkanContext::fill_scratch_telemetry`.
        if let Some(ref ctx) = self.renderer {
            let mut tlm = self.world.resource_mut::<ScratchTelemetry>();
            ctx.fill_scratch_telemetry(&mut tlm.rows);
        }

        // Run all systems.
        let systems_t0 = Instant::now();
        self.scheduler.run(&self.world, dt);
        if self.bench_frames_target.is_some() && self.renderer.is_some() {
            self.bench_systems_ns += systems_t0.elapsed().as_nanos() as u64;
            self.bench_systems_ticks += 1;
        }

        // World cell streaming (M40 Phase 1a). Runs after the
        // scheduler so the scheduler-driven `fly_camera_system` has
        // already published the player's current Transform translation
        // for this frame. No-ops outside `--esm + --grid` exterior
        // mode and when the player hasn't crossed a boundary.
        self.step_streaming();

        // Update window title with stats (throttled: every 16 frames ≈ 4×/sec at 60fps).
        let config_debug = self.world.resource::<EngineConfig>().debug_logging;
        if config_debug {
            let stats = self.world.resource::<DebugStats>();
            if stats.frame_index().is_multiple_of(16) {
                if let Some(ref win) = self.window {
                    win.set_title(&format!(
                        "ByroRedux | {:.0} FPS | {:.1}ms | {} entities | {} meshes | {} textures | {} draws",
                        stats.avg_fps(), stats.frame_time_ms,
                        stats.entity_count, stats.mesh_count, stats.texture_count, stats.draw_call_count,
                    ));
                }
            }
        }

        if let Some(ref win) = self.window {
            win.request_redraw();
        }

        // --bench-frames: once we've rendered the target number of
        // frames, emit a single machine-readable summary line and exit.
        // The renderer must be up (bench counts start after the first
        // real frame); a `--bench-frames N` that never renders (window
        // creation fails, etc.) does nothing here.
        if let Some(target) = self.bench_frames_target {
            if self.renderer.is_some() {
                if self.bench_frames_count >= target {
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
                    println!(
                        "bench: frames={} wall_fps={:.1} wall_ms={:.2} \
                         brd_ms={:.2} ui_ms={:.2} draw_ms={:.2} \
                         [fence={:.2} tlas={:.2} ssbo={:.2} cmd={:.2} submit={:.2}] \
                         systems_ms={:.2} ticks_per_frame={:.1} unaccounted_ms={:.2} \
                         entities={} meshes={} textures={} draws={}",
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
                        systems_ms,
                        ticks_per_frame,
                        unaccounted_ms,
                        stats.entity_count,
                        stats.mesh_count,
                        stats.texture_count,
                        stats.draw_call_count,
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
                                bridge
                                    .requested
                                    .store(true, std::sync::atomic::Ordering::Release);
                                drop(bridge);
                                self.screenshot_requested = true;
                                self.screenshot_deadline_frames = 60;
                                return; // keep running frames
                            }
                        }

                        // Poll the result slot until the PNG arrives.
                        let maybe_bytes = self
                            .world
                            .try_resource::<byroredux_core::ecs::ScreenshotBridge>()
                            .and_then(|b| b.result.lock().unwrap().take());
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

                    event_loop.exit();
                }
            }
        }
    }
}
