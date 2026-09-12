# PERF-D9-2026-09-11-03: `gpu_timers.rs` still documents itself as a 16-bracket/32-query pool after #4052 made it 17/34

Labels: low,performance,doc-rot,documentation

**Description**: #4052 added the ground-cover-bench bracket and correctly bumped `QUERIES_PER_FRAME` to 34, `active_bits` to `u32`, and added `BIT_GROUNDCOVER_BENCH` — but four prose counts in the same file's comments still say "32"/"sixteen" and now contradict the code beside them.

**Evidence**:
`crates/renderer/src/vulkan/gpu_timers.rs:5,441,449,451`.

**Impact**: Documentation only — runtime paths key off `QUERIES_PER_FRAME`/the bitmask correctly. Risk is a future reader trusting the stale prose while sizing a new bracket, in a file whose central hazard is reading a query slot that was never written.

**Related**: #4052 (the change these comments describe incorrectly).

**Suggested Fix**: Replace the four literals with current counts, or word them to reference `QUERIES_PER_FRAME` directly so they can't rot; add a `const _: () = assert!(...)` cross-check.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
