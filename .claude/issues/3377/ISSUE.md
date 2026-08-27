# CONC-D7-2026-08-27-02: `PersistentCellApplyJob` has no `cancel`, so every streaming drain leaks its `ReferenceLoadJob`'s pending `AnimationClipRegistry` handles

- **Issue**: [#3377](https://github.com/matiaszanolli/ByroRedux/issues/3377)
- **Finding ID**: `CONC-D7-2026-08-27-02`
- **Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `medium,concurrency,memory,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3377 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds — streaming-state teardown
- **Location**: `byroredux/src/streaming_helpers.rs:385-450` (`drain_streaming_state`), `byroredux/src/cell_loader/exterior.rs:201-214` + `:930-935`, `byroredux/src/cell_loader/references/mod.rs:98-110`
- **Status**: NEW
- **Trigger Conditions**: Any `drain_streaming_state` call while `WorldStreamingState.persistent_apply` is `Some(job)` **and** that job's `references` continuation has accumulated at least one entry in `RefLoadAccum.pending_clip_handles` (i.e. at least one cache-miss REFR with an embedded animation clip has been spawned since the job started). Reachable from all six drain sites: `app_step.rs:744` (exterior→interior door), `app_step.rs:840` (exterior→exterior crossing), `debug_load.rs:279` and `:368` (`dbgload`), `save_io.rs:1125` and `:1237` (save-load reload).
- **Verification Path**: `cargo test`-invisible for the same reason as finding 01 (needs a `VulkanContext` + game data). Observable at runtime as a monotonically growing `AnimationClipRegistry` length across repeated door transitions — `mem.frag` / the debug-UI registry counters, or a `byro-dbg` session that walks in and out of an exterior several times.
- **Description**: `ReferenceLoadJob` deliberately defers its clip-handle bookkeeping: handles acquired for cache-miss REFRs are staged in `accum.pending_clip_handles` (`cell_loader/references/synth_child.rs:571`) and committed to `NifImportRegistry` only at end-of-cell (`cell_loader/references/complete.rs:126`). Because a cancelled cell never reaches that commit, the type carries an explicit release path:

  ```rust
  // byroredux/src/cell_loader/references/mod.rs:101-109
  pub(super) fn cancel(self, world: &World) {
      if self.accum.pending_clip_handles.is_empty() { return; }
      let mut clip_reg = world.resource_mut::<AnimationClipRegistry>();
      for handle in self.accum.pending_clip_handles.into_values() { clip_reg.release(handle); }
  }
  ```

  `ExteriorCellApplyJob::cancel` calls it (`exterior.rs:930-935`). `PersistentCellApplyJob` — which drives the *same* `ReferenceLoadJob` through the *same* `load_references_budgeted` (`exterior.rs:225-255`) — has **no `cancel` method at all**, and `drain_streaming_state` never asks for one: it takes `state.persistent_root` and calls `unload_cell` on it (`streaming_helpers.rs:395`, `:423-425`) but leaves `state.persistent_apply` to be dropped with the struct.
- **Evidence**: `grep -n "fn cancel" byroredux/src/cell_loader/exterior.rs` returns exactly one hit (line 930, on `ExteriorCellApplyJob`). `drain_streaming_state`'s body contains no reference to `persistent_apply`; `cancel_active_streaming_apply` (`streaming_helpers.rs:520-529`) only takes `state.active_apply`.
- **Impact**: A bounded-per-teardown but unbounded-across-a-session leak in `AnimationClipRegistry` — the clip's refcount never reaches 0, so its `AnimationClip` data (and the registry slot) pins for the process lifetime. Not per-frame, so below the HIGH bar for resource leaks, but it compounds over a play session with many transitions and it silently defeats the `#863` release discipline that the exterior-cell path already honours. The partially-spawned *entities* are not leaked in the non-preserved case (they are stamped into the root's `CellRoot` range by `stamp_cell_root_range` on every yield, and `unload_cell(persistent_root)` reclaims them).
- **Related**: CONC-D7-2026-08-27-01 (same missing-cancel root cause); `#863` (the clip-handle release contract this path skips); `#1536` (the structurally identical "this reclaim path was never wired into `drain_streaming_state`" bug, for LOD blocks).
- **Suggested Fix**: Give `PersistentCellApplyJob` a `cancel(self, world)` that mirrors `ExteriorCellApplyJob::cancel`'s `references.take().map(|r| r.cancel(world))` half (the `unload_cell` half is already done by the drain via `persistent_root`), and call it from `drain_streaming_state` next to `cancel_active_streaming_apply`.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_CONCURRENCY_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `CONC-D7-2026-08-27-02`._
