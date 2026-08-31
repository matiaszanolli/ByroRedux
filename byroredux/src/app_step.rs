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
use std::time::{Duration, Instant};
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
    ///
    /// Four milliseconds proved counterproductive in the FO4 boundary gate:
    /// hundreds of otherwise-cheap hashes/REFRs were serialized behind a
    /// complete render cycle whose own CPU/GPU cost was much larger than the
    /// apply allowance. One 60 Hz frame budget amortizes that fixed cost while
    /// preserving a hard yield point between atomic units; it does not enlarge
    /// the already-dominant single-hash outlier.
    const STREAMING_APPLY_BUDGET: Duration = Duration::from_millis(16);
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
        // #2451 / EXAL-03 — a cell may pin its own CLMT via XCCM, which
        // re-resolves sky + weather (through the same crossfade a
        // worldspace change uses) when it differs from what is installed.
        // Deliberately OUTSIDE the `grid_changed` guard below: bootstrap
        // seeds `last_player_grid` before the first step, so a session
        // starting *inside* an override cell would otherwise never apply
        // it until the player left and came back. Costs one map lookup
        // and an `Option<u32>` compare on every other frame.
        crate::scene::apply_cell_climate_override(
            &mut self.world,
            ctx,
            &state.tex_provider,
            &state.wctx,
            player_grid,
            &mut state.applied_climate_form,
        );
        // EX-16 item 1 (#2372) — same "outside the grid_changed guard"
        // placement as the climate override immediately above, and for
        // the same reason: a session that starts inside a region-tagged
        // cell must get its ambient directive on frame 0, not only on
        // the first subsequent boundary crossing. Unlike the climate call,
        // its own per-grid-cell cache (`applied_region_ambient`, #3679) is
        // what keeps the common frame cheap — `RegionAmbientRes::resolve`
        // itself allocates and sorts a `Vec`, so it must not run
        // unconditionally the way the climate override's map lookup does.
        crate::scene::apply_cell_region_ambient(
            &mut self.world,
            &state.wctx,
            player_grid,
            &mut state.applied_region_ambient,
        );
        let grid_changed = state.last_player_grid != Some(player_grid);
        if grid_changed {
            let dispatch_started = Instant::now();
            state.last_player_grid = Some(player_grid);
            // EX-09/17 item 4 — keep the save-visible identity mirror
            // current as the player walks; `begin_exterior_streaming`
            // only stamps it at session start, so without this a save
            // taken mid-walk would reload back at the *arrival* grid
            // cell instead of wherever the player actually is now.
            if let Some(mut ectx) = self
                .world
                .try_resource_mut::<cell_loader::CurrentExteriorContext>()
            {
                ectx.grid = player_grid;
            }
            state.recenter_lod_water(&mut self.world, player_grid);
            state.lod_reconcile_pending = true;
            state
                .telemetry
                .begin_boundary(player_grid, dispatch_started);
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
            let unload_started = Instant::now();
            let unloads = deltas
                .to_unload
                .into_iter()
                .filter_map(|coord| {
                    state
                        .loaded
                        .remove(&coord)
                        .map(|slot| (coord, slot.cell_root))
                })
                .collect::<Vec<_>>();
            let unloaded_count = unloads.len();
            let unloaded_any = !unloads.is_empty();
            if unloaded_any {
                // #3688 — invalidates `reconcile_lod_rings`' diagnostics gate.
                state.loaded_residency_changed = true;
            }
            let unload_timings = if unloaded_any {
                let roots = unloads.iter().map(|(_, root)| *root).collect::<Vec<_>>();
                let timings = cell_loader::unload_cells(&mut self.world, ctx, &roots);
                for (coord, root) in unloads {
                    log::info!("Unloaded cell ({},{}) (root {})", coord.0, coord.1, root);
                }
                Some(timings)
            } else {
                None
            };
            state
                .telemetry
                .record_unload_slice(unload_started.elapsed(), unloaded_count);
            if let Some(timings) = unload_timings {
                state.telemetry.record_unload_phases(timings);
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
            let queued = state.queue_loads(deltas.to_load, cached_keys);
            state.telemetry.record_queued_cells(queued);
            state.telemetry.observe_pending(state.pending.len());
            state
                .telemetry
                .record_dispatch_slice(dispatch_started.elapsed());
        }

        // ── 2. Advance one resumable full-detail transaction ───────
        //
        // The same deadline spans main-thread NIF finalization, terrain/
        // precombine setup, and placed-reference spawning. Boundary diffing
        // ran first, so removing a stale `pending` generation above cancels
        // and reclaims a partial cell before it can consume another slice.
        let streaming_deadline = Instant::now() + Self::STREAMING_APPLY_BUDGET;
        let mut apply_budget = cell_loader::FrameTimeBudget::until(streaming_deadline);
        let apply_started = Instant::now();
        let full_detail_worked =
            advance_streaming_apply(&mut self.world, ctx, state, &mut apply_budget);
        state
            .telemetry
            .record_apply_slice(apply_started.elapsed(), full_detail_worked);
        state.telemetry.observe_pending(state.pending.len());
        if state.pending.is_empty()
            && state.active_apply.is_none()
            && state.persistent_apply.is_none()
        {
            if let Some((grid, elapsed)) = state.telemetry.settle_full_detail(Instant::now()) {
                log::info!(
                    "Exterior full detail settled around ({}, {}) in {:.2} ms",
                    grid.0,
                    grid.1,
                    elapsed.as_secs_f64() * 1000.0,
                );
            }
        }

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
        let lod_started = Instant::now();
        let progress = reconcile_lod_rings(
            &mut self.world,
            ctx,
            state,
            player_grid,
            lod_budget,
            Some(streaming_deadline),
        );
        state
            .telemetry
            .record_lod_slice(lod_started.elapsed(), progress.attempted);
        if progress.complete {
            state.lod_reconcile_pending = false;
            log::info!(
                "Exterior LOD rings settled around ({},{}); {} attempts in final slice",
                player_grid.0,
                player_grid.1,
                progress.attempted,
            );
            if let Some((grid, elapsed)) = state.telemetry.settle_lod(Instant::now()) {
                log::info!(
                    "Exterior LOD settled around ({}, {}) in {:.2} ms",
                    grid.0,
                    grid.1,
                    elapsed.as_secs_f64() * 1000.0,
                );
            }
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
    /// pose for this frame index. Normal paths use `bench_frames_count` so a
    /// capture at frame N is comparable across upscaler presets. `grid-cross`
    /// uses a logical path frame that pauses while the current full-detail +
    /// LOD handoff is active; otherwise a faster renderer gives streaming less
    /// wall time and makes the benchmark supersede its own transaction.
    ///
    /// A path that teleports also signals the camera cut explicitly. The
    /// renderer's automatic detection would catch this one (the jump is far
    /// past its distance threshold), but relying on that would make the
    /// harness measure the detector rather than the recovery it is supposed
    /// to be testing.
    pub(crate) fn seed_bench_camera_origin(&mut self) {
        if self.bench_camera.is_none()
            || self.bench_frames_target.is_none()
            || self.bench_camera_origin.is_some()
        {
            return;
        }
        let Some(active) = self
            .world
            .try_resource::<byroredux_core::ecs::ActiveCamera>()
            .map(|active| active.0)
        else {
            return;
        };
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
        let path = self.bench_camera.expect("bench camera checked above");
        let total_frames = self
            .bench_frames_target
            .expect("bench frame target checked above");
        log::info!(
            "bench camera '{path}' over {total_frames} frames from ({:.2}, {:.2}, {:.2})",
            origin.position.x,
            origin.position.y,
            origin.position.z,
        );
    }

    /// Measure how far ahead of the bench camera the scene actually is, in BU.
    ///
    /// `Orbit` and `Dolly` need a *subject* distance. They previously used
    /// `origin.distance(Vec3::ZERO)`, which is the cell's placement offset from
    /// the world origin and has nothing to do with what the camera can see —
    /// on `GSProspectorSaloonInterior` that put the orbit target 3610 BU ahead
    /// of a camera inside a small room and swung it out of the building, so the
    /// bench timed an empty view (`gpu_main_render` 11.526 ms under `pan` vs
    /// 0.010 ms under `orbit`, identical draw counts).
    ///
    /// A forward ray cast is the direct measurement of the same quantity. It
    /// cannot run in [`Self::seed_bench_camera_origin`]: that is deliberately
    /// ordered *before* the startup scheduler pass, so `physics_sync_system`
    /// has not registered any collider yet and the query pipeline is empty.
    /// Hence the lazy fill on the first stepped frame, by which point Phase 1
    /// has registered the cell and refreshed the BVH.
    ///
    /// Two sources, in order of directness:
    ///
    /// 1. **Forward ray cast** against the collision world — literally "how far
    ///    is the surface I am pointed at". Used whenever it hits.
    /// 2. **Distance to the rendered scene's centroid**, over the world-space
    ///    positions of every `MeshHandle` entity. Covers the cases the cast
    ///    cannot answer: a scene with no physics at all (the `--cornell`
    ///    redistributable control registers no colliders), and a camera aimed
    ///    at open sky.
    ///
    /// Deliberately **no fixed-BU fallback** on this path. The two bench scene
    /// families do not share a unit scale: cell scenes are Bethesda units
    /// (Prospector's room is ~150 BU deep) while `--cornell` is a ~8-unit box
    /// with its camera at `(0, 1.5, 4)`. Any constant large enough for one is
    /// absurd for the other — a 512 BU fallback flung the Cornell camera 175
    /// units out of an 8-unit room and collapsed `gpu_main_render` from
    /// 4.498 ms to 0.086 ms. A scene-derived measurement is scale-free by
    /// construction; `BenchCameraPath::FALLBACK_SUBJECT_DISTANCE_BU` remains
    /// only as `pose`'s guard against a degenerate *argument*.
    ///
    /// Returns `None` when neither source can answer yet — no collision world
    /// *and* no rendered geometry — so the caller retries next frame rather
    /// than caching a miss.
    /// Returns the distance and which source produced it, so the bench log
    /// names its own provenance instead of asserting a cast that may not have
    /// run.
    fn measure_bench_subject_distance(
        &self,
        origin: crate::bench_camera::CameraPose,
    ) -> Option<(f32, &'static str)> {
        // Far enough to cross any interior and most of an exterior grid.
        const MAX_PROBE_BU: f32 = 32_768.0;
        let cast = self
            .world
            .try_resource::<byroredux_physics::PhysicsWorld>()
            .filter(|pw| pw.static_colliders_aabb().is_some())
            .and_then(|pw| pw.cast_ray(origin.position, origin.forward, MAX_PROBE_BU, None))
            .map(|hit| hit.distance)
            .filter(|d| d.is_finite() && *d > 1e-3);
        if let Some(distance) = cast {
            return Some((distance, "forward ray cast"));
        }
        self.scene_centroid_distance(origin.position)
            .map(|distance| (distance, "rendered-scene centroid"))
    }

    /// Distance from `from` to the centroid of every rendered mesh placement,
    /// in whatever unit the scene is authored in.
    ///
    /// The centroid is taken over `GlobalTransform` translations of entities
    /// carrying a `MeshHandle` — the one geometry signal present in *every*
    /// bench scene. `LocalBound`/`WorldBound` would be tighter but the
    /// synthetic `--cornell` scene attaches neither, which is precisely the
    /// scene this path exists to serve.
    ///
    /// Falls back to the placement spread (half the AABB diagonal) when the
    /// camera sits essentially *on* the centroid, so the orbit still has a
    /// non-degenerate radius without introducing a unit-bound constant.
    fn scene_centroid_distance(&self, from: byroredux_core::math::Vec3) -> Option<f32> {
        use byroredux_core::ecs::components::MeshHandle;
        use byroredux_core::ecs::GlobalTransform;

        let meshes = self.world.query::<MeshHandle>()?;
        let globals = self.world.query::<GlobalTransform>()?;
        let mut min = byroredux_core::math::Vec3::splat(f32::INFINITY);
        let mut max = byroredux_core::math::Vec3::splat(f32::NEG_INFINITY);
        let mut count = 0u32;
        for (entity, _) in meshes.iter() {
            let Some(global) = globals.get(entity) else {
                continue;
            };
            if !global.translation.is_finite() {
                continue;
            }
            min = min.min(global.translation);
            max = max.max(global.translation);
            count += 1;
        }
        if count == 0 {
            return None;
        }
        Some(centroid_subject_distance(min, max, from))
    }

    pub(crate) fn step_bench_camera(&mut self) {
        if !crate::bench::harness_active(self.bench_summary_printed) {
            // `--bench-hold` is an interactive inspection session once the
            // finite measurement is complete. Drop the last harness pose so
            // neither this phase nor the post-scheduler restore can pin the
            // fly/player camera after input has moved it.
            self.bench_camera_applied_pose = None;
            return;
        }
        let (Some(path), Some(total_frames)) = (self.bench_camera, self.bench_frames_target) else {
            return;
        };
        self.seed_bench_camera_origin();
        let Some(active) = self
            .world
            .try_resource::<byroredux_core::ecs::ActiveCamera>()
            .map(|active| active.0)
        else {
            return;
        };

        // Seeded immediately after scene setup, before the startup scheduler
        // can replace an explicit CLI pose with the character's eye transform.
        let Some(origin) = self.bench_camera_origin else {
            return;
        };

        // `grid-cross` and `grid-soak` are streaming workloads, not visual-
        // quality paths: both advance a *logical* frame that pauses while a
        // boundary is in flight, so renderer FPS cannot shorten the wall-clock
        // handoff window and supersede the work being measured.
        let uses_streaming_clock = matches!(
            path,
            crate::bench_camera::BenchCameraPath::GridCross
                | crate::bench_camera::BenchCameraPath::GridSoak
        );
        let frame = if uses_streaming_clock {
            self.bench_camera_path_frame
                .min(total_frames.saturating_sub(1))
        } else {
            self.bench_frames_count
        };
        // Lazy one-shot: physics is empty at seed time (see
        // `measure_bench_subject_distance`), so the first stepped frame with a
        // populated collision world resolves the subject distance and every
        // later frame reuses it — a per-frame re-cast would make the orbit
        // radius drift as the camera moves, which is not a fixed-radius orbit.
        if self.bench_camera_subject_distance.is_none() {
            if let Some((distance, source)) = self.measure_bench_subject_distance(origin) {
                self.bench_camera_subject_distance = Some(distance);
                log::info!("bench camera subject distance {distance:.1} BU ({source})");
            }
        }
        let subject_distance = self
            .bench_camera_subject_distance
            .unwrap_or(crate::bench_camera::BenchCameraPath::FALLBACK_SUBJECT_DISTANCE_BU);
        let pose = path.pose(
            frame,
            total_frames,
            origin.position,
            origin.forward,
            subject_distance,
        );
        self.bench_camera_applied_pose = Some(pose);
        self.apply_bench_camera_pose(active, pose);

        if path.is_cut_frame(frame, total_frames) {
            if let Some(ctx) = self.renderer.as_mut() {
                ctx.signal_temporal_discontinuity(BENCH_CUT_RECOVERY_FRAMES);
            }
        }

        // EX-08 / #2374 — one ownership sample per completed out-and-back.
        // Arming happens on the logical frame the camera returns to origin;
        // the sample itself waits for streaming to go quiet (below), because
        // the return frame is not necessarily a settled one.
        if let Some(cycle) = path.soak_cycle_completed(frame, total_frames) {
            self.pending_soak_cycle = Some(cycle);
        }

        if uses_streaming_clock {
            let boundary_in_progress = self
                .streaming
                .as_ref()
                .is_some_and(|state| state.telemetry.boundary_in_progress());
            // Take the armed sample on the first quiet frame. Sampling while a
            // boundary transaction is mid-apply reads a half-built world: the
            // reachability classes swing with whichever cells happen to have
            // their entities attached, even though residency never moved.
            if !boundary_in_progress {
                if let Some(cycle) = self.pending_soak_cycle.take() {
                    self.record_soak_ownership_cycle(cycle);
                }
            }
            self.bench_camera_path_frame = crate::bench_camera::advance_grid_cross_frame(
                self.bench_camera_path_frame,
                total_frames,
                boundary_in_progress,
            );
        }
    }

    /// Fold one completed soak traversal into the [`OwnershipTracker`].
    ///
    /// Cycle 0 establishes the baseline rather than being compared against it
    /// — see `BenchCameraPath::soak_cycle_completed`. Every later cycle is
    /// recorded, and the verdict is read out at bench-hold via
    /// `world.owners report`.
    fn record_soak_ownership_cycle(&mut self, cycle: u32) {
        let snapshot = crate::ownership_sample::sample_all(
            &self.world,
            self.renderer.as_ref(),
            self.in_use_mesh_scratch.len(),
            self.in_use_tex_scratch.len(),
        );
        let mut tracker = self
            .world
            .resource_mut::<byroredux_core::ecs::OwnershipTracker>();
        if cycle == 0 {
            tracker.set_baseline(snapshot);
            log::info!("soak: ownership baseline recorded after traversal 0");
        } else {
            tracker.record_cycle(snapshot);
            log::info!(
                "soak: ownership cycle {} recorded ({} total)",
                cycle,
                tracker.cycles().len()
            );
        }
    }

    fn apply_bench_camera_pose(
        &self,
        active: byroredux_core::ecs::EntityId,
        pose: crate::bench_camera::CameraPose,
    ) {
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
    }

    /// Reapply the already-selected pose after the scheduler's character
    /// camera sync. This deliberately does not advance `grid-cross` or signal
    /// a second cut; it only restores the transform selected above.
    pub(crate) fn restore_bench_camera_pose(&self) {
        let Some(pose) = self.bench_camera_applied_pose else {
            return;
        };
        let Some(active) = self
            .world
            .try_resource::<byroredux_core::ecs::ActiveCamera>()
            .map(|active| active.0)
        else {
            return;
        };
        self.apply_bench_camera_pose(active, pose);
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

    /// Drain player save/load input after `Scheduler::run` has joined all
    /// parallel systems, then route the definitive result to the HUD/console.
    pub(crate) fn step_player_save_actions(&mut self) {
        for (action, output) in crate::save_io::execute_pending_player_save_actions(&self.world) {
            crate::surface_save_load_output(self.debug_ui.as_mut(), action.context(), output);
        }
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
    /// discard the BGSM/BGEM template cache and `MaterialProvider::csg_cache`
    /// on every call — each rebuild re-opens and re-parses the same
    /// BSA/BA2 archives the previous provider already warmed. Fine for a
    /// single console-triggered transition; becomes a real per-door cost
    /// once Stage 4 interactive door activation ships (every door use pays
    /// the rebuild).
    ///
    /// #2706 (SF-D3-02) — this note previously also listed
    /// `MaterialProvider::sf_cdbs` among the discarded caches; no such
    /// field ever existed (`MaterialProvider::sf_cdb_count: usize` is
    /// presence-only). The real Starfield CDB byte cache
    /// (`asset_provider::material::sf_cdb_cache`, #2705) is deliberately
    /// NOT provider-scoped — it lives at module scope precisely so it
    /// keeps working across every rebuild this note describes, without
    /// waiting on the whole-provider caching below.
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

        let transition_radius = exterior_transition_radius(&args);

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

                // 2. Build the destination worldspace context FIRST,
                // before any destructive teardown — mirrors the
                // SAVE-D6-02 preflight-before-teardown posture
                // `save_io::reload_exterior_session` already established,
                // and (EX-14/15 item C2, #2369) is what makes the
                // persistent-CELL identity comparison below possible: the
                // comparison needs the destination's resolved index, which
                // only exists once this parse has run. `wrld_override`
                // pins the worldspace to what the reverse-lookup returned
                // so the heuristic search inside `build_exterior_world_context`
                // doesn't pick something else.
                match cell_loader::build_exterior_world_context(
                    &masters,
                    &esm_path,
                    grid.0,
                    grid.1,
                    transition_radius,
                    Some(&worldspace),
                ) {
                    Ok(wctx) => {
                        // 3. EX-14/15 item C2 — does this crossing resolve
                        // to the SAME persistent CELL the currently-active
                        // root already is (a child worldspace crossing
                        // back to its parent, or between siblings sharing
                        // one ancestor's persistent CELL via the WNAM
                        // chain)? If so, detach that root from the old
                        // streaming state now, before draining, so the
                        // drain's unconditional `unload_cell(persistent_root)`
                        // never sees it and it survives the crossing intact
                        // instead of being torn down and immediately
                        // rebuilt identically.
                        let preserved_persistent_root =
                            self.streaming.as_mut().and_then(|state| {
                                let root = cell_loader::persistent_root_survives_crossing(
                                    &self.world,
                                    state.persistent_root,
                                    &wctx,
                                    // #3376 — an unfinished root must be
                                    // rebuilt, not preserved: the job that
                                    // would finish it does not survive the
                                    // drain below.
                                    state.persistent_apply.is_some(),
                                )?;
                                state.persistent_root = None;
                                Some(root)
                            });

                        // 4. Tear down any existing streaming state. The
                        // ordinary grid tiles always rebuild — only the
                        // persistent-CELL root, when preserved above, is
                        // spared.
                        if self.streaming.is_some() {
                            crate::streaming_helpers::drain_streaming_state(
                                &mut self.world,
                                ctx,
                                &mut self.streaming,
                            );
                        }

                        // 5. Assemble the fresh streaming state for the
                        // destination worldspace + initial grid, handing
                        // back the preserved persistent root (if any)
                        // instead of paying a second ESM parse the way
                        // `begin_exterior_streaming` would.
                        let worldspace_key = wctx.worldspace_key.clone();
                        let tex_provider = crate::asset_provider::build_texture_provider(&args);
                        let mat_provider = crate::asset_provider::build_material_provider(&args);
                        let (state, _cam_center) = crate::scene::assemble_exterior_streaming(
                            &mut self.world,
                            ctx,
                            wctx,
                            tex_provider,
                            mat_provider,
                            grid,
                            transition_radius,
                            crate::scene::ExteriorBootstrapMode::ForegroundFirst,
                            preserved_persistent_root,
                        );
                        self.world.insert_resource(cell_loader::CurrentExteriorContext {
                            worldspace_key,
                            esm_path: esm_path.to_string(),
                            masters: masters.to_vec(),
                            grid,
                            radius_load: state.radius_load,
                            radius_unload: state.radius_unload,
                        });
                        self.streaming = Some(state);

                        // 6. Reposition the camera at the destination
                        // spawn point. Foreground-first bootstrap has made
                        // the arrival cell coherent; here we still want the
                        // XTEL-authored pose, not its terrain centre.
                        let dest_pos =
                            cell_loader::position_zup_to_yup(pending.destination_position_zup);
                        let dest_rot =
                            cell_loader::rotation_zup_to_yup_quat(pending.destination_rotation_zup);
                        cell_loader::reposition_camera(&mut self.world, dest_pos, dest_rot);
                        // #1874 — without a body move here,
                        // `camera_follow_system` snaps the camera back
                        // toward the stale (pre-transition) capsule
                        // position on the next tick, fighting this
                        // reposition every frame and producing a stuck
                        // TAA/SVGF ghost.
                        //
                        // #2869 — and it must GROUND the body against the
                        // floor-level XTEL destination, not derive it from
                        // the camera by subtracting `eye_height`
                        // (`snap_character_body_to_camera`, whose
                        // camera-is-at-eye-height premise does not hold
                        // here). See `ground_character_body_at`.
                        crate::systems::ground_character_body_at(&self.world, dest_pos);

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

/// Preserve the historical door-transition default, but honor an explicit
/// boot-time radius so traversal smokes and constrained machines do not
/// unexpectedly expand to an 11×11 exterior ring.
fn exterior_transition_radius(args: &[String]) -> i32 {
    const DEFAULT_TRANSITION_RADIUS: i32 = 5;
    args.iter()
        .position(|arg| arg == "--radius")
        .and_then(|index| args.get(index + 1))
        .map(|value| crate::scene::parse_exterior_radius(value))
        .unwrap_or(DEFAULT_TRANSITION_RADIUS)
}

#[cfg(test)]
mod tests {
    use super::exterior_transition_radius;

    #[test]
    fn door_transition_honors_the_boot_radius() {
        let args = vec!["byroredux".into(), "--radius".into(), "1".into()];
        assert_eq!(exterior_transition_radius(&args), 1);
    }

    #[test]
    fn door_transition_keeps_its_historical_default_radius() {
        assert_eq!(exterior_transition_radius(&[]), 5);
    }

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

/// Subject distance implied by a rendered-geometry AABB, seen from `from`.
///
/// Split out of [`App::scene_centroid_distance`] so the scale-free property
/// can be tested without constructing an `App`: the two bench scene families
/// differ by ~70× in unit scale, and the whole point of deriving this from the
/// scene is that neither is special-cased.
fn centroid_subject_distance(
    min: byroredux_core::math::Vec3,
    max: byroredux_core::math::Vec3,
    from: byroredux_core::math::Vec3,
) -> f32 {
    let centre = (min + max) * 0.5;
    let distance = from.distance(centre);
    if distance > 1e-3 {
        distance
    } else {
        // Camera sits on the centroid — orbit the placement spread instead so
        // the radius is non-degenerate without inventing a unit-bound number.
        ((max - min).length() * 0.5).max(1e-3)
    }
}

#[cfg(test)]
mod bench_subject_distance_tests {
    use super::centroid_subject_distance;
    use byroredux_core::math::Vec3;

    /// The property that broke Cornell: the measurement must follow the
    /// scene's own scale. A constant sized for a Bethesda interior (512 BU)
    /// flung the `--cornell` camera 175 units out of an 8-unit box and
    /// collapsed `gpu_main_render` from 4.498 ms to 0.086 ms.
    #[test]
    fn subject_distance_tracks_the_scene_scale() {
        // Cornell: ~8-unit box at the origin, camera at (0, 1.5, 4).
        let cornell = centroid_subject_distance(
            Vec3::splat(-4.0),
            Vec3::splat(4.0),
            Vec3::new(0.0, 1.5, 4.0),
        );
        assert!(
            (1.0..20.0).contains(&cornell),
            "Cornell-scale subject distance {cornell} is not room-scale for an 8-unit box"
        );

        // A Bethesda interior placed far off-origin — the Prospector shape.
        let interior = centroid_subject_distance(
            Vec3::new(400.0, 3500.0, -400.0),
            Vec3::new(700.0, 3700.0, -100.0),
            Vec3::new(536.0, 3560.0, -272.0),
        );
        assert!(
            (1.0..500.0).contains(&interior),
            "interior subject distance {interior} is not room-scale"
        );
        // Crucially it is NOT the camera's distance from the world origin,
        // which is what the pre-fix radius used.
        let from_world_origin = Vec3::new(536.0, 3560.0, -272.0).distance(Vec3::ZERO);
        assert!(
            interior < from_world_origin / 10.0,
            "subject distance {interior} is tracking the cell's 3610 BU offset \
             from the world origin again"
        );
    }

    /// A camera sitting exactly on the centroid must still get a usable
    /// radius rather than collapsing the orbit to a point.
    #[test]
    fn camera_on_the_centroid_falls_back_to_the_placement_spread() {
        let centre = Vec3::new(10.0, 20.0, 30.0);
        let distance = centroid_subject_distance(
            centre - Vec3::splat(50.0),
            centre + Vec3::splat(50.0),
            centre,
        );
        assert!(distance > 1.0, "degenerate radius {distance}");
    }

    /// A single-placement scene has zero spread; the result must still be
    /// finite and positive.
    #[test]
    fn degenerate_aabb_is_still_positive() {
        let p = Vec3::new(5.0, 5.0, 5.0);
        let distance = centroid_subject_distance(p, p, p);
        assert!(distance > 0.0 && distance.is_finite(), "got {distance}");
    }
}
