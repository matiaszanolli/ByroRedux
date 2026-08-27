# Concurrency Audit — 2026-08-27 (Dimension 7 ONLY)

**Scope**: `/audit-concurrency` **Dimension 7 only** — *Worker Threads (Streaming,
Debug Server) & Thread-Safety Bounds*. Run as part of the `streaming-deep`
audit-suite preset.

**This is NOT a full concurrency audit.** Dimensions 1–6 (Vulkan queue / AS sync,
compute→AS→fragment chains, ECS lock ordering, scheduler access declarations,
Resource↔Storage `RwLock` patterns, GPU teardown ordering) were **not executed**
in this run. The most recent full sweep is
`docs/audits/AUDIT_CONCURRENCY_2026-08-24.md`.

**Depth**: deep (traced concurrent paths and teardown windows, not just
primitive presence).

**Method**: static analysis only. No engine process was launched (the user may
have a live instance); no `gh` calls (dedup ran against the cached
`/tmp/audit/issues.json`, 400 issues open+closed, plus every prior
`docs/audits/AUDIT_CONCURRENCY_*.md`).

**Delta context**: the 2026-08-24 run reported Dimension 7 **CLEAN (0 findings)**.
Three commits have touched Dim-7 surface since then — `98eea9b3` (2026-08-25,
exterior session reload + bootstrap-mode refactor), `a47dcf0c` (2026-08-26,
`Fix #2369` persistent-CELL preservation across worldspace crossings), and
`0f651aba` (2026-08-26). All three findings below are in that delta or in code
it newly made reachable.

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 1 |
| **Total** | **3** |

| ID | Severity | Title |
|---|---|---|
| CONC-D7-2026-08-27-01 | HIGH | A preserved persistent-CELL root abandons its in-flight `PersistentCellApplyJob`, leaving the persistent CELL permanently half-spawned |
| CONC-D7-2026-08-27-02 | MEDIUM | `PersistentCellApplyJob` has no `cancel`, so every streaming drain leaks its `ReferenceLoadJob`'s pending `AnimationClipRegistry` handles |
| CONC-D7-2026-08-27-03 | LOW | `build_stream_parse_pool`'s "reserving half" rationale is false — rayon's global pool is never resized, so the stream pool is purely additive |

No Vulkan-sync (GPU-side `sync`) findings were produced in this run, so the
speculative-fix guardrail did not need to be exercised. All three findings are
CPU-side (`concurrency`) and observable from source plus, for 01 and 02, a
reproducible interactive sequence.

---

## Findings

### CONC-D7-2026-08-27-01: A preserved persistent-CELL root abandons its in-flight `PersistentCellApplyJob`, leaving the persistent CELL permanently half-spawned

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

### CONC-D7-2026-08-27-02: `PersistentCellApplyJob` has no `cancel`, so every streaming drain leaks its `ReferenceLoadJob`'s pending `AnimationClipRegistry` handles

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

### CONC-D7-2026-08-27-03: `build_stream_parse_pool`'s "reserving half" rationale is false — rayon's global pool is never resized, so the stream pool is purely additive

- **Severity**: LOW
- **Dimension**: Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds — worker thread inventory / stated invariant
- **Location**: `byroredux/src/streaming.rs:1008-1029`
- **Status**: NEW
- **Trigger Conditions**: None — this is a documentation/design-claim defect, not a runtime fault. Its practical consequence shows up whenever a fresh-parse burst (`>= PRE_PARSE_RAYON_MIN` uncached NIFs in one cell) overlaps a `Stage::Update` parallel batch.
- **Verification Path**: Static. `grep -rn "build_global\|ThreadPoolBuilder" crates byroredux` returns exactly one production hit — `streaming.rs:1022` — so nothing ever calls `rayon::ThreadPoolBuilder::build_global()`. rayon-core 1.13's default global registry is therefore built with `num_threads == 0`, i.e. `available_parallelism()`.
- **Description**: The `#3089` fix correctly gave the cell-stream worker a private rayon pool so its Phase-2 fan-out cannot occupy the global pool's workers. The accompanying rationale over-claims what that buys:

  > *"reserving half here means a large fresh-parse burst can never claim more workers than the frame's parallel stages have left to run on."*

  Nothing is reserved *from* the global pool. Building a second `ThreadPool` creates an independent registry; the global pool keeps all `N` threads. During a burst the process therefore has `N` global-pool workers **plus** `N/2` `byro-stream-parse-*` workers **plus** the cell-stream worker, main, listener and audio threads runnable at once — 1.5×N rayon threads on N hardware threads, arbitrated by the OS scheduler rather than by any partition. On the dev 7950X (`available_parallelism` = 32) that is 32 + 16 = 48 rayon workers.
