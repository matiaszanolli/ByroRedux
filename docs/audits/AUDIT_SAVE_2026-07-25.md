# Save / Load Subsystem Audit (M45 + M45.1) — 2026-07-25

Scope: `crates/save/src/*` (~1.2k LOC) and its sole engine-side consumer
`byroredux/src/save_io.rs` (1,667 LOC), audited against HEAD `ca7a4e0e`. This
audit was run as one leg of a `comprehensive` audit-suite sweep. Rather than
delegating to six parallel dimension agents (the orchestrator shape the skill
describes), every file in scope was read in full by hand, cross-checked
against the two most recent prior reports, `gh issue list`, and verified
against the live `cargo test` suite — the goal was end-to-end certainty, not
speed.

This is the **fifth** save audit (prior: `AUDIT_SAVE_2026-06-23.md`,
`AUDIT_SAVE_2026-07-02.md`, `AUDIT_SAVE_2026-07-03.md`,
`AUDIT_SAVE_2026-07-16.md`). All nine findings from the 2026-07-16 report
(`SAVE-D1-NEW-01`, `SAVE-D1-NEW-02`, `SAVE-D2-03`, `SAVE-D2-04`, `SAVE-D2-05`,
`SAVE-D2-06`, `SAVE-D4-NEW-01`, `SAVE-D6-03`, `SAVE-D6-04`) were filed as
issues `#2014`–`#2022` and closed within two days (commits `55ae73e2`,
`7bcb532e`, `0971c4a9`, 2026-07-17/18). **Every one of those nine fixes was
independently re-verified against today's code** — reading the actual diff
logic, not just trusting the "Closed" label — and all nine hold. Zero
regressions found. One new LOW finding surfaced (a narrow robustness gap in
the `#1714` guard's own detection regex). `#1848`/SAVE-05 remains the sole
open save-related issue, re-confirmed still reproducible.

## Executive Summary

Docstring design claims (`crates/save/src/lib.rs`) verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot | **CODE-CONFIRMED** — the 2026-07-16 gap (seven M42 AI-procedure components absent from the registry) is fixed: all nine Wander/Travel/Follow/Escort/Guard/Patrol/Sandbox state+marker types are now in `build_save_registry` (`#2014`). `QuestStageState`/`QuestObjectiveState` (fixed earlier, `#1862`, 2026-07-03) remain registered. A fresh sweep of every file under `crates/core/src/ecs/components/` (30 files) found no further player-mutable component missing from the registry or its `REDERIVED_NOT_SAVED` allowlist. |
| Atomic write (tmp → fsync → read-back → rename → dir-fsync) | **CODE-CONFIRMED** — exact sequence re-verified line-by-line in `disk.rs`, 0 findings. |
| Ring (never clobbers the last good save) | **CODE-CONFIRMED**, including the `#2017` fix — `SaveRing::advance()` now only runs *after* validation passes (`SaveCommand::execute` uses non-mutating `peek()` for the abort path); regression test `quicksave_ring_cursor_does_not_advance_on_validation_abort` passes. |
| Validation gate (refuse to persist inconsistent state) | **CODE-CONFIRMED** — single production call site (`SaveCommand::execute`), gate strictly precedes `save_world`/`encode`/`write_slot`, all 5 reference-integrity checks (Hierarchy, Equipment, Animation, ItemInstance, FormId) run pre-write. |
| Off-frame load (never runs inside the scheduler) | **CODE-CONFIRMED** — `LoadCommand` (holds `&World`) only decodes + queues; `execute_pending_save_loads` (the sole consumer of `&mut World`) runs from `App::step_save_loads` (now in `byroredux/src/app_step.rs`, post-`main.rs`-split), called every frame right after `capture_player_pose`. Structurally impossible for a `ConsoleCommand` to obtain `&mut World`. |

**Findings this cycle: 1 new, 0 CRITICAL, 0 HIGH, 0 MEDIUM, 1 LOW.** Nine
prior findings re-verified fixed (no regressions). One pre-existing OPEN issue
(`#1848`) re-confirmed still reproducible, not re-filed.

By Data-Loss Class: **none** — 1 (SAVE-D2-NEW-07, test/guard-robustness gap
only, not currently exploitable).

## Regression Verification Ledger (2026-07-16 → today)

Every finding from the prior report, independently re-checked against current
code (not just the issue tracker's "Closed" state):

| Finding | Issue | Fix commit | Re-verified today |
|---|---|---|---|
| SAVE-D1-NEW-01 (7 AI components absent from registry) | #2014 | `55ae73e2` | **HOLDS** — all 9 types (`WanderState`/`TravelState`/`Traveled`/`FollowState`/`EscortState`/`Escorted`/`GuardState`/`PatrolState`/`Seated`) registered in `build_save_registry` (`byroredux/src/save_io.rs:218-226`); 6 delta-safe ones also in `MUTABLE_DELTA_COLUMNS`; `FollowState`/`EscortState`/`Seated` correctly excluded (each carries a raw `EntityId` field — confirmed by reading `follow.rs:59-61`, `escort.rs:74-77`, `sandbox.rs:54-57`). Regression test `ai_procedure_state_and_terminal_markers_survive_save_load_round_trip` passes. |
| SAVE-D1-NEW-02 (release-mode `insert_batch` bound-check gap) | #2020 | `7bcb532e` | **HOLDS** — `restore_world` now calls `validate_entity_ids_in_bounds` (real check, not `debug_assert`-gated) before any mutation; `SaveError::EntityIdOutOfBounds` variant added. Test `restore_world_rejects_snapshot_with_out_of_bounds_entity_id` passes. |
| SAVE-D2-03 (`SAVE_TYPE_SOURCES` omitted `actor_values.rs`) | #2015 | `55ae73e2` | **HOLDS** — `actor_values.rs` present in the list (`save_io.rs:1596`), plus all 7 new AI-component files (`wander.rs`…`sandbox.rs`) added in the same fix wave. |
| SAVE-D2-04 (`LightSource`/`LightFlicker` untested) | #2021 | `7bcb532e` | **HOLDS** — `binary_registry_round_trips_including_scripttimer` now asserts both types' fields survive round-trip. |
| SAVE-D2-05 (`AnimationStack` untested) | #2016 | `0971c4a9` | **HOLDS** — `animation_stack_round_trips_through_container` exists in `crates/save/tests/round_trip.rs` and passes. |
| SAVE-D2-06 (`ItemInstancePool` untested, informational) | #2022 | `7bcb532e` | **HOLDS** — premise re-confirmed unchanged: `ItemInstance` is still a `_reserved: ()` placeholder. |
| SAVE-D4-NEW-01 (ring cursor advances on validation abort) | #2017 | `0971c4a9` | **HOLDS** — `SaveCommand::execute` calls `ring.peek()` for the quicksave slot choice and only calls `ring.advance()` after `issues.is_empty()` (`save_io.rs:466-502`). Test `quicksave_ring_cursor_does_not_advance_on_validation_abort` passes. |
| SAVE-D6-03 (FlyCam pose reverted in live Character mode) | #2018 | `0971c4a9` | **HOLDS** — `apply_player_pose` now branches on `character_now` alone (`save_io.rs:352`), converting a FlyCam-saved camera position to the body's feet position via `cam_pos - eye_height` before relocating the body, and clears momentum. Test `player_pose_flycam_saved_relocates_body_in_live_character_mode` passes. |
| SAVE-D6-04 (silent FormId-remap misses, no diagnostic) | #2019 | `0971c4a9` | **HOLDS** — `build_form_id_remap` now collects `unresolved` pairs and logs a bounded (`take(20)` + "… and N more") `log::warn!` summary (`driver.rs:221-246`). |

`cargo test -p byroredux-save` (33 tests: 20 unit + 13 integration) and
`cargo test --bin byroredux save_io::` (16 tests) both **pass 100%** as of
this audit.

## Completeness Ledger

`build_save_registry` registrations × `MUTABLE_DELTA_COLUMNS` membership,
refreshed against current code:

| Type | Registered (saved) | In `MUTABLE_DELTA_COLUMNS` | Status |
|---|---|---|---|
| `Transform` | yes | yes | SAVED+OVERLAID |
| `Name` | yes | no | structural-identity |
| `Parent` | yes | no | structural-identity |
| `Children` | yes | no | structural-identity |
| `Inventory` | yes | yes | SAVED+OVERLAID |
| `EquipmentSlots` | yes | yes | SAVED+OVERLAID |
| `LightSource` | yes | yes | SAVED+OVERLAID (round-trip tested since #2021) |
| `LightFlicker` | yes | yes | SAVED+OVERLAID (round-trip tested since #2021; gained a new required `animation_flags: u32` field on 2026-07-20 (`41eedfe1`) — no `#[serde(default)]`, so a pre-2026-07-20 save with a `LightFlicker` column now fails to decode with a clean `SaveError::Serde` rather than silently default-filling, which is the *designed* backstop working as intended; not a finding) |
| `AnimationPlayer` | yes | **no** (deliberate, #1696) | SAVED-only |
| `AnimationStack` | yes | **no** (deliberate, #1696) | SAVED-only (round-trip tested since #2016) |
| `ScriptTimer` | yes | yes | SAVED+OVERLAID |
| `ActorValues` | yes | yes | SAVED+OVERLAID |
| `WanderState` | yes | yes | SAVED+OVERLAID |
| `TravelState` | yes | yes | SAVED+OVERLAID |
| `Traveled` | yes | yes | SAVED+OVERLAID (terminal marker) |
| `GuardState` | yes | yes | SAVED+OVERLAID |
| `PatrolState` | yes | yes | SAVED+OVERLAID |
| `Escorted` | yes | yes | SAVED+OVERLAID (terminal marker) |
| `FollowState` | yes | **no** (carries `EntityId`) | SAVED-only, correctly excluded from overlay |
| `EscortState` | yes | **no** (carries `EntityId`) | SAVED-only, correctly excluded from overlay |
| `Seated` | yes | **no** (carries `EntityId`) | SAVED-only, correctly excluded from overlay |
| `FormIdComponent` | yes | N/A | structural — the remap key itself |
| *(resources)* `ItemInstancePool`, `CurrentCellContext`, `PlayerPose`, `QuestStageState`, `QuestObjectiveState` | yes | N/A (whole-resource restore via `restore_resources`, precedes `apply_deltas`) | SAVED+RESTORED |
| `FactionRanks`, `CharacterLevel`, `Background`, `Perks` | no | no | `REDERIVED_NOT_SAVED` allowlist — write-once from ESM at NPC spawn, no runtime mutator exists (verified: no console command or system writes any of the four outside `npc_spawn.rs` / test code) |

No drift found. Every registered mutable column absent from
`MUTABLE_DELTA_COLUMNS` is either structural/identity or a documented,
tested, `EntityId`-hazard exclusion. Every NPC-spawn-stamped component
(`Transform`/`Name`/`Inventory`/`EquipmentSlots`/`ActorValues`/`FactionRanks`/
`CharacterLevel`/`Background`/`Perks` — cross-checked line-by-line against
`stamp_faction_ranks`/`stamp_actor_values`/`stamp_character_components` in
`byroredux/src/npc_spawn.rs`) is either registered or allowlisted, matching
the `npc_spawn_stamped_components_are_saved_or_intentionally_rederived`
guard's own list exactly — no drift between the guard and the real spawn
code.

**Component sweep (fresh this cycle)**: every one of the 30 files under
`crates/core/src/ecs/components/` was read or grepped for `impl Component
for`. None expose a persistent, player-mutable field outside the registry:
`SubmersionState`/`WaterContact`/`WaterFlow` are per-frame-derived from live
position (self-correcting, not authoritative); `CollisionShape`/
`RigidBodyData`/`PhysicsSourceForm` are structural/derived from the NIF import,
no runtime mutator; `BSXFlags`/`SceneFlags`/`RenderLayer`/`LocalBound`/
`FogVolume`/`Billboard`/`Furniture`/`AttachPoints` are all either static
import-time data or GPU/render-derived state, none carrying live gameplay
progress.

## Findings

### LOW

#### SAVE-D2-NEW-07: The `#1714` guard's `#[serde(default)]` detection is a line-prefix string match — a reordered serde attribute list would slip past it
- **Severity**: LOW
- **Dimension**: Registry & (De)serialization Fidelity
- **Location**: `byroredux/src/save_io.rs:1649-1656` (`serde_default_on_saved_struct_requires_format_major_bump`)
- **Status**: NEW
- **Data-Loss Class**: none (latent robustness gap; no live instance triggers it)
- **Description**: The `#1714` guard scans every save-participating source file for a line whose trimmed text `starts_with("#[serde(default")`, flagging any addition of a bare `#[serde(default)]` (or `#[serde(default = "...")]`) attribute on a saved struct's field. This correctly catches `#[serde(default)]` and `#[serde(default, ...)]` (both start with the matched prefix), but would **miss** the semantically identical `#[serde(skip_serializing_if = "...", default)]` — a legal, idiomatic serde ordering where `default` appears after another key in the same attribute list. Verified this exact ordering does not currently exist anywhere in the 21 files `SAVE_TYPE_SOURCES` scans (grepped every file for any `serde(...)` attribute combining multiple keys — none found), so there is no live gap today; this is purely a static-analysis blind spot in the guard itself.
- **Evidence**:
  ```rust
  if line.trim_start().starts_with("#[serde(default") {
      offenders.push(format!("{rel}:{}", i + 1));
  }
  ```
  A future field like `#[serde(skip_serializing_if = "Vec::is_empty", default)]` on any registered type would not trip this check, silently reintroducing the exact SAVE-D2-01 hazard (an old save missing the new field loads with it silently default-filled) with the regression guard reporting green.
- **Impact**: None today. Becomes live only if a maintainer adds `default` as a non-first key inside a multi-key `#[serde(...)]` attribute on any of the ~15 save-participating types — an easy mistake to make since it's valid, common serde style, and nothing in the codebase currently steers away from it.
- **Related**: Sibling gap-class to the guard's own documented residual (the "new-`Option`" half it already admits it can't catch statically) — this is a narrower miss on the half it claims to catch fully.
- **Suggested Fix**: Broaden the match to `line.contains("#[serde(") && line.contains("default")` (accepting some false positives, e.g. a field literally named `default`, which is rare and cheap to allowlist), or parse the attribute with `syn` for an exact check. Either is a small diff to a test-only file.

## Known Open Issue (Cross-Referenced, Not Re-Filed)

- **`#1848` / SAVE-05** — `LoadCommand::execute` still does `pending.0 =
  Some(snapshot)` unconditionally (`byroredux/src/save_io.rs:659-660`), no
  `is_some()` guard — a second `load` issued before the next frame's drain
  silently discards the first queued snapshot. Re-confirmed reproducible by
  direct code reading (not just trusting the issue tracker). Still LOW
  (requires two `load` commands issued within the same frame, a narrow
  console-misuse window with no gameplay-visible corruption — the discarded
  snapshot is the *queued* one, not anything on disk), still OPEN, not
  re-filed.

## Verified Clean — No New Findings

- **Disk Format & Durability** (`crates/save/src/disk.rs`, `snapshot.rs`):
  read in full. Atomic write dance (`create_dir_all` → `.tmp` write → flush →
  `sync_all` → byte-exact read-back → `rename` → parent-dir `sync_all`)
  matches the documented sequence exactly, in the exact order, with the
  read-back mismatch path correctly deleting the tmp and returning
  `SaveError::Io` before any rename. Header gate ordering in `decode`
  (length → magic → major version → schema fingerprint → payload-length
  bounds with `checked_add` overflow guard → CRC → `from_slice`) precedes
  JSON parsing in every branch. CRC scope is payload-only, confirmed by
  `rejects_major_version_skew` (a header-only edit doesn't trip CRC).
  `parse_slot_filename` correctly rejects `.tmp` and non-numeric names.
  `SaveRing::resume` correctly restarts the cursor one past the newest
  on-disk mtime. All 9 relevant unit tests pass.
- **Validation Gates** (`crates/save/src/validate.rs`): read in full. Single
  production call site (`SaveCommand::execute`), abort strictly precedes
  `save_world`. Five reference classes all present and running: Hierarchy
  (bidirectional Parent⇄Children + dangling-id via `>= next_entity`),
  Equipment (occupant-index bounds against the same entity's `Inventory`),
  Animation (`clip_handle` registry resolution + `root_entity` dangling
  check), ItemInstance (`Inventory` stacks resolve against
  `ItemInstancePool`), and the binary-side `validate_form_ids` (cross-plugin
  `FormIdComponent` resolvability). Post-load diagnostic re-run
  (`log_validation_warnings`) wired into both `restore_world` and
  `execute_pending_save_loads`, diagnostic-only (no abort), confirmed.
- **Registry & (De)serialization Fidelity** (`crates/save/src/registry.rs`):
  read in full. `form_id_column()` keys off the explicit `is_form_id` flag
  (not an `apply.is_none()` heuristic), with a registration-time `assert!`
  against a second form-id column — both guard tests pass.
  `register_form_id_component` resolves `FormId → FormIdPair` at save time
  and skips (with a `log::warn!`) any handle that doesn't resolve, never
  panics; load returns `SaveError::MissingResource` rather than panicking
  when `FormIdPool` is absent. `FnvHasher` uses the canonical 64-bit FNV-1a
  offset basis (`0xcbf2_9ce4_8422_2325`) and prime (`0x100_0000_01b3`),
  hashing only registered names + kind tag (no address/TypeId dependency).
- **Frame-Boundary Capture & Off-Frame Apply**: `capture_player_pose` runs
  immediately before `step_save_loads()` every frame (now in
  `byroredux/src/app_step.rs` post-`main.rs`-split, at line 1244/1249 of the
  App's per-frame method — the `main.rs`-line-number references in
  `SKILL.md`/`_audit-common.md` are stale post-split, noted below as
  doc-rot, not a code defect). `SaveCommand`/`LoadCommand` both hold only
  `&World`; `execute_pending_save_loads` is the sole `&mut World` consumer,
  invoked from `step_save_loads`, which is not part of the ECS scheduler —
  structurally unreachable from inside a system.
- **M45.1 Live Load-Apply** (`execute_pending_save_loads`, read in full):
  apply ordering confirmed exact — drain slot → resolve
  `CurrentCellContext` → `validate_cell_loadable` pre-flight (non-destructive,
  `#1697`) → teardown (`drain_streaming_state` + `unload_current_interior`,
  gated only on `streaming.is_some()`, so idempotent across repeated loads)
  → `load_cell_with_masters` → lighting + `signal_temporal_discontinuity` +
  `LoadedPluginSet` → `restore_resources` (precedes deltas, so
  `ItemInstancePool` ids resolve) → `build_form_id_remap` →
  `apply_deltas(MUTABLE_DELTA_COLUMNS)` → post-load validation diagnostic →
  `apply_player_pose` (last). The player body's `PLAYER_FORM_ID_PAIR`
  (`#1846`) is confirmed still attached at spawn in `byroredux/src/scene.rs`,
  participating in the remap like any NPC. `set_kinematic_translation`
  confirmed to no-op (return `false`) without a live Rapier handle rather
  than panicking.

## Regression Guards Discovered / Reconfirmed

| Test | Invariant it pins |
|---|---|
| `delta_columns_carry_only_session_stable_fields` | `MUTABLE_DELTA_COLUMNS` entries carry no `FixedString`/`EntityId`/session-local handle — pinned exact-set assertion against `AUDITED` |
| `serde_default_on_saved_struct_requires_format_major_bump` (#1714) | Every save-participating type is scanned for a `#[serde(default)]` addition without a `FORMAT_MAJOR` bump — **has a narrow detection blind spot, see SAVE-D2-NEW-07** |
| `form_id_column_resolves_the_flagged_entry` / `registering_a_second_form_id_column_panics` (#1845) | `form_id_column()` keys off the explicit `is_form_id` flag |
| `form_id_restore_without_pool_errors_cleanly` (#1716) | Load returns `SaveError`, never panics, when `FormIdPool` is absent |
| `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` (#1835) | Every component `spawn_npc_entity` stamps is saved XOR rederived-allowlisted — re-verified matches current spawn code with zero drift |
| `write_read_round_trip_and_atomic_rename` | tmp file removed after clean write, final file round-trips |
| `cursor_after_newest_points_past_latest_mtime` / `resume_on_empty_dir_starts_at_zero` (#1706) | Ring cursor resumes from on-disk mtimes, not slot 0 |
| `rejects_major_version_skew` | A header-only edit trips the version gate, not the CRC |
| `parse_slot_names` | `.tmp` / non-numeric slot files never register as loadable |
| `validation_catches_dangling_parent` / `validation_catches_equipment_out_of_bounds` | Core referential-integrity gates |
| `restore_world_rejects_snapshot_with_out_of_bounds_entity_id` (#2020) | Release-mode entity-id bound check is real, not `debug_assert`-only |
| `quicksave_ring_cursor_does_not_advance_on_validation_abort` (#2017) | Ring only advances on a committed write |
| `player_pose_flycam_saved_relocates_body_in_live_character_mode` (#2018) | Mode-mismatch pose restore relocates the body, not just the camera |
| `binary_registry_round_trips_including_scripttimer` (+ #2021 extension) | Cross-crate `ScriptTimer` + `LightSource`/`LightFlicker` round-trip |
| `actor_values_survive_save_load_round_trip` (#1834/#1835) | `ActorValues` round-trips |
| `ai_procedure_state_and_terminal_markers_survive_save_load_round_trip` (#2014) | All 9 M42 AI-procedure component shapes round-trip, incl. terminal markers and the `Seated.furniture` `EntityId` |
| `quest_stage_and_objective_state_survive_snapshot_round_trip` (#1862) | Quest progress survives both `restore_world` and the live `restore_resources` overlay path |
| `animation_stack_round_trips_through_container` (#2016) | `AnimationStack` round-trips |
| `delta_apply_reroutes_by_form_id_after_cell_reload` / `player_body_inventory_survives_live_load` | End-to-end live-load remap correctness |
| `full_world_round_trips_through_container` | Full container round trip |

## Doc-Rot Observations (Not Filed as Findings)

- `.claude/commands/audit-save/SKILL.md`'s cross-cut references to
  `byroredux/src/main.rs` line numbers (~1014, ~2300, ~1345) are stale: the
  Session 34/35 refactors split `main.rs` into `byroredux/src/boot.rs`
  (resource install, including the save registry/state/pending-slot/pose at
  boot) and `byroredux/src/app_step.rs` (`step_save_loads`, called
  immediately after `capture_player_pose` in the per-frame method). The
  *invariant* the skill describes is still exactly true; only the file/line
  pointer rotted. Per the skill's own path-reference convention, this
  should be updated to unquoted plain-text file names or refreshed paths
  next time the skill is edited.
- `docs/feature-matrix.md:169`'s `TD3-002` comment (Save/load M45/M45.1
  shipped 2026-06-21) re-confirmed still reads correctly.

---

No new HIGH/CRITICAL/MEDIUM findings this cycle — the subsystem is in a
verified-clean state modulo one pre-existing LOW issue (`#1848`) and one new
LOW guard-robustness note (`SAVE-D2-NEW-07`).

Suggested next step: `/audit-publish docs/audits/AUDIT_SAVE_2026-07-25.md`
