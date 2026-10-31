# Save / Load Subsystem Audit (M45 + M45.1) — 2026-10-31

**HEAD**: 670f81917 · **Baseline**: `docs/audits/AUDIT_SAVE_2026-10-27.md` (HEAD `fa6be4a6b`) · **Audited**: Dimensions 1, 2, 5 (delta-first; 136 commits in `fa6be4a6b..HEAD`, of which about 15 touch save-relevant paths) and the Dimension 4 transient-state check · **Unchanged since baseline (skimmed)**: Dimension 3 (Container & Disk), and the `commands.rs` half of Dimension 4. Nothing in the window touched `crates/save/src/{disk,lib,driver,registry}.rs`, `crates/core/src/atomic_file.rs`, `crates/save/Cargo.toml` or `save_io/commands.rs`. `snapshot.rs` changed only in the `FORMAT_MAJOR` constant and its ledger doc.

This run is part of `/audit-suite --preset comprehensive`. I analysed every dimension myself, with no sub-agents. Scratch notes are in `/tmp/audit/save/dim_{1..5}.md`. I ran only `cargo test` (repo `target/`, `CARGO_INCREMENTAL=0`, `TMPDIR=/mnt/data/tmp`). I started no engine.

Save-relevant changes in the window (almost all M43 gameplay):
- **`FORMAT_MAJOR` 119 -> 122**:
  - v120: `KnownWords` resource (`Game.TeachWord` / `UnlockWord`) and `Effect::ObtainWord`.
  - v121: `SmNodeRuntime.last_node_fire_hours` (node-level `HNAM` reset window).
  - v122: `ItemInstance.quality` (Skyrim tempering level; improving at the grindstone / armor workbench).
- **New unsaved resources and components**, all on `NOT_SAVED_BY_DESIGN`:
  - crafting: `CraftingBook`, `ActiveCrafting`, `CraftingRequest`
  - dropping: `PendingDroppedItems`
  - conversations: `RandomConversationState`, `ConversationHold`
  - corpses: `DeadBodyState`
  - mines: `MineBook`, `MineClock`, `ArmedMine`, `SpentMine`
  - attraction: `AttractionBook`, `AttractionState`
  - lock/aim/armor: `TerminalBases`, `LockpickItem`, `AimWeapons`, `IronSightsState`, `ComplexSceneObject`, `ArmorBook`
  - jail: `JailEscapeWatch`
- **Skyrim jail**: `Faction.SendPlayerToJail` (`36d7bfbd6`) moves the player's whole inventory through `PendingEvidenceDeposits` (see SAVE-2026-10-31-01).
- **#6133** `1e2c1d30e`: `release_held_dead_actor_reconciliations` was added after `restore_resident`.
- **Both 10-27 findings are still present and open**: #6508 (SAVE-2026-10-27-01) and #6509 (SAVE-2026-10-27-02). Neither is re-filed.

Dedup sources: `/tmp/audit/issues.json` (98 open), `/tmp/audit/SUITE_RULES.md` § 6 (read at the start and again before finalising), and the 10-27 report.
- ECS-2026-10-31-01 (the `improve_plan` lock order) is not a save finding, but it is the only red CI lane. I cross-checked it: `save_world` and `execute_pending_save_loads` run outside the scheduler, so they add no new edge to it.
- GAME-D4-2026-10-31-01 (a detonated mine re-arms after reload) is the load-side face of the `SpentMine` allowlist row. The row's claim that the ledger entry is the persistent half is true on the save side: `detonate` writes `ReferenceEnableState.set_enabled(form, false)`. The re-arm is `mine_system` not honouring that ledger, which is the gameplay audit's finding. I do not re-report it.

## Executive Summary

