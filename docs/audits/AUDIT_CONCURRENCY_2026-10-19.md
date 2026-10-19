**HEAD**: `807c58890` (the tree moved to `1d7e2c6bd` mid-run; that commit touches only `crates/sfmaterial`, `ROADMAP.md`, `docs/engine/nifal.md`, nothing in the area) · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-10-17.md` (@ `e16a624e9`; itself volumetrics-only, so not a full baseline — the last full-codebase concurrency report is `AUDIT_CONCURRENCY_2026-10-12.md` @ `5d31fd637`) · **Audited**: Dim 7 (streaming worker + pre-parse pipeline + texture prefetch + load-order rayon walk), Dim 5 (`cell_loader/unload.rs` and the area's resource/physics hand-offs), Dim 3 (CPU lock ordering of everything reachable from `streaming/`, `npc_spawn/`, `cell_loader/`, `streaming_helpers.rs`, the `app_step.rs` steppers; scope=streaming-deep AREA-SCOPED), Dim 4 (guard line + the area's two registered systems) · **Unchanged since baseline (skimmed)**: Dim 1, Dim 2, Dim 6 are out of area (one line each below).

**SCOPE: AREA-SCOPED (streaming-deep suite), NOT a full-codebase audit; the next run must not treat this as a full baseline.**

# Concurrency & Synchronization Audit — 2026-10-19 (streaming-deep)

**Command**: `/audit-concurrency` inside `/audit-suite --area streaming-deep`. **Severity scale**: `.claude/commands/_audit-severity.md`.
**Method**: synchronous, no sub-agents, no source edits, no engine/GPU process. Delta-first (`git log 5d31fd637..HEAD` over the area: 53 files, +4821/-1277, newest least-reviewed code read first), then a structural read of the whole worker/main-thread flow (`streaming/mod.rs` 1028 lines, `pre_parse.rs` 884, `asset_provider/texture_prefetch.rs`, `streaming_helpers.rs` 530-1160, `unload.rs`, `reference_state.rs`, `stream_snapshot.rs`, `loot_appearance.rs`, `ai_package.rs` 640-1110, `resumable/{mod,runtime,prebaked}.rs`, `load_order.rs` 540-830, `created.rs`, `crates/bsa/src/{csg,read_at}.rs`).
**Dedup**: `/tmp/audit/issues.json` (121 open); closed lookups for `subtree_has_skinned_mesh`, `tag_descendants_as_actor`, "SkinnedMesh Children lock order"; `docs/audits/AUDIT_SAFETY_2026-10-19.md` read (its SAFE-2026-10-19-01 `head_look`/`magic_shader` cycle is verified fixed, see below).

**Forwarded dedup (exterior audit, `AUDIT_EXTERIOR_2026-10-19.md`)**: EXT-D6-01 (capacity-refused LOD upload cached as permanent miss), EXT-D6-02 (#6054 Starved arm never calls `settle_lod`), EXT-D6-03 (= #6053), EXT-D6-04 (coarse terrain-LOD hole mask vs default radius 12), EXT-D2-05 (doc rot). None overlaps CONC-SD-01..03 (lock order, admission-control test gap, unguarded `Children` walk); a grep of that report for `subtree_has_skinned_mesh`, `tag_descendants_as_actor`, `ParseInputBudget` and `SkinnedMesh` found nothing.

## Guard runs (repo `target/`, rustc 1.96.0 pinned toolchain via rustup shims; the only private target dir is the probe's, see "Probe")

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` | **3035 passed, 0 failed, 63 ignored** (the 10-19 safety run saw 2 red tests, `head_look` / `magic_shader`; both fixed by `e4d86a29f`, #6062) |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-core -p byroredux-physics -p byroredux-scripting` | core 825/0, physics 258/0, scripting **1166 passed, 2 failed once** (`obscript_effects::tests::{get_is_reference_compares_form_ids,stop_combat_with_a_target_stops_only_that_fight}`, `QuestObjectiveState <-> QuestStageState` cycle). Not reproducible: 4 further default-parallelism runs, 1 `--test-threads=1` run and 40 direct runs of the test binary were all 1168/0. Out of area; see "Out-of-area" |
| scheduler proof tests (`scheduler_access*`, `system_access_declaration_tests`, `build_scheduler_reports_zero_access_conflicts`, `equipment_appearance_system_declares_everything_it_acquires`) | all `ok` inside the bin run; none `#[ignore]`d |
| streaming guards (`cancel_flag_stops_a_worker_with_a_queued_backlog`, `join_with_timeout_*`, `worker_panic_safe_*`, `worker_batch_memo_is_cleared_at_an_empty_queue_boundary`, `stream_parse_pool_*`, `pre_parse_parallel_branch_uses_the_dedicated_pool`, the six `texture_prefetch` tests, `unload_captures_do_not_invert_production_lock_orders`, `picked_up_restore_walks_meshes_before_taking_the_marker`, `view_restamp_does_not_close_the_render_skip_lock_cycle`) | all `ok` |

## Findings summary

| ID | Severity | Dimension | Status | Title |
|---|---|---|---|---|
| CONC-SD-01 | MEDIUM | ECS Lock Ordering | NEW | `subtree_has_skinned_mesh` holds a `SkinnedMesh` read guard across `collect_subtree`'s `Children` acquisition: the inverse of the canonical `Children -> SkinnedMesh` order, on a path no CI lane exercises |
| CONC-SD-02 | LOW | Worker Threads (test gap) | NEW | The streaming parse-input admission control (`ParseInputBudget`: byte cap, task cap, oversized-alone rule) has no test that makes any limit bind |
| CONC-SD-03 | LOW (adjacent, owner `/audit-ecs`) | ECS hierarchy walk | NEW | `tag_descendants_as_actor` is the one whole-subtree `Children` walk with no visited set or traversal budget; #5513 / #5713 did not list it |

Counts: **0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW** (3 NEW, 0 regressions).

## Findings

### CONC-SD-01: `subtree_has_skinned_mesh` takes `SkinnedMesh` before `Children`, against the canonical order, and the lane cannot see it
- **Severity**: MEDIUM. `_audit-severity` lists "ECS deadlock potential" as HIGH minimum, but the premise fails here: the only caller owns `&mut World` (see Evidence), so no second thread can hold the opposite order and there is no live deadlock. What survives is a documented-order violation that aborts a debug build under `BYRO_LOCK_ORDER_CHECK=1` the first time an FO3/FNV NPC head part spawns after the bounds system has run. Rated like the 10-17 C-1 (same `&mut World`, same class). The publisher may raise it to HIGH on the table's letter.
- **Dimension**: ECS Lock Ordering & Deadlock (Dim 3), reached from the NPC spawn tail of `cell_loader` streaming
- **Location**: `byroredux/src/npc_spawn/resumable/runtime.rs:1277-1284` (`subtree_has_skinned_mesh`); sole caller `runtime.rs:1164-1166` inside `spawn_shared_skeleton_part` (`:1122`, takes `world: &mut World`)
- **Status**: NEW. The 10-12 ECS report (D3-01, #5713) only de-duplicated this function's hierarchy walk; none of `docs/audits/`, `.claude/issues/` or the open/closed issue list mentions its lock order. The same SkinnedMesh-then-Children order existed in the pre-#5713 body (`cfabdfd6a^`: `(world.query::<SkinnedMesh>(), world.query::<Children>())`), so #5713 kept it rather than introducing it.
- **Flow**: NPC spawn job (`NpcSpawnJob::advance`) -> `spawn_shared_skeleton_part` (FO3/FNV only: `head_parts_use_head_bone_mount(GameKind::Fallout3NV)`, labels `hair` / `eyebrow HDPT` / `eye mesh` / `head sub-part`) -> `subtree_has_skinned_mesh`.
- **Description**: `docs/engine/ecs.md` § "Canonical acquisition order" (`:595-606`) fixes `... Parent -> Children -> GlobalTransform -> SkinnedMesh -> MeshHandle ...`, and `systems/bounds.rs:133-148` follows it (`Children`, `GlobalTransform`, then `SkinnedMesh`). The function does the reverse:
  ```rust
  let skinned = world.query::<byroredux_core::ecs::SkinnedMesh>();     // runtime.rs:1280, guard live
  byroredux_core::ecs::collect_subtree(world, root)                    // acquires Children inside, :1281
      .into_iter()
      .any(|entity| skinned.as_ref().is_some_and(|q| q.get(entity).is_some()))
  ```
  This is the 10-17 C-1 / #5954 shape (`common_shape_rotation` held `MeshHandle` across `collect_subtree`), left behind in the function directly above it. #5954 fixed one of the two #5713 call sites.
- **Evidence**: no test reaches it. The function has one non-doc reference (the call at `:1166`); `spawn_shared_skeleton_part` needs real NIFs, and no test in `npc_spawn/tests.rs` or `resumable/mod.rs` calls it. That is why the green 3035-test lane does not contradict this. A probe reproduces the detector verdict with the exact body (scratch crate under `/tmp/audit/streaming-deep/concurrency/probe`, depends on `byroredux-core` by path):
  ```
  canonical order recorded                      # Children -> SkinnedMesh, as bounds.rs does
  ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of `...hierarchy::Children`
  while holding `...skinned_mesh::SkinnedMesh` ... `Children` -> `SkinnedMesh` -> `Children`
  ```
- **Trigger Conditions**: debug build, `BYRO_LOCK_ORDER_CHECK=1`, an FO3/FNV (or any `Fallout3NV`) cell with an NPC whose head parts take the head-bone mount path, with bounds having already recorded `Children -> SkinnedMesh` in an earlier frame. Release builds and runs without the env var are unaffected. A real cross-thread ABBA would need this function to run on a worker thread while `bounds` runs in the parallel batch; today it cannot (it needs `&mut World`).
- **Impact**: a developer or CI lane that runs a real FNV/FO3 session under the detector aborts at NPC head spawn (a panic, not a warning); and it is one more site that makes the sink-and-order rules unreliable, the pattern behind #4982, #5535, #5954, #6062. Latent hang risk only if the spawn tail is ever moved off `&mut World`.
- **Related**: #5954 (closed, same shape, sibling function), #5535 (closed), #5713 (closed, the refactor), #6062 (open, same fix idiom).
- **Suggested Fix**: collect first, then take the guard, exactly as `common_shape_rotation` (`runtime.rs:1338-1345`) and the #6062 fix do: `let subtree = collect_subtree(world, root); let Some(skinned) = world.query::<SkinnedMesh>() else { return false }; subtree.into_iter().any(|e| skinned.get(e).is_some())`. Add a test with `Children` and `SkinnedMesh` registered and the canonical order recorded first (the probe above is the shape), because a fixture without the `SkinnedMesh` storage records no edge (`query` returns `None`), which is the #5372 blind-spot class.

### CONC-SD-02: no test makes any `ParseInputBudget` limit bind
- **Severity**: LOW (coverage gap; the code reads correct, see "Verified")
- **Dimension**: Worker Threads & Thread-Safety Bounds (Dim 7)
- **Location**: `byroredux/src/streaming/pre_parse.rs:439-482` (`ParseInputBudget::acquire`, `ParseInputPermit::drop`) and `:511-572` (`parse_nif_pipeline`'s parallel branch); the fixture `parse_model_keys` at `:576-598` and `streaming_tests.rs:746` (`pre_parse_parallel_branch_uses_the_dedicated_pool`)
- **Status**: NEW (no open issue: searched "budget", "admission", "pre_parse", "condvar"; #5715 is the unrelated stale-salvage hitch)
- **Description**: the admission control is a `Mutex` + `Condvar` pair on the worker thread, waited on while pool tasks release permits. The only test that crosses `PRE_PARSE_RAYON_MIN` builds its inputs from an empty `TextureProvider`, so every `declared_bytes` is `0` and `max_tasks` is `pool_threads * 2` with no bytes in play. The byte cap (`STREAM_PARSE_INPUT_BYTES`), the `tasks >= max_tasks` wait, the "an oversized input is admitted alone" rule and the permit release on a panicking task are all untested. This is a deadlock-capable primitive whose failure shape (a worker parked forever, so streaming silently stops) is invisible to every other guard.
- **Evidence**: `grep -rn 'ParseInputBudget\|STREAM_PARSE_INPUT_BYTES\|STREAM_PARSE_MAX_TASKS\|ParseInputPermit' byroredux/src` returns only `pre_parse.rs`.
- **Impact**: none today. A later edit to the wait predicate (it has three clauses) or to the permit lifetime (permit moved out of the task closure) would not fail a test.
- **Suggested Fix**: a unit test over `ParseInputBudget` alone (no archives): (1) `max_tasks` permits taken, a thread blocks on the next and wakes when one drops; (2) an oversized request is admitted only when `tasks == 0` and blocks the next; (3) a permit moved into a panicking `catch_unwind` closure is released. Pure CPU, milliseconds.

### CONC-SD-03: `tag_descendants_as_actor` walks `Children` with no visited set and no traversal budget
- **Severity**: LOW. A corrupt/cyclic `Children` graph would spin the main thread with an unbounded `to_tag`; a cycle cannot arise from the spawn path itself (it walks a hierarchy the spawn job just built), so impact needs another bug.
- **Dimension**: adjacent to the lens (hierarchy-walk termination, not lock order); owner `/audit-ecs` Dim 3. Reported because it was found while reading the spawn tail and the area's walks were audited to completion.
- **Location**: `byroredux/src/npc_spawn.rs:1678-1707`
- **Status**: NEW. #5513 and #5713 (both closed) routed the other whole-subtree walks through `collect_subtree`; the 10-12 ECS report explicitly set aside the "BFS walks that interleave per-node work and lock-held queries" (`ragdoll.rs`, `bounds.rs`), and this one was not named either. Unlike `bounds.rs` (which carries a `HierarchyTraversalGuard`), it has none.
- **Description**: `let mut queue = vec![root]; while let Some(e) = queue.pop() { ... queue.push(c) for each child ... }` with `children_q` and `mesh_q` guards live. No `seen` set, no budget. The "interleaved per-node work" is only a `MeshHandle` membership test, so it is exactly `collect_subtree` plus a filter (the shape `common_shape_rotation` and `mesh_entities_under` now use).
- **Suggested Fix**: `collect_subtree(world, root)` first, then take `MeshHandle` and filter, as `common_shape_rotation` does; this also removes the hand-held `Children`+`MeshHandle` pair.

## Verified clean (area-scoped; the next streaming-deep run can skim these)

Each item is a code-reading conclusion plus, where noted, a passing guard; none involves a GPU or game data.

### Dim 7 — worker threads and thread-safety bounds
- **Shutdown handshake** (`streaming/mod.rs:788-816`): `worker.take()` first, `cancel.store(true, Release)` before `request_tx.take()`, then `join_with_timeout` (10 ms `is_finished` poll, detach on timeout). The worker checks `cancel` after each receive (`recv_next_live_request`, `pre_parse.rs:195-206`), so a deep backlog is discarded unparsed (#5491; `cancel_flag_stops_a_worker_with_a_queued_backlog` passes). `Drop` re-enters `shutdown` and short-circuits on `worker == None`. A detached worker keeps its `Arc<rayon::ThreadPool>` / `Arc<TextureProvider>` clones alive and exits at its next `payload_tx.send` once `payload_rx` drops. Shutdown order in `app_events.rs:126-158` unloads resident cells, then shuts the worker, then removes `AllocatorResource`.
- **Worker <-> main flow**: the worker never sees a `World`; it holds `Arc<ExteriorWorldContext>`, `Arc<TextureProvider>`, the request's frozen `cached_keys` snapshot and its own `csg_handles` / `batch_keys`. `PartialNifImport: Send` is compile-asserted (`streaming/mod.rs:261-264`). `StringPool` interning happens on the main thread only (the worker interns into its own `worker_pool`, re-interned in `partial.rs:77-118`). `merge_external_material` stays main-thread (`finish_streaming_import` takes `&mut MaterialProvider`).
- **Panic containment**: `pre_parse_cell_panic_safe` (cell) and `parse_one_nif`'s `catch_unwind` (per NIF); `batch_keys` is extended only after a successful cell (`pre_parse.rs:870`), so a panic does not poison the memo. A panic in `extract_mesh` (outside `parse_one_nif`'s net) propagates out of the rayon scope after every task joined and is caught at the cell level, producing an empty payload (the apply then falls back to a main-thread parse). Slow, not wrong.
- **`ParseInputBudget`** (`pre_parse.rs:445-482`): single waiter (the coordinator thread, which is not a pool thread, via `in_place_scope_fifo`), so `notify_one` after every release cannot lose a wakeup; the predicate admits when `tasks == 0`, so no input can wait on a limit it can never meet; `ParseInputPermit` is moved into the task closure and drops on unwind; poisoned locks recover (`unwrap_or_else(|e| e.into_inner())`). No pool task blocks on the budget, so no cycle with the coordinator. (CONC-SD-02 is that it is untested.)
- **`PrefetchStore`** (`texture_prefetch.rs`): epoch-guarded `start` / `finish`; a `Queued` slot is withdrawn, never waited for; only a `Running` slot is waited on, and the reader finishes it even on panic (`catch_unwind` around `provider.extract`); `clear` bumps the epoch and clears all slots, so no waiter outlives its epoch; the over-cap path removes the slot before waking. Resolve-side `take` and `clear` both run on the main thread, and a pool task never calls `take`, so there is no main/pool wait cycle. Byte accounting is symmetric (`finish` adds `bytes.len()`, `take` subtracts it).
- **BSA / BA2 / CSG reads**: `ReadAt` is positional (`pread`); `CsgArchive`'s one `Mutex<ChunkCache>` is taken only for the get and the insert, never across IO or inflate (`csg.rs:326-381`), and duplicate inflates of one chunk are accepted by design.
- **Load-order walk** (`cell_loader/load_order.rs:650-742`): slot allocation, remaps, `read_archive` and the `merge_from` fold stay on the calling thread in load order; each walk owns its `bytes`, `remap` and `tables`, writes only its own `walked` slot, and the scope joins before the fold. `StringsTableGuard` is installed inside the spawned closure and restores the previous value on drop; the plugin crate has no rayon, so a walk cannot steal another plugin's walk onto its own stack and clobber the thread-local. (`load_order_parallel_tests.rs` is the equality reference.)
- **Thread census** (`grep -rnE 'thread::(spawn|Builder)|rayon::|mpsc::'` over `crates byroredux tools`, test lines excluded): no worker outside the Dim 7 list except `asset_provider/material/cdb.rs` (`sf-cdb-index`, open #5665) and test-only spawns.

### Dim 5 — `cell_loader/unload.rs` and the physics / resource hand-offs
- `release_victim_rapier_bodies` (`:948-981`) and `release_victim_item_instances` (`:881-905`) collect under read guards in a scoped block, drop them, then take the `PhysicsWorld` / `ItemInstancePool` write guard; `remove_collision_bodies` (`loot_appearance.rs:158-178`) does the same with an explicit `drop(handles_q)`. All run under `&mut World` or interior-mutable `&World` after the reads.
- `reference_state::capture` (`:122-248`) and `stream_snapshot::capture_actor_snapshots` (`:186-321`) hold exactly one guard per pass and write the store after every pass (#4982); the lane tests that pin them pass. `restore_actor_snapshot` takes one guard at a time.
- `detach_victims_from_surviving_parents` (`:658-702`) drops the `Parent` read guard before the `Children` write. `collect_victim_gpu_handles` (`:784-867`) holds seven read guards at once, but under `&mut World` and without a reverse edge in the lane; it is the shape #4982 warns about and is recorded here as a watch item, not a finding.
- **Fix-in-place checks**: #5954 (`common_shape_rotation` collects before taking `MeshHandle`, `runtime.rs:1338-1345`) intact; #6062 (`head_look::find_head` and `magic_shader::mesh_entities` walk `Children` first) intact and the lane that was red in the safety run is green; #4997 / #4982 / #4983 / #4991 guards pass.

### Dim 3 / Dim 4 — everything else reachable from the area
- The `&mut World` steppers (`advance_streaming_apply`, `consume_streaming_payload`, `drain_streaming_state`, `step_created_references`, `step_streaming`'s camera read) take one resource or one storage guard per statement; `step_streaming`'s `ActiveCamera` -> `Transform` order (`app_step.rs:68-73`) matches the billboard / camera systems.
- `created.rs`: `place_created_references` / `unspawned_in` snapshot into owned `Vec` / `HashSet` before any second acquisition; `CreatedReferenceEntities` is held across `world.has::<CellRoot>` (`:181-191`), a Resource -> storage edge with no reverse found.
- `object_sequences.rs`, `spawn.rs:1075-1095`, `partial.rs:77-180`: `StringPool` guard always dropped before `AnimationClipRegistry`; the pool is last (sink rule).
- `loot_appearance.rs`: `equipment_appearance_system` is `add_exclusive_with_access` (`boot/schedule/late.rs:510-540`) and its declaration test passes; its helpers snapshot before every write (`queue_gear_releases` holds `PendingGearRelease` across `Dead` / `CellRoot` / `Inventory` reads, a one-way chain).
- `ai_package.rs`: `ambient_ai_package_system` (`add_exclusive`, `update.rs:463`) holds the `PackageRegistry` read guard across the selection loop (`:996-1063`, condition evaluation and `script_package_of`). No acquirer of `PackageRegistry` takes it beneath another guard, and `add_script_package` drops it before the `ScriptPackage` write, so no reverse edge. Exclusive, so no thread to race.
- `tone.rs`: `Mutex<ToneCache>` held across `provider.extract` (`:96-105`) on the main thread only; no ECS guard is live (the resource guard drops at the `clone()`), so no lock held across a World guard.
- Watch items, no finding: `cell_loader/water.rs:772-790` holds `WaterPlane` + `WaterFlow` read guards across `collect_subtree` (records `WaterPlane -> Children`; the only `WaterPlane` holders are water / character / render-water systems and none holds `Children`); `unload.rs:57-70` holds the `PlayerBodyRootEntity` resource guard across `collect_subtree` (Resource -> `Children`; no reverse found). Both would become cycles only if a `Children`-holding site later reads those types.

### Out of area (one line each)
- Dim 1 (Vulkan queue and AS sync), Dim 2 (compute -> AS -> fragment chains), Dim 6 (GPU teardown ordering): out of area; the streaming code makes no queue submit of its own beyond `flush_pending_uploads` on the main thread (outside this audit's files).
- Dim 4 (scheduler proof): the two area-registered systems are exclusive; `equipment_appearance_system` declares its access and the source-scan guard passes; no parallel system lives in the area.

## Out-of-area observations (forward, not counted)

- **Scripting lane, one transient red** (`obscript_effects` tests, `QuestObjectiveState <-> QuestStageState`). Seen once under `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-core -p byroredux-physics -p byroredux-scripting`; not reproduced in 45 reruns. Candidate sites for whoever owns `/audit-scripting` Dim 5 if it recurs: `crates/scripting/src/obscript_quests.rs:855-880` (the objective writes) against `fragment/effects/deferred.rs:496,1226` and `commands/quest.rs:221` (`resource_2_mut::<QuestStageState, QuestObjectiveState>`). The shape matches open #6003 / #6008 (scripting-side quest-state lock cycles). Not attributed here.
- **Main-thread archive re-open** (perf, `/audit-performance`): `App::step_created_references` (`app_step.rs:1004-1036`) calls `build_texture_provider` / `build_material_provider` on every spawn event for the interior branch, while `AppearanceProviders` (#5061) exists to share one pair. Not a concurrency finding.
- Existing and unchanged: #5715 (stale-payload salvage runs unbudgeted on the main thread), #5665 (`sf-cdb-index` worker, no `catch_unwind`).

## Probe

`/tmp/audit/streaming-deep/concurrency/probe` (a scratch crate depending on `byroredux-core` by path; never part of the repo) reproduced CONC-SD-01 with `BYRO_LOCK_ORDER_CHECK=1 CARGO_TARGET_DIR=/tmp/audit/streaming-deep/concurrency/target cargo run`. That is the **one private cargo target dir** used in this run; the suite deletes `/tmp/audit/streaming-deep/` itself.

## Not checked

- No `BYRO_VALIDATION` run, no engine, no GPU and no game data (suite rules). The streaming worker / main hand-off was verified by reading plus the passing unit guards; a live cell-crossing under the lock-order detector (the only way to reach CONC-SD-01 dynamically) was not run. The `vulkan-validation` CI job drives only the 5-frame default scene and exercises none of the area.
- Dim 5's physics-side guards (`pull_dynamic_does_not_close_...`, `register_newcomers_parallel_section_holds_no_world_guard`) are outside the area; the physics crate lane (258/0) was run, the two named tests were not individually checked.
- CI job state was not pulled.

Suggest: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-10-19.md` (CONC-SD-01: domain label *concurrency*; CONC-SD-02: *concurrency* + *test-gap*; CONC-SD-03: *ecs* + *tech-debt*).
