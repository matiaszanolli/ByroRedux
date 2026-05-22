title:	PERF-DIM7-02: BLAS refit unconditional even when skin compute dispatch would be a no-op
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, M29, medium, memory, performance, renderer
comments:	0
assignees:	
projects:	
milestone:	
number:	1196
--
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-05-19.md (Dim 7, MEDIUM)
**Status**: NEW, CONFIRMED
**Paired with**: #1195 (PERF-DIM7-01 — dispatch gate; refit must use the same bool)
**Blocked on**: #1194 (PERF-DIM7-INSTR — instrumentation prerequisite)

## Symptom

Same shape as #1195 but on the BLAS side: every entity in `dispatches` gets a `refit_skinned_blas` UPDATE call. On NVIDIA the per-BLAS UPDATE cost on a ~5K-vertex mesh is ~30–80 µs (historical bench notes referenced at [crates/renderer/src/vulkan/acceleration/constants.rs:91-97](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/renderer/src/vulkan/acceleration/constants.rs#L91-L97)). 34 entities × 50 µs = ~1.7 ms uncategorized BLAS work per frame on Prospector.

## Cause

[crates/renderer/src/vulkan/context/draw.rs:994-1024](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/renderer/src/vulkan/context/draw.rs#L994-L1024) — the refit loop has no gate paired to PERF-DIM7-01's hypothetical dispatch gate. A BLAS whose source vertex buffer wasn't written this frame doesn't need a refit — the BVH is still consistent with the vertex positions it last saw.

## Fix

Same skip-flag plumbing as #1195. The compute dispatch and refit are 1:1 paired today; the skip decision is the same boolean.

**Critical sub-fix**: bump `last_used_frame` on the skipped path so the LRU eviction sweep at [draw.rs:1077](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/renderer/src/vulkan/context/draw.rs#L1077) doesn't reap a quiescent-but-live slot. The existing dispatch loop already does this at draw.rs:899; the skip path must mirror that.

## Estimated Impact

Needs measurement (gate on #1194). Combined with #1195: ~3 ms / frame upper bound on Prospector (34 NPCs, 20 idle).

## Regression Risk: HIGH

- A BLAS skipped under "bones unchanged" but whose entity transform DID change (parent rotation, FormID despawn-respawn at same ID) would render the old pose.
- The fix must gate on the **same** bool as the dispatch — never split the decision.
- Missing the `last_used_frame` bump on skip path causes LRU to evict a still-needed slot → next-frame artifact.

## Testability (post-#1194)

- `SkinCoverageFrame` `refits_attempted` / `refits_succeeded` drop in lockstep with `dispatches_skipped`
- Synthetic `--bench-hold` test: NPC held still, refit count → 0; then perturb transform parent → refit count → 1; verify pose updates correctly

## Completeness Checks

- [ ] **UNSAFE**: refit skip path doesn't bypass any unsafe Vulkan barrier ordering (COMPUTE→AS_BUILD at draw.rs:919-925, AS_WRITE→AS_READ at draw.rs:1028-1034)
- [ ] **SIBLING**: #1195 dispatch gate uses the same bool; verify both gate at the same site
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: regression test pins refit-skip behavior; `last_used_frame` bump verified on skip path; transform-change-without-bone-change case forces refit
