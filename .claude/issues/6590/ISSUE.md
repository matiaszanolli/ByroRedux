# PERF-2026-10-31-D7-01: the eviction deferral turns the NIF registry trim into an O(excess x N) main-thread loop, about 1 s at the Fallout 4 working set

Labels: medium,performance,import-pipeline,game:fo4,bug
Source: docs/audits/ (2026-10-31 report)

**Severity**: MEDIUM
**Dimension**: Performance / Streaming & Cells
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-31.md` PERF-2026-10-31-D7-01

## Location
`byroredux/src/cell_loader/nif_import_registry.rs` - `evict_over_cap` (~:907-1002), reached from `set_eviction_deferred(false)`; caller `sync_nif_cache_eviction` in `byroredux/src/app_step.rs`. Introduced by `304f3b587` (the #6114 eviction deferral). #6114 is closed and only covers the deferral itself, not this cost.

## Description
`evict_over_cap` removes one victim per loop iteration, and choosing the victim sweeps `access_tick` up to three times (cell-unique and clip-free, then clip-free, then any), each entry doing a `clip_handles.contains_key(&str)` SipHash probe. That was fine when it ran once per insert at a registry one entry over the cap. With the deferral the registry may hold up to `DEFERRED_EVICTION_HEADROOM` (8) x cap (16,384), and when the batch settles `set_eviction_deferred(false)` calls `evict_over_cap(None)` on the main thread to drop everything above the cap in one go: E evictions x O(N) sweeps = O(E x N).

## Evidence
`while self.core.len() > cap { let victim = self.access_tick.iter().filter(... is_cell_unique_key(k) && !self.clip_handles.contains_key(k.as_str())).min_by_key(...).or_else(|| ...second sweep...).or_else(|| ...third sweep...) ...; self.core.remove(&victim_key); ... }`. The existing tests (`deferred_eviction_keeps_the_batch_resident_then_trims_to_the_cap`, `deferred_eviction_is_bounded_by_the_headroom`) use a handful of entries, so nothing times the trim. A standalone replica of the loop (`rustc -O`, synthetic path keys, same three-tier victim choice) measured 0.11 s for 4,096 entries, 0.96 s for 10,000 and 2.98 s for 16,384. Real keys and the real `core` map will shift the constant but not the shape. The audit did not measure this inside the engine.

## Impact
A main-thread hitch at the end of every large streaming batch on Fallout 4's default radius (about 9,700 shared models plus per-cell precombines against a 2,048 cap). It runs between frames, but nothing budgets it (`FrameTimeBudget` does not wrap `sync_nif_cache_eviction`). No quantitative guard exists for this site.

## Suggested Fix
Make the trim a single pass: when `len > cap`, collect `(tier, tick, key)` for all entries once, `select_nth_unstable` the `len - cap` smallest, and remove them (O(N)). Keep the per-insert path as is for the steady state. Add a test that trims a 10,000-entry registry under a loose wall-clock bound, or at least asserts the sweep count.

## Related
#6114 (closed; this is a cost it introduced), `docs/engine/memory-budget.md` "Streaming deferral" row (per-entry RAM at the deferred peak is also unmeasured).

## Completeness Checks
- [ ] **SIBLING**: Per-insert eviction path and other registry trims checked for the same O(E x N) shape
- [ ] **TESTS**: A timed or sweep-count test pins the single-pass trim at a 10,000-entry registry
