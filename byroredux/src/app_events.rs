//! `impl ApplicationHandler for App` — the winit event-loop translation,
//! split out of `main.rs` under #2731 (TD1-2026-08-12-01) when that file
//! crossed the 2000-LOC threshold.
//!
//! Everything winit hands the process lands here: `resumed` (window +
//! renderer bring-up), `window_event` (input, resize, close, redraw),
//! `device_event` (raw mouse deltas for the fly camera), and
//! `about_to_wait` (the game tick — scheduler, streaming steppers, frame
//! request). The work each arm dispatches to lives in
//! [`crate::app_step`] and [`crate::app_frame`]; `App` itself and its
//! construction stay in `main.rs`.
//!
//! Moved verbatim: the split is a relocation, not a rewrite.

use byroredux_core::ecs::{
    ActiveCamera, Camera, DebugStats, DeltaTime, EngineConfig, RtIntegrityStats, ScratchRow,
    ScratchTelemetry, ShadowMaskCensus, SkinCoverageStats, TotalTime,
};
use byroredux_core::settings::SettingsRegistry;
use byroredux_platform::window::{self, WindowConfig};
use byroredux_renderer::VulkanContext;
use byroredux_ui::UiManager;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowId;

use crate::cell_loader;
use crate::components::InputState;
use crate::helpers::world_resource_set;
use crate::systems::toggle_player_mode;
use crate::App;
use crate::{bench_frame_distribution, bench_gpu_inactive_token};

/// One [`ScratchRow`] for a `Vec`. The element type is inferred from the field,
/// so a row cannot report a size for a type the field no longer has.
#[allow(clippy::ptr_arg)] // `capacity()` is the whole point; a slice has none.
fn vec_row<T>(name: &'static str, vec: &Vec<T>) -> ScratchRow {
    ScratchRow {
        name,
        len: vec.len(),
        capacity: vec.capacity(),
        elem_size_bytes: std::mem::size_of::<T>(),
    }
}

/// [`vec_row`] for a hash map. Like the renderer's own hash-container rows,
/// `capacity x elem_size` under-counts a hash table's real footprint (no
/// control bytes, no load-factor slack): a proportional signal, not an
/// allocator-exact figure.
fn map_row<K, V, S>(name: &'static str, map: &std::collections::HashMap<K, V, S>) -> ScratchRow {
    ScratchRow {
        name,
        len: map.len(),
        capacity: map.capacity(),
        elem_size_bytes: std::mem::size_of::<(K, V)>(),
    }
}

/// [`map_row`] for a hash set.
fn set_row<K, S>(name: &'static str, set: &std::collections::HashSet<K, S>) -> ScratchRow {
    ScratchRow {
        name,
        len: set.len(),
        capacity: set.capacity(),
        elem_size_bytes: std::mem::size_of::<K>(),
    }
}

impl App {
    /// Shared orderly shutdown for both the OS close button and the native
    /// pause menu's Quit action.
    pub(crate) fn shutdown(&mut self, event_loop: &ActiveEventLoop) {
        log::info!("Shutdown requested");
        crate::extensions::shutdown_extension_host(&self.world);
        self.cancel_interior_cell_apply();
        if let (Some(ref mut state), Some(ref mut ctx)) =
            (self.streaming.as_mut(), self.renderer.as_mut())
        {
            let cells: Vec<_> = state.loaded.drain().collect();
            log::info!(
                "Streaming shutdown: unloading {} streamed cells before ctx destroy",
                cells.len()
            );
            // #3386 — the same one-batch rule as `drain_streaming_state`:
            // `unload_cell`'s `finish_unload_batch` half is a whole-engine
            // pass (`shrink_storages` + the full `blas_entries` walk in
            // `shrink_blas_scratch_to_fit`), and this sweep hands it the
            // entire resident set rather than the eviction ring's three.
            let roots: Vec<_> = cells
                .into_iter()
                .map(|((_gx, _gy), slot)| slot.cell_root)
                .collect();
            cell_loader::unload_cells(&mut self.world, ctx, &roots);
            ctx.flush_pending_destroys();
        }
        if let Some(mut state) = self.streaming.take() {
            state.shutdown(std::time::Duration::from_secs(1));
        }
        self.world
            .remove_resource::<byroredux_renderer::vulkan::allocator::AllocatorResource>();
        self.renderer.take();
        self.window.take();
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let mut config = WindowConfig::default();
        match crate::cli_args::parse_window_size(&crate::cli_args::effective_args()) {
            Ok(Some((width, height))) => {
                config.width = width;
                config.height = height;
            }
            Ok(None) => {}
            Err(error) => {
                log::error!("Invalid --window-size: {error}");
                event_loop.exit();
                return;
            }
        }

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
                // #3308 — depth-capture bridge for the `depth.stats`
                // console command. The renderer copies the depth attachment
                // into a staging buffer on request; `Camera::analyze_depth_
                // field` turns the samples into the resolution report.
                let depth_handle = ctx.depth_capture_handle();
                self.world
                    .insert_resource(byroredux_core::ecs::DepthCaptureBridge {
                        requested: depth_handle.requested,
                        // The SAME `Arc` the renderer publishes into — the
                        // capture type lives in `byroredux_core` precisely so
                        // no translation step sits between the two.
                        result: depth_handle.result,
                        // #4003 — the device's answer to "can this be
                        // captured at all", so `depth.stats` can say no
                        // instead of arming a request the renderer will
                        // refuse with only a `log::warn!` the console
                        // never sees.
                        unsupported_format: depth_handle.unsupported_format,
                    });

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
                let mut debug_ui_state = byroredux_debug_ui::DebugUiState::new(event_loop, &win);
                debug_ui_state.sync_registered_settings(&self.world.resource::<SettingsRegistry>());
                let mut pending = self.pending_player_messages.drain(..);
                if let Some(first) = pending.next() {
                    debug_ui_state.push_player_message(first);
                }
                for line in pending {
                    debug_ui_state.push_console_line(line);
                }
                self.debug_ui = Some(debug_ui_state);

                self.renderer = Some(ctx);
                self.window = Some(win);
                self.last_frame = Instant::now();
                self.setup_scene();
                if self
                    .world
                    .try_resource::<crate::studio_host::StudioSession>()
                    .is_some()
                {
                    if let Some(ui) = self.debug_ui.as_mut() {
                        ui.open_studio();
                    }
                    self.release_world_input_for_ui();
                }
                crate::sync_camera_setting(&self.world);
                // Preserve the scene/CLI-authored camera before the startup
                // scheduler's character-follow system overwrites it. The
                // frame loop will derive and reapply the requested bench path
                // from this seed on every rendered frame.
                self.seed_bench_camera_origin();
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
        // Focus loss must release gameplay state even if a native/Scaleform
        // menu consumes the event below. Key-up events may go to another
        // application after Alt-Tab; retaining those keys makes movement stick.
        if matches!(event, WindowEvent::Focused(false)) {
            self.loading_screen.focus_lost();
            self.release_world_input_for_ui();
        }
        if self.loading_screen.active()
            && matches!(&event, WindowEvent::KeyboardInput { .. }
                | WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. }
                | WindowEvent::Touch(_) | WindowEvent::Ime(_))
        {
            return;
        }
        // A finite named benchmark owns the measured world state. Matrix runs
        // repeatedly create focus-stealing windows; accepting a coincident E
        // press here can activate a door and turn an interior benchmark into
        // an exterior-streaming benchmark mid-run. Keep lifecycle/rendering
        // events live, but quarantine gameplay input until the summary. A
        // `--bench-hold` session releases this guard immediately afterward.
        if crate::bench::harness_owns_input(self.bench_mode, self.bench_summary_printed)
            && matches!(
                &event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
            )
        {
            return;
        }

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

