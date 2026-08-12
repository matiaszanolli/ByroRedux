# PERF-D2-04: sort_draw_commands re-extracts the 11-tuple key on every comparison

**Issue**: #2681
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: [mod.rs](byroredux/src/render/mod.rs) — `sort_draw_commands` / `draw_sort_key`
- **Status**: NEW
- **Description**: Both the serial and parallel arms pass `draw_sort_key` to
  `sort_unstable_by_key`, which evaluates the key function on *each side of every
  comparison* — roughly `2·N·log₂N` extractions. Each extraction touches ~10 fields
  scattered across a `DrawCommand` whose field tally
  (21×`u32`, 19×`f32`, 11×`bool`, 9×`[f32;3]`, `[f32;16]`, `[u32;12]`, `[f32;5]`,
  2×`[f32;4]`, 3×`[f32;2]`, 3×`u8`, `RenderLayer`, `Option<u32>`) puts it near 480
  bytes — i.e. ~8 cache lines per key build — and materialises a 44-byte tuple.
  Meanwhile `collect_lights` in the sibling module was explicitly converted to
  decorate-sort-undecorate for exactly this reason (#2034: "precompute
  `gi_priority_score` once per light … instead of recomputing it on both sides of
  every comparator call"), on an array two orders of magnitude smaller. The larger,
  hotter sort never got the same treatment.
- **Evidence**:
  ```rust
  if raster_draws.len() >= DRAW_SORT_PARALLEL_THRESHOLD {
      raster_draws.par_sort_unstable_by_key(draw_sort_key);
  } else {
      raster_draws.sort_unstable_by_key(draw_sort_key);
  }
  ```
  `draw_sort_key` returns `(u8, u8, u8, u32, u32, u32, u32, u32, u32, u32, u32)` and
  its alpha-blend arm additionally branches on `material_kind` and `dst_blend`
  before assembling the tuple. The comment block above the call site already
  attributes a measurable cost to key width: "`883f57cd` widened the sort key from
  10 to 11 tuples (the stable surface ID), which raised per-comparison cost and
  moved the crossover UP" — direct in-repo evidence that per-comparison key
  extraction, not element movement, dominates this sort.
- **Impact**: A hypothesis, not a measured regression — stated as such. Sorting an
  array of `(key, u32 index)` pairs (≈48 B/element vs ≈480 B) and then applying the
  permutation would cut key extraction from `~2·N·log₂N` to `N` and shrink the bytes
  the sort itself shuffles by ~10×, at the cost of one permutation pass and either a
  scratch buffer or an in-place cycle walk. It would also very likely move
  `DRAW_SORT_PARALLEL_THRESHOLD` again, so the two must be re-tuned together. **No
  quantitative guard exists for this site**; do not land it on reasoning alone.
- **Related**: #2034 / PERF-D1-2026-07-16-02 (the same transform applied to
  `collect_lights`), #2172, #934, #2173, `883f57cd`.
- **Suggested Fix**: Prototype the index-decorate variant behind the existing
  `manual_bench_draw_sort_serial_vs_parallel` harness (which already sweeps
  N=400…10K) and extend that bench with a third arm. Ship only if the measured win
  survives at the N=1800–3400 range the current runtime baselines actually occupy
  (see PERF-D2-03); re-derive the parallel threshold in the same run.

---


---
*Filed from [`docs/audits/AUDIT_PERFORMANCE_2026-08-12.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `PERF-D2-04`.*

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or a bench delta vs the checked-in baseline) pins this fix
