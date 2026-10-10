**HEAD**: `3bcf6c8e8` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-10-08.md` (@ `00f580e09`, 81 commits ago) · **Audited**: Dim 7 in full (the streaming pre-parse worker, its channels and shutdown, the payload handoff, and the texture prefetch store — the suite's area emphasis; read in full although `streaming/` has zero commits); Dim 5 in full (#5418 detach pass, #5379 purge, #5371 follow-up, the PhysicsWorld-sink sweep); Dims 3 and 4 for their deltas (18 and 5 commits), plus the lock-order lanes · **Unchanged since baseline (skimmed)**: Dim 1, Dim 2 and Dim 6. Their only commits are CPU-side rig-key state, shader math, DDS/device-name parsing, a log-flag move in `dynamic_rgba.rs` and one doc comment in `gpu_types.rs`; I checked the guards for each. Zero-commit sub-paths, guard spot-checked only: `crates/core/src/ecs/{world,lock_tracker,access,scheduler}.rs` (production), `byroredux/src/render/`, `crates/bsa/src/read_at.rs`, `byroredux/src/asset_provider/texture_prefetch.rs`, `crates/debug-server/src/{listener,system}.rs`, `crates/ui/src/player.rs`, `crates/renderer/src/vulkan/{teardown,resize,buffer,image,egui_pass}.rs`.

# Concurrency & Synchronization Audit — 2026-10-09

**Command**: `/audit-concurrency` (all dimensions, depth `deep`), run inside `/audit-suite --preset streaming-deep`.
**Area emphasis** (suite rules): `byroredux/src/streaming/`, `byroredux/src/npc_spawn/`, `byroredux/src/cell_loader/`.
**Severity scale**: `.claude/commands/_audit-severity.md`.

## Method

- **Delta scope.** I ran `git log 00f580e09..HEAD -- <Paths>` per dimension and analysed all seven dimensions myself in this
  session, with no sub-agents. Per-dimension notes are in `/tmp/audit/concurrency/dim_{1..7}.md`. This report was reconciled
  against each of them.
- **No engine launch** (suite rule). The lock-order lane that drives a real Vulkan device (CI `vulkan-validation`) needs an
  engine run, so I did not run it locally. Its status at HEAD comes from the CI logs, read with `gh`. The test-only lanes ran
  locally.
- **One probe outside the tree.** `/tmp/audit/concurrency/mpsc_probe/probe.rs` is a 30-line program that uses only the
  standard library. It reproduces the streaming shutdown handshake so that CONC-D7-2026-10-09-01 is a measurement, not a
  reading. No source file in the repo was edited.
- **Dedup.** Sources checked:
  - open issues (`/tmp/audit/issues.json`, 147);
  - closed-issue searches: `streaming worker shutdown`, `join_with_timeout`, `request_tx queued`, `batch memo stream
    worker`, `snapshot_keys LRU evict worker`, `re-adoption purge subtree`, `CinematicReAdoption`;
  - the baseline report.

### Guard runs at HEAD (local, rustc 1.96.0, `TMPDIR=/mnt/data/tmp`)

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux --no-fail-fast` | **2743 passed, 0 failed**, 55 ignored (2705 at baseline) |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-scripting --no-fail-fast` | **532 passed, 0 failed**, 3 ignored (524 at baseline) |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-physics --no-fail-fast` | **208 passed, 0 failed** |
| scheduler-proof tests inside the bin run | all ok, none ignored |
| `cargo test -p byroredux-renderer --lib -- <Dim 1/2 guard filters>` | 11 passed, 0 failed |
| `cargo test -p byroredux-bsa --lib -- concurrent threaded parallel` | 3 passed |

CI evidence (read with `gh`). Runs 37936635642 through 38006017385 cover `f1141aebe` to `4fc462a96`, every completed run on
`main` since the baseline:
- **ABBA lock-order detector**: green on every run.
- **Vulkan validation layers (lavapipe)**: green on every run.
- **Test + Check + Clippy**: red on every run. The tests in that job pass (2743), but clippy fails with 7
  `doc_lazy_continuation` errors in `crates/spt/src/parser.rs:92-93`.

