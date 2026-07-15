# DIM2-02: Stale "9-tuple" sort-key comments + magic-literal parallel-sort threshold

**Filed**: 2026-07-15 · **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-15.md` (Dimension 2: Draw & Instancing) · **Labels**: `low,renderer,performance,documentation`

## Description

Two doc comments in `render/mod.rs` still describe `draw_sort_key`'s pre-Option-B return shape as a "9-tuple" (lines 205 and 445), while a third comment (line 161) and the function signature itself correctly describe it as a 10-tuple. The two stale comments predate whichever change added the 10th slot and were never updated.

Separately, the serial/parallel sort crossover point is an inline literal `>= 2000` (line 462) rather than a named constant living next to the benchmark table (lines ~445-459) that justifies it, so the two can drift independently with no compiler-enforced link.

## Evidence

```
$ grep -n "9-tuple\|10-tuple" byroredux/src/render/mod.rs
161:/// Both branches return the same 10-tuple shape so the compiler accepts
205:/// could reorder commands whose 9-tuple prefix tied, breaking
445:    // 9-tuple key. Measured on a 7950X (see

$ grep -n "2000\b" byroredux/src/render/mod.rs
452:    //     N=2000: 161µs ≈ 165µs                  (tied)
462:    if draw_commands.len() >= 2000 {
```

## Impact

Documentation-only; no behavior change needed. The threshold value is well-supported by the embedded benchmark: serial wins 28-35% at 400-1500 draws (typical FNV/Skyrim interiors), parallel wins 14-67% at 3K-10K+ (FO4 CSG-dense cells), with 2000 at the measured tie point. Risk is a future reader trusting the stale tuple count, or the threshold drifting from its benchmark table unnoticed since they aren't linked.

## Related

- #934 (PERF-DC-01) — original parallel-sort threshold tuning that produced the benchmark table this literal is drawn from.

## Suggested Fix

Update the two stale "9-tuple" comments (lines 205, 445) to "10-tuple". Hoist the `2000` literal into a named `const DRAW_SORT_PARALLEL_THRESHOLD: usize = 2000;` beside the benchmark table, referenced at the call site instead of the bare literal.

## Completeness Checks
- [ ] **SIBLING**: Check `render/mod.rs` and siblings for other lingering "9-tuple" references or undocumented magic-literal thresholds tied to the same sort/batch machinery

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/1995
