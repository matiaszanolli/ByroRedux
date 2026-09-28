# #4982 — ECS-2026-09-28-D1-01: #4616's hoisted capture guards invert five settled orders and keep the lock-order lane red (23 of 27 panics)

Filed 2026-09-28 via `/audit-publish docs/audits/AUDIT_ECS_2026-09-28.md`. Snapshot as filed; GitHub is authoritative for live state.

**Source**: `docs/audits/AUDIT_ECS_2026-09-28.md` (HEAD `21319618c`)

- **Severity**: HIGH. The same grading as #3819, "lock-order-check CI job is red at HEAD". The
  cycles are not live deadlocks, because both capture functions take `&mut World` (see Impact). But
  this is the dominant reason the only detector for the HIGH-floor ECS-deadlock class is blind, and
  it breaks two orders the code documents as load-bearing.
- **Dimension**: 1 — Lock Ordering (`/audit-concurrency` Dim 3 owns the system-level census)
- **Location**:
  - `byroredux/src/cell_loader/stream_snapshot.rs:185-192`
    (`capture_actor_snapshots`: `FormIdComponent`, `Transform`, `Seated`, `AmbientPackageRuntime`,
    `TravelState`, `Traveled`, then `FormIdPool`, all held together)
  - `byroredux/src/cell_loader/reference_state.rs:109-118` (`capture`: `FormIdComponent`,
    `Inventory`, `Dead`, `PickedUp`, `EquipmentSlots`, `EquippedWeapon`, `ActorValues`, then
    `FormIdPool` and `ItemInstancePool`, all held together)
- **Status**: NEW. Introduced by `fa6a551ce` "Fix #4616: unload capture passes hoist their probes"
  (2026-09-24). It is not a regression of #4603, whose lane-structure fix is intact and is what
  names the failure.
- **Description**: #4616 was a performance fix. It replaced a per-victim `world.get::<T>` for each
  probe with one hoisted guard per component. Each guard is a shared read, and the block's comment
  argues that this is safe because they "drop at the end of this block".

  The detector does not care about mode, and it should not: `std::sync::RwLock` readers deadlock
  against a queued writer. Holding N guards records N·(N−1)/2 ordered edges. Five of those edges
  invert orders that production `&World` systems already establish:

  | Edge recorded by the capture | Reverse edge, and where it comes from | Tests failing |
  |---|---|---|
  | `TravelState → Traveled` (stream_snapshot) | `Traveled → TravelState`, `travel_system_inner` (`systems/travel.rs:204-208`) | 7 travel tests |
  | `FormIdComponent → Transform` (stream_snapshot) | `Transform/FollowState/EscortState/GuardState/NavPath/NavmeshTile → FormIdComponent`: `resolve_entity_by_global_form_id` (`crates/scripting/src/condition.rs:539`) called under the follow/escort/guard/travel gather guards | 5 escort, 5 follow, 2 guard |
  | `Seated → AmbientPackageRuntime` (stream_snapshot) | closes `Seated → AmbientPackageRuntime → GlobalTransform → Furniture → Seated` through `commands/world_info.rs:869-871` and `systems/sandbox.rs:179-183` | `fully_seated_world_skips_the_seat_table_rebuild` |
  | `Seated → Traveled` (stream_snapshot) | the test body at `save_io/round_trip_tests.rs:488` (D1-03) | `ai_procedure_state_and_terminal_markers_survive_save_load_round_trip` |
  | `Dead → ActorValues` (reference_state) | `ActorValues → Dead`, `commit_actor_value_deaths` (`extensions/commands.rs:495-500`) | `deferred_actor_value_batch_that_zeroes_health_kills_the_actor` |
  | `Inventory → ItemInstancePool` (reference_state) | `ItemInstancePool → Inventory`, `validate_inventory_instances` (`crates/save/src/validate.rs:502-503`) | `consumable_health_inventory_and_arena_survive_disk_load_overlay` |

  The last row breaks a written invariant. `crates/save/src/validate.rs:495-501` says "The production
  consumer … scopes its `Inventory` query, drops it, and only then takes the pool, so it records no
  edge for this pair and there is no live cycle to close". #4616 recorded exactly that edge. Both
  capture blocks also take their resources (`FormIdPool`, `ItemInstancePool`) *after* the storages,
  which inverts the resource-before-storage rule that `validate.rs` follows (#3649 sibling).
- **Evidence**:
  ```text
  $ BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux -- --test-threads=1
  test result: FAILED. 2445 passed; 27 failed; 48 ignored
  # edge trace (instrumented run, reverted):
  AUDIT_EDGE TravelState -> Traveled      @@ cell_loader/stream_snapshot.rs:191
  AUDIT_EDGE FormIdComponent -> Transform @@ cell_loader/stream_snapshot.rs:187
  AUDIT_EDGE Seated -> AmbientPackageRuntime @@ cell_loader/stream_snapshot.rs:189
  AUDIT_EDGE Dead -> ActorValues          @@ cell_loader/reference_state.rs:116
  AUDIT_EDGE Inventory -> ItemInstancePool @@ cell_loader/reference_state.rs:118
  ```
  CI shows the same pattern. The latest ABBA job (run 36429482184, `382fa9296`) fails
  `extensions::…`, `inventory::…`, `render::…` and on through the same 27 tests.
- **Impact**:
  - **Runtime**: nothing today. Both capture functions take `&mut World`, so no other thread can
    hold an ECS guard while they run, and these particular inversions cannot deadlock.
  - **Gate**: the process-wide graph is the ECS's only mechanical guard for invariant #4, and it
    cannot tell a `&mut World` acquisition from a `&World` one. While 23 tests are red on edges it
    cannot distinguish, a new live cycle between two `&World` systems lands silently. D1-02 is
    already one.
  - **History**: this is the fourth time the lane has gone blind (after #3580, #3819 and #4603).
- **Related**: #4616, #4603, #3819, #3649, D1-02, D1-03.
- **Suggested Fix**: Keep #4616's win (one TypeId lookup per component, not per probe per victim)
  without the overlap:
  1. Resolve `victim → FormIdPair` first, in one pass under `FormIdComponent` + `FormIdPool`, taking
     the resource first.
  2. Then run one pass per component, each under a single guard, filling a per-victim row vector.
  3. Take `ItemInstancePool` before `Inventory`, as `validate_inventory_instances` does.

  Pin the result with a detector-enabled test that runs `capture` and `capture_actor_snapshots` and
  then the travel / follow / commit-deaths paths in one process, following the
  `combat_input_system_does_not_close_*_lock_cycle` pattern.

## Completeness Checks
- [ ] **LOCK_ORDER**: Capture passes hold at most one storage guard at a time; resources (`FormIdPool`, `ItemInstancePool`) taken before storages, matching `validate_inventory_instances`
- [ ] **SIBLING**: Other `&mut World` passes that hoisted multiple read guards for speed (grep `#4616`, unload/save capture paths) checked for the same fan
- [ ] **PERF**: #4616's per-component (not per-probe-per-victim) lookup win is preserved — re-read `unload` `snapshot_capture` timing
- [ ] **TESTS**: A detector-enabled test runs `capture` + `capture_actor_snapshots` and the travel/follow/commit-deaths paths in one process; `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux` drops these 23 failures
