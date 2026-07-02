# D6-05: First-sight entities pay a redundant BLAS UPDATE immediately after their fresh BUILD in the same command buffer

**Issue**: #1812
**Labels**: low,vulkan,renderer,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D6-05)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D6-05)

## Location
`crates/renderer/src/vulkan/context/draw.rs:1848-1870,1878`

## Description
A first-sight entity is always dirty, so the refit-gate condition is false and the loop proceeds to `refit_skinned_blas` — a full UPDATE against the identical vertex data the BUILD consumed moments earlier in the same command buffer. The block comment asserts the fall-through is harmless, but `refit_skinned_blas` has no "freshly built this frame" short-circuit and records a real UPDATE, also inflating `refits_attempted`/`refits_succeeded` on spawn frames. Note: this same BUILD-then-UPDATE-in-one-cmd-buffer adjacency is also the trigger condition for the separate Vulkan-spec barrier bug tracked in #1790 (missing `AS_READ` on the scratch-serialize barrier) — implementing this finding's fix (skip the redundant refit entirely on freshly-built entities) would incidentally close that hazard window too, but the two are distinct findings: this one is a wasted-work/perf issue, #1790 is a formal synchronization-correctness issue.

## Evidence
Block comment preceding the refit fall-through in `draw.rs:1848-1870`; `refit_skinned_blas` (`blas_skinned.rs`) has no built-this-frame check.

## Impact
One redundant AS UPDATE + barrier per skinned entity per spawn frame only; steady-state unaffected. Minor telemetry skew on spawn frames.

## Related
#911, #1196, #1790 (adjacent Vulkan-correctness bug at the same call site, distinct root cause — see description).

## Suggested Fix
Track entities built this frame and `continue` past the refit for them; fix the stale comment either way.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

