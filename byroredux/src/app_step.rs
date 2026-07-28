//! Per-tick streaming / cell-transition steppers split out of
//! `main.rs` (#1858 / TD1-003). Each is called once per
//! `about_to_wait` tick, in the order: streaming → debug loads →
//! save loads → cell transition (see call sites in `main.rs`).

use crate::cell_loader;
use crate::streaming;
use crate::streaming_helpers::{
    advance_streaming_apply, lod_reconcile_budget_for_frame, reconcile_lod_rings,
    SVGF_TAA_STREAMING_RECOVERY_FRAMES,
};
use crate::App;
use std::time::Duration;
use winit::event_loop::ActiveEventLoop;

/// SVGF/upscaler recovery window after a `--bench-camera cut`. Matches the
/// window the streaming and cell-transition paths use, so the harness
/// measures the engine's real recovery rather than a bench-only one.
const BENCH_CUT_RECOVERY_FRAMES: u32 = SVGF_TAA_STREAMING_RECOVERY_FRAMES;

impl App {
    /// Main-thread allowance for steady-state EXAL application. Work is
    /// cooperative and guarantees one atomic unit of progress, so an
    /// unusually expensive NIF/REFR may exceed this target once; the next
    /// unit then waits for the following frame.
    const STREAMING_APPLY_BUDGET: Duration = Duration::from_millis(4);
    /// Deferred EXAL work per active LOD provider on a frame where no
    /// full-detail cell payload was applied. Terrain plus exactly one of
    /// object/placement can therefore spend at most four attempts.
    const MAX_LOD_ATTEMPTS_PER_PROVIDER_PER_IDLE_FRAME: usize = 2;

    pub(crate) fn step_streaming(&mut self) {
        let Some(ctx) = self.renderer.as_mut() else {
            return;
        };
        if self.streaming.is_none() {
            return;
        }

        // ── 1. Diff + dispatch ──────────────────────────────────────
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
        let grid_changed = state.last_player_grid != Some(player_grid);
        if grid_changed {
            state.last_player_grid = Some(player_grid);
            state.lod_reconcile_pending = true;
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
            }
            // #2113 / D7-01 — a cell can have an in-flight worker request
            // (tracked only in `pending`, not yet in `loaded`) that the
            // `to_unload` diff above never sees. Drop any such request whose
            // coord has left the unload radius so the payload classifies as
            // `PayloadDecision::StaleNoPending` and is discarded on arrival
            // instead of paying a full spawn one boundary crossing too late.
            for coord in
                streaming::stale_pending_coords(&state.pending, player_grid, state.radius_unload)
            {
                state.pending.remove(&coord);
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
            // off-thread. Snapshot cached NIF keys once per crossing so the
            // whole request batch shares one cache view (#862).
            let cached_keys = self
                .world
                .resource::<cell_loader::NifImportRegistry>()
                .snapshot_keys();
            state.queue_loads(deltas.to_load, cached_keys);
        }

        // ── 2. Advance one resumable full-detail transaction ───────
        //
        // The same deadline spans main-thread NIF finalization, terrain/
        // precombine setup, and placed-reference spawning. Boundary diffing
        // ran first, so removing a stale `pending` generation above cancels
        // and reclaims a partial cell before it can consume another slice.
        let mut apply_budget = cell_loader::FrameTimeBudget::new(Self::STREAMING_APPLY_BUDGET);
        let full_detail_worked =
            advance_streaming_apply(&mut self.world, ctx, state, &mut apply_budget);

        // ── 3. Progress the three distant-LOD rings ─────────────────
        //
        // Any full-detail work owns the frame, including a partial transaction
        // that has not reached `loaded` yet. On idle frames, each LOD provider
        // gets a small independent attempt budget, so terrain cannot starve
        // the active object scheme.
        let Some(lod_budget) = lod_reconcile_budget_for_frame(
            state.lod_reconcile_pending,
            usize::from(full_detail_worked),
            grid_changed,
            Self::MAX_LOD_ATTEMPTS_PER_PROVIDER_PER_IDLE_FRAME,
        ) else {
            return;
        };
        let progress = reconcile_lod_rings(&mut self.world, ctx, state, player_grid, lod_budget);
        if progress.complete {
            state.lod_reconcile_pending = false;
            log::info!(
                "Exterior LOD rings settled around ({},{}); {} attempts in final slice",
                player_grid.0,
                player_grid.1,
                progress.attempted,
            );
        }
    }

    /// Drain any queued debug-UI load ops and dispatch them to the
    /// existing loader primitives. Runs once per frame after
    /// `step_streaming` (so any in-flight streaming work settles
    /// first) and before `step_cell_transition` (so a queued debug
    /// cell load can't race with a `door.teleport`-driven transition
    /// that landed the same frame). No-op when the queue is empty,
    /// which is the steady-state case.
    pub(crate) fn step_debug_loads(&mut self) {
        let Some(ctx) = self.renderer.as_mut() else {
            return;
        };
        crate::debug_load::execute_pending_debug_loads(&mut self.world, ctx, &mut self.streaming);
    }

