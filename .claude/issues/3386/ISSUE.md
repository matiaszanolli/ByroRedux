# PERF-D7-2026-08-27-02: The worldspace drain and shutdown sweep tear the resident set down one cell at a time, re-running global batch finalization per cell

- **Issue**: [#3386](https://github.com/matiaszanolli/ByroRedux/issues/3386)
- **Finding ID**: `PERF-D7-2026-08-27-02`
- **Source report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `low,performance,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3386 --json state`.

---

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/streaming_helpers.rs:420-425`
  (`drain_streaming_state`) and `byroredux/src/app_events.rs:50-52`
  (`App::shutdown`); the batched alternative is
  `byroredux/src/cell_loader/unload.rs:111-124` (`unload_cells`), already used at
  `byroredux/src/app_step.rs:141`.
- **Status**: NEW
- **Description**: `unload_cell` is `unload_cell_inner` **plus**
  `finish_unload_batch` (`unload.rs:99-102`). `finish_unload_batch`
  (`unload.rs:321-342`) is the *global* pass: `world.shrink_storages()` over every
  registered storage (#2148) and `AccelerationManager::shrink_blas_scratch_to_fit`
  (#495). The 2026-08-05 boundary-teardown change added `unload_cells` precisely
  so the usual three-cell eviction ring pays that pass **once** instead of three
  times, and its own doc says so: *"repeating those global passes per cell only
  multiplies the boundary hitch."*

  The two remaining bulk-teardown call sites never adopted it, and they unload
  far more cells at once than the boundary path ever does — the whole resident
  set (`state.loaded.drain()`), which is 49 cells at `--radius 3` and 121 at
  `exterior_transition_radius`'s `DEFAULT_TRANSITION_RADIUS = 5`
  (`app_step.rs:926`).
- **Evidence**: `streaming_helpers.rs:420-425`
  ```rust
  for ((_gx, _gy), slot) in cells {
      cell_loader::unload_cell(world, ctx, slot.cell_root);
  }
  if let Some(cell_root) = persistent_root {
      cell_loader::unload_cell(world, ctx, cell_root);
  }
  ```
  Reached from three live paths: the Exterior→Interior door transition and the
  Exterior→Exterior worldspace swap (`app_step.rs:744, 840`), the M45.1 live
  save-load reload (`save_io.rs:1125, 1237`), and the debug-UI load ops
  (`debug_load.rs:279, 368`). Per-cell semantics are identical between
  `unload_cell` and `unload_cells` — the only difference is how many times the
  finalization runs — so the substitution is mechanical.
- **Impact**: modest and bounded, and I want to be honest about which half is
  which. `world.shrink_storages()` largely **amortises**: `shrink_sparse_tail`
  (`crates/core/src/ecs/sparse_set.rs:202-214`) is a backwards scan that stops at
  the first live slot, and `Vec::shrink_to_fit` is a no-op once `len ==
  capacity`, so across a drain the total scan/realloc work is roughly one pass'
  worth however many times it is called. The **non-amortised** term is
  `shrink_blas_scratch_to_fit`, which walks `blas_entries: Vec<Option<BlasEntry>>`
  (`acceleration/mod.rs:81`) in full on every call to recompute the scratch peak
  — and that `Vec` is indexed by mesh handle, which is documented as never
  reused (`unload.rs:86-90`), so it grows monotonically across a session. 121
  full walks of a five-figure handle table, plus 121 iterations over the ~150
  registered storages, is single-digit-to-low-tens of milliseconds added to a
  transition that the code's own note already prices at *"a few-hundred-ms BSA
  re-open"* (`app_step.rs:673-675`). Real, mechanical to remove, not a hitch on
  its own.
- **Related**: the 2026-08-26 FNV audit verified "batch finalization runs once,
  not per cell" — but checked only the `unload_cells` path, not these two call
  sites. #3254 (OPEN) is a correctness issue in the same function.
- **Suggested Fix**: collect the drained `cell_root`s (plus `persistent_root`)
  into one `Vec<EntityId>` and call `cell_loader::unload_cells` once; do the same
  in `App::shutdown`. `drain_streaming_state` gains the `UnloadPhaseTimings`
  return for free, which is a prerequisite for ever budgeting the interior
  transition.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `PERF-D7-2026-08-27-02`._
