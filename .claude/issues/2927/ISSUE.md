# PERF-D3-03: Mid-batch eviction's pending_bytes ledger stops at Phase 1, missing the real batch peak

- **Issue**: [#2927](https://github.com/matiaszanolli/ByroRedux/issues/2927)
- **Finding ID**: `PERF-D3-03`
- **Labels**: `low,performance,memory,bug`
- **Source report**: [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](../../../docs/audits/AUDIT_PERFORMANCE_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2927 --json state`.

---

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` (`build_blas_batched` — the `pending_bytes` accumulator and the `alloc_compact` closure), `crates/renderer/src/vulkan/acceleration/predicates.rs` (`should_evict_mid_batch`, `blas_over_budget`)
- **Status**: NEW
- **Description**: `pending_bytes` accumulates `sizes.acceleration_structure_size` for the *uncompacted* Phase-1 result buffers only, and is the last value the budget ever sees for this batch. Phase 5+6 (`alloc_compact`) then allocates a **second** full set of buffers — one compacted destination per mesh — while every Phase-1 original is still live; the originals are not destroyed until Phase 7, after the compaction copy submission retires. Real peak static-BLAS residency during a batch is therefore `static_blas_bytes + total_before + total_after`, but the guard only ever tests `static_blas_bytes + total_before`. There is also no `should_evict_mid_batch` / `evict_unused_blas` call anywhere inside `alloc_compact`, so the interval-based check that exists for Phase 1 has no counterpart during the phase that actually pushes residency to its maximum.
- **Evidence**: `pending_bytes = pending_bytes.saturating_add(sizes.acceleration_structure_size);` sits in the Phase-1 loop and goes out of scope before `alloc_compact` is defined. `alloc_compact` computes `total_before` (sum of `prepared[i].buffer.size`) and `total_after` (sum of `compacted_sizes`) purely for the closing `log::info!("Batched BLAS build: … compacted {:.1} KB → {:.1} KB ({:.0}% savings)")` — neither is compared against `blas_budget_bytes`. Phase 7 destroys the originals *after* the compaction copy's `submit_one_time` has returned, confirming both sets are simultaneously resident. `static_blas_bytes` is incremented in Phase 7 with the *compacted* size, so the committed ledger is correct; only the in-flight peak is under-counted.
- **Impact**: The budget under-states the transient peak by roughly `total_after` (empirically ~50–60% of `total_before`, per the savings figure the same function logs). Bounded by one batch, so on a well-behaved cell this is tens of MB. It matters only when a single batch is large relative to the budget — the same "OOM-on-first-huge-cell" scenario #1792 closed the *other* half of. **Unreachable on the 12 GB dev card** (~4 GB budget); relevant to the 6 GB RT-minimum target. Reported as LOW rather than dropped because it is the one remaining structural blind spot in an accounting path that has already been wrong once (#1792) in a way that made a whole mechanism a no-op.
- **Related**: #1792 (PERF-D3-NEW-01 — the Phase-1 half of this accounting, fixed), #510 (mid-batch eviction), #316 / PERF-D3-02 above (same closure).
- **Suggested Fix**: Carry `pending_bytes` into `alloc_compact` and add `compact_size` to it as each destination is allocated, checking `should_evict_mid_batch` on the same `BATCH_EVICTION_CHECK_INTERVAL` cadence; or, cheaper and sufficient, note in the `pending_bytes` doc comment that it deliberately excludes the compaction destinations and that the true peak is ~1.5× the tracked value, so a future budget tune is made against the right number.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
