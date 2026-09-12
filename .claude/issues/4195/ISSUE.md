# PERF-D2-2026-09-11-05: The FO4 runtime baseline carries `bench_draws_batches` (296) > `bench_draws_raster_cmds` (256) — a pair that cannot come from one capture

Labels: low,performance,tech-debt,bug

**Description**: `batches <= raster_cmds` holds structurally (a `DrawBatch` is only created from a command surviving the raster-prefix filter). The FO4 TSV violates it because two rows were regenerated 12 days apart from different `entities_total` states (a #3660 partial regen held the older #3006 `batches` row). The other four committed TSVs are self-consistent.

**Evidence**:
`.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv:84-85`.

**Impact**: No engine defect — a stale-baseline-merge risk: this dimension's own checklist instructs cross-checking the parallel-sort threshold against `bench_draws_raster_cmds`, which cannot be sanity-checked against its own sibling on this row.

**Related**: #3660, #3006.

**Suggested Fix**: Re-capture all four FO4 `bench_draws_*` rows in one run; add an invariant check (`gpu_calls <= batches <= raster_cmds <= cmds`) to `/audit-runtime` Phase 3.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
