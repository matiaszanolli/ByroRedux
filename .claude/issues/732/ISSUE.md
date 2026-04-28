# LIFE-H2: SIGSEGV at exterior shutdown — 22 outstanding allocator refs persist after explicit per-cell unload sweep

## Severity: MEDIUM
Process always crashes on clean window-close after exterior streaming.

## Surfaced By
M40 Phase 1b first FNV WastelandNV streaming session (2026-04-27, commits `592e7bf` and `7dc354a`).

## Description
Closing the engine window after any exterior streaming session SIGSEGVs (exit 139) during `VulkanContext::Drop`. `gpu_allocator` reports 22 outstanding references both before and after the new shutdown sweep landed in `7dc354a`.

The `7dc354a` fix walks `WorldStreamingState.loaded` and calls `cell_loader::unload_cell` per cell_root before `self.renderer.take()`. Worker thread exits cleanly. But 22 outstanding refs persist.

## Hypothesis
`unload_cell` queues per-mesh BLAS for destruction via `AccelerationManager::drop_blas` which uses a `MAX_FRAMES_IN_FLIGHT` countdown (`pending_destroy_blas` queue). On gameplay cell transitions, subsequent frames tick the counter and the destroy fires. On shutdown, NO subsequent frames between unload sweep and `ctx.take()`, so the pending queue never drains.

`#639` (LIFE-H1) closed the runtime counterpart of this.

## Suggested Fix
Option 2: drain pattern in shutdown sweep — after the per-cell `unload_cell` loop, explicitly call `accel.drain_all_pending(...)` then `mesh_registry.drain_pending(...)` before `self.renderer.take()`.

## Repro
```
cargo run --release -- --esm "Fallout New Vegas/Data/FalloutNV.esm" --grid 0,0 --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" --textures-bsa "Fallout - Textures2.bsa"
```
Wait for streaming bootstrap → close window. Process exits 139.
