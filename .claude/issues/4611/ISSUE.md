# PERF-D6-2026-09-21-01: `plan_palette_dispatch` allocates two fresh Vecs on every frame with a dirty skinned slot

**Labels**: bug, renderer, low, performance

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW · **Dimension**: 6 — Skinning & BLAS
**Location**:
- `crates/renderer/src/vulkan/skin_compute.rs:884-920`: `plan_palette_dispatch`
- Caller: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:183-190`, every frame on the skinning path

**Status**: NEW (`eb7c82043`, 2026-09-16, the #4204 fix)
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

`plan_palette_dispatch` does three things on each call:
- collects the clamped dirty bone ranges into a fresh `ranges` Vec, then sorts it;
- returns a freshly allocated `runs` Vec (`Vec::with_capacity(ranges.len())`);
- in the dense case, returns a one-element `vec![..]` instead.

An empty dirty set allocates nothing, but in a populated cell idle animations change the pose hash, so the set is non-empty and both allocations happen essentially every frame.

The neighbouring per-frame renderer buffers are persistent fields. That is the #243 convention, which #4193 applied to `build_instance_map`'s `instance_map_scratch`.

## Impact

Two small heap allocations per frame on the skinning path. Allocation hygiene only.

## Related

- #4204 (closed): the dispatch narrowing that introduced this planner. The fix itself is sound; only its scratch is fresh per call.
- #4193 and #243: the persistent-scratch convention.
- #4610 (PERF-D8-2026-09-21-02): if this becomes a persistent scratch, it needs a `ScratchTelemetry` row too.

## Suggested Fix

Make both Vecs persistent scratch fields (for example on `ScratchBuffers`), or have the planner write into a caller-owned `&mut Vec`. Add the new scratch to `fill_scratch_telemetry`.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D6-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: The other per-frame planners on the skin / TLAS path are checked for fresh-per-call Vecs
- [ ] **TESTS**: The existing `plan_palette_dispatch` plan tests still pass against the caller-owned-buffer form, and a new scratch has a telemetry row

