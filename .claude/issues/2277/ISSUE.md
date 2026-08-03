# PERF-D7-03: SCOL/PKIN child-placement expansion is recomputed from scratch on every resumed tick that yields mid-REFR

Filed from: `docs/audits/AUDIT_PERFORMANCE_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2277
Labels: medium, performance, bug

**Severity**: MEDIUM
**Dimension**: World Streaming & Cell Transitions (7)

**Location**: `byroredux/src/cell_loader/references/mod.rs:415-433` (`expand_pkin_placements`/`expand_scol_placements` call, `synth_refs` local), `:450` (skip-forward via `synth_idx < job.next_synth`), struct `ReferenceLoadJob` at `:58-74`

## Description
The expanded `synth_refs: Vec<...>` for a SCOL/PKIN's children is a **local** variable inside the per-REFR processing step, not stored on the resumable `ReferenceLoadJob` (confirmed: `ReferenceLoadJob` at `references/mod.rs:58-74` has `next_ref`/`next_synth` but no `synth_refs` field). If `budget.should_yield()` fires partway through a large static-collection's children, the next resumed tick re-enters the same `next_ref` and recomputes the *entire* expansion (re-walking `scol.parts`/`.placements`, recomposing every child transform via `GlobalTransform::compose_trs`) before skip-forwarding past already-processed entries using `job.next_synth`.

## Evidence
```rust
// references/mod.rs:418
let synth_refs = expand_pkin_placements(...)
    .unwrap_or_else(|| expand_scol_placements(...));
...
if synth_idx < job.next_synth {
    // skip-forward — but synth_refs itself was just fully recomputed above
}
```
`ReferenceLoadJob` struct (`:58-74`) carries `next_ref`/`next_synth` as resumption cursors but no cached expansion buffer.

## Impact
For an unusually large SCOL (clutter/rock/tree collections can carry hundreds of placements) split across several budget-limited frames, this is O(k·m) work instead of O(k), where k is the SCOL's child count and m is the number of resume ticks needed. Narrow blast radius (only large SCOLs that straddle a budget boundary), but it is CPU work that did not exist when this code ran synchronously to completion, so it is a real cost introduced by the resumable rewrite.

## Suggested Fix
Cache the expanded `synth_refs` (and any overlay data) on `ReferenceLoadJob` alongside `next_ref`/`next_synth` so a mid-REFR yield resumes without re-expanding.

## Completeness Checks
- [ ] **SIBLING**: Check whether any other resumable expansion path (e.g. precombined-mesh child expansion) has the same recompute-on-resume pattern
- [ ] **TESTS**: A regression test pins this specific fix (e.g. a large synthetic SCOL split across 2+ budget ticks, asserting `expand_*_placements` is called once, not once per resume)
