//! Free-function helpers for the per-frame cell-streaming chain — split
//! out of `main.rs` to stay below the 2000-LOC ceiling (TD9-NEW-01 /
//! #1267). These functions intentionally take their dependencies as
//! arguments rather than `&mut self` on `App` so the caller can
//! split-borrow `&mut self.world` / `&mut self.streaming` / `&mut
//! self.renderer` without aliasing — an `App::foo(&mut self)` method
//! signature can't express that.

use crate::cell_loader::{
    FrameTimeBudget, LodReconcileInput, LodWorkBudget, ObjectLodBlock, PlacementLodBlock,
};
use crate::streaming::LodBlock;
use crate::{cell_loader, streaming};
use std::collections::HashMap;

/// Cell-streaming SVGF/TAA recovery window — bumps both pipelines'
/// elevated-α / history-reset windows when a cell loads or unloads,
/// so trail ghosting on freshly-streamed geometry is washed out in
/// this many frames instead of 30+ at the steady-state α=0.2 floor.
/// At 60 FPS that's ~130 ms of recovery, comparable to TAA history-
/// reset windows. See #801 / STRM-N1.
pub const SVGF_TAA_STREAMING_RECOVERY_FRAMES: u32 = 8;

/// Result of reconciling all game-specific distant-LOD providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LodReconcileProgress {
    /// Every desired coordinate is resident or a known asset miss.
    pub complete: bool,
    /// Archive/import/upload attempts charged across all three providers.
    pub attempted: usize,
}

/// Decide whether and how much deferred LOD work belongs in this frame.
///
/// Full-detail cell spawns win the main-thread budget. A boundary crossing
/// still runs a zero-budget reconcile so stale/out-of-ring geometry is
/// reclaimed immediately, but new LOD work waits for the next idle frame.
pub(crate) fn lod_reconcile_budget_for_frame(
    reconcile_pending: bool,
    cells_spawned: usize,
    grid_changed: bool,
    idle_attempts_per_provider: usize,
) -> Option<usize> {
    if !reconcile_pending {
        None
    } else if cells_spawned == 0 {
        Some(idle_attempts_per_provider)
    } else if grid_changed {
        Some(0)
    } else {
        None
    }
}

/// Reconcile terrain, baked-object, and placement-object LOD through one
/// shared policy boundary. Each provider gets its own allowance so a large
/// terrain ring cannot starve the active game-specific object scheme.
///
/// Reclaims are always immediate inside the provider functions. The budget
/// covers only potentially expensive archive/import/upload attempts. Passing
/// `usize::MAX` preserves the deterministic full-radius bootstrap contract.
pub(crate) fn reconcile_lod_rings(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    state: &mut streaming::WorldStreamingState,
    player_grid: (i32, i32),
    max_attempts_per_provider: usize,
) -> LodReconcileProgress {
    let tex_provider = state.tex_provider.clone();
    let wctx = state.wctx.clone();
    let input = LodReconcileInput {
        tex_provider: tex_provider.as_ref(),
        wctx: wctx.as_ref(),
        player_grid,
        max_full_cell_radius: state.radius_unload,
    };
    let make_budget = || {
        if max_attempts_per_provider == usize::MAX {
            LodWorkBudget::unlimited()
        } else {
            LodWorkBudget::new(max_attempts_per_provider)
        }
    };

    let mut terrain_budget = make_budget();
    let terrain_complete = cell_loader::stream_lod_blocks(
        world,
        ctx,
        &input,
        &mut state.lod_blocks,
        &mut state.lod_missing_blocks,
        &mut terrain_budget,
    );

    let mut object_budget = make_budget();
    let object_complete = cell_loader::stream_object_lod_blocks(
        world,
        ctx,
        &input,
        &mut state.object_lod_blocks,
        &mut object_budget,
    );

    let mut placement_budget = make_budget();
    let placement_complete = cell_loader::stream_placement_lod_blocks(
        world,
        ctx,
        &input,
        &mut state.placement_lod_blocks,
        &mut placement_budget,
    );

    let attempted = terrain_budget.spent() + object_budget.spent() + placement_budget.spent();
    if attempted > 0 {
        flush_pending_lod_textures(ctx);
    }

    LodReconcileProgress {
        complete: terrain_complete && object_complete && placement_complete,
        attempted,
    }
}

