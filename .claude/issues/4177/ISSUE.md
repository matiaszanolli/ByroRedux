# CONC-D1-01: Static BLAS batch build emits no scratch-serialize barrier before its first build (HYPOTHESIS — would be HIGH)

Labels: high,sync,renderer,bug

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
