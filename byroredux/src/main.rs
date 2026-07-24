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
mod app_step;
mod asset_provider;
mod boot;
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
mod ragdoll;
mod render;
mod save_io;
mod scene;
mod scene_import_cache;
#[cfg(test)]
mod scheduler_access_tests;
mod sf_smoke;
mod streaming;
mod streaming_helpers;
mod systems;

use anyhow::Result;
use byroredux_core::console::CommandRegistry;
use byroredux_core::ecs::{
    ActiveCamera, Camera, DebugStats, DeltaTime, EngineConfig, Scheduler, ScratchTelemetry,
    SkinCoverageStats, TotalTime, World,
};
use byroredux_core::settings::SettingsRegistry;
use byroredux_core::string::StringPool;
use byroredux_platform::window::{self, WindowConfig};
use byroredux_renderer::vulkan::context::{DrawCommand, FrameInputs};
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{RendererConfig, VulkanContext};
use byroredux_ui::UiManager;
use std::collections::HashMap;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::components::InputState;
use crate::helpers::world_resource_set;
use crate::render::build_render_data;
use crate::systems::{compute_underwater_params, toggle_player_mode};

fn main() -> Result<()> {
    boot::run()
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
    fn new(debug_mode: bool, args: &[String], renderer_config: RendererConfig) -> Self {
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
            Some(byroredux_debug_server::start(&mut scheduler, debug_port))
        };

        boot::install_runtime_registries(&mut world, &scheduler);

        // Universal settings live in core and are presented by the on-screen
        // overlay. Subsystems can register additional entries here without
        // teaching the Settings tab about renderer/game-specific resources.
        let mut settings = SettingsRegistry::default();
        byroredux_debug_ui::register_builtin_settings(&mut settings)
            .expect("debug-UI built-in settings must be valid and unique");
        world.insert_resource(settings);

        Self {
            window: None,
            renderer: None,
            renderer_config,
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
            in_use_mesh_scratch: std::collections::HashSet::new(),
            in_use_tex_scratch: std::collections::HashSet::new(),
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
            // #1791 / D6-01 — mirror of `pending_with_data`'s (slot, entity)
            // pairs, kept alive so a `draw_frame` early return (see the
            // `skin_dispatch_ran` check below) can requeue exactly what was
            // about to be uploaded. Deliberately NOT the raw `pending` drain:
            // an entry filtered out here (its `SkinnedMesh` is already gone)
            // must stay dropped, not come back through the requeue path.
            let mut pending_for_requeue: Vec<(u32, byroredux_core::ecs::EntityId)> =
                Vec::with_capacity(pending.len());
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
                            pending_for_requeue.push((slot, entity));
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
                camera_near: frame.camera_near,
                camera_far: frame.camera_far,
                camera_fov_y: frame.camera_fov_y,
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
            let frame_time_delta_ms = self
                .world
                .try_resource::<DeltaTime>()
                .map_or(1000.0 / 60.0, |delta| delta.0 * 1000.0);
            match ctx.draw_frame(FrameInputs {
                clear_color,
                view_proj: &frame.view_proj,
                draw_commands: &self.draw_commands,
                lights: &self.gpu_lights,
                bone_world: &self.bone_world,
                bind_inverse_pending_uploads: &pending_with_data,
                materials: self.material_table.materials(),
                camera_pos: frame.camera_pos,
                render_origin: frame.render_origin,
                ambient_color: frame.ambient,
                fog_color: frame.fog_color,
                fog_near: frame.fog_near,
                fog_far: frame.fog_far,
                fog_clip: frame.fog_clip,
                fog_power: frame.fog_power,
                ui_texture_handle: ui_tex,
                sky_params: &frame.sky,
                dof,
                frame_time_delta_ms,
                timings: frame_timings.as_mut(),
                water_commands: &self.water_commands,
                underwater: compute_underwater_params(&self.world),
                pose_dirty: self.skin_slot_pool.pose_dirty(),
            }) {
                Ok(needs_recreate) => {
                    // #1796 / D6-02 — `draw_frame`'s two early-return guards
                    // (empty framebuffers, `ERROR_OUT_OF_DATE_KHR`) return
                    // through this same `Ok` arm, indistinguishable from a
                    // frame that actually reached the skin dispatch section.
                    // The CPU-side pose hash commit already ran (in
                    // `build_render_data`, before `ctx.draw_frame` was
                    // called), so an early return here means that commit
                    // needs undoing or the next frame's dirty gate reads
                    // "clean" against a dispatch that never happened.
                    //
                    // #1791 / D6-01 — the same two early-return guards also
                    // precede the `bind_inverses` SSBO upload (draw.rs
                    // ~2654-2676), which sits strictly before the
                    // `record_skinned_blas_refit` call that flips
                    // `skin_dispatch_ran` true — so this flag is exactly the
                    // right signal for both bugs. `pending` was already
                    // irrevocably drained from the pool above (before this
                    // call), so an early return here means those first-sight
                    // `bind_inverses` were about to be lost for good: the
                    // slot stays resident in `entity_to_slot` (never
                    // re-queued by `allocate`), so the persistent SSBO
                    // region for it is never written, corrupting the
                    // entity's skinning palette for its remaining lifetime
                    // in the cell.
                    if !ctx.skin_dispatch_ran {
                        self.skin_slot_pool.rollback_pending_pose_commits();
                        self.skin_slot_pool
                            .requeue_pending(std::mem::take(&mut pending_for_requeue));
                    }
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

        match VulkanContext::new(
            display,
            window_handle,
            [size.width, size.height],
            self.renderer_config,
        ) {
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
                        // #1603 — shared capture generation; the renderer
                        // gates each readback's publish on it so a
                        // cancelled-then-resumed straggler is discarded.
                        generation: ss_handle.generation,
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
                debug_ui_state.sync_registered_settings(&self.world.resource::<SettingsRegistry>());
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
        //
        // PERF-D1-NEW-01 / #1801 — this walk used to run unconditionally
        // every frame, but both consumers (the `stats` console command
        // and the debug-server entity evaluator) are on-demand, not
        // per-frame; `log_stats_system` doesn't print these fields
        // either. Throttled to the same once-per-wall-clock-second
        // boundary `log_stats_system` already uses for its own summary
        // line, so a console/debug-server read is at most ~1 second
        // stale — indistinguishable from before for a human operator,
        // for a cost paid once/second instead of every frame.
        let total = self.world.resource::<byroredux_core::ecs::TotalTime>().0;
        let should_refresh_handle_counts = crate::systems::crosses_one_second_boundary(total, dt);
        if should_refresh_handle_counts {
            // #1584 — reuse persistent scratch sets (clear() keeps the
            // allocation, drops the contents) so this dedup walk does
            // zero steady-state heap allocations.
            self.in_use_mesh_scratch.clear();
            if let Some(q) = self.world.query::<byroredux_core::ecs::MeshHandle>() {
                for (_, h) in q.iter() {
                    if h.0 != 0 {
                        self.in_use_mesh_scratch.insert(h.0);
                    }
                }
            }
            self.in_use_tex_scratch.clear();
            if let Some(q) = self.world.query::<byroredux_core::ecs::TextureHandle>() {
                for (_, h) in q.iter() {
                    if h.0 != 0 {
                        self.in_use_tex_scratch.insert(h.0);
                    }
                }
            }
        }
        {
            let mut stats = self.world.resource_mut::<DebugStats>();
            stats.push_frame_time(dt);
            stats.entity_count = self.world.next_entity_id();
            // Off-cadence frames keep the previous values (still fresh to
            // within ~1 second) rather than stale-to-zero.
            if should_refresh_handle_counts {
                stats.meshes_in_use = self.in_use_mesh_scratch.len() as u32;
                stats.textures_in_use = self.in_use_tex_scratch.len() as u32;
            }
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

        // M45.1 refinement — snapshot the player/camera pose now that the
        // scheduler's camera systems have published this frame's Transform,
        // so a `save` triggered this frame records where the player stands.
        crate::save_io::capture_player_pose(&self.world);

        // M45.1 — live save-load: reload the saved cell + overlay saved
        // form-id-keyed deltas. Runs alongside the other deferred drains,
        // no-op when no `load` is queued.
        self.step_save_loads();

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

    let settings = world
        .try_resource::<SettingsRegistry>()
        .map(|registry| registry.entries().cloned().collect())
        .unwrap_or_default();

    byroredux_debug_ui::PanelSnapshot {
        metrics,
        settings,
        entities,
    }
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
) {
    let mut debug_ui = debug_ui;
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
            Ok(_) => {
                if let Some(ui) = debug_ui.as_deref_mut() {
                    ui.apply_setting_change(&change);
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
    if outputs.console_evals.is_empty() {
        return;
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
}
