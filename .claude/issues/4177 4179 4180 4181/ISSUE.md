## #4177 [OPEN] CONC-D1-01: Static BLAS batch build emits no scratch-serialize barrier before its first build (HYPOTHESIS — would be HIGH)
labels: bug, renderer, high, sync

**Description**: **Severity as reported: HYPOTHESIS — would be HIGH if confirmed by validation-layer/RenderDoc.** `AccelerationManager::blas_scratch_buffer` is a single shared allocation written by three call sites: static batched builds, skinned first-sight builds, and skinned refits. The skinned path emits a scratch-serialize barrier *before its first build* precisely because a host fence-wait between submissions does not establish a device-side memory dependency (#1300). The static path's loop only emits the barrier for `i > 0`, so its first `cmd_build_acceleration_structures` in the one-time submission has none. `build_blas_batched` runs from `step_streaming`/`restore_missing_static_blas_for_draws` (`byroredux/src/app_frame.rs:263`), both *before* `draw_frame`'s top-of-frame `wait_for_fences`, so a prior frame's skinned BUILD/refit against the same scratch address may still be executing on the GPU when this batch submits. Hazard exists only when `scratch_needs_growth` is false (steady state).

**Evidence**:
```rust
// blas_static.rs:694-698 (confirmed present)
for (i, p) in prepared.iter().enumerate() {
    if i > 0 {
        self.record_scratch_serialize_barrier(device, cmd);
```
vs.
```rust
// blas_skinned.rs:294-313 (confirmed present)
if !prepared.is_empty() {
    self.record_scratch_serialize_barrier(device, cmd);
}
for (i, p) in prepared.iter().enumerate() {
    if i > 0 { self.record_scratch_serialize_barrier(device, cmd); }
```
`acceleration/predicates.rs:646-681` (`ScratchUser::CrossSubmissionBuildWithFenceWait` => true) and its test `acceleration/tests/scratch_tests.rs:400-422` enumerate exactly two sites relying on the rule — `refit_skinned_blas` and `build_skinned_blas_batched_on_cmd`'s `i == 0`. The static path's `i == 0` is absent from that enumeration.

**Impact**: Write-after-write on the shared AS-build scratch region between an in-flight skinned BUILD/refit and a streaming static BLAS build. Undefined BVH contents for the loser: garbled/exploded RT shadows/reflections/GI on streamed statics or the skinned actor, persisting until the next refit-count rebuild threshold; worst case `VK_ERROR_DEVICE_LOST`. RT-only blast radius; raster unaffected. **Verification path**: `BYRO_VALIDATION=1` with synchronization validation on a cell-load-while-NPCs-visible run; look for `SYNC-HAZARD-WRITE-AFTER-WRITE`/`WRITE-AFTER-READ` naming the shared scratch buffer with cross-submission attribution. Not visible to `cargo test`.

**Related**: #1300, #983, #1140, #642; sibling of CONC-D1-02 (same rule, different resource).

**Suggested Fix**: Mirror #1300 — hoist an unconditional `self.record_scratch_serialize_barrier(device, cmd);` before the loop in `blas_static.rs::build_blas_batched`, and extend `scratch_tests.rs`'s doc enumeration to name this third site. Ship only after validation-layer confirmation (per the project's speculative-Vulkan-fix policy).



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*

## #4179 [OPEN] CONC-D1-02: No barrier publishes prior-submission static BLAS writes to the TLAS build's reads on skinned-actor-free frames (HYPOTHESIS — would be MEDIUM)
labels: bug, renderer, medium, sync

**Description**: **Severity as reported: HYPOTHESIS — would be MEDIUM if confirmed.** `build_tlas` reads every referenced BLAS. The only barrier in the frame publishing AS writes to that read is in `context/skinned_blas_refit.rs:827-836`, emitted only inside the skinned path (requires `skin_compute`, `accel_manager`, a bone buffer, a non-empty dispatch list). A static-only frame (actor-free interior, headless bench, or an early-return) records no AS barrier before `build_tlas`, even though the referenced static BLAS were written by a different submission (`step_streaming`'s `build_blas_batched`, or `restore_missing_static_blas_for_draws` earlier in the same frame). By the project's own cross-submission rule, that write->read needs a device-side dependency that isn't present.

**Evidence**:
Grepping `ACCELERATION_STRUCTURE` across `context/` returns AS barriers at exactly two places — the conditional pre-TLAS one in `skinned_blas_refit.rs:829`, and the unconditional one in `dispatch_skin_and_cluster.rs:310`, which runs *after* `build_tlas` (publishing the TLAS write to ray-query consumers, not the BLAS writes to the TLAS build).

**Impact**: If the strict Vulkan reading holds, a TLAS built on a static-only frame can traverse BLAS whose build writes aren't yet visible — missing/corrupt RT shadows/reflections/GI for newly-streamed meshes, self-healing the next frame with a skinned actor (or never, in an actor-free cell). If the permissive reading holds this is a no-op — hence HYPOTHESIS. **Verification path**: validation-layer (cross-submission syncval) or RenderDoc only; `SYNC-HAZARD-READ-AFTER-WRITE` at `vkCmdBuildAccelerationStructuresKHR` (TOP_LEVEL) naming a BLAS buffer written by a previous submission, on an actor-free cell load.

**Related**: CONC-D1-01 (same rule, scratch buffer); #2931.

**Suggested Fix**: If syncval confirms, emit the `ACCELERATION_STRUCTURE_BUILD_KHR`/AS_WRITE -> AS_READ barrier unconditionally in `dispatch_skin_and_cluster.rs` immediately before `accel.build_tlas(...)`, rather than leaving it nested in the skinned path's control flow.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*

## #4180 [OPEN] CONC-D1-03: Two blocking one-time submits (build + compaction) run inside the per-frame path via `restore_missing_static_blas_for_draws`
labels: bug, renderer, medium, sync, performance

**Description**: `restore_missing_static_blas_for_draws` is called every frame from the render driver (`byroredux/src/app_frame.rs:263`); when a TLAS-eligible rigid handle lacks a BLAS it runs the full `build_blas_batched` pipeline: a `submit_one_time` (builds + compaction-size queries) with a host fence-wait, then `get_query_pool_results` with `vk::QueryResultFlags::WAIT` (a second host stall), then a second `submit_one_time` for compaction copies with another fence-wait — up to `MAX_STATIC_BLAS_RESTORES_PER_FRAME = 256` meshes per frame, repeated every frame until the visible set is restored. Synchronization-correct, but load-time-shaped work executing in the frame loop; the fence wait lands on the same graphics queue as the still-in-flight previous frame.

**Evidence**:
`app_frame.rs:263` calls this per-frame, before `draw_frame`; `resources.rs:485-491` invokes `build_blas_batched`; `blas_static.rs` does two `submit_one_time` calls with a `WAIT` query readback between them; `constants.rs:166` sets the 256-mesh cap.

**Impact**: Multi-millisecond-to-second CPU+GPU stalls whenever LRU eviction has removed BLAS still visible — sustained hitching on over-budget cells, not a one-off load cost. Also the mechanism that makes CONC-D1-01's cross-submission window realistic on ordinary frames. Reproduce with `cargo run --release -- ... --bench-frames 300 --bench-hold` on a large exterior; correlate frame-time spikes with the "Restored {count} missing static shadow BLAS before TLAS build" debug log.

**Related**: #3540 (the per-frame cap), #1449 (why eviction stays deferred), CONC-D1-01.

**Suggested Fix**: Record the restore builds into the frame command buffer (as #911 did for skinned first-sight BUILDs), or move the restore into the streaming step in `about_to_wait` with an explicit per-frame budget, so no host fence-wait sits in the render driver.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*

## #4181 [OPEN] CONC-D2-01: Ground-cover counter readback copies a buffer the scatter dispatch just wrote, with no COMPUTE→TRANSFER dependency
labels: bug, renderer, medium, sync, terrain-exterior

**Description**: `GroundCoverPipeline::record_scatter` dispatches `groundcover_scatter.comp`, which writes `counter_buffer` via atomics, then emits exactly one publish barrier and immediately `vkCmdCopyBuffer`s from that same buffer into the per-FIF `counter_readback[frame]`. The barrier's dst scope is `DRAW_INDIRECT | VERTEX_SHADER` / `INDIRECT_COMMAND_READ | SHADER_READ` — confirmed by direct read — it names neither `TRANSFER` stage nor `TRANSFER_READ` access, so the copy's read has no dependency on the compute write.

**Evidence**:
```rust
// groundcover.rs:1231-1239 (confirmed present)
buffer_barrier(
    device, cmd,
    vk::PipelineStageFlags::COMPUTE_SHADER, vk::AccessFlags::SHADER_WRITE,
    vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::VERTEX_SHADER,
    vk::AccessFlags::INDIRECT_COMMAND_READ | vk::AccessFlags::SHADER_READ,
);
device.cmd_copy_buffer(cmd, counters.buffer, self.counter_readback[frame].buffer, &[..]);
```
The two sibling edges in the same file are correct by contrast (`groundcover.rs:1186-1193`, `:1306-1314`).

**Impact**: `GroundCoverPipeline::harvest` decodes the readback into `GroundCoverStats` (blade counts, density histogram, extrema) — EXAL ground-cover density tuning reads these numbers directly. Rendering itself is unaffected (the draw uses the correctly-published edge); failure mode is silently wrong telemetry driving tuning decisions, plus sync-validation noise. Verify with `BYRO_VALIDATION=1` on an exterior cell with ground cover active.

**Related**: #4054 (EXAL ground cover), #4056 (EXAL ground cover Phase 3, OPEN).

**Suggested Fix**: Widen the trailing barrier's dst scope to add `TRANSFER`/`TRANSFER_READ`, or emit a second `COMPUTE_SHADER/SHADER_WRITE -> TRANSFER/TRANSFER_READ` edge before the copy. Purely additive — same class of change #2403 made for the skinned-vertex publish mask. Do not ship unvalidated.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*