/// LOD texture resolution happens after normal cell-load texture flushing.
/// Flush those deferred DDS uploads at the same boundary that owns the LOD
/// work so a static camera never leaves the ring bound to checker slots.
fn flush_pending_lod_textures(ctx: &mut byroredux_renderer::VulkanContext) {
    let Some(allocator) = ctx.allocator.as_ref() else {
        return;
    };
    let pending = ctx.texture_registry.pending_dds_upload_count();
    if pending == 0 {
        return;
    }
    if let Err(e) = ctx.texture_registry.flush_pending_uploads(
        &ctx.device,
        allocator,
        &ctx.graphics_queue,
        ctx.transfer_pool,
        &ctx.transfer_fence,
    ) {
        log::warn!("LOD texture flush failed ({pending} pending): {e}");
    }
}

/// Drain all three distant-LOD rings out of a worldspace-streaming state,
/// returning the resident blocks so the caller can hand each to its
/// canonical reclaim fn (`unload_lod_block` / `unload_object_lod_block` /
/// `unload_placement_lod_block`). Pure over the maps (no `World` /
/// `VulkanContext`) so the "LOD blocks are part of the worldspace-drain
/// reclaim set" contract is unit-testable without a GPU device — these
/// blocks carry no `CellRoot`, so the only thing that proves they're
/// reclaimed on a mid-session transition is this collection step (#1536).
/// Mirrors the `collect_victim_gpu_handles` (#1341) extraction in
/// `cell_loader::unload`. `placement_lod_blocks` (#1726) and
/// `object_lod_blocks` are mutually exclusive per game, but both are drained
/// unconditionally so the reclaim set is game-agnostic.
pub(crate) fn drain_lod_reclaim_targets(
    lod_blocks: &mut HashMap<(i32, i32), LodBlock>,
    object_lod_blocks: &mut HashMap<(i32, i32), ObjectLodBlock>,
    placement_lod_blocks: &mut HashMap<(i32, i32), PlacementLodBlock>,
) -> (Vec<LodBlock>, Vec<ObjectLodBlock>, Vec<PlacementLodBlock>) {
    (
        lod_blocks.drain().map(|(_, b)| b).collect(),
        object_lod_blocks.drain().map(|(_, b)| b).collect(),
        placement_lod_blocks.drain().map(|(_, b)| b).collect(),
    )
}

