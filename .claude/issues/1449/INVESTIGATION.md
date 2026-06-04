# Investigation — #1449 MEM-01 evict_unused_blas immediate-destroy invariant

**Domain:** renderer (GPU memory / acceleration structures)

## Finding
`evict_unused_blas` (`crates/renderer/src/vulkan/acceleration/blas_static.rs`)
destroys the `VkAccelerationStructureKHR` immediately (no `pending_destroy_blas`
round-trip). The existing doc already proves this sound via
`MIN_IDLE_FRAMES > MAX_FRAMES_IN_FLIGHT` (const-assert). MEM-01 identifies an
*additional, unstated* assumption: that `frame_counter` advances at most once
per **retired** frame. `build_blas_batched` bumps `frame_counter` once per batch
(`:397`) with no `draw_frame`/`build_tlas` between batches, so during a
multi-batch cell load the counter can outrun the GPU's retired frames. Today
that is harmless because cell-load bursts run inside the gated load flow, never
interleaved with in-flight draw frames.

## Action taken
Per the issue ("no code change recommended now" beyond the note), added the
missing INVARIANT paragraph to the `evict_unused_blas` doc comment, recording
that the immediate-destroy path additionally assumes cell-load bursts are not
interleaved with in-flight draw frames, and what to do if a future
streaming-during-render refactor breaks that (route through
`pending_destroy_blas`, or gate the per-batch bump). No behavioural change.

## Completeness checks
- [x] Invariant note added at the immediate-destroy site.
- [x] **DROP**: N/A — eviction was NOT rerouted through `pending_destroy_blas`
  (the issue recommends that only *if* streaming-during-render lands); no
  lifecycle change in this commit.

## Verification
Doc-only change. `cargo test` (workspace): 2790 passed, 0 failed.