That red job is not a lock lane and does not hide one. It is tech-debt territory (compare the open #5474).

## Summary

| Severity | NEW | Regression | Existing |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 1 | 1 | 0 |
| LOW | 1 | 0 | 2 (#5442, #5069 — unchanged) |

**Headline.**
- **All six baseline findings are fixed and confirmed:**
  - CONC-D5-2026-10-08-01 → #5371
  - CONC-D3-2026-10-08-01 → #5372 (#5066 closed with it)
  - CONC-D4-2026-10-08-01 → #5414
  - CONC-D4-2026-10-08-02 → #5415
  - CONC-D3-2026-10-08-03 → #5416
  - CONC-D3-2026-10-08-04 → #5417 / `bfa538beb`

  Each fix has a test that runs under the detector. Two of those tests close the blind spots the baseline flagged:
  - `forcegreet_walk_with_real_physics_world`;
  - `bound_world` now registers `StoryEvent`.
- **The lock graph is clean.** I found no new lock-order hazard, and no new thread, rayon section or channel. The
  PhysicsWorld-sink sweep the baseline asked for finds every `PhysicsWorld` acquisition in `byroredux/src/systems/` scoped
  with nothing acquired under it.
- **The streaming area's problems are lifecycle and handoff contracts, not locks:**
  1. **Shutdown waits on the whole queue.** The worker shutdown that every drain runs does not stop the worker. The standard
     library still delivers buffered requests after the sender drops, and the payload receiver stays alive for the whole
     join. A door walk or save load during the exterior fill window therefore pays the full 1 s join timeout. I measured
     this with the probe.
  2. **The save-load purge despawns only the roots.** The purge #5379 added for released cinematic convoys despawns only the
     convoy roots, because #5384 later narrowed the pending list to roots. It also skips the GPU and Rapier release.
  3. **The worker's skip filters can skip a key the main thread never caches.** This costs performance only; the
     main-thread parse fallback still loads the model.

| ID | Sev | Dim | Status | Title |
|---|---|---|---|---|
| CONC-D7-2026-10-09-01 | MEDIUM | D7 | NEW | Streaming `shutdown` does not stop the worker: buffered requests are still delivered after `request_tx` drops and `payload_rx` stays alive through the join, so every drain during a worker backlog stalls the main thread for the full 1 s timeout |
| CONC-D5-2026-10-09-01 | MEDIUM | D5 | Regression of #5379 (via #5384) | The session-replace purge despawns only the pending convoy roots, with a bare `despawn_batch`: render subtrees survive the load as frozen ghost geometry, and no mesh/texture/BLAS ref or Rapier body is released |
| CONC-D7-2026-10-09-02 | LOW | D7 | NEW | The worker's two skip filters (the per-dispatch memo, the cache snapshot) can skip a key the main thread will not have at apply time, which forces a main-thread NIF parse |

---

## Findings

### CONC-D7-2026-10-09-01: Streaming `shutdown` does not stop the worker — the queued backlog keeps it parsing through the whole join timeout
- **Severity**: MEDIUM. The join timeout bounds the damage, so it cannot deadlock, and the worker touches neither the `World`
  nor Vulkan, so there is no use-after-free. The cost is a deterministic main-thread stall on a hot path (every
  transition), plus wasted CPU and memory overlap. The documented invariant it rests on is false.
- **Dimension**: Worker Threads
- **Location**:
  - `byroredux/src/streaming/mod.rs:680-704` (`shutdown`); the false claim is in the comment at `:687-693`.
  - `byroredux/src/streaming/mod.rs:425`: `payload_rx` is a plain field, alive until `self` drops.
  - `byroredux/src/streaming/pre_parse.rs:145-175` (worker loop) and `:183-202` (`recv_next_batch_request`).
  - Callers:
    - `streaming_helpers.rs:620` (`drain_streaming_state`, 1 s). That drain serves every exterior→interior door,
      exterior→exterior transition, save-load reload (`save_io.rs:1438`, `:1593`) and debug load (`debug_load.rs:401`,
      `:497`).
    - `app_events.rs:130` (CloseRequested, 1 s).
    - The `Drop` safety net (`mod.rs:717-721`, 1 s).
- **Status**: NEW. Searches matched the closed #856, #1167, #1168 and #1169. Those concern joining versus detaching and the
  watcher-thread leak; none covers the queued backlog.
- **Trigger Conditions**: A drain runs while the worker has more than about 1 s of queued work. That is the normal state for
  a while after exterior entry. Interactive startup is foreground-first: it waits for the centre cell, then leaves the
  rest of the 11×11 default radius (120 cells) queued on the worker (`docs/engine/exterior-grid-streaming.md` §1,
  `queue_loads` at `mod.rs:628-660`). Fast travel across boundaries also builds a backlog.
- **Verification Path**: `cargo test`-level. The shutdown path has no test with a queued backlog; the `join_with_timeout_*`
  tests pin the helper against synthetic threads only. Measured with the probe below.
- **Description**: `shutdown` takes the handle, drops `request_tx`, then polls `is_finished` until the deadline. The comment
  says the worker's `recv()` *"returns Err on its next loop iteration and the thread exits"*, and the skill's Dim 7
  checklist repeats it. That is not how `std::sync::mpsc` works. `recv` and `try_recv` keep returning every message sent
  before the disconnect, and report `Disconnected` only once the queue is empty.

  The worker therefore keeps draining the backlog. Its `payload_tx.send` keeps succeeding, because `payload_rx` is a field
  of the `WorldStreamingState` being shut down and lives until `shutdown` returns and `self` drops. The join hits its
  deadline and detaches the worker. Only after the state drops does the worker's next send fail; it exits one cell later.
  Every payload produced during the join is thrown away with the receiver.
- **Evidence**: The probe mirrors the handshake: 49 queued requests, 50 ms per request, drop the sender, a 10 ms
  `is_finished` poll and a 1 s deadline. Output:
  ```
  joined=false after 1.005532549s; payloads delivered while joining=20
  worker exit after 21 requests        # only after payload_rx was dropped
  ```
- **Impact**:
  - A 1 s main-thread stall on any door walk, save load or debug load made during the fill window, and the same on window
    close.
  - Up to 1 s of stream-pool CPU spent parsing cells the drain has already discarded.
  - After the detach, the old worker, its N/2-thread rayon pool, `Arc<ExteriorWorldContext>` and `Arc<TextureProvider>`
    outlive the state by one more cell. That overlaps with the next worldspace's freshly built worker and pool: transient
    oversubscription and duplicated record-index memory at the transition.
  - The p5 transition/save soak exercises this path.
- **Related**: #856, #1167, #1169 (shutdown design), #3670 (the batch dispatch that makes deep queues normal),
  CONC-D7-2026-10-09-02.
- **Suggested Fix**: Make shutdown cancel the queue, not only close it. Either:
  - share an `Arc<AtomicBool>` cancel flag, set it before dropping `request_tx`, and have the worker check it at the top of
    each loop iteration (optionally also between pipeline admissions); or
  - make `payload_rx` an `Option` and drop it before the join, so the in-flight cell's send fails and the loop breaks.

  Either bounds the join to the cell already in flight. Add a test that queues N slow requests and asserts that `shutdown`
  joins within one request's time. Then fix the comment and the skill text.

### CONC-D5-2026-10-09-01: The session-replace purge despawns only the pending convoy roots with a bare `despawn_batch`
- **Severity**: MEDIUM. The bug is incorrect lifecycle behaviour on a rare but real path. It leaves visible ghost geometry,
  a ghost Rapier collider, and per-occurrence leaked GPU refcounts. It is not per-frame.
- **Dimension**: RwLock Patterns / cell-unload teardown (Dim 5; Dim 6 lifecycle overlap)
- **Location**:
  - `byroredux/src/cell_loader/unload.rs:97-113` (`purge_cinematic_retention_state`: `world.despawn_batch(pending)` at
    `:105`).
  - `byroredux/src/systems/cinematic.rs:552-561` (#5384's parentless-only queue) and `:595` (`pending.extend`).
  - `crates/core/src/ecs/world.rs:170-187` (`despawn_batch`, non-recursive).
  - The canonical release path it bypasses: `unload.rs:372-537` (`release_entities_timed`: GPU drops, item instances,
    `release_victim_rapier_bodies` at `:519`, detach at `:536`).
- **Status**: Regression of #5379, through #5384.
  - #5379's commit `203be9ed4` landed first (Oct 8 22:13). At that point `CinematicReAdoption.pending` held every un-rooted
    member of the release walk, roots and render subtrees alike, so the purge removed the whole convoy.
  - #5384's commit `faf8e5682` came later (Oct 9 15:13; `git merge-base --is-ancestor` confirms the order). It queues only
    PARENTLESS members ("the root's adoption stamps the subtree"), but left the purge untouched. From then on the purge
    removes roots only.
  - The missing GPU and Rapier release dates from #5379 itself.
- **Trigger Conditions**: A scripted convoy (horse + cart + riders) finishes its route outside every loaded cell, so its
  members wait on the re-adoption pending list. A save load or debug load then runs before a cell loads beneath it.
- **Verification Path**: `cargo test`. Extend `purge_despawns_pending_readoption_entities` (`unload.rs:1144`) with a child
  mesh node and `RapierHandles`. Today it uses one childless, meshless, bodiless entity, so it cannot see either half.
- **Description**: The subtree nodes left behind have no `CellRoot`:
  - `cinematic_retained_entities` (`unload.rs:19-49`) retains "complete render hierarchies".
  - `strip_retained_cell_root` (`:161`) strips their `CellRoot` when the home cell unloads mid-tether.

  The session-replace teardown (`drain_streaming_state` → `unload_cells`, `unload_current_interior`) walks only
  `CellRootIndex`, so it cannot reach them. After the purge, their `Parent` names a dead id, so transform propagation never
  refreshes their `GlobalTransform`. They render as frozen geometry in the reloaded world: #5384's own symptom, reopened
  through the save-load path.

  The bare `despawn_batch` also skips everything `release_entities_timed` does before its own `despawn_batch`:
  - mesh, texture and BLAS refcount drops;
  - item-instance release;
  - `release_victim_rapier_bodies`.

  The purge has no `VulkanContext` parameter, so it cannot do any of these. A cart or horse root that carries
  `RapierHandles` leaves its body in `PhysicsWorld`, which is never reset on a session replace (`unload.rs:509-518`
  documents that). The result is an invisible collider where the convoy stood.
- **Evidence**:
  ```rust
  // unload.rs:97-105 — roots only (cinematic.rs:552-561 queues PARENTLESS members), no GPU/physics release
  let pending: Vec<EntityId> = world.try_resource::<CinematicReAdoption>()
      .map(|pending| pending.pending.clone()).unwrap_or_default();
  if !pending.is_empty() { …; world.despawn_batch(pending); … }
  ```
- **Impact**: After such a load:
  - ghost convoy geometry (wheels, horse body parts) frozen at the old position;
  - a phantom collider;
  - mesh, texture and BLAS refs that never reach zero, for the rest of the process.

  The FormIdPair ghost-twin half of #5379 stays fixed, because the roots are still despawned.
- **Related**: #5379, #5384, #3817, #5056, #1520 (why `RapierHandles` must be released before despawn).
- **Suggested Fix**: Hand the convoy to the caller's teardown instead of despawning it in the purge. For example, return the
  pending roots' full `Children` closures and run them through `cell_loader::release_entities(world, ctx, &victims, ..)`;
  both purge call sites in `save_io.rs` and `debug_load.rs` hold `ctx`. Alternatively, stamp them onto a scratch
  `CellRoot` so the drain's `unload_cells` reclaims them through the canonical path.

### CONC-D7-2026-10-09-02: The worker's skip filters can skip a key the main thread will not have at apply time
- **Severity**: LOW. This is a performance issue only: the main-thread parse fallback (`references/synth_child.rs:633-650`)
  still loads the model.
- **Dimension**: Worker Threads (worker ↔ main handoff)
- **Location**:
  - `byroredux/src/streaming/pre_parse.rs:135-145` (memo scope comment), `:183-202` (`recv_next_batch_request`), `:841`
    (`batch_keys.extend`).
  - Stale drops: `byroredux/src/streaming_helpers.rs:795-806` and `:940-953`.
  - `byroredux/src/cell_loader/nif_import_registry.rs:420-426` (2048-entry LRU) and `:632-635` (`snapshot_keys`).
- **Status**: NEW. It is the residual of #3670's memo (closed). The closed #3670 and #4207 match the area; neither covers a
  memo entry outliving a dropped payload.
- **Description**: The worker skips a model key in two cases:
  1. The key is in the request's `cached_keys` snapshot.
  2. An earlier request in the same "dispatch batch" already emitted it (`batch_keys`, #3670).

  Both assume the key will be in `NifImportRegistry` when this cell applies. Neither holds under the conditions where
  payloads go stale:
  - **The memo is cleared only when the queue empties.** Under a sustained backlog (cold fill plus continued movement) it
    spans several dispatches. The main thread drops a stale payload before any cache mutation, so a key emitted only in a
    dropped payload never reaches the registry. Two later requests then miss it:
    - another cell sharing the model (shared statics are the norm);
    - the same coordinate re-requested at a newer generation.

    Both skip the key as `BatchDuplicate`, and their applies hit a cache miss.
  - **`snapshot_keys` can name a key the LRU has since evicted.** The registry's 2048-entry LRU can evict a snapshot key
    before the request applies, as other payloads' `FinishImports` insert entries. `snapshot_keys`' doc says the snapshot
    "never under-skips"; that is true only while nothing is evicted.

  The comment at `pre_parse.rs:135-140` claims the memo "prevents a later, independent crossing from losing a needed
  payload to an old memo entry". That holds only when the queue drains between dispatches.
- **Impact**: A main-thread NIF parse inside the 4 ms apply budget: a frame hitch on exactly the fast-travel / cold-fill
  frames the worker exists to protect. No correctness loss.
- **Related**: #3670, #862, #4207, CONC-D7-2026-10-09-01.
- **Suggested Fix**: On a stale drop, still `finish_streaming_import` the payload's parsed entries. They are valid imports;
  only the cell spawn is stale. That makes the memo's premise true and also salvages the work. Alternatively, clear the
  memo per request when the request's own generation is superseded. For the LRU half, count a skipped-but-missing key in
  `StreamingTelemetry`, so `mesh.cache` evictions during a fill become visible.

---

## Existing issues re-checked

- **#5442** (open): `PartialNifImport: Send` compile-time assertion dropped by the #5092 split. Still absent from
  `streaming/`. Not re-reported. The channel's `Sender<LoadCellPayload>` moving into the worker closure already requires
  `Send`, so the guard is belt-and-braces.
- **#5069** (open): `late.rs:416-429` is unchanged. It still declares no `ActorBodyClass`, `PendingGearImport` or
  `LoadedCellIndex`.
- **#5365** (open): the all-slots fence wait is unchanged and not re-filed.
- **Verified fixed since the baseline.** Each was read in code and exercised by a named test under the detector:
  - **#5371** (CONC-D5-2026-10-08-01):
    - `forcegreet.rs:83-196`: Pass 1 gathers under storage guards; `PhysicsWorld` is taken at `:131` inside the walk branch
      and dropped at `continue`; writes and `forcegreet_open` run after it.
    - `eat_sleep.rs:161-187`: physics is taken in a block expression, then the `Transform` write.
    - Tests: `forcegreet_walk_with_real_physics_world`, and the eat_sleep `PhysicsWorld` test (`eat_sleep.rs:508`).
  - **#5372** (CONC-D3-2026-10-08-01; #5066 closed):
    - `story_events.rs:108-109` resolves `location_1` before `query_mut::<StoryEvent>()`.
    - The three shadows are scoped clones (`npc_dialogue.rs:497-502`, `:683`, `:770`).
    - `bound_world` calls `story_manager::register` (`npc_dialogue.rs:1272`).
  - **#5414**: `late.rs:494-514` declares the voice and AHEL acquisitions. The scan follows
    `(DIALOGUE_VOICE_SRC, "play_line_voice")` and `(STORY_EVENTS_SRC, "raise_hello_story_event")`
    (`boot/schedule/mod.rs:1030-1031`).
  - **#5415**: `story_dispatch_and_forcegreet_update_ordering_is_pinned` ok.
  - **#5416**: `vulkan_validation_job_requires_live_rt` ok.
  - **#5417**: the debug-server count test is green in the workspace run.

## Dimension notes (clean areas)

- **D1 — Queue & AS.** No submit, fence, barrier, AS-build or destroy path moved.
  - #5369 adds only CPU-side rig-key state (`prev_restir_rig_key`, `restir_rig_static_last_build`).
  - #5275 moves a log flag's re-arm from `submitted` into `record_pending_rgba_uploads`; consumption semantics are unchanged.
  - Guards ok: `frames_in_flight_contract_names_every_dependent_resource`,
    `render_finished_is_sized_and_indexed_per_swapchain_image`, `queue_guard_released_before_one_time_fence_wait`,
    `one_time_commands_free_cmd_buffer_on_every_error_path`, `wait_failure_arm_disposes_only_on_device_loss`,
    `fresh_build_records_peak_unconditionally_of_scratch_regrow`, `rgba_updates_use_the_frame_command_buffer_and_keep_descriptors`,
    `only_the_submitted_recording_consumes_its_pixels`.
- **D2 — Compute chains.** Changes are shader math only (#5211 SVGF finite check, #5369 EMA, #5482 contrast toe), plus
  CPU parsing (#5273, #5378). Guards ok: `skin_publish_barrier_consumer_tests`, `taa_resolves_the_post_bloom_scene_tap`,
  `every_skipped_frame_drops_the_temporal_history_not_just_the_first`.
- **D3 — Lock ordering.** New code read:
  - `release_finished_tethers` / `retry_cinematic_readoption` (#5384) hold `CellRoot` + `Parent` (+ `Transform`,
    `Children`, `PlayerEntity`) read guards together. All are reads, and all drop before the per-storage writes. Both run
    in exclusives or under `&mut World`.
  - `play_line_voice` (#5383/#5410) scopes the `AudioWorld` read, then takes `VoiceSoundCache` → `SoundArchiveProvider`
    (cache before provider, like combat sound), then the `AudioWorld` write alone. `stop_sounds_for` is a kira
    handle-command push.
  - Sweeping the #5066 shape found one remaining hold: `commands/quest.rs:887`, the `forcegreet` console command. It holds
    `LoadedCellIndex` across `query_mut::<ForceGreetDirective>()`. This is a main-thread debug path, unchanged since the
    baseline, with no reverse edge, so I did not report it.
- **D4 — Scheduler proof.** All proof tests are live and none are ignored. New resources (`VoiceSoundCache`,
  `EditorPlacement`, `CinematicReAdoption`) add no cross-stage single-writer contract that needs a pin.
- **D5 — Physics / teardown.**
  - The #5418 detach pass (`unload.rs:564-605`) takes one `Parent` read guard in an `if let` scope, drops it before the
    `Children` probe, then opens one `Children` write scope. No nested guard; the source pin
    `detach_victims_takes_one_parent_guard_for_the_whole_sweep` is ok.
  - #5355/#5356 (`crates/physics/src/world/`) touch only Rapier-internal maps.
  - #5357 resolves the placed-current marker before any physics borrow (`character.rs:283`).
  - All named snapshot-then-acquire guards are green.
- **D6 — Lifecycle.** No change on Paths beyond a doc comment. The purge's GPU-release bypass is filed under D5.
- **D7 — Workers.** Beyond the two findings:
  - **Panic guards.**
    - Request level: `pre_parse.rs:211-230`.
    - Per NIF: `:256`.
    - Extraction: `:370` sits outside the per-NIF guard. A panic there propagates through `in_place_scope_fifo` to the
      request guard. The permit, moved into the task closure, is released during unwinding. The budget lock is never held
      across a panic.
  - **`ParseInputBudget`.** One waiter, the off-pool coordinator; `notify_one` in the permit's `Drop`. Pool tasks always
    make progress.
  - **Prefetch store.** `take` waits only on `Running` slots. Queued slots are withdrawn, `clear` bumps the epoch, and a
    panicking read still finishes its slot.
  - **Generation gate.** Re-checked at payload arrival and at every resumable slice (`streaming_helpers.rs:815`).
  - **New code.** No new production thread, rayon section or channel; the first-step grep's new hits are test-only.

## Skill drift (fold into the next `/audit-concurrency` sync)

- **Dim 7 checklist premise is wrong.** *"drops `request_tx` (the worker's `recv()` errors and it exits)"* is false for a
  non-empty queue (CONC-D7-2026-10-09-01). After the fix, restate the invariant as "cancel flag set / payload receiver
  dropped before the join", and name the new test.
- **Dim 5 should sweep non-canonical despawns.** Add a first step:
  `grep -rn 'despawn_batch(\|\.despawn(' byroredux/src/cell_loader byroredux/src/systems`. Require every production hit
  either to sit inside `release_entities_timed` or an explicit `unload_*_block` reclaim, or to carry no GPU/physics state.
  The #5379 purge was found this way.
- **Baseline drift items done.** The baseline's "PhysicsWorld-sink sweep" and "register what boot registers" items are
  satisfied by #5371/#5372. Keep the sweep as a standing `First step:` line.
- **Dim 3 should note** that "Test + Check + Clippy" has been red since `f1141aebe` on clippy (`crates/spt/src/parser.rs`),
  not on tests, while the ABBA lane stays green. Read which job failed before treating CI red as a lock regression.

## Out-of-scope observations (not findings)

- `AudioWorld::stop_sounds_for` (#5410) has no production caller, although `dialogue_voice.rs:288` says the
  conversation-close path uses it. A closed conversation does not cut its voice line. This belongs to `/audit-audio`.
- `story_events.rs:100-107` still says the dialogue call sites "already hold a `LoadedCellIndex` read of [their] own". The
  same commit (#5372) scoped those reads away. This is doc rot.

---

Publish with: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-10-09.md`. Domain labels:
- CONC-D7-2026-10-09-01: **concurrency** + `performance` (+ `terrain-exterior`).
- CONC-D5-2026-10-09-01: **concurrency** + `memory` + `physics` + `save-load` (+ `test-gap`).
- CONC-D7-2026-10-09-02: `performance` + **concurrency**.
