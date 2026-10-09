# Save / Load Subsystem Audit (M45 + M45.1) — 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: `docs/audits/AUDIT_SAVE_2026-10-05.md` (HEAD `a2c24b16e`) · **Audited**: Dimensions 1, 2, 3, 4, 5 (every dimension had commits in the window) · **Unchanged since baseline (skimmed)**: none at dimension level. Two sub-areas saw no production change and were only spot-checked: the extension-state payload (Dim 2) and `crates/save/src/{driver,registry,validate}.rs` (Dims 2, 4 and 5).

This run was part of `/audit-suite --preset comprehensive`. All five dimensions were analysed synchronously, with no sub-agents. The scratch notes are in `/tmp/audit/save/dim_{1..5}.md`. The window is 116 commits, and 20 of them touch the save paths. The changes that matter here:
- `FORMAT_MAJOR` went from 32 to 33 (#5367 `14cff35ae`, new saved resource `DialogueSpokenInfoForms`).
- `StoryManagerNodeState` was registered (#5366 `26b6a779c`), without a bump. That is correct: the schema fingerprint changes on a new column.
- Three fixes from the previous cycle landed: #5253, #5255 and #5247.
- #3817 (`63bf3347f`) added the cinematic release and re-adoption path.
- `00f580e09` allowlisted the Eat/Sleep components.

Dedup covered `/tmp/audit/issues.json` (113 open), closed-issue searches for each new finding, and today's sibling reports. Not re-reported, only cross-referenced: GAME-D5-03, GAME-D7-01, GAME-D7-02 and GAME-D5-01 (`AUDIT_GAMEPLAY_2026-10-08.md`), ECS D7-01 and D7-03 (`AUDIT_ECS_2026-10-08.md`).

## Executive Summary

| `lib.rs` / `snapshot.rs` design claim | Status |
|---|---|
| Full ECS snapshot of game state | **CODE-CONFIRMED for the registry.** Both new #5366/#5367 saved resources are registered and wholesale-restored. Two allowlist reasons are false: Eat/Sleep, and `CinematicReAdoption` (SAVE-D1-01, SAVE-D5-01). |
| Versioned container + CRC32 over the payload | **CODE-CONFIRMED.** The header gates are unchanged. |
| Atomic write (tmp → fsync → re-read+verify → rename → dir-fsync) | **CODE-CONFIRMED.** `write_slot` now stages through the unique `atomic_temp_path` and is pinned by `write_slot_has_no_clobber_fallback` (#5247). |
| Ring never clobbers the last good save | **CODE-CONFIRMED.** |
| Validation gate refuses to persist a poisoned save | **CODE-CONFIRMED.** #5253 now reads the context invariant (index present, no context), not only the stale flag. |
| `FORMAT_MAJOR` bump is the only schema-evolution path | **DRIFTED (enforcement).** The shape guard cannot see `StoryManagerNodeState`/`SmNodeRuntime` because of attribute order (SAVE-D2-01). The `snapshot.rs` bump ledger is missing v33 (SAVE-D2-02). |
| Load runs off-frame; additive overlay + reconcilers | **CODE-CONFIRMED for the drain sequence.** **DRIFTED for teardown completeness**: since #3817 the opening-convoy rider (the player) is adopted into a cell and despawned by the load's teardown (SAVE-D5-01). |
| Reproducible CRC at equal state | The doc disclaims it. Two more hash-order resources were added (`HashMap`/`HashSet`). No consumer hashes saves. Note only. |

**Findings: 5 NEW — 0 CRITICAL, 1 HIGH, 1 MEDIUM, 3 LOW.**

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | SAVE-D5-2026-10-08-01 |
| MEDIUM | 1 | SAVE-D2-2026-10-08-01 |
| LOW | 3 | SAVE-D1-2026-10-08-01, SAVE-D2-2026-10-08-02, SAVE-D5-2026-10-08-02 |

**Prior-cycle findings:**
- **SAVE-D4-2026-10-05-01 (#5253): FIXED and verified.**
  - `SaveCommand::execute` refuses when `LoadedCellIndex` is present but neither context is (`save_io.rs:952-987`).
  - In production, `LoadedCellIndex` is inserted only at cell load; every other insert site is test-only.
  - Pinned by `a_save_with_an_index_but_no_context_is_refused_even_when_the_flag_is_stale`.
- **SAVE-D1-2026-10-05-01 (#5255): FIXED and verified.**
  - `impl_target_type` now matches qualified trait paths and generic headers.
  - The 8 types are allowlisted, and a reverse stale-row assertion was added.
  - The `ItemEventBatch` reason is corrected.
  - `clear_player_gear_handoff_scratch` drops both gear queues.
- **SAVE-D3-2026-10-05-01 (#5247): FIXED and verified.** `disk.rs:42-55`; the hidden temp is rejected by `parse_slot_filename`.
- **#5059** (shape-guard reach) and **#5060** (no source-order pin for the drain steps): both still open and unchanged.
- **GAME-D4-2026-10-05-01** (corpse restore before propagation): addressed by #5267 `a02a9e70e` (the teardown is now queued). Not re-traced in depth.

## Data-Loss Class Matrix

| Finding | Class | Dim | Severity | Status |
|---|---|---|---|---|
| SAVE-D5-2026-10-08-01: #3817 re-adoption stamps `CellRoot` on the riding player; the load teardown despawns the player, and pending un-rooted convoy entities survive the load as ghost twins | corruption-on-load / reference-break | 5 | HIGH | NEW |
| SAVE-D2-2026-10-08-01: the shape guard only reads the last attribute, so `StoryManagerNodeState`/`SmNodeRuntime` are unfingerprinted | irrecoverable-write (a future unbumped shape change makes same-major saves unloadable) | 2 | MEDIUM | NEW |
| SAVE-D1-2026-10-08-01: Eat/Sleep allowlist rows claim a `Seated` restore the live load never performs | none (stale reason; cosmetic redo) | 1 | LOW | NEW |
| SAVE-D2-2026-10-08-02: `snapshot.rs` bump ledger stops at v32 | none (doc-rot) | 2 | LOW | NEW |
| SAVE-D5-2026-10-08-02: `reconcile_worn_gear` is called twice in the drain | none | 5 | LOW | NEW |
| #5059 / #5060 | none (latent / test gap) | 2 / 5 | MEDIUM / LOW | Existing |
| GAME-D7-2026-10-08-01 (`StoryEventAliasFill`), GAME-D7-2026-10-08-02 (`StoryLocationCursor`), GAME-D5-2026-10-08-03 (`EatSleepState` re-pick), ECS D7-03 | see gameplay/ECS reports | 1 | — | Cross-ref |

## Completeness Ledger (delta from baseline; full table in `AUDIT_SAVE_2026-09-11.md`)

| Column / type | Kind | Saved | Overlaid | Notes |
|---|---|---|---|---|
| `DialogueSpokenInfoForms` (#5367) | Resource, `HashSet<u32>` INFO FormIDs | yes (v33) | n/a (wholesale) | Installed insert-if-absent by `dialogue::register`, so the restore is not clobbered. There is no `*_survives_save_load_round_trip` test (generic registry path). |
| `StoryManagerNodeState` (#5366 P3) | Resource, `HashMap<u32, SmNodeRuntime>` | yes | n/a | Installed insert-if-absent by `install_story_manager`, which runs inside the reload's populate walk before the second `restore_resources`. Pinned by `story_manager_node_state_survives_save_load_and_still_gates`. **Not fingerprinted** (SAVE-D2-01). |
| `StoryClock`, `StoryManagerRng`, `SmTree`, `StoryEvent`, `DialogueRandomState`, `DialogueQuestPriorities`, `ForceGreetDirective` | Resource / Component | no (allowlisted) | n/a | Reasons accurate. The `StoryClock` sync precedes dispatch (`boot/schedule/update.rs:73-85`). The `ForceGreetDirective` re-install-on-load behaviour belongs to GAME-D5-01. |
| `StoryLocationCursor`, `StoryEventAliasFill`, `EatSleepState` | | no | n/a | Reasons false; filed by gameplay as GAME-D7-02, GAME-D7-01 and GAME-D5-03. |
| `EatBehavior`, `SleepBehavior` | Component | no | n/a | The Seated half of the reason is false (SAVE-D1-01). |
| `CinematicReAdoption` (#3817) | Resource, `Vec<EntityId>` | no | n/a | Correctly unsaved, but the reason is false: it is never purged on session replace (SAVE-D5-01). |
| `ScriptKilledCorpseForms`, `LoadingCoverClock`, `RaceSpells`, `RunningQuestBoundCache`, `PrebakedHeadPart`, `PendingGearRelease`, `HudControl`, `ScriptProvider`, `TriggerOccupancyState`, `CellLoadPhaseTimings`, `GracefulExitRequested`, `SceneEffectSoftCache`, `ScaleformHudDiag` | | no | n/a | Reasons spot-checked; accurate. |

Two-list drift: none. `MUTABLE_DELTA_COLUMNS` is unchanged, and both new registrations are resources.

## Findings

### HIGH

#### SAVE-D5-2026-10-08-01: #3817's cinematic re-adoption stamps `CellRoot` on the process-lifetime player, so the next save-load teardown despawns the player; pending un-rooted convoy entities survive the load as `FormIdPair` ghost twins

- **Severity**: HIGH
- **Dimension**: Live Load-Apply & Frame Boundary (teardown completeness)
- **Data-Loss Class**: corruption-on-load / reference-break
- **Location**:
  - `byroredux/src/systems/cinematic.rs:489-498` (riders = every actor whose `ActorCinematicState.vehicle` is the cart);
  - `byroredux/src/systems/cinematic.rs:524-561` (`unplaced` = every released entity without `CellRoot`, queued on `CinematicReAdoption`);
  - `byroredux/src/systems/cinematic.rs:574-640` (`retry_cinematic_readoption`: no player exclusion; `world.insert(*entity, CellRoot(*root))` at `:619` plus a `CellRootIndex` push);
  - `byroredux/src/app_step.rs:83-90` (the retry runs every exterior streaming frame);
  - `byroredux/src/cell_loader/unload.rs:77-86` (`purge_cinematic_retention_state` removes only the two state components and never touches `CinematicReAdoption`);
  - `byroredux/src/save_io.rs:1581-1589` (exterior reload: purge, then `drain_streaming_state`);
  - `byroredux/src/save_io/registry_completeness_tests.rs:476` (allowlist reason).
- **Status**: NEW. This is a regression introduced by the #3817 fix (`63bf3347f`). It is distinct from ECS-2026-10-08-D7-01, which covers subtree nodes adopted by *local* Transform. It reopens hazard (2) of #5056 for released entities.
- **Trigger**: Skyrim MQ101 opening convoy. The player is a `SetVehicle` rider; the `unload.rs:1046` test and #5056 both model `vehicle = Some(cart)` on the player. The tether releases at the authored route terminal while riders are still attached; per the release doc, riders keep `cart_seat` for the later scripted exit. After that, the player does any in-process load (F9, the pause menu, console `load`) or a cell transition.
- **Description**:
  1. **The player is adopted into a cell.**
     - `release_finished_tethers` collects riders without excluding the player.
     - `release_set` walks their `Children`, which include the player body root.
     - `unplaced` keeps every member without a `CellRoot`. The player never has one: it is process-lifetime, and the body root is "never `CellRoot`-owned" (`player_body.rs:74`). So the player and its subtree are queued every time.
     - On the next streaming frame, `retry_cinematic_readoption` finds the player's world `Transform` in a loaded grid cell (by definition the player's own cell). It inserts `CellRoot(cell_root)` and registers the player in `CellRootIndex`.
  2. **The save load then destroys the player.**
     - `reload_exterior_session` runs `drain_streaming_state`, which unloads every loaded cell through `CellRootIndex` victims. The player is now one of them and is despawned, along with its physics body and GPU handles.
     - Nothing respawns it, because the live path relies on the player outliving the reload.
     - `PlayerEntity` dangles, and `build_form_id_remap` finds no live `PLAYER_FORM_ID_PAIR`. The player's saved `Inventory`, `ActorValues`, `CharacterController` and the rest go unresolved, and #5054's `park_unresolved_snapshot_rows` parks them as a `ReferenceState` that nothing will ever respawn.
     - `apply_player_pose` has no body.
     - The same despawn happens without any load, as soon as the player walks out of the arrival cell's ring or takes a door.
  3. **Pending entities survive the load.**
     - Released entities that are still pending have no `CellRoot`, and no state component, since release cleared it. Examples: the horse that drove past the loaded ring (the #3817 test asserts it "stays pending"), and subtree nodes per ECS-D7-01.
     - The purge does not touch them and the teardown cannot enumerate them, so they survive the load.
     - The reload spawns fresh copies of the same REFR/ACHR, and ghost and fresh copy share a `FormIdPair`. `build_form_id_remap`'s `HashMap` collect then keeps one at random, which is #5056's hazard (2).
     - The un-purged pending list then adopts the ghost into the loaded session.
     - The allowlist reason ("a save/session replacement tears the whole world down anyway (purge_cinematic_retention_state)") is false on both counts.
- **Evidence**: `let unplaced: Vec<EntityId> = { let roots = world.query::<CellRoot>(); release_set.iter().filter(|e| roots…get(**e).is_none())… }` → `pending.pending.extend(unplaced)`. The retry runs `world.insert(*entity, CellRoot(*root)); idx.map.entry(*root).or_default().push(*entity)` with no `PlayerEntity` check. `rg 'Player' cell_loader/unload.rs` finds nothing, so the unload has no player guard. `tether_releases_at_the_authored_route_terminal_and_detaches_riders` uses a non-player rider, so it cannot see this.
- **Impact**: after the Helgen convoy arrives, every later in-process load produces a session with no player. Workaround: restart the process and fresh-load. Separately, every later streaming unload of the arrival cell deletes the player, which is the gameplay-breaking half and is owned by `/audit-gameplay` or `/audit-scripting`. Saves already on disk are intact.
- **Related**: #3817 and #5056 (both closed); ECS-2026-10-08-D7-01; #3254.
- **Suggested Fix**:
  - Exclude the player and its body subtree from `unplaced`; the process-lifetime player must never be cell-owned. Add a player-rider case to the #3817 test that asserts the player has no `CellRoot` after the retry.
  - In `purge_cinematic_retention_state`, also despawn (or hand to the teardown) every entity on `CinematicReAdoption.pending`, then clear the list.
  - Fix the allowlist reason.

### MEDIUM

#### SAVE-D2-2026-10-08-01: the shape-fingerprint guard reads only the last `#[…]` attribute before a declaration, so `StoryManagerNodeState` and `SmNodeRuntime` (save derive placed above a plain `#[derive(Debug…)]`) are invisible to it

- **Severity**: MEDIUM
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: irrecoverable-write (latent). A future shape change without a bump makes same-major saves fail the typed preflight.
- **Location**:
  - `byroredux/src/save_io/serde_default_guard_tests.rs:241-253` (`prior.rfind("#[")`, then the derive/Serialize test on that one span);
  - `crates/scripting/src/story_manager.rs:161-181` (`#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]` followed by `#[derive(Debug, Clone, Default, PartialEq)]` on both types).
- **Status**: NEW. It is a different mechanism from #5059: that issue is about files outside the scan, while this one is about types inside a scanned file. Same family as the closed #3164.
- **Description**:
  - `normalized_serialized_shapes` takes the *nearest* `#[` above each `struct`/`enum` line and requires that one span to contain both `derive` and a Serialize needle.
  - For both new Story Manager types, the nearest span is the plain `#[derive(Debug, Clone, Default, PartialEq)]`, so they are skipped. `story_manager.rs` is in `save_type_sources()` (it carries a `cfg_attr(feature = "save"` derive and defines a registered type), but it contributes no shape.
  - Empirical confirmation: `26b6a779c` added two serde-derived types to a scanned file and did not refresh `BASELINE_SHAPE_FINGERPRINT`, yet the guard stayed green. Compare `14cff35ae`: `DialogueSpokenInfoForms` has the derives in the other order, and that commit had to refresh.
  - A Python emulation of the guard's attribute test over every workspace `.rs` file finds these two as the only in-scan saved types with the blind ordering. The other hits are sdk and inspect-only types outside the scan, the known #5059 territory.
- **Impact**: a field added to, retyped in, or newly `Option`-wrapped inside `SmNodeRuntime` or `StoryManagerNodeState` passes every guard without a `FORMAT_MAJOR` bump. The `serde(default)` guard would still catch a defaulted field. Saves of the same major then fail `validate_snapshot_types` with a decode error instead of a clean version refusal. Any future type that copies this attribute order joins the blind spot silently.
- **Suggested Fix**:
  - Treat the whole contiguous attribute block above the declaration as `between`, not just the last `#[`.
  - Add a coverage assertion: every registered type defined in a scanned file must contribute a shape.
  - Refresh the baseline in the same commit, as a refresh without a bump with a justification comment. The two types join the hash and have not changed shape since registration.

### LOW

#### SAVE-D1-2026-10-08-01: Eat/Sleep allowlist reasons claim "the seated pose … carries via the registered Seated restore", but `Seated` is excluded from the live overlay

- **Severity**: LOW
- **Dimension**: Snapshot Completeness & the Two Lists
- **Data-Loss Class**: none (stale reason; cosmetic redo)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:568-570`; `byroredux/src/save_io.rs:113-128` (`Seated` deliberately absent from `MUTABLE_DELTA_COLUMNS`) and `:435-453` (register comment: the full round trip is `restore_world`-only).
- **Status**: NEW. Distinct from GAME-D5-2026-10-08-03, which concerns `EatSleepState`'s "idempotent" destination claim; this one is the `Seated` claim on all three rows.
- **Description**: `Seated` is registered but never replayed by `execute_pending_save_loads`, the only production load path. A diner or sleeper seated at save time therefore respawns standing. It re-walks and re-seats through `eat_sleep_system`, which is exactly the "silently redo its Seat behavior" outcome that the `Seated` registration comment says registration prevents. The guard checks only that a reason exists, so the false mechanism stays green.
- **Suggested Fix**: Restate the three reasons as "re-derived: the actor re-walks and re-seats after a live load (`Seated` is `restore_world`-only)". Alternatively, add a FormID-keyed seated ledger if the redo is unwanted.

#### SAVE-D2-2026-10-08-02: the `FORMAT_MAJOR` bump ledger in `snapshot.rs` stops at v32 while the constant is 33

- **Severity**: LOW
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: none (doc-rot)
- **Location**: `crates/save/src/snapshot.rs:246-262`; v33's rationale exists only in `serde_default_guard_tests.rs:739-743`.
- **Status**: NEW
- **Description**: `14cff35ae` changed only the constant line. The skill and the crate doc treat this comment as *the* bump ledger. The v33 note in the guard comment also says pre-v33 saves can "legitimately re-qualify" a line after load, but pre-v33 saves are rejected outright, by both the major and the schema fingerprint.
- **Suggested Fix**: Add a `v32 -> v33 (#5367 Phase L): new saved resource DialogueSpokenInfoForms …` entry, and correct the guard comment's wording.

#### SAVE-D5-2026-10-08-02: `reconcile_worn_gear` is called twice back-to-back in the load drain

- **Severity**: LOW
- **Dimension**: Live Load-Apply & Frame Boundary
- **Data-Loss Class**: none
- **Location**: `byroredux/src/save_io.rs:1932-1934` (merge slip in `6b494d002`).
- **Status**: NEW
- **Description**: The #5255 edit inserted a new `reconcile_worn_gear` line above the existing one. The second call is idempotent: visibility already agrees, and `queue_midlife_imports` skips a wearer that already has a pending import. The cost is a second `NpcEquipmentPart` scan and subtree walks per load. The new `load_clears_both_player_gear_handoff_queues` test calls the helper directly, so the drain wiring stays unpinned (#5060).
- **Suggested Fix**: Delete one call. Fold the helper into #5060's source-order pin.

## Notes (not filed)

- `atomic_temp_path` names are unique, so a crash mid-stage leaves an orphan `.save_N.ess.<pid>.<n>.tmp` that no later save overwrites. Previously the fixed name was reused. Orphans grow only across crashes.
- `StoryManagerNodeState` and `DialogueSpokenInfoForms` key on load-order-resolved global `u32` FormIDs and positional pool indices, the same posture as `QuestStageState`. A load-order change misaligns pools in the safe direction, per the type's own doc.
- `DialogueSpokenInfoForms` has no dedicated round-trip test. It rides the generic `register_resource` path.

## Guards Verified

| Guard | State |
|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` (+ #5255 reverse stale-row check, `qualified_impl_paths_are_discovered`) | green. Reasons spot-checked (about 20 new rows); 3 false, as above. |
| `serde_default_on_saved_struct_requires_format_major_bump` | green |
| `saved_type_shape_changes_require_format_major_bump` | green. `BASELINE_MAJOR = 33 = FORMAT_MAJOR`, refreshed with the bump in `14cff35ae`. The two refreshes without a bump (#5293, #4415) are justified file-sweep cases. Blind spot: SAVE-D2-01. |
| `set_in_chargen_renames_still_decode_v23_keys`, `quest_revision_keys_never_reach_a_save` | green |
| `delta_columns_*` (2), `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` | green |
| `a_save_with_an_index_but_no_context_is_refused_even_when_the_flag_is_stale` (new), mid-transition/chargen refusals, ring-cursor, quiescent drain, quickload fallback | green |
| `load_clears_both_player_gear_handoff_queues`, `story_manager_node_state_survives_save_load_and_still_gates` (new) | green |
| `write_slot_has_no_clobber_fallback` (new) and `crates/save` header/CRC/atomic/ring tests | green |

Commands run (`TMPDIR=/mnt/data/tmp`; no engine binary or GPU process was launched):
- `cargo test -p byroredux-save`: 41 + 15 passed.
- `cargo test -p byroredux --bin byroredux save_io` (toolchain 1.96.0): 74 passed, 4 ignored. The ignored tests are exactly the 4 real-master `consumable_tests`.
- A guard-emulation script (`/tmp/audit/save/blind.py`, read-only) for SAVE-D2-01.

## Summary per Dimension

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & the Two Lists | 1 LOW (+ the CinematicReAdoption reason, folded into D5-01) | #5255 is verified fixed. No two-list drift. |
| 2 — Format & Schema Discipline | 1 MEDIUM, 1 LOW | v33 is sound. The attribute-order blind spot hides the SM types. The ledger is missing v33. |
| 3 — Container & Disk Durability | 0 | #5247 is verified fixed. |
| 4 — Save-Side Gates | 0 | #5253 is verified fixed. |
| 5 — Live Load-Apply & Frame Boundary | 1 HIGH, 1 LOW | #3817 adopts the player into a cell, and the load teardown then despawns it. |

Suggested next step: `/audit-publish docs/audits/AUDIT_SAVE_2026-10-08.md`. Use the domain label `save-load` throughout. Add `gameplay` and `game:skyrim` for SAVE-D5-01, `test-gap` for SAVE-D2-01, and `doc-rot` for SAVE-D2-02 and SAVE-D1-01.
