# ECS-2026-08-16-03: sample_blended_transform interpolates every channel twice per bone per frame

**Issue**: #3031
**Severity**: MEDIUM
**Dimension**: 10 — Animation Runtime
**Labels**: `medium,ecs,performance,bug`
**Source report**: `docs/audits/AUDIT_ECS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ECS_2026-08-16.md` (Dimension 10 — Animation Runtime).

**Location**: `crates/core/src/animation/stack.rs`:325-357 (pass 1) and :369-390 (pass 3)

## Description

`sample_blended_transform` interpolates **every channel twice per bone per frame** — pass 1 and pass 3 each perform the same translation/rotation/scale interpolation over the same keys.

## Evidence

Re-verified 2026-08-17: the two ranges each run the channel-sampling work, with no memoisation of pass 1's result for pass 3 to reuse.

## Impact

Pure waste on the animation hot path, scaled by (bones × animated entities × frames). On a crowded interior this is per-bone-per-frame duplicated interpolation, and the user's hardware profile makes CPU-side waste the kind that shows up as a frame-time floor.

Correctness is unaffected — both passes compute the same values — which is why it is MEDIUM rather than a bug.

## Suggested Fix

Compute each channel once and reuse the result across both passes, or restructure so the blend consumes a single sampled set. Measure before and after on a scene with many animated actors; the fix is only worth landing if the saving is real.

## Related

- `crates/core/src/animation/interpolation.rs` (`sample_translation` / `sample_rotation` / `sample_scale` — the functions being duplicated)

## Completeness Checks
- [ ] **MEASURED**: The saving is benchmarked, not assumed
- [ ] **IDENTICAL-OUTPUT**: The refactor is provably output-identical — blended transforms are visible
- [ ] **SIBLING**: `advance_stack` and the root-motion split checked for the same double-sampling
- [ ] **TESTS**: Existing animation tests still pass bit-for-bit

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3031 --json state` when live state is needed.*
