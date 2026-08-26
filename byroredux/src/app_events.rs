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
    ActiveCamera, Camera, DebugStats, DeltaTime, EngineConfig, RtIntegrityStats, ScratchTelemetry,
    SkinCoverageStats, TotalTime,
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

use crate::bench_frame_distribution;
use crate::cell_loader;
use crate::components::InputState;
use crate::helpers::world_resource_set;
use crate::systems::toggle_player_mode;
use crate::App;

impl App {
    /// Shared orderly shutdown for both the OS close button and the native
    /// pause menu's Quit action.
    pub(crate) fn shutdown(&mut self, event_loop: &ActiveEventLoop) {
        log::info!("Shutdown requested");
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
                    .try_resource::<byroredux_sdk::studio::StudioSession>()
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
        if let DeviceEvent::MouseMotion { delta } = event {
            let ui_focused = self
                .ui_manager
                .as_ref()
                .is_some_and(UiManager::has_input_focus);
            let native_ui_focused = self
                .debug_ui
                .as_ref()
                .is_some_and(byroredux_debug_ui::DebugUiState::captures_gameplay_input);
            if ui_focused || native_ui_focused {
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
            if input.mouse_captured {
                let sensitivity = input.look_sensitivity;
                input.yaw -= delta.0 as f32 * sensitivity;
                let vertical_sign = if input.invert_look_y { 1.0 } else { -1.0 };
                input.pitch += delta.1 as f32 * sensitivity * vertical_sign;
                // Clamp pitch to avoid flipping.
                input.pitch = input.pitch.clamp(
                    -std::f32::consts::FRAC_PI_2 + 0.01,
                    std::f32::consts::FRAC_PI_2 - 0.01,
                );
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Menu focus can change through engine code without a corresponding
        // winit event. Enforce modal ownership before the scheduler reads
        // InputState so a held movement key cannot leak for one frame.
        let native_ui_focused = self
            .debug_ui
            .as_ref()
            .is_some_and(byroredux_debug_ui::DebugUiState::captures_gameplay_input);
        if native_ui_focused
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
        let dt = if crate::bench::harness_active(self.bench_summary_printed) {
            self.bench_mode.map_or_else(
                || {
                    // Preserve the environment override for non-benchmark
                    // tools. Finite benches resolve it once into a named mode
                    // in boot.rs.
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

        // Debug-UI load queue (Phase 2 of the debug-UI plan). Drains
        // the `PendingDebugLoadSlot` populated by the debug-server's
        // `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell`
        // handlers. Sequenced BEFORE `step_cell_transition` so a
        // debug load that arrives the same frame as a queued
        // `door.teleport` doesn't trample the transition's mid-load
        // state.
        self.step_debug_loads();
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
                    // Full per-pass GPU breakdown. The FSR benchmark matrix
                    // (execution phase 7) needs the render-resolution passes
                    // and the output-resolution ones separable, because only
                    // the former shrink with a preset — reporting a frame-time
                    // win without netting out presentation and the upscale
                    // dispatch would overstate what a player actually gets.
                    let gpu = self
                        .world
                        .try_resource::<byroredux_core::ecs::SkinCoverageStats>()
                        .map(|s| {
                            // Copy just the timing fields out from under the
                            // resource read guard; the resource itself carries
                            // non-Copy diagnostic state we do not need here.
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
                            ]
                        })
                        .unwrap_or([0.0; 12]);
                    let rt_integrity_line = self
                        .world
                        .try_resource::<RtIntegrityStats>()
                        .map(|snapshot| snapshot.machine_line());
                    println!(
                        "bench: mode={} gate={} dt={} camera={} frames={} \
                         wall_fps={:.1} wall_ms={:.2} \
                         frame_p50_ms={:.2} frame_p95_ms={:.2} frame_max_ms={:.2} \
                         brd_ms={:.2} ui_ms={:.2} draw_ms={:.2} \
                         [fence={:.2} tlas_ms={:.2} ssbo={:.2} cmd={:.2} submit={:.2}] \
                         [gpu_skin_disp={:.3} gpu_blas_refit={:.3} gpu_taa={:.3} \
                         gpu_upscale={:.3} gpu_main_render={:.3} gpu_svgf={:.3} \
                         gpu_composite={:.3} gpu_ssao={:.3} gpu_bloom={:.3} \
                         gpu_volumetrics={:.3} gpu_cluster_cull={:.3} \
                         gpu_presentation={:.3}] \
                         systems_ms={:.2} ticks_per_frame={:.1} unaccounted_ms={:.2} \
                         camera_pos={:.3},{:.3},{:.3} camera_forward={:.6},{:.6},{:.6} \
                         sim_time_s={:.6} entities={} meshes={} textures={} \
                         draws={}/{}b/{}c lights={} tlas={} state_hash={:016x}",
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
                        brd_ms,
                        ui_ms,
                        draw_ms,
                        fence_ms,
                        tlas_ms,
                        ssbo_ms,
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
                        scene_state.lights,
                        scene_state.tlas_eligible,
                        scene_state.state_hash,
                    );
                    if let Some(line) = rt_integrity_line {
                        println!("{line}");
                    }
                    drop(stats);
                    if let Some(streaming) = self.streaming.as_ref() {
                        println!("{}", streaming.telemetry.bench_line());
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