- **Evidence**: `byroredux/src/streaming.rs:1017-1029` (the builder — `num_threads((total / 2).max(1))`, no `build_global`), and the absence of any other `ThreadPoolBuilder` / `build_global` call site in the workspace.
- **Impact**: No correctness impact. The isolation benefit `#3089` actually delivers is real and worth keeping (a burst can no longer starve `par_iter_mut` of global-pool workers). The risk is the stale premise: a future reader sizing the pool, or auditing a frame-time regression during streaming bursts, will reason from a core partition that does not exist. This is the same class as `#3091` (a streaming doc comment describing the wrong function) and is why the project treats stated invariants as auditable.
- **Related**: `#3089` (CLOSED — the pool itself), `#3211` (CLOSED — the guards that pin the pool constructor and rayon's `install`, but not this claim), `#3091` (CLOSED — the sibling doc-accuracy fix in the same function's neighbourhood).
- **Suggested Fix**: Reword the comment to state what is true — the pool *isolates* stream parsing from the frame's global-pool batch and is deliberately sized to `N/2` to limit oversubscription — and drop the "can never claim more workers than the frame has left" sentence. If a real cap is wanted, `rayon::ThreadPoolBuilder::new().num_threads(N/2).build_global()` at boot would actually partition, at the cost of halving the ECS scheduler's parallelism.

---

## Checklist items verified clean (no finding)

Every Dimension-7 checklist item was re-derived against current code rather than
carried over from `AUDIT_CONCURRENCY_2026-08-24.md`.

1. **Streaming Drop ordering (`#1167`)** — intact. `WorldStreamingState::shutdown`
   (`streaming.rs:907-930`) `take()`s `worker` first, then `request_tx`, then joins;
   `Drop` (`streaming.rs:944-948`) delegates to `shutdown(1 s)` and short-circuits on
   the already-taken handle, so the join runs exactly once regardless of field
   declaration order. `join_with_timeout` (`streaming.rs:978-1005`) polls
   `is_finished` at 10 ms and drops (detaches) the handle at the deadline — no watcher
   thread, no `Arc`-held resource on the timeout path (`#1169`).
2. **Worker ↔ main data flow** — clean. `cell_pre_parse_worker` / `pre_parse_cell` /
   `parse_one_nif` take no `World` parameter of any kind, so touching the ECS is
   structurally impossible; the `assert_send::<PartialNifImport>()` compile-time guard
   (`#1171`) is still at `streaming.rs:578-581`. `Arc<TextureProvider>` is immutable
   after construction (`asset_provider/texture.rs:7-10` — two `Vec<Archive>`, no
   interior mutability) and both archive backends serialise their `File` behind a
   `Mutex` (`crates/bsa/src/archive/mod.rs:49`, `crates/bsa/src/ba2.rs:120`).
   `merge_external_material` has zero call sites reachable from the worker — the
   `MaterialProvider` stays on `WorldStreamingState` and is only borrowed from
   main-thread drain code (`streaming_helpers.rs:479`, `:547`, `:627`, `:656`, `:738`). The NIF
   import cache is reached from the worker only through an immutable
   `Arc<HashSet<String>>` snapshot (`NifImportRegistry::snapshot_keys`, consumed at
   `streaming.rs:1283-1288`), with every write deferred to the main-thread
   `finish_streaming_import`.
