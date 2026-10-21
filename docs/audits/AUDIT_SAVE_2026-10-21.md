# Save / Load Subsystem Audit (M45 + M45.1) — 2026-10-21

**HEAD**: 4a3e89d56 · **Baseline**: `docs/audits/AUDIT_SAVE_2026-10-12.md` (HEAD `5d31fd637`, the last full run). The area-scoped `AUDIT_SAVE_2026-10-19.md` (HEAD `41f9c2c6c`) was used only to dedup and to re-check its findings. · **Audited**: Dimensions 1, 2, 4, 5 (delta-first). · **Unchanged since baseline (skimmed)**: Dimension 3 (Container & Disk). Over `5d31fd637..HEAD`, `crates/save/src/{lib,disk,driver}.rs`, `crates/save/Cargo.toml` and `crates/core/src/atomic_file.rs` have no commits, and `snapshot.rs` changed only the `FORMAT_MAJOR` constant and its ledger doc.

This run is part of `/audit-suite --preset comprehensive`. I analysed every dimension myself, one after another, with no sub-agents; scratch notes are in `/tmp/audit/save/dim_{1..5}.md`. I ran nothing except `cargo test` (repo `target/`, `TMPDIR=/mnt/data/tmp`). No engine, smoke script or GPU process was started.

The window holds about 230 commits. The save-relevant ones are:

- **`FORMAT_MAJOR` 38 → 115.** Almost all of these bumps are M47.2/M47.3 scripting state: new saved resource columns, and new `Effect` / `FragmentGuard` / `ValueExpr` shapes reachable through `FragmentExecutionQueue`.
  - v113 (#4256) `Material.source_shader_type`.
  - v114 (#6071–#6074) and v115 (#6085): the eviction row `ReferenceState` / `ScriptState` gained new fields.
- **#5802 slices** (`bde3478db`, `e619dbbaf`): eleven subsystems now register their own saved rows through `register_saved`.
- **#6081 / #6083 / #6085** fixed the 10-19 findings SAVE-SD-01 / 02 / 03.
- **#5686** added the `register_component_preserving` hook for `RigidBodyData`.

Dedup sources:
- `/tmp/audit/issues.json` (119 open).
- `/tmp/audit/SUITE_FINDINGS.md` (read twice: before writing and before finalising).
- Closed-issue searches: `SetMotionType`, `RigidBodyData overlay`/`remap`, `PhysicsSourceForm save`, `outgoing session`, `PlayerMoveRequest load`, `save during dialogue`, `quicksave menu`, `GetButtonPressed save`.

## Executive Summary

| `lib.rs` / `snapshot.rs` design claim | Status |
|---|---|
| Full ECS snapshot; the registry is the completeness contract | **CODE-CONFIRMED** for classification: the guard is green and every `Component`/`Resource` impl is registered XOR allowlisted. **DRIFTED** for one overlaid column: `RigidBodyData` is captured and listed in `MUTABLE_DELTA_COLUMNS`, but no production row can ever be remapped (SAVE-2026-10-21-01). |
| Versioned container, CRC, atomic write, ring | **CODE-CONFIRMED (unchanged)**: 39 + 15 `byroredux-save` tests pass, and `SaveState::new` uses `SaveRing::resume` (`save_io/registry.rs:206`). |
| `FORMAT_MAJOR` bump discipline | **CODE-CONFIRMED.** `FORMAT_MAJOR == BASELINE_MAJOR == 115`. Every bump since 10-12 refreshed the baseline in the same commit. Every refresh without a bump (about 30) carries a justification comment, and I spot-checked four of them. The v32→v33 ledger gap is still open (#5459). |
| "Every load behaves like a fresh boot" (#5435 / #5864 / #6083) | **DRIFTED** in two more places: the reload records cell visits against the outgoing session's clock (SAVE-2026-10-21-02), and transient request queues straddle the drain (SAVE-2026-10-21-03). |
| Live-load ordering (#4815 re-seat before the overlay) | Still **DRIFTED for script-package winners**: #6087 (SAVE-SD-04) is open and unchanged (`live_load.rs:685` re-seats before `apply_deltas` at `:703`). |

**Findings: 6 NEW. 0 CRITICAL, 0 HIGH, 3 MEDIUM, 3 LOW.**

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 3 | SAVE-2026-10-21-01, -02, -03 |
| LOW | 3 | SAVE-2026-10-21-04, -05, -06 |

Prior findings re-verified in place:
- #6081 (SAVE-SD-01): `LiveReloadLedgers::capture` runs before the subset restore, with `reinstate` on both abort arms (`live_load.rs:567-579`, `:597-606`).
- #6083 (SAVE-SD-02): `discard_pending_unload_deliveries` (`:620`) and `rebaseline_actor_watch` (`:761`).
- #6085 (SAVE-SD-03): the third-list partition guard `every_delta_column_is_parked_or_explicitly_not`.
- #5530: pose refusal.
- #5686: the preserve hook. It is present, but in production it is vacuous (see -01).

Still open and not re-reported: #6087 (SAVE-SD-04), #5459 (ledger gap), #5802 (hub growth).

## Data-Loss Class Matrix

| Finding | Severity | Class |
|---|---|---|
| SAVE-2026-10-21-01 | MEDIUM | silent-drop (scripted `SetMotionType` state on placed references never survives a live load) |
| SAVE-2026-10-21-02 | MEDIUM | corruption-on-load (a spurious `OnReset` mutates the loaded session's script state, which the next save persists) |
| SAVE-2026-10-21-03 | MEDIUM | corruption-on-load (the outgoing session's player MoveTo / XP / skill grants are applied to the loaded session) |
| SAVE-2026-10-21-04 | LOW | silent-drop on eviction (not on save/load) |
| SAVE-2026-10-21-05 | LOW | none (allowlist hygiene) |
| SAVE-2026-10-21-06 | LOW | none (a guard gap; a reorder would reject every save, not corrupt one) |

## Completeness Ledger

**Registry × `MUTABLE_DELTA_COLUMNS`.** This is a cross-check against the guard's lists, not a fresh derivation.

| Class | Columns |
|---|---|
| SAVED + OVERLAID (31) | Transform, Inventory, EquipmentSlots, LightSource, LightFlicker, ScriptTimer, TwoStateActivator, ScriptVariables, ActorValues, Perks†, CharacterLevel†, LevelUpPoints†, TimedRestorations†, SpellList, EquippedWeapon, Dead†, WanderState, TravelState, Traveled, GuardState, PatrolState, Escorted, ActorControlState†, Bleedout†, ScriptPackage†, WeaponDrawn†, IgnoreCrime†, ActiveMagicEffects†, CharacterController, RigidBodyData (preserving; **never matches in production**, -01), RumbleOnActivate. († = replacing.) |
| SAVED only (documented at their `register_component` site) | Name (FixedString), Parent / Children (structural), AnimationPlayer / AnimationStack (`root_entity` / clip handles), ActorVitals (write-once key), FollowState / EscortState / Seated (`EntityId`, registry handle), ActorCinematicState / HorseTetherState (cinematic pair), Material (blast radius) |
| Structural | FormIdComponent (`register_form_id_component`) |
| Resources | about 75 rows, 11 of them through the #5802 `register_saved` delegates. `e619dbbaf` kept every row's position (its diff moves no row; fingerprint `889b2c5b41ef75de` before and after, per its message). |

**Third list** (`ReferenceState` / `ScriptState` partition, #6085): 17 columns parked, 3 carried by the stream snapshot, 11 on `NOT_PARKED`. Nine of those 11 are still marked `UNAUDITED` (SAVE-2026-10-21-04).

**`NOT_SAVED_BY_DESIGN` reasons spot-checked** (about 170 rows were added since 10-12):
- True: `TerrainHeightService`, `TerrainLayerHeights`, `PersistentCellWindow`, `FallbackActorCollider`, `PlayableRaces`, `ActorWatch`, `PendingUnloadDeliveries`, `ObScriptQuestTimers`, `ChargenWatch`, `ScriptAlphaApplied`, `GameClockGlobals`, `AiSuspended`, `ActivePackages`, `DecompiledPexCache`.
- `MessageBoxQueue` / `ButtonPresses`: true in practice. F5/F9 cannot reach the save queue while any game-menu page is open (`app_events.rs:424-443` returns early for every key but Escape/Inventory), and a pending box re-opens its page the next frame.
- False: `PlayerMoveRequest`, `PendingSkillAdvances` ("drained the same frame") and `PendingCellResets` (see -02 and -03).
- Overstated: `ActiveBarter` (-05).

## Findings

### SAVE-2026-10-21-01: The `RigidBodyData` delta column can never overlay a placed reference, so a scripted `SetMotionType` is lost on every live load (the #2379 fix only covers `restore_world`)
- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness & the Two Lists
- **Data-Loss Class**: silent-drop
- **Location**:
  - `byroredux/src/save_io/registry.rs:134` (`"RigidBodyData"` in `MUTABLE_DELTA_COLUMNS`) and `:397` (`register_component_preserving`).
  - `crates/save/src/driver.rs:273-282` (`build_form_id_remap` keys only `FormIdComponent`).
  - `byroredux/src/cell_loader/spawn.rs:1488-1497`, `:1545-1577`, `:288-306`, `:419-427` (every collision entity carries `PhysicsSourceForm`, never `FormIdComponent`).
  - `byroredux/src/systems/cinematic.rs:436-445` (the `SetMotionType` writer).
  - `byroredux/src/save_io/round_trip_tests.rs:766-814` (the #2379 regression test).
- **Status**: NEW (the fix for closed #2379 is incomplete; related #5686)
- **Description**:
  1. The live load overlays a column only onto entities that `build_form_id_remap` resolves, and it resolves only entities carrying a `FormIdComponent`.
  2. Every production `RigidBodyData` carrier for a placed reference is a separate collision entity: per-shape bhk colliders, graph-node bodies, the packed proxy ghost and the trimesh ghost. Each is stamped with `PhysicsSourceForm`, which is deliberately not `FormIdComponent`, so that `find_by_form_id` stays unambiguous (the `spawn.rs:1560-1571` comment).
  3. These rows are therefore captured by `save_world` (one row per collider, for every resident body) and then dropped by `apply_deltas`' `filter_map`. They are also not counted as "unresolved", because the body entity has no saved FormId row at all.
  4. `SetMotionType` writes `motion_type` on exactly these child bodies (`reference_body_entities` → `RigidBodyData` + `PhysicsWorld::set_motion_type`). A scripted motion-type change therefore survives only a `restore_world` round trip.
  5. `restore_world` is the test/loose path. `rigid_body_data_survives_save_load_round_trip` exercises only that path, so it stays green.
  6. The same mismatch makes #5686's `keep_reimported_part_surface_materials` hook and the `RigidBodyData` "NoReconcilerNeeded" disposition vacuous in production. The row #5686 described ("every FormID-matched collider") does not exist.
- **Evidence**:
  - `driver.rs:277-280`: `q.iter().filter_map(|(eid, comp)| pool.resolve(comp.0)...)` over `world.query::<FormIdComponent>()`.
  - `spawn.rs:1491-1497`: `world.insert(entity, body); … world.insert(entity, PhysicsSourceForm(fid));`. No production site inserts `RigidBodyData` on an entity that also gets a `FormIdComponent`. I grepped every `insert(.., RigidBodyData` in `byroredux/src`.
- **Impact**:
  - A quest that makes a prop dynamic, or freezes one keyframed, reverts to the authored motion type after any save/load.
  - `defaultDisableHavokOnLoad`-style scripts re-apply on `OnLoad` and self-heal, which is why this is MEDIUM and not the HIGH floor for "a load that drops gameplay state": no progression is lost.
  - Every save also carries one dead `RigidBodyData` row per resident collider (thousands in an exterior), and they bloat the payload for nothing.
  - #2379 reads as fixed while the production path never received the fix.
- **Related**: #2379, #5686, SAVE-2026-10-21-04 (the same column's `UNAUDITED` eviction row), PHYS-D3-2026-10-21-02 (bone bodies rebuilt from `RigidBodyData::default()`).
- **Suggested Fix**:
  - Persist scripted motion-type overrides the way enable and lock state are persisted: a FormID-keyed ledger resource (`ReferenceMotionTypeState`, placement FormID → motion type), written by the `SetMotionType` system, read at body spawn and restored before the reload (`PRE_RELOAD_RESOURCES`).
  - Then drop `RigidBodyData` from `MUTABLE_DELTA_COLUMNS` and from the registry, and retire the preserve hook.
  - Pin it with a live-path test: spawn a collider carrying `PhysicsSourceForm`, run `build_form_id_remap` + `apply_deltas`, and assert that the override survives.

### SAVE-2026-10-21-02: The load's own cell reload records the visit against the outgoing session's clock, so an in-process load can owe the loaded cell a spurious `OnReset`
- **Severity**: MEDIUM
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: corruption-on-load
- **Location**:
  - `byroredux/src/cell_loader/load.rs:581`, `byroredux/src/cell_loader/exterior.rs:1170`, `:2205` (`note_player_entered_cell` inside the reload).
  - `byroredux/src/cell_loader.rs:636-645`.
  - `crates/scripting/src/obscript_state.rs:155-158`, `:200-215` (`CellVisits::enter`, `note_cell_entered` → `PendingCellResets`).
  - `byroredux/src/save_io/live_load.rs:37-42` (`PRE_RELOAD_RESOURCES`), `:608-620` (the outgoing-session discards).
  - `byroredux/src/boot/schedule/update.rs:522` (`mark_cell_reset`).
- **Status**: NEW
- **Description**:
  1. `reload_interior_session` / `reload_exterior_session` run the ordinary loaders, and each cell root they build calls `note_player_entered_cell`.
  2. That call reads `GameTimeRes` and writes `CellVisits`. Neither is in `PRE_RELOAD_RESOURCES`, so during the reload both still hold the **outgoing** session's values.
  3. If the outgoing session last entered that cell ≥ `iHoursToRespawnCell` (72 h default) before its current clock, `CellVisits::enter` reports the cell as reset, and `note_cell_entered` pushes it onto `PendingCellResets`.
  4. The wholesale `restore_resources` afterwards replaces `CellVisits` with the saved one. `PendingCellResets` is unsaved and none of the post-reload discards touch it. The #5435 / #5864 / #6083 calls cover the CLOC cursor, story events, package reports and unload deliveries only.
  5. The next frames' `mark_cell_reset` (Update) therefore stamps `OnResetEvent` on every scripted reference of the loaded cell, and its object scripts' `OnReset` blocks run.
  6. A fresh-process load never does this, because `CellVisits` is empty at that point, so in-process and cold loads diverge. That breaks the contract #5435 established.
- **Evidence**:
  - `cell_loader.rs:640-644`: `let Some(now) = world.try_resource::<GameTimeRes>()… ; note_cell_entered(world, cell_form, now, respawn)`, called inside `without_parked_state(… reload …)` (`live_load.rs:585-596`), before the post-reload `restore_resources` (`:639`).
  - The allowlist reason (`registry_completeness_tests.rs:802`) says only "drained within a frame or two of the cell loading".
- **Impact**:
  - Trigger: an ordinary in-process load of an older save in a location the current session left ≥ 3 game days ago.
  - `OnReset` blocks typically re-arm do-once variables, re-lock containers and reset activator state. All of those are saved `ScriptVariables` / ledger state, so the spurious reset is persisted by the next save.
  - Only Oblivion/FO3/FNV object scripts (M47.3) carry `OnReset`. Exterior reloads note every tile in the radius, which widens the trigger.
- **Related**: #5435, #6083 (same class: outgoing-session state leaking across the drain), SCR-2026-10-21-05 (a different `PendingCellResets` defect: `PersistentCellApplyJob` never retires its entry).
- **Suggested Fix**: in `execute_pending_save_loads`, clear `PendingCellResets` beside `discard_pending_unload_deliveries`. A load is not a cell reset, and the saved `CellVisits` is already authoritative after the restore. Pin it with a `live_reload_tests` case that seeds an outgoing `CellVisits` entry older than 72 h and asserts that no `PendingCellResets` survives the drain.

### SAVE-2026-10-21-03: Transient request queues straddle the load drain: a same-frame player `MoveTo` runs after the load, and Late-stage XP/skill grants land in the loaded session
- **Severity**: MEDIUM
- **Dimension**: Live Load-Apply (frame boundary) / Snapshot Completeness (allowlist reasons)
- **Data-Loss Class**: corruption-on-load
- **Location**:
  - `byroredux/src/app_events.rs:1103` (`step_save_loads`) before `:1109` (`step_cell_transition`).
  - `byroredux/src/app_step.rs:1210-1213` (`step_cell_transition` drains `step_player_move_request` first, before its loading-screen gate) and `:952-1001`.
  - `byroredux/src/boot/schedule/late.rs:233`, `:247` (`scripted_experience_system`, `scripted_skill_system`) versus the Late fragment runners registered after them, `:441` (`papyrus_provider_system`), `:543` (dialogue selection), `:848` (`obscript_item_system`).
  - `crates/scripting/src/fragment/effects/deferred.rs:823`, `:853-861` (`PendingSkillAdvances` / `PendingPlayerXp` flush).
  - Allowlist rows `registry_completeness_tests.rs:517`, `:557`, `:711`.
- **Status**: NEW
- **Description**: The load drain runs between the scheduler and the rest of the App steppers. There are two leak paths.
  1. **`PlayerMoveRequest`** (`Option<u32>` placement FormID). A fragment that runs `Game.GetPlayer().MoveTo(x)` (Papyrus or ObScript `MoveTo player`) writes it during the scheduler run. It is consumed only by `step_player_move_request`, at the top of `step_cell_transition`, which runs **after** `step_save_loads` in the same `about_to_wait`. When a load drains in that frame (F9, console `load`, `EssentialDeathReload`, or the frame a loading cover finishes presenting), the outgoing session's FormID is resolved in the **loaded** world: the player is moved in place, or a transition to the outgoing session's destination is queued. That happens after `apply_player_pose` has just restored the saved pose. The allowlist reason ("drained … the frame it is written") is true only of a frame without a load.
  2. **`PendingPlayerXp` / `PendingSkillAdvances`** (amounts, skill names). The deferred-effect batch flushes them where the fragment runs. Their consumers are Late systems registered before three Late fragment runners. A grant raised by those runners waits for the next frame's consumer and is applied to the loaded player's saved `CharacterLevel` / `LevelUpPoints` / `SkillProgress` / `ActorValues`. The `PendingPlayerXp` row admits "the same or the next frame". The `PendingSkillAdvances` row ("drained the same frame") is false.
- **Evidence**:
  - `app_events.rs:1093-1109` order: `capture_player_pose` → `step_player_save_actions` → `step_save_loads` → `step_cell_transition`.
  - `step_player_move_request` takes `request.0` unconditionally, then resolves the FormID against the current world.
  - `execute_pending_save_loads` discards CLOC / story / package / unload state (`live_load.rs:608-620`) but no request queue.
- **Impact**:
  - The trigger window is one frame. The outcome is cross-timeline contamination that the next save persists: a teleport or cell change away from the loaded pose, or unearned XP or skill progress.
  - It is the same class as #5435 / #6083. Each of those was fixed one queue at a time, and no guard enumerates the rest. Other same-shape value-keyed queues (`PendingSleep`, `PendingMusic`, `PendingScriptMessages`, `MessageBoxQueue`) have the same exposure where their producer runs after their consumer.
- **Related**: #5435, #5864, #6083, SAVE-2026-10-21-02.
- **Suggested Fix**:
  - Add one `discard_outgoing_session_requests(world)` beside the existing discards. It should clear `PlayerMoveRequest`, `PendingPlayerXp`, `PendingSkillAdvances`, `PendingSleep` and the other FormID/value-keyed `Pending*` request resources.
  - Add a `live_reload_tests` source guard that lists every `NOT_SAVED_BY_DESIGN` row whose reason says "drained"/"consumed" and requires it to be either discarded in the drain or marked entity-keyed (stale ids never resolve, because ids are not recycled).
  - Correct the three allowlist reasons.

### SAVE-2026-10-21-04: #6085's `NOT_PARKED` partition still has nine `UNAUDITED` rows; two of them revert player-visible state on every cell exit
- **Severity**: LOW
- **Dimension**: Snapshot Completeness & the Two Lists (third list)
- **Data-Loss Class**: silent-drop on eviction (save/load is unaffected)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:1176-1197`
- **Status**: NEW (follow-up to closed #6085, which seeded the rows)
- **Description**: The partition guard accepts `"UNAUDITED"` as a reason, so these rows stay green indefinitely. Proposed dispositions, from reading their mutators:
  - **CharacterLevel / LevelUpPoints**: player progression, and the player is never evicted. The reason should say so, as `CharacterController`'s row does.
  - **TimedRestorations**: the only production insert is on the player (`inventory.rs:1567`). Same reason.
  - **LightSource / LightFlicker**: mutated only by `light_anim` flicker playback, which is re-derived. Not parked is correct.
  - **GuardState / PatrolState**: AI runtime that re-rolls on respawn, the same reason `WanderState` carries.
  - **Escorted**: a terminal marker on a finished escort. After eviction the escort package can run again and fire its PF_ fragments a second time. Needs a park-or-justify decision (gameplay-owned).
  - **RigidBodyData**: a scripted `SetMotionType` (`cinematic.rs:436-445`) reverts on cell exit. It is moot until SAVE-2026-10-21-01 moves motion overrides to a FormID ledger, which would then cover eviction as well.
- **Impact**: low. The Escorted and RigidBodyData rows are the only ones with gameplay effect. Leaving `UNAUDITED` as a valid reason defeats the guard's purpose.
- **Related**: #6085, GAME-SD-01/02, SAVE-2026-10-21-01.
- **Suggested Fix**: write the reasons above. Make the guard reject `"UNAUDITED"` once they are written, or at least fail if the count grows.

### SAVE-2026-10-21-05: `NOT_SAVED_BY_DESIGN` carries a duplicate `CrimeFactions` row that the guard silently collapses, and `ActiveBarter`'s reason overstates its save gate
- **Severity**: LOW
- **Dimension**: Snapshot Completeness & the Two Lists
- **Data-Loss Class**: none
- **Location**:
  - `byroredux/src/save_io/registry_completeness_tests.rs:511`, `:984` (two `("CrimeFactions", …)` rows), `:1040-1042` (`allowlisted` is a `HashSet`).
  - `:542` and `byroredux/src/barter.rs:202-204` (`ActiveBarter`).
- **Status**: NEW
- **Description**:
  - `f2d559a79` added a second `CrimeFactions` row with a different reason. The guard builds a `HashSet` of names, so duplicates disappear without a diagnostic. Its own #3710 comment names exactly this hazard ("a reintroduced short-name collision wouldn't show up as a duplicate key error"), but it guards only the two player-entity names.
  - `ActiveBarter`'s reason says every page-leave path ends the session "before a save can run". The remote and overlay console `save` ingresses dispatch with the barter page open. A save taken then captures the stand-in merchant container: a `FormIdComponent` entity with an `Inventory` and no cell root. On load it is benign: the row is unresolved and #5054's `park_unresolved_snapshot_rows` parks it (`reference_state.rs:898-1040`). The reason is still inaccurate.
- **Impact**: guard hygiene only.
- **Suggested Fix**: delete one `CrimeFactions` row, and assert `NOT_SAVED_BY_DESIGN` has no duplicate names (compare `len()` against the set). Reword `ActiveBarter` as "player ingresses are blocked while the page is open; a console save parks the stand-in's stock through the unresolved-row park".

### SAVE-2026-10-21-06: `schema_fingerprint` is row-order-sensitive and no test pins it, while #5802 keeps moving registration rows
- **Severity**: LOW
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: none (a reorder rejects every existing save with `SchemaMismatch`; it does not corrupt one)
- **Location**: `crates/save/src/registry.rs:377-395`; `byroredux/src/save_io/registry.rs:511-558` (the builder chain split at each `register_saved` call)
- **Status**: NEW (related #5802)
- **Description**:
  1. The fingerprint is FNV-1a over component, then resource, names in registration order.
  2. The snapshot itself is a name-keyed `BTreeMap`, so order carries no compatibility meaning. A pure reorder still invalidates every save, with no `FORMAT_MAJOR` bump or ledger entry to explain it.
  3. Each #5802 slice must keep each delegate at its row's former position. `bde3478db` and `e619dbbaf` verified this by printing the fingerprint by hand on two trees, as their commit messages record.
  4. No test pins the value. `SaveRegistry Order-Sensitive` is tracked only as a memory note.
  5. The next slice that moves a delegate one line goes unnoticed in CI.
- **Impact**: every player save becomes unloadable after a refactor that changed no data. It is caught only by a live load.
- **Suggested Fix**: either hash the sorted names (a one-time fingerprint change to fold into the next genuine `FORMAT_MAJOR` bump), or add a `const BASELINE_SCHEMA_FINGERPRINT` test beside `BASELINE_SHAPE_FINGERPRINT`, refreshed with a justification on every column add or remove, so a reorder fails CI.

## Dimension notes

- **Dimension 2 (format & schema)**:
  - About 30 refreshes without a bump in the window. Each carries a justification comment. I spot-checked four: `722421899` (appended `Effect` variants; `LevelingModel` edit is a method, not a field), `10b4c9c7e`, `48b52b198`, `0de42be24` (appended variants). All are justified.
  - The new `RigidBodyData.havok_layer` is `serde(skip)` and is handled by the preserve hook (moot per -01).
  - Extension-state payload: `crates/sdk/src/component.rs` has no commits since 10-12, and `EXTENSION_STATE_FORMAT_VERSION` is still 5. `e634fbf9b` touched only the extension store's row staging.
  - Feature chain intact (the guards compile and run against `save`).
- **Dimension 3 (container & disk)**: unchanged since baseline. Crate tests green.
- **Dimension 4 (save-side gates)**:
  - `SaveCommand::execute` order is unchanged (`commands.rs:121-300`): refusals (transition flag, context invariant, chargen) → `validate_world` + `validate_form_ids` + `validate_cinematic_entity_refs` → `save_world` → extension capture → `encode` → `write_slot` → ring advance.
  - The newly registered component columns (`ScriptPackage`, `WeaponDrawn`, `IgnoreCrime`, `ActiveMagicEffects`) and saved scripting resources carry FormIDs, not `EntityId`s (shallow scan of each type body), so `validate_saved_entity_references` needs no new rows.
  - Fragment `RequestSave` only increments `CinematicPresentationState.save_requested_count`; there is no save path to gate.
- **Dimension 5**:
  - #6081's reinstate also runs on the post-teardown failure arms (`load_cell_with_masters` error, exterior worker disconnect). That matches `without_parked_state`'s own put-back, so outgoing resources stay mutually consistent.
  - The extension-restore / `restore_resources` / `apply_deltas` failure arms after the reload still leave the save's four pre-reload ledgers over otherwise-outgoing resources. This is pre-existing "unexpected failure after preflight" posture, noted rather than filed.

## Guards Verified

- `cargo test -p byroredux-save`: 39 + 15 passed, 0 failed.
- `cargo test -p byroredux --bin byroredux save_io`: 95 passed, 4 ignored (the four `consumable_tests` real-master legs), 0 failed. Live by name: `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`, `every_delta_column_is_parked_or_explicitly_not`, `serde_default_on_saved_struct_requires_format_major_bump`, `saved_type_shape_changes_require_format_major_bump`, `delta_columns_carry_only_session_stable_fields`, `delta_columns_removed_at_runtime_have_a_load_reconciler`, `post_reload_drain_steps_keep_their_order`, `an_aborted_load_hands_the_live_ledgers_back`, `live_load_drain_discards_the_outgoing_sessions_script_events`.
- `app_step::tests::the_save_drain_publishes_the_transition_flag_before_draining`: passed.
- Guard limits found this run:
  - `rigid_body_data_survives_save_load_round_trip` exercises `restore_world` only (-01).
  - The partition guard accepts `"UNAUDITED"` (-04).
  - The completeness guard cannot see duplicate allowlist rows (-05).
  - Nothing pins the schema fingerprint (-06).
- Not checkable here: any engine, smoke or Vulkan leg (`p5-*`, `p6-loading-model.sh`), and the game-data `#[ignore]`d tests. Findings -01 to -03 are static, with no live reproduction.