        let pressed_key = match &event {
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                match event.physical_key {
                    PhysicalKey::Code(code) => Some(code),
                    _ => None,
                }
            }
            _ => None,
        };
        let game_menu_open = self
            .debug_ui
            .as_ref()
            .is_some_and(byroredux_debug_ui::DebugUiState::game_menu_visible);
        let inventory_pressed = pressed_key.is_some_and(|key| {
            self.world
                .try_resource::<crate::interaction::ActionBindings>()
                .and_then(|bindings| {
                    bindings.key_for_action(crate::interaction::InputAction::Inventory)
                })
                == Some(key)
        });
        if game_menu_open {
            if pressed_key == Some(KeyCode::Escape) {
                self.toggle_game_menu();
                return;
            }
            if inventory_pressed {
                if self
                    .debug_ui
                    .as_ref()
                    .is_some_and(byroredux_debug_ui::DebugUiState::inventory_menu_visible)
                {
                    self.toggle_game_menu();
                } else {
                    self.open_inventory_menu();
                }
                return;
            }
            if !matches!(event, WindowEvent::CloseRequested | WindowEvent::Resized(_)) {
                self.release_world_input_for_ui();
                return;
            }
        }

        // F3 is an engine-global developer binding. Treat the overlay as a
        // modal native surface so it gets a visible cursor and world movement
        // cannot continue behind a slider or text field.
        if pressed_key == Some(KeyCode::F3) {
            let visible = if let Some(ui) = self.debug_ui.as_mut() {
                ui.toggle();
                ui.visible
            } else {
                false
            };
            if visible {
                self.release_world_input_for_ui();
                if self
                    .world
                    .try_resource::<byroredux_core::ecs::SchedulerSystemTimings>()
                    .is_none()
                {
                    self.world
                        .insert_resource(byroredux_core::ecs::SchedulerSystemTimings::default());
                }
            } else {
                self.capture_world_input();
            }
            return;
        }
        let debug_overlay_open = self.debug_ui.as_ref().is_some_and(|ui| ui.visible);
        if debug_overlay_open
            && !matches!(event, WindowEvent::CloseRequested | WindowEvent::Resized(_))
        {
            self.release_world_input_for_ui();
            return;
        }
        if egui_consumed && !matches!(event, WindowEvent::CloseRequested | WindowEvent::Resized(_))
        {
            return;
        }
        if self.route_scaleform_window_event(&event) {
            return;
        }

        let save_action = pressed_key.and_then(|key| {
            self.world
                .try_resource::<crate::interaction::ActionBindings>()
                .and_then(|bindings| bindings.action_for_key(key))
        });
        let queued_save_action = match save_action {
            Some(crate::interaction::InputAction::Quicksave) => {
                Some(crate::save_io::PlayerSaveAction::Quicksave)
            }
            Some(crate::interaction::InputAction::Quickload) => {
                Some(crate::save_io::PlayerSaveAction::Quickload)
            }
            _ => None,
        };
        if let Some(action) = queued_save_action {
            if let Err(error) = crate::save_io::queue_player_save_action(&self.world, action) {
                crate::surface_save_load_output(
                    self.debug_ui.as_mut(),
                    action.context(),
                    byroredux_core::console::CommandOutput::error(error),
                );
            }
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                self.shutdown(event_loop);
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
                            // Escape opens the native pause menu. Scaleform
                            // focus was routed above, so a compatibility menu
                            // still receives its own Escape first.
                            if code == KeyCode::Escape && !event.repeat {
                                drop(input);
                                self.toggle_game_menu();
                            } else if inventory_pressed && !event.repeat {
                                drop(input);
                                self.open_inventory_menu();
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
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if !self.world.resource::<InputState>().mouse_captured => self.capture_world_input(),
            WindowEvent::MouseInput { state, button, .. }
                if self.world.resource::<InputState>().mouse_captured =>
            {
                let mut input = self.world.resource_mut::<InputState>();
                match state {
                    ElementState::Pressed => {
                        input.mouse_buttons_held.insert(button);
                    }
                    ElementState::Released => {
                        input.mouse_buttons_held.remove(&button);
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
        if crate::bench::harness_owns_input(self.bench_mode, self.bench_summary_printed) {
            return;
        }
        if let DeviceEvent::MouseMotion { delta } = event {
            let ui_focused = self
                .ui_manager
                .as_ref()
                .is_some_and(UiManager::has_input_focus);
            let native_ui_focused = self
                .debug_ui
                .as_ref()
                .is_some_and(byroredux_debug_ui::DebugUiState::captures_gameplay_input);
            if ui_focused || native_ui_focused || self.loading_screen.active() {
                self.release_world_input_for_ui();
                return;
            }
            let looking_enabled = self
                .world
                .try_resource::<byroredux_scripting::PlayerControlState>()
                .map(|controls| controls.looking_enabled)
                .unwrap_or(true);
            if !looking_enabled {
                return;
            }
            let mut input = self.world.resource_mut::<InputState>();
            crate::ui_input::apply_mouse_look(
                &mut input,
                delta,
                self.window.as_ref().is_some_and(|window| window.has_focus()),
            );
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.loading_screen.take_restore_capture() {
            self.capture_world_input();
        }
        // Keep the engine-owned UI compatibility snapshot current before the
        // scheduler runs provider callbacks. The manager is main-thread-only,
        // so this small projection is the bridge into the sandbox host.
        if let Some(ui) = self.ui_manager.as_ref() {
            crate::extensions::extension_ui_menu_sync(
                &self.world,
                Some(ui.menu_name.as_str()),
                ui.visible,
            );
        } else {
            crate::extensions::extension_ui_menu_sync(&self.world, None, false);
        }

        // Menu focus can change through engine code without a corresponding
        // winit event. Enforce modal ownership before the scheduler reads
        // InputState so a held movement key cannot leak for one frame.
        let native_ui_focused = self
            .debug_ui
            .as_ref()
            .is_some_and(byroredux_debug_ui::DebugUiState::captures_gameplay_input);
        if self.loading_screen.active()
            || native_ui_focused
            || self
                .ui_manager
                .as_ref()
                .is_some_and(UiManager::has_input_focus)
        {
            self.release_world_input_for_ui();
        }

        let atw_pre_t0 = Instant::now();
        let now = atw_pre_t0;
        // Finite benchmarks resolve one named mode before the event loop
        // starts, so delta-time cannot drift independently of camera policy.
        // Outside a finite bench, retain BYROREDUX_FIXED_DT as a diagnostic
        // override for tools that do not emit benchmark conclusions.
        let wall_dt = now.duration_since(self.last_frame).as_secs_f32();
        let dt = if self.loading_screen.active() {
            // Continue the scheduler's debug drain while holding simulation
            // time still; gameplay input is cleared above on every load tick.
            0.0
        } else if crate::bench::harness_active(self.bench_summary_printed) {
            self.bench_mode.map_or_else(
                || {
                    // Preserve the environment override for non-benchmark
                    // tools. Finite benches resolve it once into a named mode
                    // in boot/schedule/.
                    std::env::var("BYROREDUX_FIXED_DT")
                        .ok()
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(wall_dt)
                },
                |mode| mode.delta_time(wall_dt),
            )
        } else {
            // A held session becomes interactive after its finite benchmark:
            // fixed dt (including a legacy environment override resolved by
            // the bench) must no longer suppress walk/fly movement.
            wall_dt
        };
        self.last_frame = now;

        let simulation_paused = self
            .debug_ui
            .as_ref()
            .is_some_and(byroredux_debug_ui::DebugUiState::game_menu_visible);

        // Update time resources.
        world_resource_set::<DeltaTime>(&self.world, |r| r.0 = dt);
        if !simulation_paused {
            world_resource_set::<TotalTime>(&self.world, |r| r.0 += dt);
        }

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
        // #2689 — read before the `DebugStats` write-lock block opens
        // (not nested inside it) so this stays a plain sequential pair of
        // resource acquisitions rather than a `DebugStats` write held
        // across a second, independent resource read.
        let (anim_clip_count, anim_clip_stub_count) = {
            let registry = self
                .world
                .resource::<byroredux_core::animation::AnimationClipRegistry>();
            (
                registry.len() as u32,
                registry.stub_slot_count() as u32,
            )
        };
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
            // #2689 (SAFE-D8-01) — surface the AnimationClipRegistry's
            // permanently-stranded stub-slot count so the `stats` console
            // command makes the evict/reload leak observable.
            stats.anim_clip_count = anim_clip_count;
            stats.anim_clip_stub_count = anim_clip_stub_count;
        }

        // Refresh renderer-side scratch-Vec telemetry (R6). Reuses the
        // resource's `rows` Vec so this is amortized to ~zero allocs
        // after the first frame; capacity stabilises at the count of
        // declared scratches in `VulkanContext::fill_scratch_telemetry`.
        if let Some(ref ctx) = self.renderer {
            let mut tlm = self.world.resource_mut::<ScratchTelemetry>();
            ctx.fill_scratch_telemetry(&mut tlm.rows);
            tlm.renderer_row_count = tlm.rows.len();

            // #3694 — the engine binary's own `build_render_data` scratches
            // (`render/mod.rs`'s own doc names this set as "owned by the
            // caller and cleared on entry"). Appended after
            // `fill_scratch_telemetry`'s rows rather than folded into it:
            // that function only ever sees renderer-owned fields, and
            // `draw_commands` in particular is the quantity five of its
            // own rows (`gpu_instances_scratch`, `previous_models_scratch`,
            // `batches_scratch`, both rigid-motion maps) are `reserve()`d
            // against, so it belongs in the same report even though it
            // lives on `App`, not `VulkanContext`.
            //
            // #4610 — every container field of `App` is either a row here or
            // named, with its reason, in `app_scratch_telemetry_coverage_tests`
            // at the bottom of this file, which derives that set from
            // `main.rs`. The ground-cover collectors, the ground-cover model
            // tier and the handle-dedup sets used to have no row at all.
            let gc = &self.groundcover_collect_scratch;
            tlm.rows.extend([
                vec_row("draw_commands", &self.draw_commands),
                vec_row("cover_template_draws", &self.cover_template_draws),
                vec_row("groundcover_model_records", &self.groundcover_model_records),
                vec_row("groundcover_model_table", &self.groundcover_model_table),
                vec_row("water_commands", &self.water_commands),
                vec_row("gpu_lights", &self.gpu_lights),
                vec_row("gpu_fog_volumes", &self.gpu_fog_volumes),
                vec_row("light_sort_scratch", &self.light_sort_scratch),
                vec_row("bone_world", &self.bone_world),
                map_row("skin_offsets", &self.skin_offsets),
                vec_row("groundcover_cells", &self.groundcover_cells),
                vec_row("groundcover_chunks", &self.groundcover_chunks),
                vec_row("groundcover_species", &self.groundcover_species),
                vec_row("groundcover_species_table", &self.groundcover_species_table),
                vec_row("groundcover_disturbers", &self.groundcover_disturbers),
                // `GroundCoverCollectScratch`'s four containers, named
                // `<App field>.<its field>`.
                vec_row(
                    "groundcover_collect_scratch.resident_cells",
                    &gc.resident_cells,
                ),
                vec_row("groundcover_collect_scratch.candidates", &gc.candidates),
                map_row("groundcover_collect_scratch.emitted", &gc.emitted),
                vec_row(
                    "groundcover_collect_scratch.disturber_found",
                    &gc.disturber_found,
                ),
                set_row("in_use_mesh_scratch", &self.in_use_mesh_scratch),
                set_row("in_use_tex_scratch", &self.in_use_tex_scratch),
            ]);
        }

        // EX-05 / #2736 — mirror the pre-tonemap non-finite pixel counters so
        // the console and bench summary can read them without renderer access.
        if let Some(ref ctx) = self.renderer {
            let ((last_rgb, last_alpha), (total_rgb, total_alpha)) = ctx.image_health();
            let mut health = self
                .world
                .resource_mut::<byroredux_core::ecs::ImageHealth>();
            health.last_non_finite_rgb = last_rgb;
            health.last_non_finite_alpha = last_alpha;
            health.total_non_finite_rgb = total_rgb;
            health.total_non_finite_alpha = total_alpha;
        }

        // EX-08 / #2374 — refresh the cross-subsystem ownership snapshot on the
        // same throttled cadence as the handle counts above. It reuses the
        // scratch sets those already populated rather than re-walking the ECS,
        // so the added per-sample cost is a handful of `len()` calls; off-
        // cadence frames keep the previous sample.
        if should_refresh_handle_counts {
            let snapshot = crate::ownership_sample::sample_all(
                &self.world,
                self.renderer.as_ref(),
                self.in_use_mesh_scratch.len(),
                self.in_use_tex_scratch.len(),
            );
            self.world
                .resource_mut::<byroredux_core::ecs::OwnershipTelemetry>()
                .current = snapshot;
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
        if let Some(ref ctx) = self.renderer {
            let mut integrity = self.world.resource_mut::<RtIntegrityStats>();
            ctx.fill_rt_integrity_stats(&mut integrity);
        }
        if let Some(ref ctx) = self.renderer {
            let mut census = self.world.resource_mut::<ShadowMaskCensus>();
            ctx.fill_shadow_mask_census(&mut census);
        }

        // Refresh the upscaler line `ctx.upscaler` prints — the FSR provider
        // version and the SDK's own GPU reservation, which live outside
        // `gpu-allocator` and so never appear in `ctx.memory`.
        if let Some(ref ctx) = self.renderer {
            let mut telemetry = self
                .world
                .resource_mut::<byroredux_core::ecs::UpscalerTelemetry>();
            ctx.fill_upscaler_telemetry(&mut telemetry);
        }

        // Select the deterministic bench-camera pose before the scheduler so
        // camera-dependent systems observe this frame's requested path.
        self.step_bench_camera();

        // End of pre-scheduler phase (Phase 10 bracket).
        let atw_pre_ns = atw_pre_t0.elapsed().as_nanos() as u64;

        // Run all systems.
        let systems_t0 = Instant::now();
        if !simulation_paused {
            self.scheduler.run(&self.world, dt);
        }
        let atw_scheduler_ns = systems_t0.elapsed().as_nanos() as u64;
        if self.bench_frames_target.is_some() && self.renderer.is_some() {
            self.bench_systems_ns += atw_scheduler_ns;
            self.bench_systems_ticks += 1;
        }

        // Post-scheduler phase starts here (Phase 10 bracket).
        let atw_post_t0 = Instant::now();

        // Character-mode camera sync runs inside the scheduler and otherwise
        // replaces an explicit --camera-pos / --bench-camera pose with the
        // player capsule's eye transform. Restore the selected bench pose
        // before both streaming and rendering. This is a no-op outside a
        // deterministic bench.
        self.restore_bench_camera_pose();

        // World cell streaming (M40 Phase 1a). Runs after the
        // scheduler so the scheduler-driven `fly_camera_system` has
        // already published the player's current Transform translation
        // for this frame. No-ops outside `--esm + --grid` exterior
        // mode and when the player hasn't crossed a boundary.
        self.step_streaming();

        // #4180 — static-BLAS recovery, moved out of the render driver. Must
        // follow `step_streaming`: it shares that step's deadline, and a cell
        // load's own batched builds should land before recovery decides what
        // is still missing.
        self.step_static_blas_restore();

        // Debug-UI load queue (Phase 2 of the debug-UI plan). Drains
        // the `PendingDebugLoadSlot` populated by the debug-server's
        // `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell`
        // handlers. Sequenced BEFORE `step_cell_transition` so a
        // debug load that arrives the same frame as a queued
        // `door.teleport` doesn't trample the transition's mid-load
        // state.
        self.step_debug_loads();
        self.step_studio();
        self.step_upscaler_switch(event_loop);

        // M45.1 refinement — snapshot the player/camera pose now that the
        // scheduler's camera systems have published this frame's Transform,
        // so a `save` triggered this frame records where the player stands.
        crate::save_io::capture_player_pose(&self.world);

        // #3113 — execute F5/F9 and pause-menu requests only after the
        // scheduler's parallel batch has joined. The drain itself drops its
        // queue guard before entering the save registry's wide lock surface.
        self.step_player_save_actions();

        // M45.1 — live save-load: reload the saved cell + overlay saved
        // form-id-keyed deltas. Runs alongside the other deferred drains,
        // no-op when no `load` is queued.
        self.step_save_loads();

        // Cell-transition dispatch (M40 Phase 2 Stage 3). Drains the
        // `PendingCellTransitionSlot` posted by `door.teleport`
        // (and future F-key activate) and dispatches the orchestrator.
        // No-op when the slot is `None` — the common per-frame case.
        self.step_cell_transition();

        // Persistent-cell apply owns a cross-frame entity-range cursor;
        // don't let it claim appearance entities belonging to another cell.
        if self.interior_transition.is_none()
            && !self.loading_screen.active()
            && self.streaming.as_ref().is_none_or(|stream| stream.persistent_apply.is_none())
        {
            if let Some(ctx) = self.renderer.as_mut() {
                self.loot_appearance_loader.step(&mut self.world, ctx);
            }
        }

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
        if self.bench_frames_target.is_some() {
            let samples = self.bench_cpu_frame_ms.len() as u32;
            if samples < self.bench_frames_count {
                self.bench_cpu_frame_ms
                    .push(atw_pre_t0.elapsed().as_secs_f64() * 1000.0);
            }
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
                // Both streaming paths run on the logical clock, so both must
                // ask the clock (not the rendered-frame count) whether they are
                // done — otherwise a soak would end mid-boundary and report the
                // in-flight cell as a leaked owner.
                let uses_streaming_clock = matches!(
                    self.bench_camera,
                    Some(crate::bench_camera::BenchCameraPath::GridCross)
                        | Some(crate::bench_camera::BenchCameraPath::GridSoak)
                );
                let path_complete = if uses_streaming_clock {
                    let boundary_in_progress = self
                        .streaming
                        .as_ref()
                        .is_some_and(|state| state.telemetry.boundary_in_progress());
                    crate::bench_camera::grid_cross_complete(
                        self.bench_camera_path_frame,
                        target,
                        boundary_in_progress,
                    )
                } else {
                    self.bench_frames_count >= target
                };
                if path_complete && !self.bench_summary_printed {
                    // Capture the renderer-facing state only after the timed
                    // window has closed. Hashing a large bone palette can be
                    // measurable CPU work, so it must not feed back into the
                    // frame time being reported.
                    let scene_state = crate::bench::capture_scene_state(
                        &self.world,
                        &self.draw_commands,
                        &self.water_commands,
                        &self.gpu_lights,
                        &self.gpu_fog_volumes,
                        &self.bone_world,
                    );
                    let bench_mode = self
                        .bench_mode
                        .expect("every finite benchmark resolves a named mode");
                    let bench_camera = self
                        .bench_camera
                        .map_or_else(|| "free".to_owned(), |path| path.to_string());
                    let stats = self.world.resource::<DebugStats>();
                    let elapsed_secs = self
                        .bench_start
                        .map(|t| t.elapsed().as_secs_f64())
                        .unwrap_or(1.0);
                    let wall_fps = self.bench_frames_count as f64 / elapsed_secs;
                    let wall_ms = elapsed_secs * 1000.0 / self.bench_frames_count as f64;
                    let [frame_p50_ms, frame_p95_ms, frame_max_ms] =
                        bench_frame_distribution(&self.bench_cpu_frame_ms);
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
                    // #3629 — the HOST-side TLAS build cost, from
                    // `FrameTimings`. Printed as `cpu_tlas_ms=` (renamed from
                    // the bare `tlas_ms=`) because the device-side bracket
                    // `gpu_tlas_build=` now appears on the same line, and the
                    // old name was the only `tlas` token a harness could
                    // extract — inviting the host number to be read as the
                    // device one.
                    let cpu_tlas_ms = ft.tlas_build_ns as f64 / n / 1e6;
                    let ssbo_ms = ft.ssbo_build_ns as f64 / n / 1e6;
                    // #3467 — the resumable geometry-rebuild slice. Reported
                    // here so `GEOMETRY_REBUILD_CHUNK_BYTES` can finally be
                    // re-picked against a measured number instead of the
                    // "chosen conservatively pending live tuning" its own doc
                    // still carries.
                    let geom_rebuild_ms = ft.geometry_rebuild_ns as f64 / n / 1e6;
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
                    // Full per-pass GPU breakdown. The FSR benchmark matrix
                    // (execution phase 7) needs the render-resolution passes
                    // and the output-resolution ones separable, because only
                    // the former shrink with a preset — reporting a frame-time
                    // win without netting out presentation and the upscale
                    // dispatch would overstate what a player actually gets.
                    // #2821 — the `_active` mirrors travel with the values.
                    // A bracket that did not run reports `0.000` here like any
                    // other, because widening a numeric field to `n/a` would
                    // break every `key=<float>` extractor across the four
                    // sweep harnesses; instead the companion `gpu_inactive=`
                    // token below names the brackets whose zero is "skipped",
                    // so a skipped pass no longer lands in the TSV as a hard
                    // 0.000 indistinguishable from a measured one.
                    let (gpu, gpu_active) = self
                        .world
                        .try_resource::<byroredux_core::ecs::SkinCoverageStats>()
                        .map(|s| {
                            // Copy just the timing fields out from under the
                            // resource read guard; the resource itself carries
                            // non-Copy diagnostic state we do not need here.
                            (
                                [
                                    s.gpu_skin_dispatch_ms,
                                    s.gpu_skin_blas_refit_ms,
                                    s.gpu_taa_ms,
                                    s.gpu_upscale_ms,
                                    s.gpu_main_render_ms,
                                    s.gpu_svgf_ms,
                                    s.gpu_composite_ms,
                                    s.gpu_ssao_ms,
                                    s.gpu_bloom_ms,
                                    s.gpu_volumetrics_ms,
                                    s.gpu_cluster_cull_ms,
                                    s.gpu_presentation_ms,
                                    // #3629 — appended, not inserted, so the
                                    // four sweep harnesses' `key=<float>`
                                    // extractors keep matching. These two
                                    // brackets existed in `gpu_timers.rs` and
                                    // in every other consumer (the debug-UI
                                    // grid, `gpu_breakdown`, the
                                    // `SkinCoverageStats` fill) but the bench
                                    // line reported 12 of 15 before #3629 while
                                    // its own comment claimed a "full per-pass GPU
                                    // breakdown".
                                    s.gpu_tlas_build_ms,
                                    s.gpu_caustic_splat_ms,
                                    s.gpu_skin_palette_ms,
                                    s.gpu_depth_history_copy_ms,
                                    // SKYAL — appended last, same #3629 rule.
                                    s.gpu_sky_cube_ms,
                                    // #4315 — appended last, same rule.
                                    s.gpu_groundcover_scatter_ms,
                                    // #4618 — appended last, same rule.
                                    s.gpu_exposure_meter_ms,
                                ],
                                [
                                    s.gpu_skin_dispatch_active,
                                    s.gpu_skin_blas_refit_active,
                                    s.gpu_taa_active,
                                    s.gpu_upscale_active,
                                    s.gpu_main_render_active,
                                    s.gpu_svgf_active,
                                    s.gpu_composite_active,
                                    s.gpu_ssao_active,
                                    s.gpu_bloom_active,
                                    s.gpu_volumetrics_active,
                                    s.gpu_cluster_cull_active,
                                    s.gpu_presentation_active,
                                    s.gpu_tlas_build_active,
                                    s.gpu_caustic_splat_active,
                                    s.gpu_skin_palette_active,
                                    s.gpu_depth_history_copy_active,
                                    s.gpu_sky_cube_active,
                                    s.gpu_groundcover_scatter_active,
                                    s.gpu_exposure_meter_active,
                                ],
                            )
                        })
                        .unwrap_or((
                            [0.0; crate::BENCH_GPU_KEYS.len()],
                            [false; crate::BENCH_GPU_KEYS.len()],
                        ));
                    let gpu_inactive = bench_gpu_inactive_token(gpu_active);
                    let rt_integrity_line = self
                        .world
                        .try_resource::<RtIntegrityStats>()
                        .map(|snapshot| snapshot.machine_line());
                    println!(
                        "bench: mode={} gate={} dt={} camera={} frames={} \
                         wall_fps={:.1} wall_ms={:.2} \
                         frame_p50_ms={:.2} frame_p95_ms={:.2} frame_max_ms={:.2} \
                         frame_max_over_p95={:.1} \
                         brd_ms={:.2} ui_ms={:.2} draw_ms={:.2} \
                         [fence={:.2} cpu_tlas_ms={:.2} ssbo={:.2} geom_rebuild={:.2} \
                         cmd={:.2} submit={:.2}] \
                         [gpu_skin_disp={:.3} gpu_blas_refit={:.3} gpu_taa={:.3} \
                         gpu_upscale={:.3} gpu_main_render={:.3} gpu_svgf={:.3} \
                         gpu_composite={:.3} gpu_ssao={:.3} gpu_bloom={:.3} \
                         gpu_volumetrics={:.3} gpu_cluster_cull={:.3} \
                         gpu_presentation={:.3} gpu_tlas_build={:.3} \
                         gpu_caustic_splat={:.3} gpu_skin_palette={:.3} \
                         gpu_depth_history_copy={:.3} gpu_sky_cube={:.3} \
                         gpu_groundcover_scatter={:.3} gpu_exposure_meter={:.3}] \
                         gpu_inactive={} \
                         systems_ms={:.2} ticks_per_frame={:.1} unaccounted_ms={:.2} \
                         camera_pos={:.3},{:.3},{:.3} camera_forward={:.6},{:.6},{:.6} \
                         sim_time_s={:.6} entities={} meshes={} textures={} \
                         draws={}/{}b/{}c bench_draws_raster_cmds={} \
                         lights={} tlas={} state_hash={:016x} \
                         skin={}/{}+{}",
                        bench_mode,
                        bench_mode.gate_label(),
                        bench_mode.dt_label(),
                        bench_camera,
                        self.bench_frames_count,
                        wall_fps,
                        wall_ms,
                        frame_p50_ms,
                        frame_p95_ms,
                        frame_max_ms,
                        // #3559 — appended as its own token so a harness can
                        // gate on the relationship rather than a per-scene
                        // millisecond threshold. `frame_max_ms` alone cannot
                        // tell a heavy scene from one blocked frame.
                        crate::bench_frame_max_over_p95([frame_p50_ms, frame_p95_ms, frame_max_ms]),
                        brd_ms,
                        ui_ms,
                        draw_ms,
                        fence_ms,
                        cpu_tlas_ms,
                        ssbo_ms,
                        geom_rebuild_ms,
                        cmd_ms,
                        submit_ms,
                        gpu[0],
                        gpu[1],
                        gpu[2],
                        gpu[3],
                        gpu[4],
                        gpu[5],
                        gpu[6],
                        gpu[7],
                        gpu[8],
                        gpu[9],
                        gpu[10],
                        gpu[11],
                        gpu[12],
                        gpu[13],
                        gpu[14],
                        gpu[15],
                        gpu[16],
                        gpu[17],
                        gpu[18],
                        gpu_inactive,
                        systems_ms,
                        ticks_per_frame,
                        unaccounted_ms,
                        scene_state.camera_position[0],
                        scene_state.camera_position[1],
                        scene_state.camera_position[2],
                        scene_state.camera_forward[0],
                        scene_state.camera_forward[1],
                        scene_state.camera_forward[2],
                        scene_state.simulated_time_s,
                        scene_state.entities,
                        stats.mesh_count,
                        stats.texture_count,
                        // #1258 — `draws=N/Mb/Kc` = N input DrawCommands
                        // / M post-merge batches / K actual GPU calls.
                        // Pre-fix this was a single `draws=N` that
                        // looked like a GPU call count but was actually
                        // the input. Format change preserves the
                        // existing first number for audit comparability.
                        scene_state.draws,
                        stats.batch_count,
                        stats.indirect_call_count,
                        stats.raster_draw_command_count,
                        scene_state.lights,
                        scene_state.tlas_eligible,
                        scene_state.state_hash,
                        // #4417 — appended last, same #3629 rule. The
                        // once-a-second `engine::stats` line is gated on
                        // `TotalTime` crossing a boundary, and
                        // `renderer-static` freezes `dt`, so that line never
                        // fires inside the bench window and a capture can
                        // quit before `--bench-hold` produces one. Reading
                        // the pool here ties the `skin_pool_*` baseline rows
                        // to the measured frame like every other column.
                        stats.skin_pool_live,
                        stats.skin_pool_max,
                        stats.skin_pool_overflow_attempts,
                    );
                    if let Some(line) = rt_integrity_line {
                        println!("{line}");
                    }
                    drop(stats);
                    if let Some(streaming) = self.streaming.as_ref() {
                        println!("{}", streaming.telemetry.bench_line());
                    }

                    // #4053 — TLAS membership by policy verdict. Printed
                    // unconditionally, because its value is as a *diff*: the
                    // failure mode in this area is "something quietly entered
                    // or left the TLAS", which no test sees and no frame-time
                    // graph shows. A row that is always present is one a
                    // before/after comparison can rely on being there.
                    println!("{}", self.tlas_policy.bench_line());

                    // #4054 — the scatter's own telemetry, including §11.3's
                    // `d_ground` histogram. The design is explicit that the
                    // density field cannot be unit-tested (it exists only in
                    // GLSL, deliberately), and that this histogram over real
                    // cells is what pins it instead.
                    if let Some(ref ctx) = self.renderer {
                        if let Some(stats) = ctx.groundcover_stats() {
                            println!("{}", stats.bench_line());
                        }
                        // #4413 — the authored-model tier's last placement:
                        // instances the placed plants asked for, and how many
                        // fit the tail budget.
                        if let Some(models) = ctx.groundcover_model_stats() {
                            println!(
                                "groundcover-models: demanded={} emitted={}",
                                models.demanded, models.emitted
                            );
                        }
                    }

                    // #4052 — EXAL ground cover §11.1. One row per measured
                    // variant plus the bake row. Printed as their own lines
                    // rather than folded into the `bench:` line because there
                    // are five of them and each carries its own sample count;
                    // an existing `bench:` extractor sees nothing new.
                    if self.groundcover_bench.is_some() {
                        if let Some(ref ctx) = self.renderer {
                            if ctx.groundcover_bench_has_samples() {
                                for line in ctx.groundcover_bench_report() {
                                    println!("{line}");
                                }
                            } else {
                                // An all-zero report would read as a result.
                                // The overwhelmingly likely cause is an
                                // interior cell: §11.1 is about LAND terrain,
                                // and there is none to sample indoors.
                                println!(
                                    "groundcover-bench: no samples — the bench needs a loaded \
                                     exterior worldspace (try --grid X,Y --radius N)"
                                );
                            }
                        }
                    }

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
                    } else if let Some(endpoint) = self.debug_server_endpoint() {
                        eprintln!(
                            "bench-hold: engine held open in live interactive mode — \
                             attach via `cargo run -p byro-dbg` \
                             ({endpoint}). Ctrl+C / window close to exit.",
                        );
                    } else {
                        eprintln!(
                            "bench-hold-unavailable: engine is held open, but the debug \
                             server did not bind; byro-dbg cannot attach. Ctrl+C / \
                             window close to exit."
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
        //
        // #3674 — that 501 ms figure was measured before
        // `between_frames_ms`'s sample point was fixed to the true
        // frame-start anchor (it used to be re-read after `draw_frame`
        // returned, folding in this frame's own render-path cost). It's
        // kept here as the historical motivation for the pre/scheduler/
        // post split, not as a currently-accurate reference number — a
        // fresh measurement would read lower.
        const NS_TO_MS: f32 = 1.0e-6;
        let atw_post_ns = atw_post_t0.elapsed().as_nanos() as u64;
        let mut cpu_t = self
            .world
            .resource_mut::<byroredux_core::ecs::CpuFrameTimings>();
        cpu_t.atw_pre_ms = atw_pre_ns as f32 * NS_TO_MS;
        cpu_t.atw_scheduler_ms = atw_scheduler_ns as f32 * NS_TO_MS;
        cpu_t.atw_post_ms = atw_post_ns as f32 * NS_TO_MS;
    }

    /// Last callback with the display connection still open. `run_app` takes
    /// the `EventLoop` by value, so the Wayland connection closes when it
    /// returns — but `App` lives on until `main` returns. Anything bound to
    /// that connection must be released here, on every exit path (pause-menu
    /// Quit, close button, and the error arms that skip `shutdown`).
    ///
    /// The debug UI's `egui_winit::State` is one such thing: when
    /// `egui-winit/clipboard` is unified in (a workspace-wide build pulls it
    /// through `byro-launcher`'s eframe), it owns a `smithay-clipboard` worker
    /// thread on the display. Dropped with `App`, that worker's teardown runs
    /// against the closed connection and the process segfaults after an
    /// otherwise clean shutdown.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.debug_ui.take();
    }
}

// Moved here with the `resumed` arm it pins (#2731); see the note in
// `app_frame.rs` for why the `include_str!` target has to follow the code.
/// A deterministic bench must remember the scene/CLI camera before the
/// startup scheduler runs `camera_follow_system`. The per-frame reapply is too
/// late to recover the authored origin if this ordering regresses.
#[cfg(test)]
mod bench_camera_startup_order_tests {
    #[test]
    fn authored_bench_origin_is_seeded_before_startup_scheduler() {
        let src = include_str!("app_events.rs");
        let setup = src
            .find("self.setup_scene();")
            .expect("renderer startup must set up the scene");
        let startup = &src[setup..];
        let seed = startup
            .find("self.seed_bench_camera_origin();")
            .expect("startup must preserve the authored bench camera");
        let scheduler = startup
            .find("self.scheduler.run(&self.world, 0.0);")
            .expect("startup must prime the scheduler");
        assert!(
            seed < scheduler,
            "bench origin must be captured before character camera sync can overwrite it"
        );
    }
}

/// #4610 — `about_to_wait` publishes a `ScratchTelemetry` row for every
/// persistent scratch on `App`, so a buffer that grows with the scene is
/// visible in `ctx.scratch`. The maintenance rule used to be "remember to add
/// one", which is how the ground-cover collectors, the ground-cover model tier
/// and the handle-dedup sets shipped with none.
///
/// This derives the set to cover from `main.rs` instead: every container field
/// of `App` must be a row below, or be named in [`NOT_SCRATCH`] with the reason
/// it is not a scratch. A new container field therefore forces a decision.
#[cfg(test)]
mod app_scratch_telemetry_coverage_tests {
    /// Container fields of `App` that are deliberately not per-frame scratch.
    const NOT_SCRATCH: [(&str, &str); 3] = [
        (
            "ui_reported_host_methods",
            "a dedup set capped by MAX_DISTINCT_HOST_METHOD_NAMES (#2964), not a per-frame buffer",
        ),
        (
            "bench_cpu_frame_ms",
            "one sample per rendered frame of the finite --bench window, dropped at exit",
        ),
        (
            "pending_player_messages",
            "drained by the resume path (`.drain(..)`), so it holds nothing across frames",
        ),
    ];

    /// Structs an `App` field can hold whose own container fields are scratch,
    /// with the source that declares them. A row is named
    /// `<App field>.<struct field>`.
    const SCRATCH_GROUPS: [(&str, &str); 1] = [(
        "GroundCoverCollectScratch",
        include_str!("render/groundcover.rs"),
    )];

    const CONTAINERS: [&str; 6] = [
        "Vec",
        "VecDeque",
        "FxHashMap",
        "FxHashSet",
        "HashMap",
        "HashSet",
    ];

    /// The text between the braces of `struct <name> {`.
    fn struct_body<'a>(src: &'a str, header: &str) -> &'a str {
        let start = src
            .find(header)
            .unwrap_or_else(|| panic!("`{header}` not found"))
            + header.len();
        let mut depth = 1;
        for (offset, c) in src[start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                return &src[start..start + offset];
            }
        }
        panic!("`{header}` is never closed");
    }

    /// `(name, type)` of each field, splitting on commas outside any bracket so
    /// `HashMap<K, V>` and multi-line types stay whole. Comment lines are
    /// dropped first.
    fn fields(body: &str) -> Vec<(String, String)> {
        let code: String = body
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let (mut out, mut current, mut depth, mut prev) = (Vec::new(), String::new(), 0i32, ' ');
        let mut flush = |current: &mut String| {
            if let Some((name, ty)) = current.split_once(':') {
                let ty = ty.split_whitespace().collect::<Vec<_>>().join(" ");
                out.push((
                    name.trim().trim_start_matches("pub(crate) ").to_string(),
                    ty,
                ));
            }
            current.clear();
        };
        for c in code.chars() {
            match c {
                '<' | '(' | '[' | '{' => depth += 1,
                // `->` inside a `Fn() -> T` type is not a closing bracket.
                '>' if prev == '-' => {}
                '>' | ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
            if c == ',' && depth == 0 {
                flush(&mut current);
            } else {
                current.push(c);
            }
            prev = c;
        }
        flush(&mut current);
        out
    }

    /// The last path segment of a type, before any generics.
    fn head(ty: &str) -> &str {
        let base = ty.split('<').next().unwrap_or(ty);
        base.rsplit("::").next().unwrap_or(base).trim()
    }

    fn is_container(ty: &str) -> bool {
        CONTAINERS.contains(&head(ty))
    }

    #[test]
    fn every_container_field_of_app_is_a_scratch_row_or_named_not_scratch() {
        let main_src = include_str!("main.rs");
        let app_fields = fields(struct_body(main_src, "\nstruct App {"));

        // The rows this file publishes, and nothing else it says: the test
        // module below spells every name it looks for.
        let production = include_str!("app_events.rs")
            .split_once("\n#[cfg(test)]\nmod ")
            .expect("app_events.rs has test modules")
            .0;
        let rows_start = production
            .find("tlm.rows.extend([")
            .expect("about_to_wait must publish the App scratch rows");
        let rows = &production[rows_start..];
        let rows = &rows[..rows.find("]);").expect("the row list is closed")];

        // Liveness: a parser that stopped matching would pass vacuously.
        let names: Vec<&str> = app_fields.iter().map(|(n, _)| n.as_str()).collect();
        for sentinel in [
            "draw_commands",
            "groundcover_model_records",
            "skin_offsets",
            "groundcover_collect_scratch",
        ] {
            assert!(
                names.contains(&sentinel),
                "the App field scan no longer finds `{sentinel}`; fix the scan first"
            );
        }

        let mut problems = Vec::new();
        for (name, ty) in &app_fields {
            if is_container(ty) {
                let excused = NOT_SCRATCH.iter().any(|(field, _)| field == name);
                let has_row = rows.contains(&format!("\"{name}\", &self.{name})"));
                if excused && has_row {
                    problems.push(format!("`{name}` is both a row and in NOT_SCRATCH"));
                } else if !excused && !has_row {
                    problems.push(format!(
                        "`App::{name}: {ty}` has no ScratchTelemetry row and is not in NOT_SCRATCH"
                    ));
                }
            } else if let Some((group, group_src)) =
                SCRATCH_GROUPS.iter().find(|(g, _)| *g == head(ty))
            {
                let inner = fields(struct_body(group_src, &format!("struct {group} {{")));
                for (field, field_ty) in inner.iter().filter(|(_, t)| is_container(t)) {
                    if !rows.contains(&format!("\"{name}.{field}\"")) {
                        problems.push(format!(
                            "`{group}::{field}: {field_ty}` (App::{name}) has no \
                             `\"{name}.{field}\"` row"
                        ));
                    }
                }
            } else if head(ty).ends_with("Scratch") {
                problems.push(format!(
                    "`App::{name}: {ty}` looks like a scratch group; add it to SCRATCH_GROUPS \
                     so its containers are covered"
                ));
            }
        }

        // An entry naming a field that is gone would silently excuse a future
        // field of the same name.
        for (field, _) in NOT_SCRATCH {
            if !names.contains(&field) {
                problems.push(format!(
                    "NOT_SCRATCH names `{field}`, which is no longer an App field"
                ));
            }
        }

        assert!(
            problems.is_empty(),
            "\n{}\n(add a row in `about_to_wait`, or a NOT_SCRATCH entry saying why it is not a scratch — #4610)",
            problems.join("\n")
        );
    }
}
