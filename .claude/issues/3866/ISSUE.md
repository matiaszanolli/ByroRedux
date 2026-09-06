# #3866: TD3-2026-09-05-02: today's `fa5c4191` renamed `compute_blas_budget` → `probe_blas_heap_bytes` and changed its formula; four doc sites still describe the old name and the old `heap / 3` math

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-02) via `/audit-publish`, 2026-09-05. Labels: `low,renderer,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3866 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-02), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/mod.rs` — the `blas_budget_bytes` field doc (name + formula + lifecycle)
  - `crates/renderer/src/vulkan/acceleration/constants.rs` — the `MIN_BLAS_BUDGET_BYTES` doc (name + formula)
  - `crates/renderer/src/vulkan/acceleration/tests/predicates_tests.rs` — inside `should_evict_mid_batch`'s zero-budget case (name only)
  - `docs/engine/memory-budget.md:406` — the `MIN_BLAS_BUDGET_BYTES` reserve-floor table row (formula only)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: `fa5c4191` (2026-09-05, **today** — *"Fix #3829, fix #3839, fix #3840"*).
- **Description**: `fa5c4191` split the old composed `compute_blas_budget` into `probe_blas_heap_bytes` (device probe) + `blas_budget_for_heap(heap, reserved)` (the arithmetic), and changed the arithmetic from `heap / 3` to `(heap - reserved) / 3` where `reserved` is the resolution-scaled froxel/pass reservation, re-derived on every swapchain recreate. Four doc sites still describe the pre-`fa5c4191` world. The exact stale sentences:
  1. `mod.rs`, `blas_budget_bytes` field doc:
     *"Derived **at construction time** from DEVICE_LOCAL heap size (**VRAM / 3**) with a 256 MB floor. On a 12 GB GPU this **yields 4 GB** (eviction virtually never fires); on a 6 GB GPU it **yields 2 GB**"* — followed by a rustdoc intra-doc link
     ``[`compute_blas_budget`](super::predicates::compute_blas_budget)``.
     All four claims are now wrong: the budget is *not* fixed at construction (its own sibling field `blas_heap_bytes`, added in the same commit, says *"Retained so [`Self::recompute_blas_budget`] can re-derive against a new screen-scaled reservation on resize"*), the formula subtracts a reservation first, the two worked GPU examples no longer hold, and the link target does not exist.
  2. `constants.rs`, `MIN_BLAS_BUDGET_BYTES` doc: *"Computed budget is **`device_local / 3`** capped no lower than this … See **`compute_blas_budget`**."*
  3. `predicates_tests.rs`: *"degenerate configuration; **`compute_blas_budget`** floors at 256 MB"* — the 256 MB floor claim is still true; only the function name is dead.
  4. `docs/engine/memory-budget.md:406`: `` | `MIN_BLAS_BUDGET_BYTES` | 256 MB | Minimum BLAS-budget floor (**BLAS allocation heap / 3**, capped below) | ``
- **Evidence**:
  ```
  $ git show fa5c4191 -- .../acceleration/predicates.rs | grep -E "^[-+].*fn (compute_blas_budget|probe_blas_heap_bytes|blas_budget_for_heap)"
  -pub(super) fn blas_budget_for_heap(heap_bytes: vk::DeviceSize) -> vk::DeviceSize {
  +pub(super) fn blas_budget_for_heap(
  -pub(super) fn compute_blas_budget(
  +pub(super) fn probe_blas_heap_bytes(
  ```
  Live arithmetic (`predicates.rs`):
  ```rust
  pub(super) fn blas_budget_for_heap(heap_bytes, reserved_bytes) -> vk::DeviceSize {
      (heap_bytes.saturating_sub(reserved_bytes) / 3).max(MIN_BLAS_BUDGET_BYTES)
  }
  ```
  `grep -rn "compute_blas_budget"` over `crates/` returns only doc-comment/test-comment hits plus the unrelated *method* `AccelerationManager::recompute_blas_budget` (`memory.rs`), which does exist.
- **Impact**: The `mod.rs` site is a rustdoc **intra-doc link to a non-existent path**, so `cargo doc` will emit a `broken_intra_doc_links` warning and the rendered docs get a dead reference. Beyond the build noise, this is the fifth recorded instance of the BLAS-budget doc block drifting from its code (#1625 closed a skill citing a non-existent `predicates.rs::blas_budget_bytes` *function*; #3043 corrected the heap-selection prose; #3842 was filed **today** against the sibling orphaned doc comment inside `predicates.rs` itself). The `mod.rs` block is now internally self-contradicting between two adjacent fields written by the same commit, which is the state most likely to send the next reader or auditor to the wrong conclusion about when the budget is computed.
- **Related**: #3842 (filed today — the *orphaned* `compute_blas_budget` doc comment inside `predicates.rs`, and its surviving "`VRAM / 3`" phrasing; **a different file and a different doc block** — the four sites above are not covered by it), #3043, #1625, #3839.
- **Suggested Fix**: Rewrite the `mod.rs` `blas_budget_bytes` doc to say the budget is `(probed DEVICE_LOCAL heap − screen-scaled pass reservation) / 3` floored at `MIN_BLAS_BUDGET_BYTES`, re-derived by `recompute_blas_budget` at init and on every swapchain recreate; drop the two now-wrong 12 GB/6 GB worked examples or recompute them against a stated reservation; repoint the intra-doc link at ``[`blas_budget_for_heap`](super::predicates::blas_budget_for_heap)``. In `constants.rs` and `memory-budget.md:406`, change `device_local / 3` / `BLAS allocation heap / 3` to `(heap − reserved) / 3`. In `predicates_tests.rs`, swap the name to `blas_budget_for_heap`.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
