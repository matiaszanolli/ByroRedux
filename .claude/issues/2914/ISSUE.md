# REN-D1-02: build_blas_for_mesh has no caller, but memory-budget.md documents it as live

- **Issue**: [#2914](https://github.com/matiaszanolli/ByroRedux/issues/2914)
- **Finding ID**: `REN-D1-02`
- **Labels**: `low,renderer,tech-debt,bug`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2914 --json state`.

---

- **Severity**: LOW
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/context/resources.rs` (`build_blas_for_mesh`), `crates/renderer/src/vulkan/acceleration/blas_static.rs` (`build_blas`), `docs/engine/memory-budget.md` §"LRU eviction", `crates/renderer/src/vulkan/acceleration/mod.rs` (`pending_destroy_scratch` field doc)
- **Status**: NEW
- **Description**: `AccelerationManager::build_blas` is reachable from exactly one place,
  `VulkanContext::build_blas_for_mesh`, and that function has **zero callers** anywhere in the
  workspace — no binary, no test, no example. The entire single-shot BLAS build path is dead.
  Three pieces of documentation describe it as live, and one of them is an authoritative doc.
- **Evidence**:
  - `grep -rn --include='*.rs' "build_blas_for_mesh" .` → the definition in `resources.rs`, one
    module-index comment in `context/mod.rs`, and one doc-comment cross-reference. No call.
  - `grep -rn --include='*.rs' "\.build_blas(" .` → `resources.rs` only.
  - `docs/engine/memory-budget.md` §"LRU eviction": *"a single-shot guard inside `build_blas`
    itself … for the **ad-hoc / UI-quad / lazy-upload path** that sits outside the M40 cell-loader
    batched hot path (#915)"*. The doc side is wrong twice over: there is no caller at all, and
    `register_ui_quad` uploads the UI quad with `for_rt = false`, so it never had a BLAS.
  - `acceleration/mod.rs`'s `pending_destroy_scratch` doc names the grow-replace sites as
    *"three sites — `blas_static::build_blas`, `blas_static::build_blas_batched`, and
    `memory::shrink_blas_scratch_to_fit`"*; only two are reachable.
  - Consequences of the death: #915's eviction guard and #1782's deferred-scratch route on that
    path are unexercised; `build_blas` sets `STATIC_BLAS_FLAGS` (with `ALLOW_COMPACTION`) but runs
    no compaction pass, so it would produce uncompacted BLAS if revived; and its
    `.expect("BLAS build requires a per-mesh vertex buffer…")` would panic on a global-only mesh
    because, unlike `resources.rs::build_blas_batched`, `build_blas_for_mesh` does not filter on
    `mesh.rt_capable`.
- **Impact**: No runtime impact (dead). Two costs: an authoritative memory doc asserts a call site
  that does not exist, and ~300 LOC of unexercised AS-build code carries a revive-time panic and a
  missing compaction pass that a future "lazy BLAS upload" author would inherit silently.
- **Related**: #915 (CLOSED — the guard it added is on the dead path), #658 (CLOSED — the
  `ALLOW_COMPACTION` flag it added is on the dead path), #1141 (the same "delete the dead build
  entry point" call made for the skinned sibling `build_skinned_blas`).
- **Suggested Fix**: Either delete `build_blas_for_mesh` + `AccelerationManager::build_blas`
  (the #1141 precedent) and drop the three doc references, or — if the lazy-upload path is
  genuinely planned — add the `mesh.rt_capable` filter to `build_blas_for_mesh` and correct
  `memory-budget.md` to say the path is provisioned but unwired.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
