# #4795: PERF-D3-2026-09-23b-01: #3540's fit projection was removed in a docs-titled commit; over-budget scenes now rebuild-churn every frame of camera travel where they used to decline

**Severity**: MEDIUM
**Labels**: medium, performance, renderer, memory, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D3-2026-09-23b-01)

- **Severity**: MEDIUM (measure first)
- **Dimension**: GPU Memory Pressure
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/predicates.rs:1105-1131` (`plan_static_blas_restore`);
  - `acceleration/static_working_set.rs:1-28`;
  - `context/resources.rs:424-475,530-575`;
  - `acceleration/blas_static.rs:86-112,1258-1270`;
  - `docs/engine/memory-budget.md:596-601`.
- **Status**: NEW. This is a policy change to #3540, not a regression of its hang.
- **Description**: `b9e961eeb` ("Enhance physical lighting documentation…") also rewrote the recovery policy.
  - **Before**: the planner projected `mean_resident_size × visible` and declined the whole set when the projection exceeded the budget.
  - **After**: it declines only when `required_resident_bytes >= budget` (`:1127`). Convergence now comes from `StaticBlasWorkingSet`, which protects the upcoming TLAS handles from `evict_unused_blas`. The doc comment explains the intent: a large unused mesh should no longer strand a small shadow set.
  - A static camera converges.
  - An over-budget scene with a moving camera is where it changes:
    - the TLAS set is distance-gated (`render/static_meshes.rs:461-466`), so meshes leave the ring every frame and become evictable after `MIN_IDLE_FRAMES`;
    - meshes re-entering the ring are missing, and required residency is now below budget, so the planner runs;
    - each restore evicts the just-departed meshes, up to the 16 ms deadline plus one guaranteed chunk.
  - The old code declined this regime outright. No bench accompanies the change.
- **Evidence**: the orchestrator re-read `plan_static_blas_restore` at HEAD:
  ```rust
  if required_resident_bytes >= budget_bytes { return 0; }
  missing.min(per_frame_cap)
  ```
- **Impact**:
  - *est.* 0 → up to ~16 ms per frame of between-frames rebuild while travelling in over-budget scenes (FO4 downtown, Starfield `citycydoniamainlevel`, or any GPU clamped to the 256 MB floor).
  - In exchange, RT completeness improves. There is no VRAM or correctness risk, since admission still bounds residency.
  - `memory-budget.md:600` still documents the removed "Fit projection".
- **Related**: #3540, #4180, #4196; AUDIT_CONCURRENCY_2026-09-23 (stale TLAS vs working set)
- **Suggested Fix**:
  - Bench old against new on an over-budget cell with a moving camera (`override_blas_budget_for_test` set low).
  - If churn shows, add hysteresis: don't re-admit a handle evicted within N frames while at budget.
  - Update `memory-budget.md` § per-frame BLAS recovery.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **DROP**: Any eviction/hysteresis change still routes freed BLAS through `pending_destroy_blas` with the `MAX_FRAMES_IN_FLIGHT` countdown
- [ ] **TESTS**: A regression test pins this specific fix
