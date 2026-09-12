# PERF-D8-2026-09-11-02: `pre_parse_cell`'s per-REFR model-path loop allocates two-to-three throwaway strings before the dedup check that would make most of them unnecessary

Labels: medium,performance,nif-parser,bug

**Description**: For every REFR in a cell (not just unique models), the loop does a full-string `.to_ascii_lowercase()` purely to test `.ends_with(".spt")` (discarded), then calls `canonical_model_path_key` — itself a second `to_ascii_lowercase().replace(...)` plus, conditionally, a `format!` allocation — before the cache/batch dedup check that determines whether the computed key is even used. The function's own comments document ~95% cache-hit rate on a 7x7 exterior grid and heavy per-cell model-path duplication (chairs/lanterns/rocks sharing one path each) — meaning these 2-3 allocations per REFR run essentially every time regardless of outcome.

**Evidence**:
`byroredux/src/streaming.rs:1430-1472` (allocations at :1452,:1460), `byroredux/src/cell_loader/nif_import_registry.rs:49-56` (`canonical_model_path_key` at :50,:54).

**Impact**: Bounded, load-path (streaming-worker thread, not render-frame) allocation churn proportional to `cell.references.len()` rather than the unique-model count, which the surrounding comments document as roughly an order of magnitude smaller. Cell-load latency waste, not a frame-time regression.

**Related**: Distinct from #3038's correctness fix and #877/#830/#1262's phase-split work, unaffected by this finding.

**Suggested Fix**: Non-allocating suffix test for `.spt`; memoize `canonical_model_path_key` per-call keyed on the raw `&str`, or check the dedup sets with a borrowed lowercase compare before committing to an owned `String`.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