3. **Debug server** — clean. Per-client threads (`listener.rs:255-330`) never
   reference `World`; they only push into the `Arc<Mutex<Vec<PendingCommand>>>`. The
   queue is bounded at `MAX_QUEUED_COMMANDS = 64` with an atomic check-and-push under
   one lock (`try_enqueue_command`, `listener.rs:69-89`), so two clients cannot both
   slip past the cap (`#1010`). Shutdown side channel (`#1009`), the per-command
   `cancel` flag (`#1007`), the owner-tagged screenshot claim (`#1006`), the cancel-on-
   timeout (`#1011`) and the capture-generation gate (`#1603`,
   `context/screenshot.rs:115-131`) are all present. The drain system is a
   `Stage::Late` exclusive running on the main thread strictly before
   `render_one_frame`, so it cannot race the fence-gated readback.
4. **Allocator sharing** — clean. `SharedAllocator = Arc<Mutex<vulkan::Allocator>>`
   (`allocator.rs:15`); every `.lock()` site in `crates/renderer` is a single-statement
   lock-then-allocate/free. The one *cross-thread* holder is
   `metrics_sample_system` (`systems/metrics.rs:104-112`), a parallel `Stage::Late`
   system that reads `AllocatorResource` under a declared access
   (`boot.rs:1500-1507`) and only calls `generate_report()`; it cannot overlap
   `draw_frame`, which runs outside `Scheduler::run`. No holder keeps the guard across
   a queue submit; the one-time submit in `crates/renderer/src/vulkan/texture.rs:803-836` correctly scopes the
   *queue* guard to the submit and keeps the *fence* guard across the wait (`#1713`).
5. **`Send + Sync` bounds** — clean. `Component` (`crates/core/src/ecs/storage.rs:17`)
   and `Resource` (`crates/core/src/ecs/resource.rs:13`) both require
   `'static + Send + Sync`, and the workspace contains **zero** `unsafe impl Send` /
   `unsafe impl Sync` (the only `unsafe impl`s are three `AnyBitPattern` marker impls
   in `crates/nif/src/blocks/bs_geometry.rs`). `UiManager` (Ruffle/wgpu) is
   deliberately *not* a `Resource` — it is a plain `App` field, keeping the non-`Sync`
   player on one thread. `crates/debug-ui` contains no threading primitive at all.
6. **Thread inventory** — unchanged at three production spawns plus one rayon pool:
   `streaming.rs:753` (cell worker), `debug-server/src/listener.rs:169` (listener),
   `:228` (per-client), and the `byro-stream-parse-{i}` pool owned by and confined to
   the cell worker. Every other `thread::spawn` / `thread::scope` hit in the workspace
   is inside a `#[cfg(test)]` module (`crates/core/src/ecs/resources/mod.rs:1715`,
   `crates/papyrus/src/parser/script.rs:1244`, `crates/scripting/src/quest_stages.rs:1515`
   and `:1551`, `crates/core/src/ecs/lock_tracker.rs:455`, `:788`, `:799`).

---

## Candidates considered and NOT reported

Recorded so a later sweep does not re-derive them.

1. **Poisoned-`Mutex` `.unwrap()` in the debug server.** `try_enqueue_command`
   (`listener.rs:76`), `DebugDrainSystem::run` (`system.rs:137`) and
   `DebugServerHandle::shutdown_and_join` (`listener.rs:127`) all `.unwrap()` their
   lock, unlike the project's established poison-recovery pattern (`#1174`, `#2385`).
   Not reported: no code path holds either mutex across anything that can panic — the
   `CommandQueue` guard spans only a `len()` check + `push` or a `mem::take`, and the
   `StreamRegistry` guard spans only a `retain` + `push` or a best-effort
   `shutdown(Both)` whose `Result` is discarded. Poisoning is unreachable, so the
   `.unwrap()` is a style divergence, not a live hazard.
2. **`DebugDrainSystem::run` returns early on a cancelled screenshot, skipping that
   frame's whole command drain** (`system.rs:72-78`). Real, but already tracked as
   **`#3090` (OPEN)** — skipped per dedup rule 4.
3. **Unbounded `payload_rx` backlog between the cell worker and the main-thread
   apply.** `advance_streaming_apply` holds exactly one `active_apply` at a time under
   `STREAMING_APPLY_BUDGET`, while the worker races ahead through the whole dispatched
   batch (up to 15×15 coords at `--radius 7`), so fully-parsed `LoadCellPayload`s
   accumulate in an unbounded `mpsc`. Not reported as a defect: the backlog is bounded
   by the `pending` map, which `queue_loads` (`streaming.rs:855-891`) caps at the
   grid's own size, and `stale_pending_coords` (`#2113`) reclaims entries that leave
   the radius. The related "one empty `cached_keys` snapshot for the whole bootstrap
   batch means shared statics are parsed once per cell" behaviour is explicitly
   documented and accepted at `world_setup.rs:816-827`, and `finish_partial_import`
   early-outs on an already-cached key (pinned by
   `finish_partial_import_early_outs_on_already_cached_positive_entry`), so it costs
   worker CPU and transient RAM but never duplicate GPU uploads.