pub fn drain_streaming_state(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    streaming_slot: &mut Option<streaming::WorldStreamingState>,
) {
    let Some(mut state) = streaming_slot.take() else {
        return;
    };
    cancel_active_streaming_apply(world, ctx, &mut state);
    let cells: Vec<_> = state.loaded.drain().collect();
    // #1536 — LOD blocks (terrain + object) carry no `CellRoot`, so
    // `unload_cell`'s `CellRootIndex` victim walk can't reach them; their
    // ONLY reclaim path is `unload_{,object_}lod_block`. Pre-fix
    // `drain_streaming_state` iterated only `state.loaded`, so an
    // exterior→interior door-walk mid-session leaked the entire resident LOD
    // ring (up to ~hundreds of blocks: a global-geometry SSBO range + base
    // ground texture refcount + ECS row each). Collect both rings via the
    // pure `drain_lod_reclaim_targets` (unit-tested without a GPU) and feed
    // each through its canonical reclaim fn.
    let (lod_blocks, object_lod_blocks, placement_lod_blocks) = drain_lod_reclaim_targets(
        &mut state.lod_blocks,
        &mut state.object_lod_blocks,
        &mut state.placement_lod_blocks,
    );
    log::info!(
        "Cell transition: draining {} streamed cells + {} terrain-LOD + {} object-LOD + {} placement-LOD blocks before swap",
        cells.len(),
        lod_blocks.len(),
        object_lod_blocks.len(),
        placement_lod_blocks.len(),
    );
    for ((_gx, _gy), slot) in cells {
        cell_loader::unload_cell(world, ctx, slot.cell_root);
    }
    for block in &lod_blocks {
        cell_loader::unload_lod_block(world, ctx, block);
    }
    for block in &object_lod_blocks {
        cell_loader::unload_object_lod_block(world, ctx, block);
    }
    for block in &placement_lod_blocks {
        cell_loader::unload_placement_lod_block(world, ctx, block);
    }
    // Mirrors the CloseRequested path — release per-queue Arc
    // clones explicitly before tearing down the rest of the
    // streaming state.
    ctx.flush_pending_destroys();
    state.shutdown(std::time::Duration::from_secs(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lod(entity: u32, mesh: u32) -> LodBlock {
        LodBlock {
            entity, // EntityId == u32
            mesh_handle: mesh,
            texture_handle: 0,
            hole_mask: 0,
        }
    }

    /// #1536 / #1726 — the worldspace drain must reclaim ALL THREE LOD rings.
    /// The pure collector empties every map and returns every resident block
    /// so the caller's reclaim loop sees them (pre-fix the maps were never
    /// touched, leaking the whole ring on every exterior→interior transition).
    #[test]
    fn drain_collects_and_empties_all_lod_rings() {
        let mut terrain: HashMap<(i32, i32), LodBlock> = HashMap::new();
        terrain.insert((0, 0), lod(1, 10));
        terrain.insert((1, 0), lod(2, 11));
        let mut objects: HashMap<(i32, i32), ObjectLodBlock> = HashMap::new();
        objects.insert(
            (0, 0),
            ObjectLodBlock {
                entities: vec![3],
                mesh_handles: vec![12, 13],
                texture_handle: 0,
            },
        );
        let mut placements: HashMap<(i32, i32), PlacementLodBlock> = HashMap::new();
        placements.insert(
            (2, 0),
            PlacementLodBlock {
                entities: vec![4, 5],
                mesh_handles: vec![14],
                texture_handles: vec![20],
            },
        );

        let (terrain_out, object_out, placement_out) =
            drain_lod_reclaim_targets(&mut terrain, &mut objects, &mut placements);

        assert_eq!(terrain_out.len(), 2, "both terrain LOD blocks collected");
        assert_eq!(object_out.len(), 1, "the object LOD quad collected");
        assert_eq!(placement_out.len(), 1, "the placement LOD cell collected");
        assert!(
            terrain.is_empty(),
            "terrain ring drained — no leak left behind"
        );
        assert!(
            objects.is_empty(),
            "object ring drained — no leak left behind"
        );
        assert!(
            placements.is_empty(),
            "placement ring drained — no leak left behind"
        );
        // Mesh handles that the reclaim loop will `drop_mesh` are preserved.
        let mut meshes: Vec<u32> = terrain_out.iter().map(|b| b.mesh_handle).collect();
        meshes.extend(
            object_out
                .iter()
                .flat_map(|b| b.mesh_handles.iter().copied()),
        );
        meshes.extend(
            placement_out
                .iter()
                .flat_map(|b| b.mesh_handles.iter().copied()),
        );
        meshes.sort_unstable();
        assert_eq!(meshes, vec![10, 11, 12, 13, 14]);
    }

    #[test]
    fn deferred_lod_uses_only_idle_main_thread_frames() {
        const CAP: usize = 2;
        assert_eq!(
            lod_reconcile_budget_for_frame(true, 0, false, CAP),
            Some(CAP),
            "an idle frame advances the ring"
        );
        assert_eq!(
            lod_reconcile_budget_for_frame(true, 1, false, CAP),
            None,
            "full-detail cell apply owns a normal frame"
        );
        assert_eq!(
            lod_reconcile_budget_for_frame(true, 1, true, CAP),
            Some(0),
            "a crossing still performs immediate reclaim with no new work"
        );
        assert_eq!(
            lod_reconcile_budget_for_frame(false, 0, true, CAP),
            None,
            "a settled ring does no steady-state work"
        );
    }

    /// Empty rings drain to empty vecs — the common interior→interior or
    /// no-LOD-resident transition is a clean no-op.
    #[test]
    fn drain_of_empty_rings_is_noop() {
        let mut terrain: HashMap<(i32, i32), LodBlock> = HashMap::new();
        let mut objects: HashMap<(i32, i32), ObjectLodBlock> = HashMap::new();
        let mut placements: HashMap<(i32, i32), PlacementLodBlock> = HashMap::new();
        let (t, o, p) = drain_lod_reclaim_targets(&mut terrain, &mut objects, &mut placements);
        assert!(t.is_empty() && o.is_empty() && p.is_empty());
    }
}

/// Result of applying one worker payload through the canonical exterior
/// main-thread boundary.
pub enum StreamingPayloadOutcome {
    /// The payload no longer matches an in-flight request and was discarded
    /// before any cache, ECS, or GPU mutation.
    DroppedStale,
    /// The request was current and its main-thread apply path ran. `center`
    /// is present when the coordinate resolved to a real exterior cell; it is
    /// absent for worldspace holes and failed cell spawns.
    Applied {
        coord: (i32, i32),
        center: Option<byroredux_core::math::Vec3>,
    },
}

fn finish_streaming_import(
    world: &mut byroredux_core::ecs::World,
    state: &mut streaming::WorldStreamingState,
    model_path: String,
    partial_opt: Option<streaming::PartialNifImport>,
) {
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
            let freed = {
                let mut reg = world.resource_mut::<cell_loader::NifImportRegistry>();
                reg.insert(cache_key, None)
            };
            // #863 — negative cache inserts can evict a parsed entry carrying
            // an animation clip, so release those slots just like the
            // synchronous payload path.
            if !freed.is_empty() {
                let mut clip_reg =
                    world.resource_mut::<byroredux_core::animation::AnimationClipRegistry>();
                for h in freed {
                    clip_reg.release(h);
                }
            }
        }
    }
}

