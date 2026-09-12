# PERF-D4-2026-09-11-02: `memory-budget.md`'s scene-buffer total (225 MB) is exactly one `GpuInstance`-growth stale

Labels: low,performance,doc-rot,documentation

**Description**: The page's own row table was updated for #3231's `GpuInstance` 128->160 B growth, but the two totals below it (225 MB, ~223 MB) were not — they reproduce exactly the pre-#3231 sum. Current row sum is approx 242.3 MB (delta = 262144 x 32 B x 2 FIF = 16.8 MB, reproduced exactly).

**Evidence**:
`docs/engine/memory-budget.md:109,793`.

**Impact**: Under-counts resident scene VRAM by ~17 MB (7%) for any reader of the doc, including audit skill instructions that cite "~223-225 MB".

**Related**: #3231 (the growth this doc missed).

**Suggested Fix**: Update both totals to approx 242 MB; state the MiB/MB unit convention once in the table header (the table currently mixes both under one "MB" label).



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
