# Issue #3823: REN-WD-D15-02: boundary-crossing LOD reconcile (incl. LOD water) is now unbounded and deadline-free

**Labels**: medium,water,terrain-exterior,performance,bug
**Filed**: 2026-09-04, via /audit-publish from the water-deep audit suite

---

**Severity**: MEDIUM
**Dimension**: Water (LOD water / streaming interaction)
**Location**: `byroredux/src/streaming_helpers.rs` (`lod_reconcile_budget_for_frame`'s `grid_changed` arm and `reconcile_lod_rings`'s `make_budget`), `byroredux/src/app_step.rs` (the `(!grid_changed).then_some(streaming_deadline)` argument)
**Source report**: `docs/audits/AUDIT_RENDERER_2026-09-04.md` (water-deep suite, Dim 15)

## Description
`b15b0527` changed the exterior boundary-crossing frame from doing **zero** LOD reconcile work (`Some(0)`) to doing **unbounded** work: `lod_reconcile_budget_for_frame` now returns `Some(usize::MAX)` when `grid_changed`, and `app_step.rs` simultaneously passes `None` for the wall-clock deadline on exactly those frames. `make_budget`'s `(usize::MAX, _) => LodWorkBudget::unlimited()` arm then drops both the per-provider attempt cap and the deadline for terrain, object and placement LOD (LOD water planes ride the same reconcile). The stated rationale — presentation-atomic handoff, no budget-shaped empty strip — is sound, but the mechanism removes both bounds at the single frame with the most newly-exposed LOD footprint.

This is the same shape as #3540, where an unbounded per-frame recovery batch (`restore_missing_static_blas_for_draws`) put Starfield's `citycydoniamainlevel` on one frame for over ten minutes and was fixed by adding `plan_static_blas_restore` + `MAX_STATIC_BLAS_RESTORES_PER_FRAME`.

## Evidence
```
streaming_helpers.rs:48-53   } else if grid_changed { ... Some(usize::MAX) }
streaming_helpers.rs:119     (usize::MAX, _) => LodWorkBudget::unlimited(),
app_step.rs:264              (!grid_changed).then_some(streaming_deadline)
```
The `make_budget` comment at `streaming_helpers.rs:92,116` still describes the old contract ("`usize::MAX` remains the deterministic full-radius bootstrap contract"), which is no longer the only caller.

## Impact
A hitch proportional to the newly-exposed LOD ring on every exterior cell-boundary crossing, on the largest worldspaces, with no ceiling. Correctness is unaffected. Whether it's observable depends on the ring size and provider cost — needs a bench/frame-time measurement on a large exterior (`--grid` traversal) rather than reasoning.

## Related
#3540 (the precedent bound + fix shape), #2376 / EX-07 (the deadline contract this bypasses), `b15b0527`.

## Suggested Fix
Keep the atomic-handoff intent but bound it — e.g. a boundary-specific cap analogous to `MAX_STATIC_BLAS_RESTORES_PER_FRAME`, or keep the deadline and only lift the per-provider attempt cap — and update the `make_budget` comment, which still claims `usize::MAX` is bootstrap-only.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the new bound (mirroring `plan_static_blas_restore`'s test style from #3540)
- [ ] **SIBLING**: Terrain, object, and placement LOD providers all ride this same budget — verify the fix covers all three, not just water
