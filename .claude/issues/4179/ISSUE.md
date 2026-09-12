# CONC-D1-02: No barrier publishes prior-submission static BLAS writes to the TLAS build's reads on skinned-actor-free frames (HYPOTHESIS — would be MEDIUM)

Labels: medium,sync,renderer,bug

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
