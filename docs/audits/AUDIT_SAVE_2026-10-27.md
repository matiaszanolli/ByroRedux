# Save / Load Subsystem Audit (M45 + M45.1) — 2026-10-27

**HEAD**: fa6be4a6b · **Baseline**: `docs/audits/AUDIT_SAVE_2026-10-21.md` (HEAD `4a3e89d56`) · **Audited**: Dimensions 1, 2, 4, 5 (delta-first; 395 commits in `4a3e89d56..HEAD`) · **Unchanged since baseline (skimmed)**: Dimension 3 (Container & Disk). In the window, nothing touched `crates/save/src/{disk,lib,driver,registry}.rs`, `crates/core/src/atomic_file.rs` or `crates/save/Cargo.toml`. `snapshot.rs` changed in three ways only: the `FORMAT_MAJOR` constant, its ledger doc, and the new ledger-contiguity pin (#5459).

This run is part of `/audit-suite --preset comprehensive`. I analysed every dimension myself, synchronously, with no sub-agents. Scratch notes are in `/tmp/audit/save/dim_{1..5}.md`. The only commands run were `cargo test` (repo `target/`, `TMPDIR=/mnt/data/tmp`). I started no engine, smoke script or GPU process.

Save-relevant changes in the window:
- **All six 10-21 findings are fixed.**
  - -01: #6237 `6765e60eb`. `RigidBodyData` left the registry, and the `ReferenceMotionTypeState` FormID ledger took its place.
  - -02: #6238 `078df54e1`.
  - -03: #6239 `337bf25c8`.
  - -04: #6294 `8fc521650` / `f3ab49f55`.
  - -05: #6295 `a0fbf0113`.
  - -06: #6296 `5a3f571f4`.
- **`FORMAT_MAJOR` went from 115 to 119:**
  - v116: #6237.
  - v117: #6190, the `ObScriptLedger` prune.
  - v118: Oblivion jail fields on `ObScriptLedger`.
  - v119: the `CrimeKnowledge` resource.
- **#5802 slice `0f11a98e9`**: linked refs, animation archetypes, scanning and form lists now register through their own `register_saved` delegates.
- **New gameplay state** in the window: the crime system, AI packages (Find / Ambush / UseWeapon / FleeNotCombat), the VATS mode machine, and PlayGroup swaps. Every new `Component`/`Resource` is classified.

Dedup sources:
- `/tmp/audit/open_issues_snapshot.txt` (84 open).
- `/tmp/audit/SUITE_RULES.md` § 6, read before writing and again before finalising. No overlap.
- `gh issue list --state all` search for `PendingEvidenceDeposits` / `VatsState`. The only hit is the #6350 tracker, whose body does not mention save or load.

## Executive Summary

| `lib.rs` / `snapshot.rs` design claim | Status |
|---|---|
| Full ECS snapshot; the registry is the completeness contract | **CODE-CONFIRMED.** The guard is green. All 28 new `Component`/`Resource` impls in the window are registered XOR allowlisted. `RigidBodyData` stays out, and the removed-from-allowlist set is still registered. Two allowlist *reasons* are false (SAVE-2026-10-27-01). |
| Versioned container, CRC, atomic write, ring | **CODE-CONFIRMED (unchanged).** The 40 + 15 `byroredux-save` tests pass. `SaveState::new` uses `SaveRing::resume` (`save_io/registry.rs:202`). |
| `FORMAT_MAJOR` bump discipline | **CODE-CONFIRMED.** `FORMAT_MAJOR == BASELINE_MAJOR == 119`. Four of the five shape-baseline refreshes bump in the same commit. The fifth, `4156e68cd`, is the documented `serde_pairs` no-bump precedent. The ledger is contiguous and pinned. The schema fingerprint is pinned (`0x28a8_2a40_9627_56e4`). One guard blind spot is new (SAVE-2026-10-27-02). |
| "Every load behaves like a fresh boot" (#5435 / #5864 / #6083 / #6238 / #6239) | **DRIFTED** for two transients added after #6239 (SAVE-2026-10-27-01). |
| Live-load ordering | **CODE-CONFIRMED.** `execute_pending_save_loads` (`live_load.rs:490-841`) matches the specified sequence step for step, including the #6087 `ScriptPackage` pre-seat. |

**Findings: 2 NEW. 0 CRITICAL, 0 HIGH, 0 MEDIUM, 2 LOW.**

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 2 | SAVE-2026-10-27-01, SAVE-2026-10-27-02 |

## Data-Loss Class Matrix

| Finding | Severity | Class |
|---|---|---|
| SAVE-2026-10-27-01 | LOW | Save side: silent-drop (confiscated goods never reach the jail chest). Load side: corruption-on-load (goods duplicated into the loaded chest; the world comes up frozen in VATS). |
| SAVE-2026-10-27-02 | LOW | none today. Latent corruption-on-load: a `MotionType` variant change would make saves holding a motion override unloadable, with no CI signal. |

## Completeness Ledger

**Registry × `MUTABLE_DELTA_COLUMNS`.** This is a cross-check against the guard's lists.

| Class | Columns |
|---|---|
| SAVED + OVERLAID (30) | The 10-21 set minus `RigidBodyData`: Transform, Inventory, EquipmentSlots, LightSource, LightFlicker, ScriptTimer, TwoStateActivator, ScriptVariables, ActorValues, Perks†, CharacterLevel†, LevelUpPoints†, TimedRestorations†, SpellList, EquippedWeapon, Dead†, WanderState, TravelState, Traveled, GuardState, PatrolState, Escorted, ActorControlState†, Bleedout†, ScriptPackage†, WeaponDrawn†, IgnoreCrime†, ActiveMagicEffects†, CharacterController, RumbleOnActivate. († = replacing.) |
| SAVED only | Unchanged: Name, Parent / Children, AnimationPlayer / AnimationStack, ActorVitals, FollowState / EscortState / Seated, the cinematic pair, Material. |
| NOT_SAVED (moved there) | `RigidBodyData`, since v116. Its job is now done by `ReferenceMotionTypeState`, which is in `PRE_RELOAD_RESOURCES` (`live_load.rs:44`) and has a `LiveReloadLedgers` field. |
| Resources added | `ReferenceMotionTypeState` (`registry.rs`) and `CrimeKnowledge` (`crime_knowledge::register_saved`; a `BTreeSet` keyed by FormID). The `0f11a98e9` delegates kept their rows' positions; the fingerprint pin is green. |

**Third list** (`NOT_PARKED` partition): `UNAUDITED` is now rejected (#6294), and `Escorted` is parked in the stream snapshot.

**`NOT_SAVED_BY_DESIGN` reasons spot-checked** (the 28 new types):
- **True**:
  - AI-package config: AmbushBehavior, FleePackageBehavior, FindPackageBehavior, FindProgress, UseWeaponPackageBehavior, UseWeaponPackageStarted.
  - Load-order derived: CrimeFactionData, CrimeModel, GuardClasses, JailTable, QuestStageResultScripts, VatsModel.
  - Spawn-derived: HeadPartRoot, SpawnIdle, UnloadedReferenceStub, RetiredBodyRows, WaterBedField.
  - Per-frame transients: RootMotionYaw, VatsClock, PendingGroupSwap.
  - StoryLocationMemo: a pure function of the context.
  - InstalledOutdoorEnvironment: the reload re-installs it, and the interior forced-weather step self-heals against the restored request.
  - AssaultedByPlayer: the reason states its post-load re-charge honestly.
- **False**: `PendingEvidenceDeposits` ("drained within the frame") and `VatsState` ("dies with the session"). Both are covered in SAVE-2026-10-27-01.

## Findings

### SAVE-2026-10-27-01: Two transients added after #6239 escape the load's outgoing-session sweep, and their allowlist reasons are false (`PendingEvidenceDeposits`, `VatsState`)
- **Severity**: LOW
- **Dimension**: Live Load-Apply (frame boundary) / Snapshot Completeness (allowlist reasons)
- **Data-Loss Class**: Save side, silent-drop. Load side, corruption-on-load.
- **Location**:
  - `byroredux/src/save_io/live_load.rs:459-481` (`discard_outgoing_session_requests`) and `:628-644` (the discard block). Neither type is touched there.
  - `crates/scripting/src/jail.rs:62-67`, `:210-215` (`confiscate_stolen_goods` pushes the deposit).
  - `byroredux/src/crime.rs:706-719` (`evidence_deposit_system`).
  - `byroredux/src/boot/schedule/update.rs:322` (the consumer) versus `:577` (`obscript_stage_result_system`).
  - `byroredux/src/systems/npc_dialogue.rs:472`, `:484` (dialogue result scripts run inside the Late selection system, `late.rs:549`).
  - `byroredux/src/components/vats.rs:58-100`; `byroredux/src/boot/world.rs:37` (the only production `VatsState` reset, at boot).
  - `byroredux/src/app_events.rs:1078-1099` (F5/F9 intercept, not gated on VATS).
  - Allowlist rows: `registry_completeness_tests.rs:454` (`VatsState`) and `:734` (`PendingEvidenceDeposits`).
- **Status**: NEW. Same class as closed #6239 / #6238 / #6083 / #5435.
- **Description**: #6239 cleared the value-keyed request queues that existed on 10-21. Neither the sweep nor a guard catches the two transients that landed after it.
  1. **`PendingEvidenceDeposits`** (`98d6eedfe`) holds `(chest FormID, item FormID, count)`.
     - **Producers**: `PayFine` and `GoToJail` push the deposit. They run from INFO result scripts inside the Late dialogue-selection system, or from quest-stage result scripts in an Update exclusive registered after `evidence_deposit_system`. Either way the consumer runs on the *next* frame's Update, so the queue always straddles the frame boundary where the save and load drains run. The reason "drained within the frame that queued them" is false.
     - **Save at that boundary**: the queued deposit is dropped. The ledger's `evidence` is saved, but nothing re-queues it (`ObScriptLedger::evidence()` has no production reader). The confiscated goods therefore never reach the `StolenGoods` chest, and the player can no longer recover them.
     - **Load at that boundary**: the outgoing deposit is FormID-keyed, so it resolves in the loaded world. `deposit_into_reference` then adds the goods to the loaded world's chest (directly or into its parked row), and `release_evidence` decrements the *loaded* ledger. The next save persists both.
  2. **`VatsState`** (`818eba7f4`, #6350) holds the mode, the target list and the queue, all keyed by `EntityId`.
     - `execute_pending_save_loads` never resets it, and F9 is not gated on VATS mode.
     - An in-process quickload taken inside VATS brings the loaded session up in `TargetSelect`. `world_time_scale()` is 0, so the world is frozen. The target and queue lists still name the outgoing session's despawned entities.
     - Pressing the VATS key exits. The console `vats.confirm` instead charges the loaded player's AP and inserts `HitEvent`s on those stale ids.
     - A cold-start load cannot do either of these, so it diverges from an in-process load. That breaks the #5435 contract.
- **Evidence**:
  - `grep -n 'Vats\|Evidence' byroredux/src/save_io/live_load.rs` finds nothing.
  - `exit_all` has one production caller, `vats_mode_system` (`systems/vats.rs:44`), and it runs only on the VATS key press.
  - `update.rs:320-331` registers the consumer as an Update exclusive at `:322`. Exclusives sequence in registration order, and the stage-result runner is registered at `:577`.
- **Impact**:
  - The evidence leg needs a save or load to drain on the exact frame `PayFine` or `GoToJail` ran. On the dialogue path F5/F9 are blocked while the page is open, so the reachable triggers are the console `save`/`load` and a stage-result `PayFine`. That narrowness is why this is LOW.
  - The VATS leg needs one keypress (F9 while in VATS). The visible effect is a frozen post-load world until the player presses V. It loses progress only through the console confirm path.
  - The suggested generic guard from 10-21 -03 was never added: list every `NOT_SAVED_BY_DESIGN` row whose reason says "drained" or "consumed", and require each to be discarded in the drain or justified as entity-keyed. These two are the cost of that.
- **Related**: #6239, #6238, #6083, #5435, #6350, GAME-D4-2026-10-27-02 (a different VATS gate gap).
- **Suggested Fix**:
  - Clear `PendingEvidenceDeposits` in `discard_outgoing_session_requests`.
  - Reset `VatsState` there too (`exit_all()`, or `VatsState::default()`, keeping `roll_state` if determinism wants it). Consider refusing F5/F9 while `VatsState.mode != None`.
  - Correct both allowlist reasons.
  - Extend `live_load_drain_discards_the_outgoing_sessions_requests` with the two types.
  - Add the generic "drained-reason ⇒ discarded or entity-keyed" source guard.

### SAVE-2026-10-27-02: #6237 dropped `collision.rs` out of the shape scan, leaving `MotionType`, the payload of the saved `ReferenceMotionTypeState`, unfingerprinted
- **Severity**: LOW
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: none today. Latent corruption-on-load.
- **Location**:
  - `byroredux/src/save_io/serde_default_guard_tests.rs:106-136` (`save_type_sources` retain arms, plus the explicit nested-payload list).
  - `crates/core/src/ecs/components/collision.rs:68-70` (`MotionType`, an `inspect`-gated derive).
  - `crates/scripting/src/fragment/state.rs:378-382` (`ReferenceMotionTypeState { overrides: HashMap<u32, MotionType> }`).
- **Status**: NEW. This is the residue class of closed #5059.
- **Description**:
  1. `save_type_sources()` keeps a file only if it contains `cfg_attr(feature = "save"` or defines a registered type name.
  2. Before v116, `collision.rs` qualified through the registered `RigidBodyData`. #6237 unregistered that type, and every derive in the file is `inspect`-gated, so the file silently left the scan. The v116 baseline refresh absorbed the move along with the intended change.
  3. `MotionType` is now serialized inside the newly registered `ReferenceMotionTypeState`. Its file is outside both serde guards, and it is not on the explicit nested-payload list that #5059 created for exactly this shape (`lighting.rs`, the `sdk` files).
- **Impact**:
  - Renaming or removing a `MotionType` variant changes no fingerprint, and no `FORMAT_MAJOR` bump is demanded.
  - Every save holding a scripted `SetMotionType` override would then fail the typed preflight ("save load ABORTED"), which leaves that save unloadable.
  - The enum is small and stable, which is why this is LOW.
- **Suggested Fix**: add `manifest.join("../crates/core/src/ecs/components/collision.rs")` to the explicit list, with a #6237 comment, and refresh the baseline without a bump, because no saved shape changed. Optionally, add a coverage marker asserting that `MotionType` contributes a shape, as #5403's markers do for `StoryManagerNodeState`.

## Dimension notes

- **Dimension 1**:
  - Every prior finding is fixed in place: `discard_pending_cell_resets` (`live_load.rs:643`), `discard_outgoing_session_requests` (`:644`), the `ReferenceMotionTypeState` pre-reload restore with its `LiveReloadLedgers` reinstate (`:587-626`), the `UNAUDITED` rejection, the duplicate-name assertion, and the fingerprint pin.
  - `CrimeKnowledge` and the new `ObScriptLedger` jail fields carry only u32 FormIDs.
  - Other entity-keyed request queues on the allowlist (for example `PendingScriptKills`, `PendingStageChanges`, `PendingMagicHits`) name outgoing `EntityId`s that are not recycled, so stale rows do not resolve. The one exception is the process-lifetime player id, and it is not filed.
- **Dimension 2**:
  - In the window, the crime commits touching `crates/core/src/character/crime.rs` after `CrimeKnowledge` registered (`99051a4eb`, `4cfcee4b9`, `870722b99`) changed only `CrimeModel`, which has a plain derive. `CrimeKind`'s body is unchanged.
  - The `0f11a98e9` slice moved registration lines, not type definitions, so the shape hash is unmoved. That is correct.
  - The extension-state payload (`crates/sdk/src/component.rs`) is unchanged.
- **Dimension 4**:
  - `commands.rs` is unchanged in the window.
  - Every ingress still goes through `queue_player_save_action`: the F5/F9 intercept, the pause menu (`main.rs:1705/1716`), debug `input.press` (`interaction.rs:608`) and EssentialDeathReload (`script_requests.rs:398`).
  - The newly registered types hold no `EntityId`, so `validate_saved_entity_references` needs no new rows.
- **Dimension 5**:
  - The drain matches the specified sequence.
  - Every failure arm after the reload returns before `apply_player_pose`.

## Guards Verified

- `cargo test -p byroredux-save`: 40 unit + 15 `round_trip` passed, 0 failed. This includes `the_bump_ledger_is_contiguous_and_current` and `typed_snapshot_preflight_rejects_bad_column_without_world_mutation`.
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
- `app_step::tests::the_save_drain_publishes_the_transition_flag_before_draining`: passed.
- Guard limits found this run:
  - The outgoing-session discard test enumerates a fixed set, so new queues are invisible to it (-01).
  - The shape scan's file discovery depends on a type staying registered (-02).
- Not checkable here: any engine, smoke or Vulkan leg, and the `#[ignore]`d game-data tests. Both findings are static, with no live reproduction.
