# PERF-D2-2026-09-11-04: The raster sort permutes ~480-byte records and recomputes the 12-tuple key O(n log n) times

Labels: low,performance,renderer,bug

**Description**: `sort_unstable_by_key(draw_sort_key)` doesn't cache keys, so the branchy 12-tuple key build is re-evaluated on every comparison, and `DrawCommand` itself is a ~480-byte struct being memmoved on every swap. The measured crossover table is therefore mostly memmove-and-rekey cost, not comparison cost.

**Evidence**:
`byroredux/src/render/mod.rs:891-897` (`sort_draw_commands`), key at `:696-860`.

**Impact**: Nil on every committed baseline (max `bench_draws_raster_cmds` 283, well under any measurable threshold); matters only on exterior grids reaching the 3000-command parallel-sort gate.

**Related**: None named.

**Suggested Fix**: Do not change the `3000` constant. Extend the existing bench harness with a `(key, index)`-pair-sort + apply-permutation variant and re-run the sweep; only act on the measured result.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
