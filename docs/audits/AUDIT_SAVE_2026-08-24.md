# Save / Load Subsystem Audit (M45 + M45.1) — 2026-08-24

Scope: `crates/save/src/` plus the engine-side consumer `byroredux/src/save_io.rs`
and its `byroredux/src/save_io/*_tests.rs` siblings. Audited at HEAD `048a8bd8`.
This is the **tenth** save audit (prior: `2026-06-23`, `2026-07-02`, `2026-07-03`,
`2026-07-16`, `2026-07-25`, `2026-08-03`, `2026-08-07`, `2026-08-16`, `2026-08-20`).
Run solo (no sub-agent fan-out) per this cycle's explicit instruction; all
dimensions were covered directly by one pass of Read/Grep/Bash plus the
project's own test suite (`cargo test -p byroredux-save`, `cargo test -p
byroredux save_io::`, and the SAVE-D1-12 completeness guard — all green,
42 + 26 + 14 tests, see *Regression Guards* below).

The delta since the last audit (`bb0b92f2` → `048a8bd8`, 26 save-relevant
commits) has two faces. First, **all six 2026-08-20 findings are fixed** —
the largest single-cycle closure rate this subsystem has ever had, verified
line-by-line against the current tree, not taken on the fix commits' word.
Second, the delta shipped a genuinely new capability this audit had never
exercised before: **exterior save/load** (`0a847910`, EX-09/17 item 4) — a
save taken while streaming an exterior worldspace can now queue and drive a
live reload instead of being rejected outright. That feature is exactly
where this cycle's one new finding lives: a load-order race between the
one-shot delta overlay and the background streaming worker that discards
saved actor state for every cell but the one the player is literally
standing in.

Per the task's own note, `cargo test --workspace` (bare) is blocked by an
unrelated `E0004` in `crates/scripting/examples/fragment_coverage.rs:59`
(owned by `/audit-scripting`); every test claim in this report ran via the
scoped `cargo test -p byroredux-save` / `cargo test -p byroredux <filter>`
invocations named above, which build clean.

## Executive Summary

`crates/save/src/lib.rs` docstring claims verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot (curated game-state set) | **CODE-CONFIRMED** — the SAVE-D1-12 completeness guard is green with `SCAN_ROOTS` now covering the whole of `crates/core/src` + `crates/scripting/src` + `crates/physics/src` + `crates/audio/src` + `crates/plugin/src` + `byroredux/src` (widened this cycle, closing #3166). `CurrentExteriorContext`, the exterior counterpart of `CurrentCellContext`, is registered alongside it. |
| Atomic write (tmp → fsync → read-back → rename → dir-fsync) | **CODE-CONFIRMED** — `disk.rs`'s only change this cycle is `latest_slot` refactored onto a new `slots_by_recency`, purely additive/read-only; the write dance itself is untouched. |
| Ring never clobbers the last good save | **CODE-CONFIRMED** — `SaveState::new` still calls `SaveRing::resume`; unchanged. |
| Validation gate refuses to persist inconsistent state, AND is now player-visible | **CODE-CONFIRMED, and the 2026-08-20 gap is closed.** `surface_save_load_output` (new, `byroredux/src/main.rs:738`) routes every F5/F9/pause-menu `CommandOutput` through `log::warn!` (on failure) and the debug-UI's `push_player_message`/`push_console_line`, so a validation-aborted quicksave is no longer indistinguishable from a written one (closes #3162). |
| A typed-decode preflight rejects a bad snapshot before any live-world teardown | **CODE-CONFIRMED** — `validate_snapshot_types` (new, `crates/save/src/driver.rs`) is wired into both `restore_world` (before `clear_entities`) and `execute_pending_save_loads` (before any cell/streaming teardown), closing #3163's mid-column-failure hazard at the root instead of patching `apply_deltas` itself. |
| `FORMAT_MAJOR` bump is the only sanctioned schema evolution path | **CODE-CONFIRMED** — `FORMAT_MAJOR` is now `5` (was `4`), with a doc-comment naming exactly the three required-field additions that forced it (`Material.water_shader_flags`/`.is_water_shader`, `RigidBodyData.collidable`) plus the new `CharacterController` column. The `saved_type_shape_changes_require_format_major_bump` guard's baseline (`BASELINE_MAJOR = 5`) was regenerated in the same commit, closing #3164. |
| Off-frame load, never inside the scheduler | **CODE-CONFIRMED** — `restore_world` still has zero production callers; the new exterior reload path (`reload_exterior_session`) is driven from the same `execute_pending_save_loads` between-frames drain as the interior path, not from inside `Scheduler::run`. |
| A save taken mid-exterior-streaming can be reloaded live | **NEW THIS CYCLE, and CORRECT AT THE HAPPY PATH but NOT AT THE RACE** — `LoadCommand`/`execute_pending_save_loads` now accept either `CurrentCellContext` or `CurrentExteriorContext`, and the preflight-before-teardown discipline the interior path established is honoured (`build_exterior_world_context` is non-destructive and runs before any teardown). But the one-shot delta overlay that follows races the exterior bootstrap's own deliberately-partial initial-radius load — see SAVE-D6-2026-08-24-01. |

**Findings this cycle: 2. 0 CRITICAL, 1 HIGH, 0 MEDIUM, 1 LOW.**

By Data-Loss Class: **silent-drop — 1** (HIGH); **none / doc-rot — 1** (LOW).

## Data-Loss Class Matrix

| Finding | Class | Dimension | Severity |
|---|---|---|---|
| SAVE-D6-2026-08-24-01 (exterior live-load's delta overlay races the background streaming worker; every non-arrival cell's saved mutable state is silently and permanently dropped) | silent-drop | 6 — M45.1 Live Load-Apply | HIGH |
| SAVE-D6-2026-08-24-02 (escalation of OPEN #3028 — `save-load-roundtrip.md` now contains a materially false claim that exterior saves cannot live-reload) | none (doc rot) | 6 — M45.1 Live Load-Apply | LOW |

## Per-Dimension Coverage

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & Determinism | **0** | SAVE-D1-12 guard green (re-run live, not re-derived): `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` passes with the widened `SCAN_ROOTS`. Spot-checked every new `impl Component`/`impl Resource` site introduced since `bb0b92f2` (`CurrentExteriorContext`, `SaveLoadNotifications`, `PendingDeathReconciliations`, `MeleeDamageConfig`, `PersistentRefIndex`, `CellRootRefIndex`, `RegionAmbientRes`, `TerrainSeamStats`, water/nav/telemetry types) against their allowlist reasons — all check out against real mutator sites. `ActorVitals`'s undocumented `MUTABLE_DELTA_COLUMNS` exclusion (OPEN #3027) is unchanged; investigated its semantics directly (see *Disproved Candidates*) and confirmed the exclusion is *correct*, just still undocumented — noted for whoever picks up #3027, not re-filed. |
| 2 — Registry & (De)serialization Fidelity | **0** | `ValidateFn` (the third per-`Entry` closure, #3163) verified present on all three `register_*` variants, decoding the same target type `load` does. `FnvHasher` constants, `form_id_column`'s `is_form_id` flag, and `FormIdPair`-not-handle behavior all re-verified unchanged. All three residual holes from SAVE-D2-2026-08-20-02 (#3167) — the dead `register_form_id_component::<` match prefix, the unscanned `form_id.rs`/`script_instance.rs` nested-type files, the line-bound attribute matcher — are closed: explicit non-turbofish source edges were added and `serde_attribute_body`/`attribute_spans` now join rustfmt-wrapped multi-line attributes. |
| 3 — Disk Format & Durability | **0** | `latest_slot` now delegates to `slots_by_recency` (new, exposes the full newest-first ordering `quickload_latest` walks) with a deterministic mtime-tie-break by slot number; the write-dance itself (`create_dir_all` → tmp write → `sync_all` → byte-exact read-back → `rename` → parent-dir fsync) has zero diff this cycle. Two new unit tests (`latest_slot_ignores_newer_tmp_and_empty_directory`, `recency_tie_breaks_by_slot_number`) pass. |
| 4 — Validation Gates | **0** | Gate-before-write ordering unchanged and re-confirmed. The 2026-08-20 HIGH (`SAVE-D4-2026-08-20-01`, aborted saves invisible to the player) is fixed — see Executive Summary. `validate_snapshot_types` is now the FIRST thing `execute_pending_save_loads` does after draining the pending slot, before even resolving which kind of session (interior/exterior) to reload. |
| 5 — Frame-Boundary Capture & Off-Frame Apply | **0** | `SaveLoadNotifications` (new resource) is drained via `mem::take` in `app_frame.rs` **unconditionally** — `self.world.try_resource_mut::<SaveLoadNotifications>()...unwrap_or_default()` runs regardless of whether `self.debug_ui` is `Some`, so a headless/bench run can't leak the `Vec` across failed load attempts (verified in code, not just asserted). The drain and `step_save_loads` still run inside the same `about_to_wait` call, drain after the step, so a load-failure toast still surfaces in the same frame's draw. `capture_player_pose` → `step_save_loads` ordering unchanged. |
| 6 — M45.1 Live Load-Apply | **2** (1 HIGH, 1 LOW) | The interior path's ordering, remap correctness, idempotency, and the `apply_deltas`-`Err`-arm handling (2026-08-20's other HIGH, `SAVE-D6-2026-08-20-01`) are all fixed and re-verified: a delta-apply failure now runs `reconcile_dead_actor_runtime_state` on **both** the `Ok` and `Err` arms and `return`s before validation/pose-restore on failure. The new exterior path shares that same discipline for its own preflight (`build_exterior_world_context` before teardown) — but introduces a new, distinct hazard: SAVE-D6-2026-08-24-01. |

## Completeness Ledger

`build_save_registry` (`byroredux/src/save_io.rs`): unchanged component/resource
*count* pattern from last cycle plus one new resource — `CurrentExteriorContext`.
`MUTABLE_DELTA_COLUMNS`: unchanged from last cycle (`CharacterController`'s
addition already landed and was verified last cycle; no further additions this
cycle). Cross-checked against the SAVE-D1-12 guard's `NOT_SAVED_BY_DESIGN`
allowlist (now scanning the whole `crates/core/src` tree, not a subdirectory) —
green, per the live test run.

| Column | Kind | Saved | Overlaid | Status |
|---|---|---|---|---|
| `Transform`, `Inventory`, `EquipmentSlots`, `LightSource`, `LightFlicker`, `ScriptTimer`, `TwoStateActivator`, `ScriptVariables`, `ActorValues`, `EquippedWeapon`, `Dead`, `WanderState`, `TravelState`, `Traveled`, `GuardState`, `PatrolState`, `Escorted`, `ActorControlState`, `CharacterController`, `RigidBodyData`, `RumbleOnActivate` | Component | yes | yes | SAVED+OVERLAID, pinned by `delta_columns_carry_only_session_stable_fields`. `CharacterController`'s overlay precedes `apply_player_pose`'s momentum-zeroing, as required — verified in `execute_pending_save_loads`'s call order and by the passing `character_controller_breath_state_survives_live_delta_overlay` test. |
| `Name`, `Parent`, `Children`, `FormIdComponent` | Component | yes | no | structural identity — correct by design |
| `AnimationPlayer`, `AnimationStack` | Component | yes | no (deliberate) | #1696 hazard, unchanged |
| `FollowState`, `EscortState`, `Seated` | Component | yes | no (deliberate) | `EntityId` hazard, covered by `validate_saved_entity_references` |
| `ActorCinematicState`, `HorseTetherState` | Component | yes | no (deliberate) | #2380, unchanged |
| `Material` | Component | yes | no (deliberate) | #2378 blast-radius; gained the two required v5 fields, `FORMAT_MAJOR` bump now covers them |
| `ActorVitals` | Component | yes | **no (undocumented)** | **OPEN #3027**, unchanged at HEAD. Investigated this cycle: `ActorVitals { health: u32 }` is a resolved per-game **Health AVIF FormID**, not a live HP value (`crates/core/src/ecs/components/actor_values.rs:19`); `combat.rs`'s `apply_health_damage` mutates `ActorValues` (which *is* a delta column) keyed by that FormID, never `ActorVitals` itself. The exclusion is therefore semantically **correct** — the field is write-once per-actor identity, not mutable gameplay state — it just still lacks the one-line reason comment every other exclusion in this file carries. Not re-filed; noted for whoever closes #3027. |
| `ItemInstancePool`, `CurrentCellContext`, **`CurrentExteriorContext`**, `PlayerPose`, `GameTimeRes`, `QuestStageState`, `QuestObjectiveState`, `QuestAliasInjectionState`, `PlayerControlState`, `FragmentExecutionQueue`, `ReferenceEnableState`, `CinematicPresentationState`, `Globals` | Resource | yes | n/a | replaced wholesale by `restore_resources`, before `apply_deltas` — correct. `CurrentExteriorContext` (new) mirrors `CurrentCellContext` exactly: `snapshot_exterior_context` extracts it, `LoadCommand` accepts either context, `reload_exterior_session` re-stamps it after rebuild. |

## Findings

### HIGH

#### SAVE-D6-2026-08-24-01: exterior live-load's one-shot delta overlay runs before the streaming radius has resolved — every actor's saved mutable state outside the single arrival cell is silently and permanently dropped
- **Severity**: HIGH
- **Dimension**: 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: silent-drop
- **Location**: `byroredux/src/save_io.rs` (`reload_exterior_session`, `execute_pending_save_loads`'s call to `byroredux_save::apply_deltas` immediately after it), `crates/save/src/driver.rs` (`build_form_id_remap`, a synchronous full-`World` query), `crates/save/src/registry.rs:135-151` (the component `apply` closure's silent `filter_map`), `byroredux/src/scene/world_setup.rs` (`stream_initial_radius`, `ExteriorBootstrapMode::ForegroundFirst`, `bootstrap_waiting`)
- **Status**: NEW — this subsystem's first exterior-save/load-specific finding, made possible by `0a847910` (EX-09/17 item 4), which this audit had never reviewed before
- **Description**: `reload_exterior_session` reconstructs a saved exterior session by calling `crate::scene::assemble_exterior_streaming(..., ExteriorBootstrapMode::ForegroundFirst)`. That mode is a deliberate, documented design choice for interactive responsiveness: `stream_initial_radius`'s own doc comment says it "blocks only for the center cell... then leaves the surrounding radius to the steady-state per-frame budget." Concretely, `bootstrap_waiting(ForegroundFirst, pending, center)` returns as soon as the *arrival* cell has been consumed from `state.pending`, even though every other cell inside `radius_load` is still queued behind it on the streaming worker. `stream_initial_radius` logs this explicitly when it happens:

  ```rust
  if mode == ExteriorBootstrapMode::ForegroundFirst && !state.pending.is_empty() {
      log::info!(
          "Exterior foreground ready at ({cx},{cy}); {} peripheral cells continue streaming",
          state.pending.len(),
      );
  }
  ```

  `reload_exterior_session` returns to `execute_pending_save_loads` at exactly this point — with the world containing only the arrival cell's entities. The very next thing `execute_pending_save_loads` does, in the same synchronous call, is:

  ```rust
  let remap = byroredux_save::build_form_id_remap(world, &registry, &snapshot);
  match byroredux_save::apply_deltas(world, &registry, &snapshot, &remap, MUTABLE_DELTA_COLUMNS) { ... }
  ```

  `build_form_id_remap` runs `world.query::<FormIdComponent>()` — a full, synchronous `World` scan — to build the `saved-id → live-id` map `apply_deltas` needs. At this instant, the only entities carrying a `FormIdComponent` are the arrival cell's. Every entity from every other cell in the load radius (the ones still in `state.pending`, streamed in over the *following* frames by the normal per-frame budget) simply doesn't exist in the ECS yet, so it cannot appear in the remap.

  `apply_deltas`'s per-column `apply` closure (`registry.rs`) then does exactly what it's documented to do for an entity absent from the remap — silently drop the row:

  ```rust
  let remapped: Vec<(u32, T)> = rows
      .into_iter()
      .filter_map(|(old, comp)| remap.get(&old).map(|&live| (live, comp)))
      .collect();
  ```

  There is **no logging** at either the per-row drop site or the caller — `execute_pending_save_loads` logs only `applied` (the count that *did* land) and `remap.len()`, both silently smaller than the saved state warrants, with nothing to indicate anything was skipped. And crucially, `apply_deltas` runs exactly once, synchronously, inside this one drain call — there is no hook anywhere in the streaming pipeline (`streaming.rs`, `world_setup.rs`, `cell_loader/*`) that re-applies the saved deltas to a cell once it finishes streaming in later. `grep -rn "apply_deltas"` across `byroredux/src/streaming*.rs` and `byroredux/src/cell_loader/*.rs` returns zero hits outside `save_io.rs` itself.

  Net effect: reload a save taken while standing in an exterior worldspace, and every mutated component this subsystem is built to preserve — `Dead` (an NPC the player killed comes back alive), `ActorValues` damage/temporary layers, `Inventory`/`EquipmentSlots` changes, the seven AI-procedure state components, `RigidBodyData.collidable`, `RumbleOnActivate` — silently reverts to its ESM-derived default for **every actor outside the single cell the player happened to be standing in when they saved**, and stays reverted permanently (the next save captures the now-reverted state as the new "true" state).
- **Evidence**: `byroredux/src/scene/world_setup.rs:751-762` (doc comment: "blocks only for the center cell... leaves the surrounding radius to the steady-state per-frame budget"), `:836-862` (the bootstrap-wait loop and its own "peripheral cells continue streaming" log line), `byroredux/src/save_io.rs` (`reload_exterior_session` calling `assemble_exterior_streaming` with `ExteriorBootstrapMode::ForegroundFirst`, then `execute_pending_save_loads` calling `build_form_id_remap`/`apply_deltas` unconditionally right after), `crates/save/src/driver.rs:220-230` (`build_form_id_remap`'s `world.query::<FormIdComponent>()`), `crates/save/src/registry.rs:143-146` (the silent `filter_map`). Confirmed no test exercises this path: `byroredux/src/save_io/command_queue_tests.rs`'s `save_then_load_command_queues_with_exterior_context` only exercises the `SaveCommand`/`LoadCommand` *queueing* side (which needs no renderer), never `execute_pending_save_loads`'s actual drain for an exterior context — `grep -rn "reload_exterior_session\|ForegroundFirst" byroredux/src/save_io/*.rs crates/save/tests/*.rs` returns nothing.
- **Impact**: This is not an edge case — it fires on the ordinary, expected path of quickloading (F9) or console-loading while playing outdoors, which for an open-world engine is likely the *common* case, not the exception. The blast radius is every mutable delta column across every cell in the streaming radius except the arrival cell, every time. Because the drop is completely silent (no log, no validation-gate signal — `validate_world`/`validate_form_ids` run post-load and would find nothing *wrong*, since a reverted-to-ESM-default actor is referentially consistent, just not what the player actually did), a player has no way to notice this happened short of walking back to the affected cell and observing an NPC they killed is alive again.
- **Related**: `0a847910` (the feature that introduced the surface); the interior path's analogous, already-documented "FormIdPair present in the save but not in the reloaded cell is dropped with the delta lost" note in the skill checklist — that case is about a genuinely-removed record and is correctly not-a-bug; this case is a pure timing artifact where the record *will* exist, just not yet.
- **Suggested Fix**: Either (a) make `reload_exterior_session` block for the *full* load radius (`ExteriorBootstrapMode::FullRadius`) when driven by a live load specifically — accepting the one-time frame-time cost of a save/load in exchange for correctness, mirroring how the interior path already does one complete, blocking reload — or (b) defer `build_form_id_remap`/`apply_deltas` until `state.pending` is empty, re-running them incrementally as each peripheral cell finishes streaming in over subsequent frames (more work, but preserves the interactive-responsiveness goal `ForegroundFirst` exists for). At minimum, until either lands, log a `log::warn!` naming how many saved delta rows per column were dropped for remap-miss reasons, so the loss is at least diagnosable instead of fully silent.

### LOW

#### SAVE-D6-2026-08-24-02: `save-load-roundtrip.md` — escalation of OPEN #3028 — now asserts a capability that no longer exists as described: that an exterior save "has no cell to reload into" and is rejected
- **Severity**: LOW
- **Dimension**: 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: none (doc rot)
- **Location**: `docs/engine/save-load-roundtrip.md:10` (currency note, unchanged since the 2026-08-20 escalation), `:42-47` (§2, still "today 10+ components"), `:64-70` (§3, still "checks four invariants... plus a binary-side `validate_form_ids`"), `:103-104` (§5, the newly-false claim), `:113-147` (§6, the seven-column enumeration against `MUTABLE_DELTA_COLUMNS`'s current twenty, and no mention of the exterior branch at all)
- **Status**: Further escalation of OPEN **#3028** (previously escalated 2026-08-20 as `SAVE-D6-2026-08-20-03`, itself still open and unaddressed) — filed as a continuation, not a new issue
- **Description**: The three passages #3028 originally named (§2's component count, §3's "four invariants", §6's seven-column enumeration) are unchanged and now further behind reality: `validate_world` runs *six* checks plus *two* binary-side ones (unchanged claim since last cycle, still uncorrected), and `MUTABLE_DELTA_COLUMNS` now holds twenty entries against the doc's seven.

  This cycle adds a fourth, more serious kind of staleness: §5 (*Load trigger*) still reads

  > "it can only decode + verify the slot and check it carries a `CurrentCellContext` (a loose-NIF or exterior-only save has no cell to reload into — that's an error here)"

  This is no longer true. `0a847910` (EX-09/17 item 4) gave `LoadCommand` a second accepted context, `CurrentExteriorContext`, specifically so an exterior-mode save *can* live-reload — that's the entire point of the feature this audit's HIGH finding is about. The doc doesn't just have a stale count, it makes an affirmatively wrong claim about what the engine can and cannot do, and the whole exterior branch of §6's live-load-apply trace (`reload_exterior_session`, `build_exterior_world_context`, `CurrentExteriorContext`, the `SaveLoadNotifications` player-facing surface, the `validate_snapshot_types` preflight) is entirely absent from the doc despite each having landed as a named, deliberate feature.
- **Evidence**: `git diff bb0b92f2..HEAD -- docs/engine/save-load-roundtrip.md` is empty — the file has had zero commits touch it since the 2026-08-20 cycle, despite `0a847910`, `eb582353`, and the six 2026-08-20-fix commits all landing in the same window. `grep -n "exterior" docs/engine/save-load-roundtrip.md` returns only two incidental hits (the cross-reference to `exterior-grid-streaming.md` in the header, and one mention of `Exterior directional lighting` inside §6 step 4) — `CurrentExteriorContext`/`reload_exterior_session` appear nowhere.
- **Impact**: Unchanged from the 2026-08-20 assessment — this doc is named in `.claude/commands/_audit-common.md` as the authoritative cross-cutting trace for the subsystem, so a reader who trusts it will conclude exterior save/load doesn't exist and will not know to look for SAVE-D6-2026-08-24-01's failure mode when it happens to them.
- **Related**: OPEN #3028 (all three original passages, still unfixed); SAVE-D6-2026-08-20-03 (the prior escalation, same root cause — the currency note keeps getting refreshed for whichever section the commit touched, without a full pass); SAVE-D6-2026-08-24-01 (the feature this doc most needs to but doesn't describe).
- **Suggested Fix**: Same as 2026-08-20's recommendation, now with one more item: add a §5/§6 branch describing the exterior reload path end-to-end (context capture → `build_exterior_world_context` preflight → `assemble_exterior_streaming` under `ForegroundFirst` → the delta-apply timing this cycle's HIGH finding is about), and prefer symbol references over transcribed counts throughout so the next feature landing doesn't require a fourth manual sync pass.

## Regression Guards Verified This Cycle

All run live, not re-derived from a prior report:

| Guard | Location | Invariant pinned | State |
|---|---|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | `save_io/registry_completeness_tests.rs` | every `impl Component`/`impl Resource` under the (now six, widened) scan roots is registered XOR allowlisted with a reason | **green**, `cargo test -p byroredux` |
| `saved_type_shape_changes_require_format_major_bump` | `save_io/serde_default_guard_tests.rs` | any field add/remove/retype on a saved struct requires a `FORMAT_MAJOR` bump, baseline regenerated alongside v5 | **green** |
| `serde_default_on_saved_struct_requires_format_major_bump` | same file | no bare/`cfg_attr` `#[serde(default)]` on a saved struct | **green** |
| `source_discovery_follows_registry_and_nested_save_modules` | same file | scan set derived from the registry, now including the explicit non-turbofish edges (`form_id.rs`, `ecs/components/form_id.rs`, `string/mod.rs`, `script_instance.rs`) | **green** |
| `delta_columns_carry_only_session_stable_fields` | `save_io/round_trip_tests.rs` | `MUTABLE_DELTA_COLUMNS` == hand-audited, no `FixedString`/`EntityId` | **green**, unchanged |
| `typed_snapshot_preflight_rejects_bad_column_without_world_mutation` | `crates/save/tests/round_trip.rs` | `validate_snapshot_types` runs before `clear_entities`, never touches the world on failure | **green** |
| `character_controller_breath_state_survives_live_delta_overlay` | `save_io/round_trip_tests.rs` | the v5 `CharacterController` column overlays before pose-restore's momentum clear | **green** |
| `save_then_load_command_queues_with_exterior_context`, `load_command_rejects_a_save_with_no_cell_or_exterior_context` | `save_io/command_queue_tests.rs` | `LoadCommand` accepts either context, rejects neither | **green** — but see SAVE-D6-2026-08-24-01: these cover queueing only, not the drain |
| `quickload_empty_errors_and_corrupt_newest_falls_back`, `startup_load_parser_queues_valid_slot_and_surfaces_invalid_value`, `quicksave_shares_the_console_save_command_output_contract`, `validation_aborted_quicksave_is_classified_for_player_feedback` | `save_io/command_queue_tests.rs` | the four new player-facing entry points (F5/F9/menu/`--load`) each have direct coverage now | **green** — closes the LOW test-gap flagged 2026-08-20 (`SAVE-D6-2026-08-20-02`) |
| Container gates (`rejects_bad_magic`/`rejects_truncated`/`rejects_payload_truncation`/`detects_crc_corruption`/`rejects_schema_mismatch`/`rejects_major_version_skew`) | `crates/save/src/snapshot.rs` | every gate precedes `serde_json::from_slice` | **green**, unchanged |
| Disk atomicity (`write_read_round_trip_and_atomic_rename`, `latest_slot_ignores_newer_tmp_and_empty_directory`, `recency_tie_breaks_by_slot_number`, `cursor_after_newest_points_past_latest_mtime`) | `crates/save/src/disk.rs` | atomic rename, tmp-exclusion, deterministic recency ordering | **green** |
| `validate_progression_state` (#2947) | `crates/save/src/validate.rs` | save aborts if `CharacterLevel.xp != 0` while unregistered | **green**, unchanged this cycle |

## Disproved Candidates (investigated, not filed)

- **"`ActorVitals`'s `MUTABLE_DELTA_COLUMNS` exclusion is a silent-drop bug like #1834's `ActorValues` was."** Rejected — traced `ActorVitals.health`'s actual role: it is a resolved Health-AVIF **FormID** (per-game constant, stamped once at spawn), not a live HP number. `combat.rs`'s `apply_health_damage` writes damage into `ActorValues` (a full `MUTABLE_DELTA_COLUMNS` member), keyed *by* that FormID — never into `ActorVitals` itself. The exclusion is correct; only its missing reason-comment is a (pre-existing, OPEN #3027) documentation gap.
- **"The exterior reload path leaves the camera unrepositioned when a save predates `PlayerPose`."** Investigated — `assemble_exterior_streaming`'s docstring confirms it deliberately never touches camera placement (four callers disagree on when that should happen), so `reload_exterior_session` discarding its returned `Vec3` center is intentional, matching the interior path (which also never uses `CellLoadResult.center` from a live-load reload — positioning is `apply_player_pose`'s job in both branches). Reachability of a `PlayerPose`-less *current-format* save is effectively zero: the resource is installed at boot and refreshed every frame before any save command can run, so `snapshot_player_pose` returning `None` only happens for a save that predates `PlayerPose`'s registration entirely — which the schema fingerprint would already reject before this code path is reached, per the same reasoning verified in the 2026-08-20 cycle (not re-verified fresh this cycle beyond confirming the code shape is unchanged).
- **"The exterior reload's unconditional `unload_current_interior` call could double-teardown or panic when nothing interior is loaded."** Rejected — `unload_current_interior` is a no-op guarded on `CurrentCellRoot` being `Some`; called unconditionally by both `reload_interior_session` and `reload_exterior_session` specifically so either reload direction (interior-save-while-exterior, exterior-save-while-interior) tears down whichever kind of session is actually live. Symmetric and correct by inspection.
- **"`quickload_latest`'s fallback loop could misreport failure when a fallback slot succeeds."** Rejected — `command_output_is_failure`'s `.any()` match on `"Error:"`/`"save ABORTED"` prefixes doesn't false-positive on the `"skipped invalid quickload slot N: ..."` diagnostic lines the fallback loop accumulates, since those don't carry either prefix; the final aggregated output is classified correctly whether it ends in success or in `"Error: no decodable save slots available"`.

## Deduplication

`/tmp/audit/save/issues.json` (200 issues, latest window #2361-#3242) searched
for `save`, `load`, `snapshot`, `corrupt`, `formid`, `delta`, `serde`,
`quicksave`, `quickload`, `ring`, `validate`, `breath`, `drown`, `collidable`,
`format_major`, `pose`, `registry`, `exterior`, `streaming`, `worldspace`,
`radius`, `foreground`. No open issue overlaps the new HIGH finding — the
closest are #2377/#2376/#2372 (generic exterior-streaming epics, not
save/load-specific) and #3192/#3143 (unrelated exterior/streaming findings
from other audits).

All nine prior-cycle findings re-checked at HEAD:

| 2026-08-20 finding | Issue | State at HEAD |
|---|---|---|
| SAVE-D4-…-01 CommandOutput discarded into `log::info!` | #3162 | **FIXED, verified** — `surface_save_load_output` now routes every player-facing save/load call site through `log::warn!` + `push_player_message`/`push_console_line` |
| SAVE-D6-…-01 mid-column `apply_deltas` failure falls through to pose-restore on a partial overlay | #3163 | **FIXED, verified** — `validate_snapshot_types` typed preflight added ahead of teardown; the `Err` arm now reconciles dead actors and returns instead of falling through |
| SAVE-D2-…-01 required fields added post-`FORMAT_MAJOR`-bump with no guard | #3164 | **FIXED, verified** — `FORMAT_MAJOR` bumped to 5, `saved_type_shape_changes_require_format_major_bump` baseline regenerated in the same commit |
| SAVE-D1-…-01 `CharacterController` breath/drowning fields unsaved | #3165 | **FIXED, verified** — registered + added to `MUTABLE_DELTA_COLUMNS`, overlay ordered before pose-restore's momentum clear |
| SAVE-D1-…-02 `SCAN_ROOTS` covers one subdirectory, 63 sites uncovered | #3166 | **FIXED, verified** — widened to the whole `crates/core/src` tree plus `crates/audio/src`/`crates/plugin/src` |
| SAVE-D2-…-02 serde guard's three discovery holes | #3167 | **FIXED, verified** — dead prefix removed, explicit non-turbofish edges added, multi-line attribute span matcher added |
| SAVE-D1-…-02 (2026-08-16) `ActorVitals` exclusion undocumented | #3027 | **OPEN, unchanged** — investigated fresh this cycle (see *Disproved Candidates*): the exclusion is semantically correct, only the reason-comment is still missing |
| SAVE-D6-…-03 (2026-08-16) `save-load-roundtrip.md` stale | #3028 | **OPEN, further escalated** — see SAVE-D6-2026-08-24-02 |

---

TALLY: CRITICAL=0 HIGH=1 MEDIUM=0 LOW=1
