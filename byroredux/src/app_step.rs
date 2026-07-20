//! Per-tick streaming / cell-transition steppers split out of
//! `main.rs` (#1858 / TD1-003). Each is called once per
//! `about_to_wait` tick, in the order: streaming → debug loads →
//! save loads → cell transition (see call sites in `main.rs`).

use crate::cell_loader;
use crate::streaming;
use crate::streaming_helpers::{consume_streaming_payload, SVGF_TAA_STREAMING_RECOVERY_FRAMES};
use crate::App;

impl App {
    /// #1586 / F7 — cap the steady-state spawn budget. Each applied
    /// payload runs the full main-thread spawn (terrain mesh + batched
    /// BLAS build + water + precombine decode + vertex/index upload), so
    /// draining every ready payload in one frame spikes frame time on
    /// fast-travel / teleport / post-stall catch-up. Spend at most
    /// this many real spawns per frame and leave the rest queued in
    /// the channel for subsequent frames.
    const MAX_CELLS_SPAWNED_PER_FRAME: usize = 2;

    pub(crate) fn step_streaming(&mut self) {
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
        let mut spawned_this_frame = 0usize;
        while spawned_this_frame < Self::MAX_CELLS_SPAWNED_PER_FRAME {
            let payload_opt = self
                .streaming
                .as_mut()
                .and_then(|s| s.payload_rx.try_recv().ok());
            let Some(payload) = payload_opt else { break };

            if consume_streaming_payload(
                &mut self.world,
                ctx,
                self.streaming.as_mut().unwrap(),
                payload,
            ) {
                spawned_this_frame += 1;
            }
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
        }
        // #2113 / D7-01 — a cell can have an in-flight worker request
        // (tracked only in `pending`, not yet in `loaded`) that the
        // `to_unload` diff above never sees. Drop any such request whose
        // coord has left the unload radius so the payload classifies as
        // `PayloadDecision::StaleNoPending` and is discarded on arrival
        // instead of paying a full spawn one boundary crossing too late.
        for coord in streaming::stale_pending_coords(&state.pending, player_grid, state.radius_unload) {
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
        // #1866 / #1871 (LC0703-01 / LC0703-02) — every LOD ring (terrain,
        // object, placement) gates on `radius_unload`, not `radius_load`:
        // full cells stay resident through `radius_load + 1` under the
        // streaming hysteresis band (`streaming.rs`), so gating on
        // `radius_load` let a LOD block/quad load one cell early and
        // z-fight a still-resident full model.
        let max_full_cell_radius = state.radius_unload;
        cell_loader::stream_lod_blocks(
            &mut self.world,
            ctx,
            lod_tex.as_ref(),
            lod_wctx.as_ref(),
            player_grid,
            max_full_cell_radius,
            &mut state.lod_blocks,
        );
        // Distant object LOD (Skyrim+/FO4 `.bto`) — no-op on other games.
        cell_loader::stream_object_lod_blocks(
            &mut self.world,
            ctx,
            lod_tex.as_ref(),
            lod_wctx.as_ref(),
            player_grid,
            max_full_cell_radius,
            &mut state.object_lod_blocks,
        );
        // Distant object LOD (Oblivion/FO3/FNV placement scheme) — no-op on
        // Skyrim+/FO4 (#1726). Mutually exclusive with the `.bto` ring above.
        cell_loader::stream_placement_lod_blocks(
            &mut self.world,
            ctx,
            lod_tex.as_ref(),
            lod_wctx.as_ref(),
            player_grid,
            max_full_cell_radius,
            &mut state.placement_lod_blocks,
        );
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