| `lib.rs` / `snapshot.rs` design claim | Status |
|---|---|
| Full ECS snapshot; the registry is the completeness contract | **CODE-CONFIRMED.** The guard is green. Every new `Component`/`Resource` impl is registered (`KnownWords`) or allowlisted with a reason. Three allowlist reasons are imprecise or false (SAVE-2026-10-31-01 and -02). |
| Versioned container, CRC, atomic write, ring | **CODE-CONFIRMED (unchanged).** 40 unit + 15 `round_trip` tests pass. |
| `FORMAT_MAJOR` bump discipline | **CODE-CONFIRMED.** `FORMAT_MAJOR == BASELINE_MAJOR == 122`. v120, v121 and v122 are each genuine bumps and each baseline refresh carries a justification comment. The ledger is contiguous and the schema fingerprint is pinned (`0xd2b0_3894_ed0d_b7aa`). The `MotionType` blind spot (#6509) is still open. |
| "Every load behaves like a fresh boot" (#5435 / #5864 / #6083 / #6238 / #6239) | **DRIFTED**, as on 10-27 (#6508). Two more transients escape the sweep (SAVE-2026-10-31-02). |
| Live-load ordering | **CODE-CONFIRMED.** `post_reload_drain_steps_keep_their_order` is green. `release_held_dead_actor_reconciliations` (#6133) sits right after `restore_resident`, and it runs before the remap, the pre-seat and the overlay, as it must. |

**Findings: 2 NEW. 0 CRITICAL, 0 HIGH, 1 MEDIUM, 1 LOW.** Existing and still open: #6508, #6509.

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 1 | SAVE-2026-10-31-01 |
| LOW | 1 | SAVE-2026-10-31-02 |

## Data-Loss Class Matrix

| Finding | Severity | Class |
|---|---|---|
| SAVE-2026-10-31-01 | MEDIUM | silent-drop, irrecoverable. The player's whole inventory has no durable record while the deposit is in flight. An unreachable chest deletes it outright. |
| SAVE-2026-10-31-02 | LOW | corruption-on-load, cosmetic. A stale crafting page stays open after a console load, and a duplicate `REMP` story event can fire. |
| #6508 (existing) | LOW | See the 10-27 report. SAVE-2026-10-31-01 widens its magnitude. |
| #6509 (existing) | LOW | latent corruption-on-load |

## Completeness Ledger

**Registry × `MUTABLE_DELTA_COLUMNS`**:
- **SAVED + OVERLAID (30)**: unchanged from 10-27. `MUTABLE_DELTA_COLUMNS` and the third-list partition were not touched in the window, and `every_delta_column_is_parked_or_explicitly_not` is green.
- **SAVED only**: unchanged from 10-27.
- **Resources added**: `KnownWords` (`words_of_power::register_saved`, a `BTreeSet<u32>`, so its output is deterministic and it holds only FormIDs).
- **Saved-type shape changes**: `Effect::ObtainWord` (an added externally-tagged variant, read-compatible but bumped with v120), `SmNodeRuntime.last_node_fire_hours` (v121) and `ItemInstance.quality` (v122).

**`NOT_SAVED_BY_DESIGN` reasons spot-checked** (the 19 new types):
- **True**:
  - rebuilt from the records each boot: `CraftingBook`, `ArmorBook`, `MineBook`, `AttractionBook`, `TerminalBases`, `LockpickItem`, `AimWeapons`, `ComplexSceneObject`
  - scan clocks and cooldowns re-armed like a fresh boot: `MineClock`, `AttractionState`, `RandomConversationState`, `DeadBodyState`. The `DeadBodyState` reason is honest that a body can be found twice after a load, and the saved `last_node_fire_hours` bounds how often that restarts a quest.
  - `IronSightsState`: held input re-read every frame.
  - `ArmedMine`: a mid-fuse save reloads the mine as placed. Its `victim` is a session `EntityId`.
  - `SpentMine`: the ledger write is real, but see GAME-D4-2026-10-31-01.
  - `ConversationHold`: an NPC's `Transform` is written directly while the hold walks it, and `Transform` is a saved column, so no half-state persists. The ambient package system re-selects on load.
  - `JailEscapeWatch`: restarts at false and sets true on the first interior frame. A narrow edge exists where the watch is already true in the outgoing session and the loaded save has a sentence set but the player not yet moved. It would raise a false `ESJA`. It needs a console load and I did not file it.
- **False or imprecise**:
  - `PendingEvidenceDeposits` (SAVE-2026-10-31-01; #6508 already covers the "drained within the frame" half)
  - `VatsState` (#6508)
  - `ActiveCrafting` ("a menu session that a load closes", SAVE-2026-10-31-02)
  - `PendingDroppedItems` (SAVE-2026-10-31-02)

## Findings

### SAVE-2026-10-31-01: `Faction.SendPlayerToJail` removes the whole inventory and hands it to a one-frame deposit queue that has no durable record; an unreachable chest deletes it
- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness (allowlist reason) / Live Load-Apply (frame boundary)
- **Data-Loss Class**: silent-drop (irrecoverable)
- **Location**:
  - `crates/scripting/src/jail.rs:339-376` (`send_player_to_jail`) and `:395-405` (`deposit_inventory`)
  - `crates/scripting/src/jail.rs:77` (`PendingEvidenceDeposits`)
  - `byroredux/src/crime.rs:713-729` (`evidence_deposit_system`)
  - `byroredux/src/cell_loader/reference_state.rs:117-137` (`deposit_into_reference`)
  - `byroredux/src/save_io/registry_completeness_tests.rs:756` (the allowlist row)
- **Status**: NEW. It builds on open #6508 (SAVE-2026-10-27-01) and widens it.
- **Description**:
  1. **The Oblivion path has a durable record.** `confiscate_stolen_goods` calls `ledger.confiscate(item, taken)` on `ObScriptLedger` (saved, v118), then queues the deposit. The allowlist row relies on this: "the ObScriptLedger's saved evidence is the durable record until delivered".
  2. **The Skyrim path (`36d7bfbd6`) has none.** `deposit_inventory` calls `remove_items(world, player, item, count, None)` for every stack the player holds, then pushes an `EvidenceDeposit`. It never touches the ledger. From that instant until `evidence_deposit_system` runs on the *next* Update, the only copy of the player's whole inventory is an unsaved `Vec` in `PendingEvidenceDeposits`.
  3. **A deposit that cannot land is dropped.** `evidence_deposit_system` does `mem::take` on the queue. If `deposit_into_reference` returns `false` (no resolvable PLCN chest, or no `GlobalFormIdResolver` / `PersistentReferenceStates`), it logs "stay in the evidence". On the Skyrim path there is no evidence, so the goods are gone.
  4. **Smaller side effects.**
     - `deposit_into_reference` writes `StoredStack { instance: None }`, so an improved item (v122 `quality`) loses its instance.
     - A parked row replaces the chest's authored inventory at respawn. `send_player_to_jail` checks only `player_inventory_container != 0`, not that the chest is authored empty, which the `deposit_into_reference` doc requires of its caller.
- **Trigger paths**:
  - **A save or a load drain between the removal and the next Update.**
    - `SendPlayerToJail` runs from INFO result scripts in the Late dialogue-selection system, from quest-stage fragments, or from the `story_actions` console command.
    - The save drain runs post-scheduler of the same tick, and the consumer runs on the next Update.
    - A save taken there is written with an empty inventory, an empty chest, `ledger.jail` set and `PlayerMoveRequest` pending. Loading it loses the inventory permanently.
    - A load at that boundary instead deposits the outgoing timeline's goods into the loaded world's chest (the #6508 mechanism).
    - F5/F9 are blocked while a game-menu page is open, so the practical triggers are the console `save`/`load` and a fragment-driven call. That narrowness is why #6508 was LOW.
  - **An unreachable chest.** This needs no save at all. It is theoretical on vanilla Skyrim, because `GlobalFormIdResolver` resolves any load-order FormID. A mod faction whose PLCN names a form that is not in the load order would delete the player's kit.
- **Evidence**:
  - `jail.rs:395-405` contains no `ledger` access.
  - `jail.rs:210-215` (the Oblivion path) calls `ledger.confiscate` before the push.
  - `live_load.rs` has no `Evidence` or `Jail` reference. `discard_outgoing_session_requests` does not clear the queue (the #6508 premise, unchanged).
- **Impact**: Medium rather than low, because the magnitude changed. The 10-27 finding concerned stolen goods only; this path moves the player's entire carried inventory, and there is no ledger fallback to recover it. The reachable window stays narrow.
- **Suggested Fix** (one of):
  - **Preferred**: perform the deposit synchronously in `send_player_to_jail` when `deposit_into_reference` succeeds, and take the goods from the inventory only after it returns `true`. If the chest is unreachable, take nothing. The ledger's `release_evidence` bookkeeping then becomes Oblivion-only.
  - Or: record the Skyrim goods in `ObScriptLedger` evidence exactly as `confiscate_stolen_goods` does, so the saved ledger is the durable record. This makes the allowlist row true.
  - Either way, clear `PendingEvidenceDeposits` in `discard_outgoing_session_requests` (the #6508 fix), correct the row, and refuse a chest that is not authored empty.

### SAVE-2026-10-31-02: Two more M43 transients escape the load sweep, and their allowlist reasons overstate it (`ActiveCrafting`, `PendingDroppedItems`)
- **Severity**: LOW
- **Dimension**: Live Load-Apply (frame boundary) / Snapshot Completeness (allowlist reasons)
- **Data-Loss Class**: corruption-on-load, cosmetic. Nothing is lost.
- **Location**:
  - `byroredux/src/crafting.rs:431-442`
  - `byroredux/src/inventory/actions.rs:312-343` (`PendingDroppedItems`, `dropped_item_story_event_system`)
  - `byroredux/src/save_io/registry_completeness_tests.rs:572-574`
  - `byroredux/src/save_io/live_load.rs:459-481` (the sweep)
- **Status**: NEW. This is the #6239 sweep-gap class, a sibling of open #6508.
- **Description**:
  1. **`ActiveCrafting`.** The row says "a menu session that a load closes". No code in the load path calls `end_crafting`; the only callers are `toggle_game_menu`, `open_inventory_menu` and `resume_from_game_menu` in `main.rs`.
     - F5/F9 and the pause menu cannot load while the page is open. The remote console `load` can, the same hole the `ActiveBarter` row already admits.
     - After such a load the page stays open on the old bench `EntityId` (ids are not reclaimed). `recipes_at` finds no keywords for it, so the page lists nothing, and `craft` answers "this bench does not make that". Nothing is lost, but the reason is false.
     - `CraftingRequest` ("drained the same frame") is taken in `render_one_frame`, after the save/load drain in the same tick. A bench activated that tick is therefore still queued across a drain. Its stale id is refused by `begin_crafting` ("offers no recipes"), which is harmless.
  2. **`PendingDroppedItems`.** It holds `(created-reference FormID, seconds waited)` for up to 10 s and is not in `discard_outgoing_session_requests`.
     - After a load, the outgoing timeline's pending ids stay queued.
     - `CreatedReferences` is restored with its saved id allocator, so an id from the abandoned timeline can be handed out again.
     - A drop in the loaded session within the window can raise `REMP` twice for the same reference.
     - The row's "only the not-yet-raised event is lost on a load" is the opposite of what happens.
- **Impact**: cosmetic and very narrow. It is filed so the next sweep fix covers these types, and because the suggested generic guard from 10-21 and 10-27 (a source scan that fails when a "drained" or "consumed" row is not discarded in the drain or justified as entity-keyed) would have caught both this and #6508.
- **Suggested Fix**:
  - Call `end_crafting` and clear `CraftingRequest` in the load drain; `end_barter` is the same shape.
  - Clear `PendingDroppedItems` in `discard_outgoing_session_requests`.
  - Extend `live_load_drain_discards_the_outgoing_sessions_requests`.
  - Land the generic guard that has been proposed twice.

## Existing findings re-checked (not re-filed)

- **#6508** (`PendingEvidenceDeposits`, `VatsState` escape the load sweep): still present. `grep -n 'Vats\|Evidence' byroredux/src/save_io/live_load.rs` finds nothing, and both allowlist rows keep their false reasons (`registry_completeness_tests.rs:454`, `:756`).
- **#6509** (`MotionType` unfingerprinted): still present. `save_type_sources()` is unchanged in its retain arms. The window's `collision.rs` edit was a doc comment only.

## Dimension notes

- **Dimension 1**:
  - The registry diff in the window is one line: `words_of_power::register_saved`, after `skills`.
  - `KnownWords` is a `BTreeSet`, so it is deterministic.
  - The removed-from-allowlist set (`CharacterController`, `Material`, `RumbleOnActivate`, `FragmentExecutionQueue`, the cinematic trio) is still registered, and `RigidBodyData` stays out.
  - The improved-item path stores `ItemInstance.quality` inside the already-registered `ItemInstancePool`. `validate_world`'s inventory-instance-vs-pool check covers it, and `ItemInstancePool` is still excluded from the pre-reload subset.
  - Dropped items persist through the registered `CreatedReferences` resource, which is correct.
  - Mine detonation persists through `ReferenceEnableState`, and the hacked and picked lock levels go through `ReferenceLockState`, both already saved.
- **Dimension 2**:
  - No new `Serialize` derives were added in the window (`git diff` of added derive lines: none).
  - All three shape changes (v120, v121, v122) bumped in the same commit as their baseline refresh.
  - The extension-state payload (`crates/sdk/src/component.rs`) is untouched.
- **Dimension 3**: unchanged (see the header).
- **Dimension 4**:
  - No new registered type carries an `EntityId`, so `validate_saved_entity_references` needs no new row.
  - The new transients are all on the allowlist, not in the registry.
- **Dimension 5**:
  - The drain sequence matches the specification.
  - The only new step is the `release_held_dead_actor_reconciliations` call, which is correctly placed.
  - Every failure arm still returns before `apply_player_pose`.

## Guards Verified

- `cargo test -p byroredux-save`: 40 unit + 15 `round_trip` passed, 0 failed.
- `cargo test -p byroredux --bin byroredux save_io`: 97 passed, 4 ignored (the four `consumable_tests` real-master legs), 0 failed. These guards are live by name:
  - `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`
  - `every_delta_column_is_parked_or_explicitly_not`
  - `the_schema_fingerprint_is_pinned`
  - `serde_default_on_saved_struct_requires_format_major_bump`
  - `saved_type_shape_changes_require_format_major_bump`
  - `delta_columns_carry_only_session_stable_fields`
  - `delta_columns_removed_at_runtime_have_a_load_reconciler`
  - `post_reload_drain_steps_keep_their_order`
  - `live_load_drain_discards_the_outgoing_sessions_requests`
  - `quickload_empty_errors_and_corrupt_newest_falls_back`
  - `the_bump_ledger_is_contiguous_and_current` (in `byroredux-save`)
- Guard limits found this run:
  - The completeness guard checks that a reason exists and that the type is real, not that "drained" or "closed by a load" is true (both new findings).
  - The outgoing-session discard test enumerates a fixed set, so new queues stay invisible to it.
- Not checkable here: any engine, smoke or Vulkan leg, and the `#[ignore]`d game-data tests. Both findings are static. I reproduced neither live.
