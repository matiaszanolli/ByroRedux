# SAFE-2026-08-07-04: BLAS-scratch-shrink call site lost its explicit SAFETY: tag during the unload-batching refactor

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2545
**Finding ID**: SAFE-2026-08-07-04

**Severity**: LOW
**Dimension**: 4 (Unsafe-Block Discipline)
**Location**: `byroredux/src/cell_loader/unload.rs:265-282` (`finish_unload_batch`, unsafe block at `:278-280`)
**Status**: NEW (introduced by this window's batch-despawn/unload refactor; the block was moved out of `unload_cell` into the new `finish_unload_batch` helper shared by `unload_cell`/`unload_cells`)

## Description
Before this window, the call site carried an explicit `// SAFETY: ...` comment restating both of `shrink_blas_scratch_to_fit`'s `# Safety` preconditions. The refactor that split this call into `finish_unload_batch` (run once per logical unload boundary rather than once per cell) replaced that comment with a shorter one that preserves the *substance* ("Retiring the old scratch allocation is deferred for frames-in-flight by AccelerationManager (#1782), so this is safe from the about_to_wait streaming path") but drops the literal `SAFETY:` tag and no longer restates the same-device/allocator precondition in words (trivially true here — both come from the same `&mut VulkanContext` — but no longer stated).

## Evidence
Confirmed directly at `unload.rs:265-282` — the comment reads "#495 — shrink the shared BLAS scratch buffer against the final post-drop peak once per logical unload. Retiring the old scratch allocation is deferred for frames-in-flight by AccelerationManager (#1782), so this is safe from the about_to_wait streaming path." with no `SAFETY:` prefix. `crates/renderer/src/vulkan/acceleration/memory.rs:32-49` carries the callee's unchanged, correct `# Safety` doc block. All five call sites of `unload_cell`/`unload_cells` were checked (`streaming_helpers.rs:204,207`, `app_step.rs:100`, `main.rs:1084`, `cell_loader/exterior.rs:431`, `cell_loader/transition.rs:196`) — none require the "no BLAS build in flight" precondition directly, since the callee's own deferred-destroy contract makes that a non-issue regardless of caller frame-loop phase. The invariant still holds.

## Impact
None on running code — hygiene/documentation regression only. Risk is to future maintainability: a `grep SAFETY` convention audit could mis-flag this site as "missing" even though the reasoning is present in different words.

## Related
#495, #1782/CONC-D1-01, #2148/ECS-2507-02 (all correctly referenced by the surrounding comments).

## Suggested Fix
Re-prefix the second paragraph with `// SAFETY:` and restate the same-device/allocator precondition in one clause. One-line fix, no behavior change.

## Completeness Checks
- [ ] **UNSAFE**: Comment re-prefixed with `// SAFETY:` restating the upheld invariant
