# D6-03: All skinned BLAS builds/refits in a frame are serialized on one shared scratch buffer — zero build overlap on multi-NPC frames

**Issue**: #1797
**Labels**: medium,vulkan,renderer,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D6-03)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D6-03)

## Location
`crates/renderer/src/vulkan/acceleration/blas_skinned.rs:417` (per-refit barrier), `:278-283` (per-build barrier in first-sight batch); consumed at `context/draw.rs:1835-1899`

## Description
`blas_scratch_buffer` is a single allocation sized to the max single-build demand. Because every skinned BLAS build/refit reuses the same scratch address, the Vulkan spec requires an AS_WRITE→AS_WRITE barrier between each pair — N dirty skinned entities produce N fully serialized AS builds per frame, each self-emitting the barrier. Small skinned BVHs (5-15K triangles per body part) individually underutilize the GPU; back-to-back serialization with full-pipe AS-stage drains prevents overlap. The barrier correctness chain (#642/#644/#983/#1095/#1140/#1300) is complete and intact; nothing tracks the throughput cost of the serialization.

## Evidence
`refit_skinned_blas`'s first statement is `record_scratch_serialize_barrier`; the refit loop calls it once per dirty entity; scratch sizing is grow-to-max-single-build, not per-build slots.

## Impact
GPU skin-chain time scales linearly with dirty-entity count with no overlap. On crowd scenes the `gpu_skin_blas_refit_ms` bracket absorbs the full serial sum plus per-barrier drain. Idle crowds are already saved by #1195/#1196; this is the moving-crowd ceiling only. Confidence: quantify before fixing — the #1194 GPU timer brackets exist for this (`skin.coverage` → `gpu_skin_blas_refit_ms` vs `refits_attempted`).

## Related
#642, #983, #1300 (correctness chain), #1194 (measurement hook).

## Suggested Fix
Sub-allocate the scratch buffer into K aligned slots, round-robin builds, emit the serialize barrier only every K builds; K=1 fallback under memory pressure.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

