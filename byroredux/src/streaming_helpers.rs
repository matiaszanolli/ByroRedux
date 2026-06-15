//! Free-function helpers for the per-frame cell-streaming chain — split
//! out of `main.rs` to stay below the 2000-LOC ceiling (TD9-NEW-01 /
//! #1267). These functions intentionally take their dependencies as
//! arguments rather than `&mut self` on `App` so the caller can
//! split-borrow `&mut self.world` / `&mut self.streaming` / `&mut
//! self.renderer` without aliasing — an `App::foo(&mut self)` method
//! signature can't express that.

use crate::cell_loader::ObjectLodBlock;
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

/// Drain both distant-LOD rings out of a worldspace-streaming state,
/// returning the resident blocks so the caller can hand each to its
/// canonical reclaim fn (`unload_lod_block` / `unload_object_lod_block`).
/// Pure over the two maps (no `World` / `VulkanContext`) so the "LOD blocks
/// are part of the worldspace-drain reclaim set" contract is unit-testable
/// without a GPU device — these blocks carry no `CellRoot`, so the only
/// thing that proves they're reclaimed on a mid-session transition is this
/// collection step (#1536). Mirrors the `collect_victim_gpu_handles` (#1341)
/// extraction in `cell_loader::unload`.
pub(crate) fn drain_lod_reclaim_targets(
    lod_blocks: &mut HashMap<(i32, i32), LodBlock>,
    object_lod_blocks: &mut HashMap<(i32, i32), ObjectLodBlock>,
) -> (Vec<LodBlock>, Vec<ObjectLodBlock>) {
    (
        lod_blocks.drain().map(|(_, b)| b).collect(),
        object_lod_blocks.drain().map(|(_, b)| b).collect(),
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
    let (lod_blocks, object_lod_blocks) =
        drain_lod_reclaim_targets(&mut state.lod_blocks, &mut state.object_lod_blocks);
    log::info!(
        "Cell transition: draining {} streamed cells + {} terrain-LOD + {} object-LOD blocks before swap",
        cells.len(),
        lod_blocks.len(),
        object_lod_blocks.len(),
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
            hole_mask: 0,
        }
    }

    /// #1536 — the worldspace drain must reclaim BOTH LOD rings. The pure
    /// collector empties both maps and returns every resident block so the
    /// caller's reclaim loop sees them (pre-fix the maps were never touched,
    /// leaking the whole ring on every exterior→interior transition).
    #[test]
    fn drain_collects_and_empties_both_lod_rings() {
        let mut terrain: HashMap<(i32, i32), LodBlock> = HashMap::new();
        terrain.insert((0, 0), lod(1, 10));
        terrain.insert((1, 0), lod(2, 11));
        let mut objects: HashMap<(i32, i32), ObjectLodBlock> = HashMap::new();
        objects.insert(
            (0, 0),
            ObjectLodBlock {
                entities: vec![3],
                mesh_handles: vec![12, 13],
            },
        );

        let (terrain_out, object_out) = drain_lod_reclaim_targets(&mut terrain, &mut objects);

        assert_eq!(terrain_out.len(), 2, "both terrain LOD blocks collected");
        assert_eq!(object_out.len(), 1, "the object LOD quad collected");
        assert!(terrain.is_empty(), "terrain ring drained — no leak left behind");
        assert!(objects.is_empty(), "object ring drained — no leak left behind");
        // Mesh handles that the reclaim loop will `drop_mesh` are preserved.
        let mut meshes: Vec<u32> = terrain_out.iter().map(|b| b.mesh_handle).collect();
        meshes.extend(object_out.iter().flat_map(|b| b.mesh_handles.iter().copied()));
        meshes.sort_unstable();
        assert_eq!(meshes, vec![10, 11, 12, 13]);
    }

    /// Empty rings drain to empty vecs — the common interior→interior or
    /// no-LOD-resident transition is a clean no-op.
    #[test]
    fn drain_of_empty_rings_is_noop() {
        let mut terrain: HashMap<(i32, i32), LodBlock> = HashMap::new();
        let mut objects: HashMap<(i32, i32), ObjectLodBlock> = HashMap::new();
        let (t, o) = drain_lod_reclaim_targets(&mut terrain, &mut objects);
        assert!(t.is_empty() && o.is_empty());
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
pub fn consume_streaming_payload(
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
                let freed = {
                    let mut reg = world.resource_mut::<cell_loader::NifImportRegistry>();
                    reg.insert(cache_key, None)
                };
                // #863 — release LRU-evicted clip handles. Negative
                // cache inserts can still trigger eviction of older
                // entries when `BYRO_NIF_CACHE_MAX > 0`.
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
            // Newly-spawned instances mean a TLAS rebuild + fresh
            // pixels with no history. Bump the SVGF/TAA recovery
            // window so the ghosting transient on the just-streamed
            // geometry is washed out in ~8 frames instead of 30+ at
            // the steady-state α. See #801 / STRM-N1.
            ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);
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