4. **`build_stream_parse_pool` `.expect()`s outside `pre_parse_cell_panic_safe`.**
   Same disproof as the 2026-08-20 run: `ThreadPoolBuilder::build` with an explicit
   `num_threads` has no realistic failure mode, and the surviving path (`Receiver`
   drops, `send_request` starts returning `Err`, which `queue_loads` already handles
   with a rollback + `log::error!`) is benign.
5. **A worldspace transition spawns a fresh worker + a fresh `N/2` rayon pool while
   the previous worker may still be detached after a 1 s join timeout.** Real, but
   transient: the detached worker exits on its next `payload_tx.send` failure and
   drops its pool, and the `Arc<TextureProvider>` / `Arc<ExteriorWorldContext>` it
   holds keep it memory-safe meanwhile. Not a leak.
6. **Per-client threads keep evaluating commands they have already abandoned.** After
   the 5 s `recv_timeout` the client drops its `Receiver`, so the drain's
   `response_tx.send` fails harmlessly and no stale response can be mis-attributed to
   a later request; the `cancel` flag is honoured only by the screenshot path, but the
   64-command cap bounds the wasted main-thread evaluation. Loopback-only surface
   (`#857`).

---

## Coverage gaps in this run

Per `_audit-common.md`'s un-owned-subsystem rule, stating what was skipped:

- **Dimensions 1–6 were not run.** Anything GPU-side (`sync`), ECS lock-ordering, or
  scheduler-declaration related is out of scope for this report.
- **`crates/debug-server` / `crates/debug-protocol`** are an un-owned subsystem whose
  *command surface* (the `evaluator` and what a connected client can make the engine
  do) has no owner audit. This run covered only its **threading** — listener/per-client
  lifecycle, queue bounding, World isolation — not its command semantics.
- **`crates/sdk`** (added 2026-08-25 in `21a840d5`, listed in `_audit-common.md` as
  un-owned) was **not** examined. It exposes renderer-independent world/snapshot
  contracts; whether any of them cross a thread boundary is unverified by this run.
- **`crates/fsr3-sys`** (the workspace's only live FFI crossing) and the audio
  backend's own threads were not examined — neither is on the Dimension-7 entry-point
  list, and both belong to `/audit-safety` Dim 1 and `/audit-audio` respectively.

---

## Dedup Methodology

- Cached issue baseline: `/tmp/audit/issues.json` (400 issues, open + closed, with
  titles/labels/state). Queried by regex for `stream|worker|thread|payload|channel|
  rayon|debug.server|backlog|unbounded|mpsc|listener|concurren`, then again for
  `CONC-|concurren|deadlock|lock.order|poison|join|shutdown|Drop`, then for
  `persistent|clip handle|AnimationClipRegistry|apply job|cancel`. No `gh` calls were
  made.
- Prior reports scanned: all 25 `docs/audits/AUDIT_CONCURRENCY_*.md`, with
  `2026-08-24` (the last full run, Dim 7 = 0 findings) and `2026-08-20` (Dim 7 clean +
  its "considered and NOT reported" list) read in full for the Dimension-7 sections.
  `grep -rl persistent_apply docs/audits/` returns nothing — no prior report has
  examined the persistent-CELL apply job.
- Every finding's premise was re-read against current `main` (`7f78ad9d`) before
  filing, and each was subjected to an explicit disproof attempt (recorded above for
  the ones that survived, and in "Candidates considered and NOT reported" for the ones
  that did not).

---

## Next step

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-08-27.md
```

Suggested labels — 01: `high` `bug` `concurrency` `terrain-exterior`; 02: `medium`
`bug` `concurrency` `memory`; 03: `low` `documentation` `concurrency` `doc-rot`.
