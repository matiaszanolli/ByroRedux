//! `App::render_one_frame` — the one-frame render driver, split out of
//! `main.rs` under #2731 (TD1-2026-08-12-01) when that file crossed the
//! 2000-LOC threshold.
//!
//! This is the *render* half of the frame: collect draw data, run the
//! upload/BLAS/TLAS work, present, and record the per-phase CPU timings.
//! Its siblings are [`crate::app_step`] (the per-tick streaming and
//! cell-transition steppers) and [`crate::app_events`] (the winit
//! `ApplicationHandler` translation that calls into both). `App` itself,
//! its fields, and its construction stay in `main.rs`.
//!
//! Moved verbatim: the split is a relocation, not a rewrite, so a
//! regression here would have to come from the module boundary rather than
//! from edited logic.

use byroredux_core::ecs::components::groundcover::WindField;
use byroredux_core::ecs::{DebugStats, DeltaTime, ScratchTelemetry};
use byroredux_renderer::vulkan::context::FrameInputs;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::ImageSpaceModifierView;
use byroredux_ui::{ScaleformHostDispatch, MAX_DISTINCT_HOST_METHOD_NAMES};
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;

use crate::helpers::world_resource_set;
use crate::render::build_render_data;
use crate::streaming;
use crate::systems::compute_underwater_params;
use crate::App;
use crate::{
    apply_debug_ui_outputs, build_debug_ui_snapshot, build_interaction_prompt, setting_bool,
};