    /// Drive the active camera along the `--bench-camera` path.
    ///
    /// Runs before the scheduler so the pose the systems observe — and the
    /// one `build_render_data` turns into this frame's view matrix — is the
    /// pose for this frame index. Indexing on `bench_frames_count` rather
    /// than elapsed time is what makes a capture at frame N comparable across
    /// upscaler presets running at different frame rates.
    ///
    /// A path that teleports also signals the camera cut explicitly. The
    /// renderer's automatic detection would catch this one (the jump is far
    /// past its distance threshold), but relying on that would make the
    /// harness measure the detector rather than the recovery it is supposed
    /// to be testing.
    pub(crate) fn step_bench_camera(&mut self) {
        let (Some(path), Some(total_frames)) = (self.bench_camera, self.bench_frames_target) else {
            return;
        };
        let Some(active) = self
            .world
            .try_resource::<byroredux_core::ecs::ActiveCamera>()
            .map(|active| active.0)
        else {
            return;
        };

        // Capture the authored pose on the first bench frame so the path
        // composes with whatever the scene or `--camera-pos` set up.
        let origin = match self.bench_camera_origin {
            Some(origin) => origin,
            None => {
                let Some(transform) = self
                    .world
                    .query::<byroredux_core::ecs::Transform>()
                    .and_then(|q| q.get(active).copied())
                else {
                    return;
                };
                let origin = crate::bench_camera::CameraPose {
                    position: transform.translation,
                    forward: transform.rotation * -byroredux_core::math::Vec3::Z,
                };
                self.bench_camera_origin = Some(origin);
                log::info!(
                    "bench camera '{path}' over {total_frames} frames from ({:.2}, {:.2}, {:.2})",
                    origin.position.x,
                    origin.position.y,
                    origin.position.z,
                );
                origin
            }
        };

        let frame = self.bench_frames_count;
        let pose = path.pose(frame, total_frames, origin.position, origin.forward);
        let rotation = byroredux_core::math::Quat::from_rotation_arc(
            -byroredux_core::math::Vec3::Z,
            pose.forward,
        );
        if let Some(mut transforms) = self.world.query_mut::<byroredux_core::ecs::Transform>() {
            if let Some(transform) = transforms.get_mut(active) {
                transform.translation = pose.position;
                transform.rotation = rotation;
            }
        }

        if path.is_cut_frame(frame, total_frames) {
            if let Some(ctx) = self.renderer.as_mut() {
                ctx.signal_temporal_discontinuity(BENCH_CUT_RECOVERY_FRAMES);
            }
        }
    }

    /// Apply a queued runtime upscaler switch (`r.upscaler`, or the settings
    /// panel). Runs at the frame boundary alongside the other deferred ops,
    /// which is the only safe point: the switch waits for both frame slots to
    /// retire and then rebuilds every render-resolution target.
    ///
    /// A rejected spec or a failed rebuild logs and leaves the previous
    /// upscaler running — the request is dropped either way, so a bad value
    /// cannot retry itself into a loop.
    ///
    /// An `Err` out of `set_upscaler_mode` is a different animal: it means the
    /// rollback to the previous upscaler failed too, so no drawable
    /// configuration is left. That is exactly as fatal as a failed
    /// `recreate_swapchain` in `main.rs`, and is handled the same way — exit
    /// the event loop instead of spinning a frame loop that can only skip
    /// frames (#2156).
    pub(crate) fn step_upscaler_switch(&mut self, event_loop: &ActiveEventLoop) {
        let Some(spec) = self
            .world
            .try_resource_mut::<byroredux_core::ecs::PendingUpscalerSwitch>()
            .and_then(|mut slot| slot.take())
        else {
            return;
        };
        let (Some(ctx), Some(window)) = (self.renderer.as_mut(), self.window.as_ref()) else {
            log::warn!("upscaler switch to '{spec}' dropped — no renderer or window");
            return;
        };
        let mode = match crate::cli_args::parse_upscaler_spec(&spec) {
            Ok(mode) => mode,
            Err(error) => {
                log::warn!("upscaler switch to '{spec}' rejected: {error}");
                return;
            }
        };
        let size = window.inner_size();
        if let Err(error) = ctx.set_upscaler_mode(mode, [size.width, size.height]) {
            log::error!("upscaler switch to {mode} failed unrecoverably: {error:#}");
            event_loop.exit();
        }
    }

