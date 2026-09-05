# #3840: PERF-D3-2026-09-05-02: mid-batch BLAS eviction credits itself bytes that are still resident — `pending_destroy_blas` only drains inside `draw_frame`, and `build_blas_batched` runs before it

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D3-2026-09-05-02) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3840 --json state`.

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D3-2026-09-05-02), published from `/audit-suite volumetrics-deep`. Premise re-verified against HEAD at publish time.

> Note: `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:1012-1114` (esp. `1071-1095`), `:355-440`; `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:204-224`
- **Status**: NEW
- **Description**: `evict_unused_blas` decrements `static_blas_bytes` (and
  `total_blas_bytes`) the moment it moves a `BlasEntry` onto
  `pending_destroy_blas`, but the actual destroy + allocator free happens
  `DEFAULT_COUNTDOWN` (2) frames later in `tick_deferred_destroy`. That tick
  has exactly one caller — `sync_and_acquire_frame`, inside `draw_frame`,
  after the fence wait. `build_blas_batched` runs from the streaming path in
  `about_to_wait`, **before** the next `draw_frame` — so within one batch
  there is no tick at all. The eviction loop's own stop condition
  (`blas_over_budget(static_blas_bytes, pending_bytes, budget)`, line 1071)
  therefore evaluates against a number that has already deducted memory the
  GPU still holds, and the batch resumes allocating against phantom
  headroom. The deferral itself is correct and load-bearing (#1449 —
  freeing earlier would free memory an in-flight TLAS still references); the
  defect is the **accounting**, not the lifetime. There is no
  `pending_destroy_bytes` counter anywhere — `pending_destroy_blas_count()`
  (`blas_static.rs:164`) exposes an entry *count* only, though `BlasEntry`
  carries `size_bytes` right there, unqueried.
- **Evidence**:
```rust
// blas_static.rs:1078-1094
if let Some(entry) = self.blas_entries[idx].take() {
    self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.size_bytes);
    self.static_blas_bytes = self.static_blas_bytes.saturating_sub(entry.size_bytes);
    ...
    self.pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN);   // still resident
}
```
  Only tick site: `accel.tick_deferred_destroy(&self.device, alloc);` at
  `sync_and_acquire_frame.rs:224`, reached only through `draw_frame`.
- **Impact**: Worst case within a single mid-batch eviction cycle, true
  resident static-BLAS VRAM approaches `2 × blas_budget_bytes`: evict a full
  previous-cell set on paper, build a fresh set to budget, with neither
  generation freed until the next frame. The 90% mid-batch trigger
  (`should_evict_mid_batch`) makes this reachable only on genuinely large
  multi-cell bursts — the same synchronous-burst regime #1793 already flags,
  so this compounds a known-bad path rather than opening a new one. It also
  makes the `BLAS eviction: freed N entries (X MB)` log line overstate what
  was actually reclaimed at that instant.
- **Related**: #1449 (the deferral this sits on top of), #1792 (the
  `pending_bytes` fold — verified intact, not regressed, at
  `blas_static.rs:1026-1032`/`1071-1077`), #1793 (synchronous multi-cell
  burst, documented-not-fixed), PERF-D3-2026-09-05-01.
- **Suggested Fix**: Track a `pending_destroy_bytes` running total alongside
  the queue (incremented on push, decremented in the tick destroyer) and
  fold it into `blas_over_budget`'s first argument, so a batch cannot spend
  headroom it hasn't actually reclaimed yet. Surface it next to
  `pending_destroy_blas_count()` for `ctx.scratch`.
- **Confidence**: High — verified both call sites and the single-tick-owner
  shape by reading the source directly.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
