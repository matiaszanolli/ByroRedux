# CONC-D7-2026-08-27-01: A preserved persistent-CELL root abandons its in-flight `PersistentCellApplyJob`, leaving the persistent CELL permanently half-spawned

- **Issue**: [#3376](https://github.com/matiaszanolli/ByroRedux/issues/3376)
- **Finding ID**: `CONC-D7-2026-08-27-01`
- **Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `high,concurrency,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3376 --json state`.

---

- **Severity**: HIGH
- **Dimension**: Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds — streaming-state teardown across a worldspace crossing
- **Location**: `byroredux/src/app_step.rs:823-846`, `byroredux/src/streaming_helpers.rs:385-450`, `byroredux/src/scene/world_setup.rs:774-786` + `:919-931`
- **Status**: NEW
- **Trigger Conditions**: An exterior→exterior worldspace crossing (a door whose `TransitionDestination` is `Exterior`) that (a) resolves to the **same** persistent CELL as the one currently active — a child worldspace crossing back to its parent, or two siblings sharing an ancestor's persistent CELL via the `WNAM` chain — **and** (b) happens while `WorldStreamingState.persistent_apply` is still `Some(job)`. Condition (b) is the normal interactive state for the first N frames after entering any worldspace: `ExteriorBootstrapMode::from_cli_args` (`world_setup.rs:732-738`) selects `ForegroundFirst` for every launch without `--bench-frames`, and only the `FullRadius` branch (`world_setup.rs:785-805`) drives the persistent job to `Complete` synchronously.
- **Verification Path**: Observable without a GPU or validation layer — count the entities stamped with the persistent root's `CellRoot` (or `entities` / `prid` via `byro-dbg`) before and after such a crossing, or watch for the absence of the `"Worldspace '…' authors no persistent CELL of its own"` / persistent-cell completion path re-running. `cargo test` cannot see it: `PersistentCellApplyJob::advance` needs a `VulkanContext` and on-disk game data (stated at `byroredux/src/cell_loader/exterior.rs:838-841`).
- **Description**: `#2369`'s item-C2 fix (`a47dcf0c`, 2026-08-26) added a path that lets the persistent-CELL **root entity** survive a worldspace crossing instead of being drained and rebuilt. It detaches only the root:

  ```rust
  // byroredux/src/app_step.rs:823-834
  let preserved_persistent_root = self.streaming.as_mut().and_then(|state| {
      let root = cell_loader::persistent_root_survives_crossing(
          &self.world, state.persistent_root, &wctx)?;
      state.persistent_root = None;          // detached …
      Some(root)                             // … but persistent_apply untouched
  });
  ```

  `persistent_root_survives_crossing` (`cell_loader/exterior.rs:466-479`) compares only `CellFormId` identity — it has no notion of whether the root's spawn is finished. The resumable continuation that would finish it, `state.persistent_apply`, is then destroyed: `drain_streaming_state` cancels `active_apply` only (`streaming_helpers.rs:393`, via `cancel_active_streaming_apply` at `:520-529`), and `persistent_apply` is dropped with the moved-out `state`. Finally `assemble_exterior_streaming` reinstalls the preserved root **before** `stream_initial_radius` runs:

  ```rust
  // byroredux/src/scene/world_setup.rs:923-931
  if let Some(root) = preserved_persistent_root { state.persistent_root = Some(root); }
  ...
  let cam_center = stream_initial_radius(world, ctx, &mut state, grid.0, grid.1, bootstrap_mode);
  ```

  and `stream_initial_radius`'s guard is `if state.persistent_root.is_none() && state.persistent_apply.is_none()` (`world_setup.rs:774`). With the root installed, the guard is false, no replacement job is created, and the remaining `local_refs` / `logical_stub_refs` of that persistent CELL are never spawned for the rest of the session.
- **Evidence**: The three sites above, plus `PersistentCellApplyJob`'s own field set (`cell_loader/exterior.rs:201-214`) showing the unconsumed work it carries (`local_refs`, `references: Option<Box<ReferenceLoadJob>>`, `logical_stub_refs`, `next_logical_stub`). Contrast with the sibling type: `ExteriorCellApplyJob::cancel` (`cell_loader/exterior.rs:930-935`) exists precisely because a half-applied cell job must be reclaimed, and `advance_streaming_apply` calls it on every stale-generation cancellation (`streaming_helpers.rs:602`).
- **Impact**: Silent, permanent content loss for the session. The worldspace persistent CELL is where Bethesda authors the references that must exist regardless of streaming radius — doors, quest-relevant refs, unique/persistent actors. After an affected crossing the world looks intact but an arbitrary tail of those refs is simply absent, with no log line and no way to recover short of a fresh load. Blast radius is every game with a `WNAM` parent-worldspace chain (Skyrim's Tamriel children, FO3/FNV's `Wasteland` children, FO4). Narrow trigger window, but severity follows impact per `_audit-severity.md`.
- **Related**: `#2369` (OPEN — the EX-14/15 epic; only item C2's reconcile half is closed by `a47dcf0c`); `#3299` (OPEN — EX-16 item 4, the *ordinary stream-tile* state snapshot/restore, a different boundary); CONC-D7-2026-08-27-02 (same missing-cancel root cause, different consequence). No existing issue covers this — grep of all 400 cached issues for `persistent` / `apply job` / `cancel` returns only `#3090` and `#3360`, both unrelated.
- **Suggested Fix**: Make `persistent_root_survives_crossing` (or its `app_step.rs` caller) refuse to preserve a root whose job is still in flight — i.e. return `None` when `state.persistent_apply.is_some()` — so the crossing falls back to the correct always-rebuild path. Alternatively, hand the in-flight job across the crossing alongside the root (it is already `wctx`-parameterised at `advance` time), but that is the larger change and needs the destination `ExteriorWorldContext` to be proven equivalent for the job's remaining refs.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_CONCURRENCY_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `CONC-D7-2026-08-27-01`._