    /// Drain a queued live save-load (M45.1). Reloads the saved interior
    /// cell through the existing loader (full GPU/physics/camera setup),
    /// then overlays the form-id-keyed mutable game-state deltas. No-op
    /// when nothing is queued — the steady-state case.
    pub(crate) fn step_save_loads(&mut self) {
        let Some(ctx) = self.renderer.as_mut() else {
            return;
        };
        crate::save_io::execute_pending_save_loads(&mut self.world, ctx, &mut self.streaming);
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
    /// for the single-trigger door flow reachable today only via the
    /// `door.teleport` console command.
    ///
    /// ## #2039 / PERF-D7-02 — caching design note
    ///
    /// `build_texture_provider`/`build_material_provider` (called fresh
    /// here, and identically in [`crate::save_io::execute_pending_save_loads`])
    /// discard the BGSM/BGEM template cache, `MaterialProvider::csg_cache`,
    /// and `MaterialProvider::sf_cdbs` on every call — each rebuild
    /// re-opens and re-parses the same BSA/BA2 archives the previous
    /// provider already warmed. Fine for a single console-triggered
    /// transition; becomes a real per-door cost once Stage 4 interactive
    /// door activation ships (every door use pays the rebuild).
    ///
    /// Not implemented yet — not urgent before Stage 4 — but the shape
    /// this should take when it lands:
    ///
    /// * **Cache key**: the loaded-plugin-set identity (the `masters` +
    ///   `esm_path` combination CLI args resolve to), not the CLI args
    ///   string itself — two transitions with the same effective plugin
    ///   set should share a provider even if `--esm`/`--master` ordering
    ///   differs.
    /// * **Storage**: an `Option<(PluginSetKey, TextureProvider,
    ///   MaterialProvider)>` slot on `App` (this struct), checked before
    ///   the `build_*_provider` calls here and in `save_io`'s sibling
    ///   call site; rebuild only on a key miss.
    /// * **Invalidation**: any plugin-set change (different `--esm`,
    ///   added/removed `--master`) must miss the cache — stale archives
    ///   held open across a plugin swap would resolve textures/materials
    ///   against the wrong content.
    /// * **Lifetime interaction**: `drain_streaming_state` currently
    ///   drops the streaming state's owned providers as part of teardown;
    ///   caching means that ownership needs to move to `App` instead, so
    ///   teardown no longer implies "provider goes away."
    pub(crate) fn step_cell_transition(&mut self) {
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
                    crate::streaming_helpers::drain_streaming_state(
                        &mut self.world,
                        ctx,
                        &mut self.streaming,
                    );
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
                    crate::streaming_helpers::drain_streaming_state(
                        &mut self.world,
                        ctx,
                        &mut self.streaming,
                    );
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
                            crate::scene::ExteriorBootstrapMode::ForegroundFirst,
                        );
                        self.streaming = Some(state);

                        // 4. Reposition the camera at the destination
                        // spawn point. Foreground-first bootstrap has made
                        // the arrival cell coherent; here we still want the
                        // XTEL-authored pose, not its terrain centre.
                        let dest_pos =
                            cell_loader::position_zup_to_yup(pending.destination_position_zup);
                        let dest_rot =
                            cell_loader::rotation_zup_to_yup_quat(pending.destination_rotation_zup);
                        cell_loader::reposition_camera(&mut self.world, dest_pos, dest_rot);
                        // #1874 — see the doc comment on
                        // `snap_character_body_to_camera`: without this,
                        // `camera_follow_system` snaps the camera back
                        // toward the stale (pre-transition) capsule
                        // position on the next tick, fighting this
                        // reposition every frame and producing a stuck
                        // TAA/SVGF ghost.
                        crate::systems::snap_character_body_to_camera(&mut self.world);

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

#[cfg(test)]
mod tests {
    /// #2156 / RL-D6-03 — the other half of the fix (the rollback itself
    /// lives in `renderer::vulkan::context::resize`). An `Err` out of
    /// `set_upscaler_mode` means even the rollback to the previous upscaler
    /// failed, so no drawable configuration is left: `framebuffers` is empty
    /// and the #1211 guard turns every remaining frame into a skip. That is
    /// exactly as fatal as a failed `recreate_swapchain` in `main.rs`, and
    /// must exit the event loop rather than freeze the window.
    ///
    /// Static source check — the arm needs a real allocation/SDK failure that
    /// `cargo test` cannot induce, matching the source-scan precedent in
    /// `resize.rs`.
    #[test]
    fn upscaler_switch_failure_exits_the_event_loop() {
        let src = include_str!("app_step.rs");
        let call_pos = src
            .find("ctx.set_upscaler_mode(")
            .expect("step_upscaler_switch must still drive set_upscaler_mode");
        let arm = &src[call_pos..];
        let exit_pos = arm
            .find("event_loop.exit()")
            .expect("the set_upscaler_mode Err arm must exit the event loop (#2156)");
        assert!(
            exit_pos < 400,
            "the `event_loop.exit()` must sit in set_upscaler_mode's own Err arm, \
             not somewhere further down the file (#2156)",
        );
    }
}
