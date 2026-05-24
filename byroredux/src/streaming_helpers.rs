//! Free-function helpers for the per-frame cell-streaming chain — split
//! out of `main.rs` to stay below the 2000-LOC ceiling (TD9-NEW-01 /
//! #1267). These functions intentionally take their dependencies as
//! arguments rather than `&mut self` on `App` so the caller can
//! split-borrow `&mut self.world` / `&mut self.streaming` / `&mut
//! self.renderer` without aliasing — an `App::foo(&mut self)` method
//! signature can't express that.

use crate::{cell_loader, streaming};

/// Cell-streaming SVGF/TAA recovery window — bumps both pipelines'
/// elevated-α / history-reset windows when a cell loads or unloads,
/// so trail ghosting on freshly-streamed geometry is washed out in
/// this many frames instead of 30+ at the steady-state α=0.2 floor.
/// At 60 FPS that's ~130 ms of recovery, comparable to TAA history-
/// reset windows. See #801 / STRM-N1.
pub const SVGF_TAA_STREAMING_RECOVERY_FRAMES: u32 = 8;

pub fn drain_streaming_state(
    world: &mut byroredux_core::ecs::World,
    ctx: &mut byroredux_renderer::VulkanContext,
    streaming_slot: &mut Option<streaming::WorldStreamingState>,
) {
    let Some(mut state) = streaming_slot.take() else {
        return;
    };
    let cells: Vec<_> = state.loaded.drain().collect();
    log::info!(
        "Cell transition: draining {} streamed cells before swap",
        cells.len()
    );
    for ((_gx, _gy), slot) in cells {
        cell_loader::unload_cell(world, ctx, slot.cell_root);
    }
    // Mirrors the CloseRequested path — release per-queue Arc
    // clones explicitly before tearing down the rest of the
    // streaming state.
    ctx.flush_pending_destroys();
    state.shutdown(std::time::Duration::from_secs(1));
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