fn host_method_diagnostic_key(menu_name: &str, method: &str) -> (String, String) {
    (menu_name.to_owned(), method.to_owned())
}

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
    pub(crate) fn render_one_frame(&mut self, event_loop: &ActiveEventLoop) {
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
        // a Vec of Strings every frame. Gate those diagnostics on
        // `visible`; the interaction prompt is the only snapshot field
        // populated while the operator overlay is hidden.
        let mut snapshot = if self
            .debug_ui
            .as_ref()
            .is_some_and(|ui| ui.visible || ui.game_menu_visible())
        {
            let include_inventory = self
                .debug_ui
                .as_ref()
                .is_some_and(byroredux_debug_ui::DebugUiState::inventory_menu_visible);
            build_debug_ui_snapshot(
                &self.world,
                self.debug_ui_refresh_entities,
                include_inventory,
            )
        } else {
            byroredux_debug_ui::PanelSnapshot {
                interaction_prompt: build_interaction_prompt(&self.world),
                show_crosshair: setting_bool(
                    &self.world,
                    byroredux_debug_ui::SHOW_CROSSHAIR_SETTING_ID,
                    true,
                ),
                show_prompts: setting_bool(
                    &self.world,
                    byroredux_debug_ui::SHOW_PROMPTS_SETTING_ID,
                    true,
                ),
                ..Default::default()
            }
        };
        // Benchmark screenshots are renderer evidence rather than gameplay
        // captures. Keep the new HUD reticle out of those established images.
        if self.bench_frames_target.is_some() {
            snapshot.show_crosshair = false;
        }
        self.debug_ui_refresh_entities = false;

        let save_load_messages = self
            .world
            .try_resource_mut::<crate::save_io::SaveLoadNotifications>()
            .map(|mut notifications| std::mem::take(&mut notifications.0))
            .unwrap_or_default();
        if let Some(ui) = self.debug_ui.as_mut() {
            let mut messages = save_load_messages.into_iter();
            if let Some(first) = messages.next() {
                ui.push_player_message(first);
            }
            for message in messages {
                ui.push_console_line(message);
            }
        }

        let (egui_frame, outputs) =
            if let (Some(ref mut ui), Some(win)) = (self.debug_ui.as_mut(), self.window.as_ref()) {
                let outputs = ui.run(win, &snapshot);
                let frame = ui.take_output().map(|out| (ui.egui_ctx.clone(), out));
                (frame, outputs)
            } else {
                (None, byroredux_debug_ui::PanelOutputs::default())
            };

        let (resume_game, quit_game) = apply_debug_ui_outputs(
            &mut self.world,
            outputs,
            &mut self.debug_ui_refresh_entities,
            self.debug_ui.as_mut(),
        );
        if resume_game {
            self.resume_from_game_menu();
        }
        if quit_game {
            self.shutdown(event_loop);
            return;
        }

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
                &mut self.gpu_fog_volumes,
                &mut self.light_sort_scratch,
                &mut self.bone_world,
                &mut self.skin_offsets,
                &mut self.skin_slot_pool,
                &mut self.material_table,
                ctx.particle_quad_handle,
            );
            if is_benching {
                self.bench_build_render_ns += brd_t0.elapsed().as_nanos() as u64;
            }
            // #3231/#3244 — stage this frame's morph weights for every live
            // `MorphSlot`; draw_frame publishes them after its fence wait.
            // Independent of
            // `build_render_data` above (see `render::update_morph_weights`'s
            // doc) — needs `&mut ctx`, which `build_render_data` doesn't take.
            crate::render::update_morph_weights(&self.world, ctx);

            {
                let mut tlm = self.world.resource_mut::<ScratchTelemetry>();
                tlm.materials_unique = self.material_table.unique_user_count();
                tlm.materials_interned = self.material_table.interned_count();
                tlm.materials_overflow = self.material_table.overflow_count();
            }
            // #1428 added a `debug_assert_eq!` here to catch any frame that
            // silently degraded over-cap materials to slot 0. #2795 removed
            // it: exceeding `MAX_MATERIALS` is a supported, documented
            // degrade — `MaterialTable::intern_by_hash` already fires a
            // `Once`-gated `log::warn!` on first overflow and routes every
            // over-cap material to the neutral-default slot 0 for the rest
            // of the session (see `docs/engine/memory-budget.md`) — not a
            // bug to crash a debug build over. `MAX_INSTANCES` made the same
            // call for the same reason (#956/#992, also documented there).
            // The `ctx.scratch` console command and `ScratchTelemetry` above
            // still surface the per-frame overflow count in every build.

            let defer_geometry_rebuild = self
                .streaming
                .as_ref()
                .is_some_and(streaming::WorldStreamingState::geometry_batch_in_progress);
            // #3298 — an already-running chunked rebuild must keep advancing
            // every frame regardless of `defer_geometry_rebuild`: that gate
            // only decides whether to *start* a new rebuild once the current
            // streaming transaction settles, not whether one already in
            // flight should keep copying. Gating the advance call on it too
            // would stall an in-flight copy indefinitely the moment another
            // streaming transaction (e.g. a second boundary crossing) begins
            // before the first rebuild's chunks finish.
            let rebuild_in_progress = ctx.mesh_registry.geometry_rebuild_in_progress();
            if rebuild_in_progress
                || (ctx.mesh_registry.is_geometry_dirty() && !defer_geometry_rebuild)
            {
                if let Err(e) = ctx.mesh_registry.rebuild_geometry_ssbo(
                    &ctx.device,
                    ctx.allocator.as_ref().unwrap(),
                    &ctx.graphics_queue,
                    ctx.transfer_pool,
                    ctx.device_caps.ray_query_supported,
                ) {
                    log::warn!("Failed to rebuild geometry SSBO: {e}");
                }
            }
            if ctx.mesh_registry.is_geometry_dirty() {
                // Cell/LOD application appends CPU-side global geometry a few
                // meshes at a time. Until the coherent batched rebuild lands,
                // keep those tail ranges out of raster and TLAS; otherwise the
                // old bound SSBO is indexed past its end. Existing resident
                // meshes keep rendering while the transaction progresses.
                for command in &mut self.draw_commands {
                    if !ctx.mesh_registry.is_geometry_resident(command.mesh_handle) {
                        command.in_raster = false;
                        command.in_tlas = false;
                    }
                }
                self.water_commands
                    .retain(|command| ctx.mesh_registry.is_geometry_resident(command.mesh_handle));
            }

            ctx.restore_missing_static_blas_for_draws(&self.draw_commands);

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

                // Consume what the menu asked of the host (#2714). The bridge
                // is drain-based by design and had no consumer outside its own
                // tests, so every ActionScript call was retained for the life
                // of the menu. Draining here is what keeps the queue at its
                // natural depth; `MAX_QUEUED_CALLS` is only the backstop for
                // when this does not run.
                //
                // Acting on the calls is M48 work — routing them into quest /
                // inventory / player state needs those systems' menu contracts
                // to exist first. What the engine can honestly do today is
                // consume them and say which ones it cannot answer, which
                // turns the bridge's `unknown_methods()` set from a test-only
                // observation into a live one.
                let host_calls = ui.drain_host_calls();
                // #2969 — `drain_calls`' own contract: a batch must be read
                // together with the eviction counter, because it may not be
                // contiguous. The bridge warns once at the producer when it
                // first evicts, but that says "a call was lost", not "the
                // batch you are about to act on has a hole in it" — and the
                // moment this loop routes calls into quest / inventory /
                // player state, that hole is a lost state transition with no
                // signal. Latched, so the warning tracks increases rather
                // than repeating every frame for the life of the menu.
                let dropped = ui.dropped_host_calls();
                if let Some(lost) = crate::host_call_gap(self.ui_dropped_host_calls, dropped) {
                    log::warn!(
                        "Scaleform menu '{}' lost {lost} host call(s) to the \
                         {}-entry bridge cap since the last drain ({dropped} \
                         total for this menu) — the {} call(s) drained this \
                         frame are not a contiguous record of what the menu \
                         asked for",
                        ui.menu_name,
                        byroredux_ui::MAX_QUEUED_CALLS,
                        host_calls.len(),
                    );
                }
                self.ui_dropped_host_calls = dropped;

                for call in host_calls {
                    log::debug!(
                        "Scaleform host call #{} {} -> {} ({:?}, {} arg(s))",
                        call.sequence,
                        call.transport_method,
                        call.method,
                        call.dispatch,
                        call.arguments.len(),
                    );
                    // #2964 — bounded the same way the bridge's own
                    // movie-keyed diagnostic sets are: menu/method strings are
                    // chosen by untrusted ActionScript content, so this set
                    // needs the same cap `host.rs::MAX_DISTINCT_HOST_METHOD_NAMES`
                    // gives `unknown_methods`/`unanswered_methods` upstream,
                    // or a menu calling a distinct unimplemented name every
                    // frame would grow this HashSet without limit.
                    let diagnostic_key = host_method_diagnostic_key(&ui.menu_name, &call.method);
                    if matches!(
                        call.dispatch,
                        ScaleformHostDispatch::Unknown | ScaleformHostDispatch::MissingResponse
                    ) && !self.ui_reported_host_methods.contains(&diagnostic_key)
                    {
                        if self.ui_reported_host_methods.len() >= MAX_DISTINCT_HOST_METHOD_NAMES {
                            if !self.ui_reported_host_methods_capped {
                                self.ui_reported_host_methods_capped = true;
                                log::error!(
                                    "Scaleform unimplemented-host-method diagnostic hit the \
                                     {MAX_DISTINCT_HOST_METHOD_NAMES}-entry cap; further \
                                     distinct methods are neither recorded nor logged"
                                );
                            }
                        } else {
                            self.ui_reported_host_methods.insert(diagnostic_key);
                            log::warn!(
                                "Scaleform menu '{}' called host method '{}' ({:?}) — \
                                 no engine handler is registered, so the menu received Null",
                                ui.menu_name,
                                call.method,
                                call.dispatch,
                            );
                        }
                    }
                }

                // #2972 — three-state, so "hidden" and "unchanged" no longer
                // share one `None`. Leaving `ui_tex` at `None` is what stops
                // `draw_frame` emitting the UI quad; previously a hidden
                // overlay fell into the `Unchanged` arm and kept compositing
                // its last uploaded frame over the world indefinitely.
                match ui.render() {
                    byroredux_ui::UiFrame::Fresh(pixels) => {
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
                    }
                    byroredux_ui::UiFrame::Unchanged => {
                        ui_tex = self.ui_texture_handle;
                    }
                    byroredux_ui::UiFrame::Hidden => {}
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
            // Apply deferred named debug-view / bounded-ray requests at the
            // frame boundary, then harvest any fence-lagged result from the
            // prior frame slot. Console execution never reaches into the
            // Vulkan context directly.
            let (pending_debug_mode, pending_probe_pixel) = self
                .world
                .try_resource_mut::<crate::components::RenderDebugControl>()
                .map_or((None, None), |mut control| {
                    (
                        control.pending_mode.take(),
                        control.pending_probe_pixel.take(),
                    )
                });
            if let Some(mode) = pending_debug_mode {
                ctx.set_render_debug_mode(mode);
            }
            let probe_request_result = pending_probe_pixel.map(|pixel| {
                ctx.request_selected_ray_probe(pixel)
                    .map(|generation| (pixel, generation))
            });
            let completed_probe = ctx.take_selected_ray_probe_result();
            if let Some(mut control) = self
                .world
                .try_resource_mut::<crate::components::RenderDebugControl>()
            {
                control.active_mode = ctx.render_debug_mode();
                if let Some(result) = probe_request_result {
                    match result {
                        Ok((_pixel, generation)) => {
                            control.pending_probe_generation = Some(generation);
                            control.last_error = None;
                        }
                        Err(error) => {
                            control.pending_probe_generation = None;
                            control.last_error = Some(error);
                        }
                    }
                }
                if let Some(probe) = completed_probe {
                    if control.pending_probe_generation == Some(probe.generation) {
                        control.pending_probe_generation = None;
                    }
                    control.last_probe = Some(probe);
                }
            }
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
            // The same canonical EXAL/WTHR wind that drives foliage and water
            // now drives combustion advection. Keep the source contract in
            // world units here; the shader adds it to local fire/smoke flow
            // during backtracing instead of integrating weather repeatedly
            // into the transported velocity image.
            let wind = self
                .world
                .try_resource::<WindField>()
                .map(|wind| *wind)
                .unwrap_or_default();
            let wind = if wind.is_well_formed() {
                wind
            } else {
                WindField::CALM
            };
            let wind_params = [
                wind.direction[0],
                wind.direction[1],
                wind.speed.max(0.0),
                wind.gust_amplitude.max(0.0),
            ];
            let wind_gust = [wind.gust_frequency.max(0.0), 0.0, 0.0, 0.0];
            let image_space_modifier = self
                .world
                .try_resource::<byroredux_scripting::CinematicPresentationState>()
                .map_or_else(ImageSpaceModifierView::default, |state| {
                    let frame = state.image_space_modifier_frame;
                    ImageSpaceModifierView {
                        blur_radius_pixels: frame.blur_radius_pixels,
                        double_vision_strength: frame.double_vision_strength,
                        motion_blur_strength: frame.motion_blur_strength,
                        radial_blur_strength: frame.radial_blur_strength,
                        radial_blur_ramp_up: frame.radial_blur_ramp_up,
                        radial_blur_start: frame.radial_blur_start,
                        radial_blur_ramp_down: frame.radial_blur_ramp_down,
                        radial_blur_down_start: frame.radial_blur_down_start,
                        radial_blur_center: frame.radial_blur_center,
                        saturation: frame.saturation,
                        brightness: frame.brightness,
                        contrast: frame.contrast,
                        tint_color: frame.tint_color,
                        fade_color: frame.fade_color,
                    }
                });
            let draw_result = ctx.draw_frame(FrameInputs {
                clear_color,
                view_proj: &frame.view_proj,
                draw_commands: &self.draw_commands,
                lights: &self.gpu_lights,
                fog_volumes: &self.gpu_fog_volumes,
                bone_world: &self.bone_world,
                bind_inverse_pending_uploads: &pending_with_data,
                materials: self.material_table.materials(),
                camera_pos: frame.camera_pos,
                render_origin: frame.render_origin,
                ambient_color: frame.ambient,
                fog_color: frame.fog_color,
                fog_near: frame.fog_near,
                fog_far: frame.fog_far,
                fog_extinction_per_meter: frame.fog_medium.extinction_per_meter,
                fog_single_scatter_albedo: frame.fog_medium.single_scatter_albedo,
                fog_coverage: frame.fog_medium.coverage,
                fog_clip: frame.fog_clip,
                fog_power: frame.fog_power,
                fog_height_reference: frame.fog_height_reference,
                wind_params,
                wind_gust,
                ui_texture_handle: ui_tex,
                sky_params: &frame.sky,
                dof,
                frame_time_delta_ms,
                timings: frame_timings.as_mut(),
                water_commands: &self.water_commands,
                underwater: compute_underwater_params(&self.world),
                image_space_modifier,
                pose_dirty: self.skin_slot_pool.pose_dirty(),
            });
            // #1796 / D6-02 — `draw_frame`'s two early-return `Ok` guards
            // (empty framebuffers, `ERROR_OUT_OF_DATE_KHR`) are
            // indistinguishable from a frame that actually reached the skin
            // dispatch section. The CPU-side pose hash commit already ran
            // (in `build_render_data`, before `ctx.draw_frame` was called),
            // so an early return here means that commit needs undoing or the
            // next frame's dirty gate reads "clean" against a dispatch that
            // never happened.
            //
            // #1791 / D6-01 — the same guards also precede the
            // `bind_inverses` SSBO upload (draw.rs ~2654-2676), which sits
            // strictly before the `record_skinned_blas_refit` call that
            // flips `skin_dispatch_ran` true — so this flag is exactly the
            // right signal for both bugs. `pending` was already irrevocably
            // drained from the pool above (before this call), so an early
            // return here means those first-sight `bind_inverses` were about
            // to be lost for good: the slot stays resident in
            // `entity_to_slot` (never re-queued by `allocate`), so the
            // persistent SSBO region for it is never written, corrupting the
            // entity's skinning palette for its remaining lifetime in the
            // cell.
            //
            // #2522 / PERF-D6-NEW-01 — `draw_frame` can also return `Err`
            // (fence wait, command-buffer reset/begin, FSR parameter build —
            // all execute before `record_skinned_blas_refit`, i.e. while
            // `skin_dispatch_ran` is still `false`), so this check must run
            // unconditionally on the `Result`, not only inside the `Ok` arm.
            if !ctx.skin_dispatch_ran {
                self.skin_slot_pool.rollback_pending_pose_commits();
                self.skin_slot_pool
                    .requeue_pending(std::mem::take(&mut pending_for_requeue));
            }
            match draw_result {
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
                        cpu_t.geometry_rebuild_ms = ft.geometry_rebuild_ns as f32 * NS_TO_MS;
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
                            b.geometry_rebuild_ns += ft.geometry_rebuild_ns;
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

#[cfg(test)]
mod host_method_diagnostic_tests {
    use super::host_method_diagnostic_key;

    #[test]
    fn same_unknown_method_is_reportable_once_per_menu() {
        let hud = host_method_diagnostic_key("HUDMenu", "ShowTutorial");
        let pipboy = host_method_diagnostic_key("PipboyMenu", "ShowTutorial");
        assert_ne!(hud, pipboy);
        assert_eq!(hud, host_method_diagnostic_key("HUDMenu", "ShowTutorial"));
    }
}

// Moved here with `render_one_frame` itself (#2731). The needles below are
// searched in *this* file: left pointing at `main.rs` after the split, each
// `find` would have matched the needle literal inside the test module and
// passed while pinning nothing.
/// Regression for #2522 / PERF-D6-NEW-01. `draw_frame` can return `Err`
/// from at least four sites (fence wait, command-buffer reset/begin, FSR
/// parameter build) that all execute before `record_skinned_blas_refit` —
/// the call that flips `skin_dispatch_ran` true. The `skin_dispatch_ran`
/// rollback check must therefore run unconditionally on the `Result`, not
/// nested inside the `Ok` match arm, or an early `Err` silently loses the
/// #1791/#1796 first-sight-upload/pose-hash rollback. A live `App`/
/// `VulkanContext` test is impractical here for the same reason
/// `skin_dispatch_ran_ordering_tests` in `draw.rs` gives (70+ Vulkan-loader
/// fields, no safe defaults) — a static source assertion pins the ordering
/// instead, mirroring that sibling test's technique on the caller side.
#[cfg(test)]
mod skin_dispatch_ran_rollback_scope_tests {
    #[test]
    fn rollback_check_runs_before_the_ok_err_match_not_inside_the_ok_arm() {
        let src = include_str!("app_frame.rs");

        let draw_call_pos = src
            .find("let draw_result = ctx.draw_frame(FrameInputs {")
            .expect("render_one_frame must call draw_frame and capture its Result (#2522)");
        let rollback_check_pos = src
            .find("if !ctx.skin_dispatch_ran {")
            .expect("render_one_frame must check skin_dispatch_ran for rollback (#1791/#1796)");
        let match_pos = src
            .find("match draw_result {")
            .expect("render_one_frame must match on the captured draw_result (#2522)");
        let ok_arm_pos = src
            .find("Ok(needs_recreate) => {")
            .expect("draw_result match must have an Ok(needs_recreate) arm");

        assert!(
            draw_call_pos < rollback_check_pos,
            "the skin_dispatch_ran rollback check must come AFTER the \
             draw_frame call, so it observes this frame's outcome. (#2522)"
        );
        assert!(
            rollback_check_pos < match_pos,
            "the skin_dispatch_ran rollback check must come BEFORE the \
             match on draw_result — i.e. outside and above both arms — or \
             the Err(e) arm would skip it entirely, silently losing the \
             #1791/#1796 rollback on any of draw_frame's early-Err paths. \
             (#2522)"
        );
        assert!(
            match_pos < ok_arm_pos,
            "sanity: the Ok(needs_recreate) arm must be part of the \
             draw_result match this test is reasoning about."
        );
    }
}

/// #2795 — `MAX_MATERIALS` overflow is a supported, documented degrade:
/// `MaterialTable::intern_by_hash` fires a `Once`-gated `log::warn!` and
/// routes every over-cap material to the neutral-default slot 0 for the
/// rest of the session (`docs/engine/memory-budget.md`, matching the call
/// `MAX_INSTANCES` already made under #956/#992). The `#1428`
/// `debug_assert_eq!` here contradicted that by panicking a debug build on
/// exactly this condition — reachable per this project's own recorded
/// Skyrim radius-3 measurement (4000+ unique materials). A live `App` test
/// is impractical here (same reason as the sibling test module above), so
/// this pins the removal at the source level instead.
#[cfg(test)]
mod material_overflow_no_panic_tests {
    #[test]
    fn render_one_frame_does_not_assert_on_material_overflow_count() {
        let src = include_str!("app_frame.rs");
        assert!(
            src.contains("tlm.materials_overflow = self.material_table.overflow_count();"),
            "sanity: the per-frame overflow count must still be read into \
             ScratchTelemetry — that's the supported, non-panicking way to \
             surface it (via the `ctx.scratch` console command)"
        );
        assert!(
            !src.contains(
                "debug_assert_eq!(\n                self.material_table.overflow_count(),"
            ),
            "#2795: no code in this file may `debug_assert_eq!` on \
             MaterialTable::overflow_count() being zero — a nonzero count \
             is the documented, already-degraded (warn-once + slot 0) \
             over-cap path, not a bug. Read the count for telemetry only, \
             never assert it."
        );
    }
}
