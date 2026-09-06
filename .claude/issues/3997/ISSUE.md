# #3997 — REN-2026-09-06-D1-03: `memory-budget.md`'s Acceleration-Structures section names a file the per-frame eviction call left, and its eviction-site census is one site short

**Labels**: low, memory, renderer, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: AS Correctness (doc-rot)
- **Location**: `docs/engine/memory-budget.md` §"LRU eviction". Ground truth:
  `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (the
  per-frame `accel.evict_unused_blas(&self.device, alloc, 0)` at the tail of the
  TLAS-build block), `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (`build_blas_batched`'s three internal calls: pre-batch, mid-batch, and the
  compaction-phase call inside `alloc_compact`)
- **Status**: NEW (not covered by #3866, which is scoped to the budget *formula*
  and the dead *compute_blas_budget* name; not in the OPEN cache)
- **Description**: Two divergences in the section this audit is instructed to
  treat as authoritative:
  1. The doc places the per-frame eviction call *"at the end of `draw_frame`'s
     TLAS-build block"* and links `draw.rs`. `7463204e` ("split `draw_frame`
     into phase helpers") moved that block into
     `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs`; `draw.rs`
     no longer contains an `evict_unused_blas` call at all. The behaviour is
     unchanged (it is still the tail of the TLAS-build block, still with
     `pending_bytes = 0`) — only the file is wrong. **The doc's sibling claim in
     the same section is still correct**: `shrink_tlas_to_fit` and
     `shrink_tlas_scratch_to_fit` really do remain in `draw.rs`, so the two
     statements now point at different files for what the doc describes as
     adjacent end-of-frame work, which is exactly the shape that misleads.
  2. The doc says eviction *"runs pre-batch and mid-batch"*. There is a third
     internal site: the pre-emptive call at the head of `alloc_compact`
     (#2927 / `PERF-D3-03`), which passes the exact
     `total_before + total_after` peak — the only site that sees the real
     residency peak of a batch, since the compaction destinations are allocated
     while every Phase-1 original is still live. The doc's `evict_unused_blas`
     doc-comment in `blas_static.rs` does name it ("`build_blas_batched`'s three
     internal call sites"), so the code and the doc disagree with each other.
- **Evidence**:
  - `grep -rn "evict_unused_blas" --include='*.rs' crates byroredux` → the only
    non-`blas_static.rs` production call is
    `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs`, guarded by
    `if !tlas_build_failed` immediately after `write_tlas` + the `rt_flag` patch.
  - `grep -rn "shrink_tlas_to_fit\|shrink_tlas_scratch_to_fit" --include='*.rs' crates byroredux`
    → both still in `crates/renderer/src/vulkan/context/draw.rs`.
  - `blas_static.rs`'s `evict_unused_blas` doc: *"The params are retained so the
    call sites (`build_blas_batched`'s **three** internal call sites plus
    `dispatch_skin_and_cluster.rs`) keep a stable signature."*
- **Impact**: Documentation only, but this doc is the authority an auditor is
  told to check the code against, so a wrong file name here converts into a
  wasted or wrong finding on the next sweep — the same mechanism as
  `REN-2026-09-06-D1-02`.
- **Related**: `7463204e` (the split), `#2927` / `PERF-D3-03` (the third site),
  `#1911` / `REN-D1-01`, `#1792`, `#3866` / `#3842` / `#3841` (the three other
  open doc-rot issues on this same subsystem).
- **Suggested Fix**: In the "LRU eviction" section, re-link the per-frame call to
  `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (noting it is
  still `draw_frame`'s TLAS-build tail, reached through a phase helper), and
  change "pre-batch and mid-batch" to "pre-batch, mid-batch, and once at the head
  of the compaction phase with the exact `total_before + total_after` peak
  (#2927)".

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