fn cancel_streaming_apply_job(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    job: streaming::StreamingCellApplyJob,
) {
    if let streaming::StreamingCellApplyPhase::Spawn(cell_job) = job.phase {
        cell_job.cancel(world, ctx);
    }
}

/// Cancel and reclaim the partially-applied cell, if any.
///
/// Used by worldspace drains; normal boundary cancellation happens
/// automatically when the active job's generation no longer exists in
/// `pending`.
pub(crate) fn cancel_active_streaming_apply(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    state: &mut streaming::WorldStreamingState,
) {
    if let Some(job) = state.active_apply.take() {
        state.pending.remove(&job.coord);
        cancel_streaming_apply_job(world, ctx, job);
    }
}

/// Advance steady-state main-thread cell application under one shared
/// deadline. Stale payloads are discarded for free; current work proceeds
/// through NIF finalization, exterior setup, then one-or-more placed REFRs.
pub(crate) fn advance_streaming_apply(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    state: &mut streaming::WorldStreamingState,
    budget: &mut FrameTimeBudget,
) -> bool {
    loop {
        if budget.should_yield() {
            return budget.completed_units() > 0;
        }

        if state.active_apply.is_none() {
            let payload = loop {
                let Ok(payload) = state.payload_rx.try_recv() else {
                    return budget.completed_units() > 0;
                };
                let coord = (payload.gx, payload.gy);
                match streaming::classify_payload(&state.pending, coord, payload.generation) {
                    streaming::PayloadDecision::Apply => break payload,
                    streaming::PayloadDecision::StaleNewerPending { .. }
                    | streaming::PayloadDecision::StaleNoPending => {
                        log::debug!(
                            "Dropping stale streaming payload ({},{}) gen={}",
                            payload.gx,
                            payload.gy,
                            payload.generation
                        );
                    }
                }
            };
            state.active_apply = Some(streaming::StreamingCellApplyJob::from_payload(payload));
        }

        let mut job = state
            .active_apply
            .take()
            .expect("active apply was populated above");
        if !matches!(
            streaming::classify_payload(&state.pending, job.coord, job.generation),
            streaming::PayloadDecision::Apply
        ) {
            log::debug!(
                "Cancelling stale partial streaming apply ({},{}) gen={}",
                job.coord.0,
                job.coord.1,
                job.generation,
            );
            cancel_streaming_apply_job(world, ctx, job);
            continue;
        }

        match job.phase {
            streaming::StreamingCellApplyPhase::FinishImports(mut imports) => {
                if let Some((model_path, partial)) = imports.next() {
                    finish_streaming_import(world, state, model_path, partial);
                    budget.complete_unit();
                    job.phase = streaming::StreamingCellApplyPhase::FinishImports(imports);
                    state.active_apply = Some(job);
                } else {
                    job.phase = streaming::StreamingCellApplyPhase::BeginExterior;
                    state.active_apply = Some(job);
                }
            }
            streaming::StreamingCellApplyPhase::BeginExterior => {
                let wctx = state.wctx.clone();
                match cell_loader::ExteriorCellApplyJob::begin(
                    wctx.as_ref(),
                    job.coord.0,
                    job.coord.1,
                    world,
                    ctx,
                    state.tex_provider.as_ref(),
                    Some(&mut state.mat_provider),
                    None,
                    budget,
                ) {
                    Ok(Some(cell_job)) => {
                        job.phase = streaming::StreamingCellApplyPhase::Spawn(cell_job);
                        state.active_apply = Some(job);
                    }
                    Ok(None) => {
                        state.pending.remove(&job.coord);
                    }
                    Err(e) => {
                        state.pending.remove(&job.coord);
                        log::warn!(
                            "Streaming cell ({},{}) setup failed after pre-parse: {:#}",
                            job.coord.0,
                            job.coord.1,
                            e
                        );
                    }
                }
            }
            streaming::StreamingCellApplyPhase::Spawn(cell_job) => {
                let wctx = state.wctx.clone();
                match cell_job.advance(
                    wctx.as_ref(),
                    world,
                    ctx,
                    state.tex_provider.as_ref(),
                    Some(&mut state.mat_provider),
                    budget,
                ) {
                    cell_loader::ExteriorCellApplyProgress::Pending(cell_job) => {
                        job.phase = streaming::StreamingCellApplyPhase::Spawn(cell_job);
                        state.active_apply = Some(job);
                    }
                    cell_loader::ExteriorCellApplyProgress::Complete(info) => {
                        state.pending.remove(&job.coord);
                        state.loaded.insert(
                            job.coord,
                            streaming::LoadedCell {
                                cell_root: info.cell_root,
                            },
                        );
                        ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);
                    }
                }
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
#[tracing::instrument(
    name = "consume_streaming_payload",
    skip_all,
    fields(gx = payload.gx, gy = payload.gy, generation = payload.generation),
)]
/// Returns an explicit outcome so both progressive bootstrap and steady-state
/// bootstrap modes can distinguish a current payload from a cheap stale drop.
/// Steady-state streaming uses [`advance_streaming_apply`] instead so the same
/// work can yield between NIFs and placed references.
pub fn consume_streaming_payload(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    state: &mut streaming::WorldStreamingState,
    payload: streaming::LoadCellPayload,
) -> StreamingPayloadOutcome {
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
            return StreamingPayloadOutcome::DroppedStale;
        }
    }

    // Finish-import every pre-parsed entry into the cache. Subsequent
    // load_one_exterior_cell calls now hit cache for every NIF.
    let wctx = state.wctx.clone();
    for (model_path, partial_opt) in payload.parsed {
        finish_streaming_import(world, state, model_path, partial_opt);
    }

    // Spawn pass — every NIF lookup hits cache (slow parse path skipped).
    let center = match cell_loader::load_one_exterior_cell(
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
            // Newly-spawned instances mean a TLAS rebuild + fresh
            // pixels with no history. Bump the SVGF/TAA recovery
            // window so the ghosting transient on the just-streamed
            // geometry is washed out in ~8 frames instead of 30+ at
            // the steady-state α. See #801 / STRM-N1.
            ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);
            Some(info.center)
        }
        Ok(None) => {
            // Worldspace hole — common at edges; pending entry already
            // cleared above.
            None
        }
        Err(e) => {
            log::warn!(
                "Streaming cell ({},{}) spawn failed after pre-parse: {:#}",
                payload.gx,
                payload.gy,
                e
            );
            None
        }
    };
    StreamingPayloadOutcome::Applied { coord, center }
}
