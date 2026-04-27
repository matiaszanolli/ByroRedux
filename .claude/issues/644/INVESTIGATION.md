# Investigation: MEM-2-2 / #644 — scratch barrier missing before first per-frame refit

## Audit premise (re-verified)

`build_blas` (line 472), `build_skinned_blas` (line 709), and `refit_skinned_blas` (line 902) all share `blas_scratch_buffer`. The first two use `with_one_time_commands_reuse_fence` (sync); refit records into the per-frame `cmd`.

Per `vulkan/context/draw.rs:587-611`, the per-frame refit loop emits `record_scratch_serialize_barrier` **between** iterations (#642 closure), but **not before the first iteration**. Verified with current line numbers.

## The hazard

Sequence inside one frame:

1. **First-sight loop** (draw.rs:462–536): for any new skinned entity, sync compute prime → sync `build_skinned_blas` (one-time fenced cmd, fence-wait blocks host).
2. **Per-frame steady-state** (draw.rs:542+): records compute dispatches + AS_WRITE↔AS_WRITE barriers for the refit-loop into per-frame `cmd`.
3. **Per-frame cmd is submitted later** at end-of-frame.

The first-sight BUILD in step 1 writes the shared `blas_scratch_buffer`. The fence-wait makes those writes durable on the device but does NOT establish a device-side memory dependency for *subsequent* submissions — the per-frame `cmd` records refits that READ/WRITE the same scratch with no `AS_WRITE → AS_WRITE` barrier ordering them against the prior submission's BUILD. Spec violation per `VkAccelerationStructureBuildGeometryInfoKHR > scratchData`: builds sharing scratch require an explicit memory barrier between them, regardless of submission boundary.

## Same hazard pattern, broader scope

`build_blas_batched` is also a sync one-time-fenced caller of the same scratch buffer. If a cell-load triggers `build_blas_batched` mid-frame (before the per-frame `cmd` is submitted), the same gap exists between BUILD's writes and the first per-frame refit's reads.

## Fix

Promote the existing `record_scratch_serialize_barrier` from "between iterations" to "before every iteration" in the per-frame refit loop. Cost: one extra `vkCmdPipelineBarrier` per frame in the no-first-sight-build case (negligible — AS_WRITE↔AS_WRITE same-stage barrier with no prior work is essentially free).

This single barrier covers:
- the audit's specific case (`build_skinned_blas` → first refit, same frame),
- the symmetric `build_blas_batched` → first refit case,
- and remains the correct iteration barrier for refit→refit chains (#642).

## Scope

- 1 file: `crates/renderer/src/vulkan/context/draw.rs`
- No shader change, no SPIR-V recompile.
- No `unsafe` change beyond restructuring an existing block.
