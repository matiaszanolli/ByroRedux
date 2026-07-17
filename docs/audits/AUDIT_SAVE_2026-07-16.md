# Save / Load Subsystem Audit (M45 + M45.1) — 2026-07-16

Scope: `crates/save/src/*` (~1.2k LOC) and its sole engine-side consumer
`byroredux/src/save_io.rs`. Audited against HEAD `c3e09bb5`. Six dimension
agents ran independently (max 3 concurrent): Snapshot Completeness &
Determinism, Registry & (De)serialization Fidelity, Disk Format & Durability,
Validation Gates, Frame-Boundary Capture & Off-Frame Apply, M45.1 Live
Load-Apply.

This is the **fourth** save audit (prior: `AUDIT_SAVE_2026-06-23.md`,
`AUDIT_SAVE_2026-07-02.md`, `AUDIT_SAVE_2026-07-03.md`) — the skill's "no
prior audit exists" premise is stale and was disregarded; all three prior
reports and the GitHub issue history were used as the dedup baseline. Of the
prior audits' findings, **every one is now CLOSED** with a verified fix
except `#1848`/SAVE-05 (still OPEN, LOW, re-confirmed reproducible, not
re-filed).

## Executive Summary

Docstring design claims (`crates/save/src/lib.rs`) verified against live
code:

| Claim | Status |
|---|---|
| Full ECS snapshot | **DRIFTED** — the seven M42 AI-package runtime-state components (Wander→Patrol) landed after the 2026-07-03 cutoff and are entirely absent from `build_save_registry` (SAVE-D1-NEW-01, HIGH). Everything else registered is either genuinely captured or a documented, justified rederive-on-load exclusion. |
| Atomic write (tmp → fsync → read-back → rename) | **CODE-CONFIRMED** — exact sequence verified, 0 findings (Dim 3). |
| Ring (never clobbers the last good save) | **CODE-CONFIRMED**, with one new narrow caveat: the cursor can advance on a *failed* (validation-aborted) quicksave attempt, desyncing the round-robin invariant within a session (SAVE-D4-NEW-01, MEDIUM). |
| Validation gate (refuse to persist inconsistent state) | **CODE-CONFIRMED** — single production call site, gate strictly precedes the write, no bypass exists (Dim 4). |
| Off-frame load (never runs inside the scheduler) | **CODE-CONFIRMED** — structurally impossible for a `ConsoleCommand` to obtain `&mut World`; capture is provably never torn (Dim 5, both CRITICAL-if-real hypotheses disproven by tracing `Scheduler::run`). |

**Findings: 9 new, 0 CRITICAL, 2 HIGH, 4 MEDIUM, 3 LOW.** Zero findings in
Dimensions 3 and 5 (both came back fully clean after deliberately trying to
disprove the CRITICAL-if-real hypotheses in their checklists). No prior
CLOSED issue was found to have regressed.

By Data-Loss Class: **silent-drop** — 4 (SAVE-D1-NEW-01, SAVE-D2-03-latent,
SAVE-D6-03, SAVE-D6-04); **corruption-on-load** — 1, dormant/unreachable
(SAVE-D1-NEW-02); **irrecoverable-write** — 1, narrow (SAVE-D4-NEW-01);
**none** (test-coverage gaps only) — 3 (SAVE-D2-04, SAVE-D2-05, SAVE-D2-06).

## Data-Loss Class Matrix

| Finding | Severity | Dimension | Data-Loss Class | Live-reachable today? |
|---|---|---|---|---|
| SAVE-D1-NEW-01 | HIGH | 1 — Completeness | silent-drop | Yes, but only behind 7 individual opt-in env vars, none in default scheduler |
| SAVE-D2-03 | HIGH | 2 — Registry/Serde | silent-drop (latent) | Not yet — triggers only when a future `#[serde(default)]` lands on `ActorValue` |
| SAVE-D1-NEW-02 | LOW | 1 — Completeness | corruption-on-load (dormant) | No — `restore_world`'s only non-test callers are `#[cfg(test)]` |
| SAVE-D2-04 | LOW | 2 — Registry/Serde | none | N/A — test-coverage gap only |
| SAVE-D2-05 | MEDIUM | 2 — Registry/Serde | none | N/A — test-coverage gap only |
| SAVE-D2-06 | LOW | 2 — Registry/Serde | none | N/A — informational, type is currently a stub |
| SAVE-D4-NEW-01 | MEDIUM | 4 — Validation Gates | irrecoverable-write | Yes, narrow (repeated failed quicksaves in one session, pre-restart) |
| SAVE-D6-03 | MEDIUM | 6 — Live Load-Apply | silent-drop | Yes (FlyCam-saved pose reloaded into a live Character-mode session) |
| SAVE-D6-04 | MEDIUM | 6 — Live Load-Apply | silent-drop | Yes, narrow (plugin/cell content changed between save and load) |

## Completeness Ledger

`build_save_registry` registrations × `MUTABLE_DELTA_COLUMNS` membership:

| Type | Registered (saved) | In `MUTABLE_DELTA_COLUMNS` | Status |
|---|---|---|---|
| `Transform` | yes | yes | SAVED+OVERLAID |
| `Name` | yes | no | structural-identity |
| `Parent` | yes | no | structural-identity |
| `Children` | yes | no | structural-identity |
| `Inventory` | yes | yes | SAVED+OVERLAID |
| `EquipmentSlots` | yes | yes | SAVED+OVERLAID |
| `LightSource` | yes | yes | SAVED+OVERLAID (no round-trip test — SAVE-D2-04) |
| `LightFlicker` | yes | yes | SAVED+OVERLAID (no round-trip test — SAVE-D2-04) |
| `AnimationPlayer` | yes | **no** (deliberate, #1696) | SAVED-only — full-restore-path only, correctly excluded from live overlay |
| `AnimationStack` | yes | **no** (deliberate, #1696) | SAVED-only (no dedicated round-trip test — SAVE-D2-05) |
| `ScriptTimer` | yes | yes | SAVED+OVERLAID |
| `ActorValues` | yes | yes | SAVED+OVERLAID (regression-guard scan-list gap — SAVE-D2-03) |
| `FormIdComponent` | yes | N/A | structural — the remap key itself, not an overlay target |
| *(resources)* `ItemInstancePool`, `CurrentCellContext`, `PlayerPose`, `QuestStageState`, `QuestObjectiveState` | yes | N/A (whole-resource restore via `restore_resources`, which precedes `apply_deltas`) | SAVED+RESTORED |
| `WanderState`, `PatrolState`, `GuardState`, `FollowState`, `EscortState`, `TravelState`, `Traveled`, `Escorted`, `Seated` | **no** | **no** | **UNREGISTERED — SAVE-D1-NEW-01** |

No other drift found: every registered mutable column that's absent from
`MUTABLE_DELTA_COLUMNS` is either structural/identity or a documented,
tested exclusion (`AnimationPlayer`/`AnimationStack`, guarded by the
`delta_columns_carry_only_session_stable_fields` tripwire).

## Findings

### HIGH

#### SAVE-D1-NEW-01: Seven M42 AI-procedure runtime-state components are absent from the save registry
- **Severity**: HIGH
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop
- **Location**: `crates/core/src/ecs/components/{wander,travel,follow,escort,guard,patrol,sandbox}.rs`; `byroredux/src/save_io.rs:162-208` (`build_save_registry`)
- **Status**: NEW
- **Description**: The seven M42 AI-package procedure runtimes (Wander/Travel/Follow/Escort/Guard/Patrol/Sandbox), all landed after this audit's 2026-07-03 cutoff, each pair a spawn-time `*Behavior` marker (correctly rederived from ESM `PACK` data, analogous to the existing `REDERIVED_NOT_SAVED` allowlist) with a runtime `*State`/terminal-marker component that the owning system mutates every tick or on completion. None of `WanderState`, `PatrolState`, `GuardState`, `FollowState`, `EscortState`, `TravelState`, `Traveled`, `Escorted`, `Seated` appear in `build_save_registry`. The existing `#1835` structural guard (`npc_spawn_stamped_components_are_saved_or_intentionally_rederived`) does not catch this: it only audits components `spawn_npc_entity` itself stamps, and these types are inserted lazily by their own systems on a later tick.
- **Evidence**:
  ```rust
  // crates/core/src/ecs/components/travel.rs
  pub struct Traveled; // terminal one-shot: NPC has arrived, travel_system should stop
  // byroredux/src/systems/travel.rs:194
  tq.insert(d.entity, Traveled);
  ```
- **Impact**: Continuously-updated state (`WanderState`/`PatrolState`/`GuardState`) is self-correcting on reload (cosmetic AI-continuity reset). The sharper edge is the terminal one-shot completion markers (`Traveled`/`Escorted`/`Seated`): losing these on save→load makes an NPC that has *already finished* its Travel/Escort/Seat behavior silently redo it — an arrived Travel NPC walks to its destination again, a completed Escort NPC restarts the collect+lead sequence. Blast radius is bounded today: all seven procedures are gated one-per-env-var, none in the default scheduler — but rated HIGH per "impact, not likelihood" since it's a real, non-recoverable regression the moment any flag is set, on a shipped user-facing feature.
- **Related**: Sibling failure class to closed `#1834`/`#1835` (ActorValues), but not caught by that guard because these are system-inserted, not spawn-inserted.
- **Suggested Fix**: Register the terminal markers and position/phase-only state (all plain `Vec3`/enum/`u32`, no `EntityId`) in `build_save_registry` and add the delta-safe ones to `MUTABLE_DELTA_COLUMNS`. Do **not** add `FollowState`/`EscortState`/`Seated` to `MUTABLE_DELTA_COLUMNS` — they carry `EntityId` fields (`target_entity`, `furniture`) with the same session-local-reference hazard `#1696` already excluded `AnimationPlayer.root_entity` for; they can still ride full `register_component` (`restore_world` preserves entity ids verbatim) but not the live delta overlay. Extend `delta_columns_carry_only_session_stable_fields`'s audited list deliberately, per its existing discipline.

#### SAVE-D2-03: `SAVE_TYPE_SOURCES` (the `#1714` guard's file scan list) omits `actor_values.rs` — the guard no longer scans every save-participating type
- **Severity**: HIGH
- **Dimension**: Registry & (De)serialization Fidelity
- **Data-Loss Class**: silent-drop (latent — no live corruption today)
- **Location**: `byroredux/src/save_io.rs:1196-1211` (`SAVE_TYPE_SOURCES`) vs. `save_io.rs:191` (`register_component::<ActorValues>`) and `crates/core/src/ecs/components/actor_values.rs`
- **Status**: Regression of `#1714`'s stated invariant (coverage gap introduced by `db121f96`, 2026-07-05 — two days after `#1714` shipped)
- **Description**: The `#1714` guard test (`serde_default_on_saved_struct_requires_format_major_bump`) exists to catch a save-participating struct gaining a `#[serde(default)]` field without a `FORMAT_MAJOR` bump — a change that `schema_fingerprint` (type-key-only) can't detect. It works by statically scanning `SAVE_TYPE_SOURCES`, whose own doc comment says "KEEP IN LOCKSTEP with `build_save_registry`." `db121f96` registered `ActorValues` (fixing `#1834`/`#1835`) and correctly updated `MUTABLE_DELTA_COLUMNS` and added a round-trip test — but never added `actor_values.rs` to `SAVE_TYPE_SOURCES` (confirmed via `git show db121f96 -- byroredux/src/save_io.rs`, which touches everything except that array). Today `ActorValue` has zero `#[serde(default)]` fields, so there's no live corruption — but the guard's own "scans every save-participating type" claim is now false while the test still reports green.
- **Evidence**: `git show db121f96 -- byroredux/src/save_io.rs | grep -n "SAVE_TYPE_SOURCES\|actor_values"` matches only the new test function name, never the array.
- **Impact**: No current data loss. The next field added to `ActorValue`/`ActorValues` with a `#[serde(default)]` escape hatch will not be caught by the regression guard built specifically to catch it — every existing save would silently default-fill the new field on load, on the actor-value system `#1834` already proved is read every frame (`GetActorValue`).
- **Related**: `#1714` (SAVE-D2-01, closed — guard mechanism), `#1834`/`#1835` (closed — registered `ActorValues` but missed this one line).
- **Suggested Fix**: Add `crates/core/src/ecs/components/actor_values.rs` to `SAVE_TYPE_SOURCES`. Since the list is manual/comment-driven and has now missed an entry once, consider deriving it from `SaveRegistry`'s type list plus a name→file map asserted at test time, so a future omission fails loudly instead of silently passing.

### MEDIUM

#### SAVE-D2-05: `AnimationStack` has no dedicated save/load round-trip test — the structurally riskiest untested registered type
- **Severity**: MEDIUM
- **Dimension**: Registry & (De)serialization Fidelity
- **Data-Loss Class**: none (test-coverage gap, escalated from LOW for structural complexity)
- **Location**: `crates/core/src/animation/stack.rs:14-33,84-88`; registered at `byroredux/src/save_io.rs:184`
- **Status**: NEW
- **Description**: `AnimationStack` (`Vec<AnimationLayer>` of 10+ fields each, plus `Option<EntityId> root_entity`) is registered for full save/restore and is the only registered type with both a nested `Vec` of a many-field struct and an `Option<EntityId>`. It's deliberately excluded from `MUTABLE_DELTA_COLUMNS` (`#1696`), and its structurally similar sibling `AnimationPlayer` *does* have a full-restore round-trip test (`anim_player_root_entity_not_clobbered_by_delta_apply`) — but `AnimationStack` itself is never constructed, saved, and asserted back in any test found.
- **Impact**: A future serde-shape regression in `AnimationLayer`/`AnimationStack.root_entity` would not be caught by any existing test.
- **Suggested Fix**: Add a `crates/save/tests/round_trip.rs` case building a multi-layer `AnimationStack` (varying weight/blend timers/`reverse_direction`/`clip_handle`, with a `root_entity`), round-tripping through `save_world → encode → decode → restore_world`, asserting every field survives at the same entity id.

#### SAVE-D4-NEW-01: Quicksave ring cursor advances even when the pre-save validation gate aborts the write
- **Severity**: MEDIUM
- **Dimension**: Validation Gates
- **Data-Loss Class**: irrecoverable-write (narrow trigger)
- **Location**: `byroredux/src/save_io.rs:396-421` (`SaveCommand::execute`)
- **Status**: NEW
- **Description**: For a blank-slot quicksave, `state.ring.advance()` runs at line 397 — *before* `validate_world`/`validate_form_ids` at lines 407-408. If validation fails, the function returns the abort message without ever writing — but the in-memory ring cursor has already permanently advanced. Nothing is corrupted by the failed attempt itself, but the round-robin invariant ("next quicksave lands one slot after the last *successful* one") is broken: each aborted quicksave burns a rotation with nothing written to back it.
- **Impact**: Explicit-slot saves (`save 3`) are unaffected. Within one session, repeated quicksaves while the world is transiently validation-failing (e.g. mid-scripted-sequence) each burn a ring slot; once a real save succeeds it lands further around the ring than the "one after the last real save" model assumes — in the worst case (failed attempts ≥ ring size) the eventual write overwrites an older genuinely-good save early, with no warning. Self-limiting: `SaveRing::resume` (`#1706`) recomputes the cursor from on-disk mtimes at every process start, so the desync cannot persist across a restart.
- **Related**: Adjacent to but distinct from `#1706`/SAVE-D3-02 (cursor persistence across restarts vs. this — cursor mutation racing ahead of the write it gates). No existing issue covers this specific ordering bug.
- **Suggested Fix**: Move `state.ring.advance()` after the validation gate — use a non-mutating peek for the abort-message path, and only call the mutating `advance()` once `issues.is_empty()` and the write is about to proceed.

#### SAVE-D6-03: `apply_player_pose` silently reverts a FlyCam-saved pose within one frame when the live session is in Character mode
- **Severity**: MEDIUM
- **Dimension**: M45.1 Live Load-Apply
- **Data-Loss Class**: silent-drop
- **Location**: `byroredux/src/save_io.rs:288-338` (`apply_player_pose`); interacts with `byroredux/src/systems/character.rs:358-454` (`camera_follow_system`)
- **Status**: NEW
- **Description**: The branch selection is gated on `pose.character_mode && character_now` — it only drives the player body when *both* the save-time and live mode are Character. When the save was captured in FlyCam mode but the live session is currently in Character mode, the fallback repositions only the `ActiveCamera` entity's `Transform` — it never touches the body. `camera_follow_system` (Stage::Late, every frame while `PlayerMode::Character`) unconditionally re-derives the camera position from the body's `GlobalTransform` + eye height with no awareness a pose was just restored, so the restored vantage is visible for exactly one frame and then silently overwritten. Same mechanism as the closed `#1874` (door-transition camera reversion), never patched for this path.
- **Evidence**:
  ```rust
  // save_io.rs:306 — only drives the body when BOTH modes match
  if pose.character_mode && character_now {
      if let Some(body) = body { /* ... */ return; }
  }
  // otherwise: camera-only fallback, body untouched
  crate::cell_loader::reposition_camera(world, pos, rot);
  ```
- **Impact**: A FlyCam-mode save reloaded into a live Character-mode session restores look direction (`InputState` yaw/pitch) but not position — the camera snaps back to wherever the untouched body sits, one frame later. No test exercises this saved/live mode mismatch (`player_pose_round_trips_flycam` and `player_pose_character_tracks_body` both keep modes matched).
- **Related**: Closed `#1874` (same mechanism, different trigger site — cell transition, not load).
- **Suggested Fix**: In the fallback branch, when `character_now` is true and a body exists, also relocate the body (mirroring `snap_character_body_to_camera`, camera→body direction instead of body→camera) so `camera_follow_system` re-derives the same restored position every subsequent frame. Simplest form: branch on `character_now` alone.

#### SAVE-D6-04: `build_form_id_remap` silently drops deltas for a saved `FormIdPair` no longer present in the reloaded cell — no diagnostic
- **Severity**: MEDIUM
- **Dimension**: M45.1 Live Load-Apply
- **Data-Loss Class**: silent-drop
- **Location**: `crates/save/src/driver.rs:143-178` (`build_form_id_remap`)
- **Status**: NEW
- **Description**: A saved `FormIdPair` that doesn't resolve in the reloaded cell (record removed from a plugin, cell content changed between save and load) is silently absent from the remap with zero logging. Every `MUTABLE_DELTA_COLUMNS` row keyed to that entity is then dropped by `ApplyFn`'s `filter_map`, equally silently — `apply_deltas`'s applied-count just comes back smaller, logged only as a bare aggregate number with no per-entity detail. The function's doc comment covers the "no form id at save time" case but not this one.
- **Impact**: A saved moved/customized object silently reverts to its ESM-authored defaults on live load with no trace in the log — undiagnosable without manually diffing the saved `FormIdPair` list against the reloaded cell. Low blast radius (plugin/cell content drift between a save and its later load isn't the common case). Arguably correct behavior (there's no valid target to apply the delta to), which is why this is MEDIUM (diagnosability gap) rather than HIGH (correctness bug).
- **Related**: Distinct from closed `#1847`/SAVE-04 (opposite direction — removed objects *reappearing*, not moved-object deltas silently failing to apply).
- **Suggested Fix**: Log the count (and, bounded, the identities) of saved rows that fail to resolve — mirroring the `log::warn!` already present in the same file's `FormIdComponent` save closure for the symmetric case.

### LOW

#### SAVE-D1-NEW-02: `restore_world`'s release-mode `insert_batch` bound-check gap is real but currently dormant
- **Severity**: LOW (dormant — no reachable production path)
- **Dimension**: Snapshot Completeness & Determinism
- **Data-Loss Class**: corruption-on-load (latent only)
- **Location**: `crates/core/src/ecs/world.rs:204-209` (`insert_batch`'s `debug_assert`, compiled out under `--release`); `crates/save/src/driver.rs:78-108` (`restore_world`)
- **Status**: NEW
- **Description**: If a decoded snapshot's `next_entity` were ever smaller than the highest entity id in one of its columns (a hand-tampered-but-CRC-valid file, or a hypothetical future `save_world` bug), `restore_world` would admit those rows silently in release builds — `insert_bulk` does no bounds check of its own, and the `entity < next_entity` guard is `debug_assert`-only.
- **Impact**: None today. Every non-test call site of `restore_world` is inside `#[cfg(test)]` modules; the live `load` path uses `restore_resources` + `apply_deltas`, which only insert through a remap table of already-live entity ids. Defense-in-depth gap on a currently-unreachable path.
- **Suggested Fix**: If `restore_world` (or an equivalent raw-id restore) is ever wired to a live command — the crate's own docs anticipate a future "loose/exterior save" `load` variant — promote the check to a real `Result`-returning validation rather than relying on the debug-only assert.

#### SAVE-D2-04: `LightSource` / `LightFlicker` have no dedicated save/load round-trip test
- **Severity**: LOW
- **Dimension**: Registry & (De)serialization Fidelity
- **Data-Loss Class**: none
- **Location**: `crates/core/src/ecs/components/light.rs`; registered at `byroredux/src/save_io.rs:181-182`
- **Status**: NEW
- **Description**: Both are registered and delta-columned but no test round-trips either type (unlike most of the rest of the registry). Both are flat structs (`f32`/`u32`/`[f32;3]`, no nesting/`Option`/`FixedString`/`EntityId`), so low-risk.
- **Impact**: A serde regression would currently surface only as a runtime visual bug, not a test failure.
- **Suggested Fix**: Add one assertion-bearing round trip, can piggyback on the existing `binary_registry_round_trips_including_scripttimer` test.

#### SAVE-D2-06: `ItemInstancePool` has no round-trip test but currently holds no real data (informational)
- **Severity**: LOW
- **Dimension**: Registry & (De)serialization Fidelity
- **Data-Loss Class**: none
- **Location**: `crates/core/src/ecs/resources/mod.rs:689-693,707-709`; registered at `byroredux/src/save_io.rs:193`
- **Status**: NEW (test-coverage note, not a live risk)
- **Description**: The doc comment asserts `ItemStack.instance` safety depends on `ItemInstancePool` round-tripping as a resource, but no test backs that claim. `ItemInstance` is currently a placeholder (`_reserved: ()`), so this is vacuously true today.
- **Suggested Fix**: No action required now; add a round-trip test in the same commit that gives `ItemInstance` real fields.

## Verified Clean — No New Findings (Full Detail in Per-Dimension Notes)

- **Dimension 3 (Disk Format & Durability)**: zero findings. Atomic-write dance, directory-fsync fix (`#1702`), ring-resume-from-mtime fix (`#1706`), header gate ordering, CRC scope, `parse_slot_filename` strictness, and minor-version advisory behavior all match spec exactly, `cargo test -p byroredux-save` 30/30 pass. Both `#1702` and `#1706` independently re-verified as real, production-wired fixes (not just closed-without-fix) via `git show` + `gh issue view` cross-check.
- **Dimension 5 (Frame-Boundary Capture & Off-Frame Apply)**: zero findings. Both CRITICAL-if-real hypotheses (torn-state capture mid-scheduler-tick; live load accidentally calling `restore_world`) were traced end-to-end through `Scheduler::run` and every call site of `restore_world`, and disproven. `capture_player_pose` ordering and load-drain single-shot semantics confirmed correct.
- **Dimension 4**: gate-bypass and equipment off-by-one (the two items flagged for extra scrutiny) both check out clean — exactly one production call site for `save_world`/`write_slot`, correctly gated; the equipment bounds check's `>=` comparison is mathematically correct.
- **Dimension 6**: apply ordering, remap correctness, idempotency, cell-resolve pre-flight, `AnimationPlayer`/`AnimationStack` exclusion, kinematic no-op safety, and the schema/cell-context guard all confirmed correct against live code.

## Known Open Issue (Cross-Referenced, Not Re-Filed)

- **`#1848` / SAVE-05** — A second `load` before the drain silently discards the first queued snapshot (`LoadCommand::execute` does `pending.0 = Some(snapshot)` unconditionally, no `is_some()` guard). Re-confirmed still reproducible in current code by Dimension 5. Still LOW, still OPEN.

## Regression Guards Discovered

| Test | Invariant it pins |
|---|---|
| `delta_columns_carry_only_session_stable_fields` (`save_io.rs`) | `MUTABLE_DELTA_COLUMNS` entries carry no `FixedString`/`EntityId`/session-local handle |
| `serde_default_on_saved_struct_requires_format_major_bump` (`#1714`) | Every save-participating type is scanned for a `#[serde(default)]` addition without a `FORMAT_MAJOR` bump — **currently has a coverage hole, see SAVE-D2-03** |
| `form_id_column_resolves_the_flagged_entry` / `registering_a_second_form_id_column_panics` (`#1845`) | `form_id_column()` keys off the explicit `is_form_id` flag, not an `apply.is_none()` heuristic |
| `form_id_restore_without_pool_errors_cleanly` (`#1716`) | Load returns `SaveError`, never panics, when `FormIdPool` is absent |
| `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` (`#1835`) | Every component `spawn_npc_entity` stamps is either saved or on the `REDERIVED_NOT_SAVED` allowlist — does **not** cover system-inserted-post-spawn components (see SAVE-D1-NEW-01) |
| `write_read_round_trip_and_atomic_rename` (`disk.rs`) | tmp file removed after clean write, final file round-trips |
| `cursor_after_newest_points_past_latest_mtime` / `resume_on_empty_dir_starts_at_zero` (`#1706`) | Ring cursor resumes from on-disk mtimes, not slot 0, on restart |
| `rejects_major_version_skew` | A header-only edit trips the version gate, not the CRC |
| `parse_slot_names` | `.tmp` / non-numeric slot files never register as loadable |
| `validation_catches_dangling_parent` | `>= next_entity` dangling-id detection (`round_trip.rs`) |
| `binary_registry_round_trips_including_scripttimer` | Cross-crate `ScriptTimer` round-trips through the binary's registry |
| `actor_values_survive_save_load_round_trip` (`#1834`/`#1835`) | `ActorValues` round-trips |
| `player_pose_round_trips_flycam` / `player_pose_character_tracks_body` | Pose restore correctness — **only for matched saved/live mode, see SAVE-D6-03** |
| `anim_player_root_entity_not_clobbered_by_delta_apply` | `AnimationPlayer.root_entity` survives full restore without delta-overlay clobbering |
| `delta_apply_reroutes_by_form_id_after_cell_reload` / `player_body_inventory_survives_live_load` | End-to-end live-load remap correctness |
| `full_world_round_trips_through_container` | Full container round trip (encode→decode→restore) |

## Doc-Rot Observations (Not Filed as Findings)

- `character.rs:33-35`'s `PlayerEntity` doc comment claims the player is "cleared by `cell_loader::unload_cell` when the player despawns (it's stamped with `CellRoot`)" — Dimension 6 confirmed the player body is never stamped with `CellRoot` (it's a session-lifetime entity, unaffected by cell teardown by design, which is what makes the `#1846` FormId-remap fix work at all). The comment is stale against current code but doesn't change runtime behavior.
- `docs/feature-matrix.md:169`'s `TD3-002` comment (Save/load M45/M45.1 shipped 2026-06-21) reads correctly — re-confirmed, not re-flagged.

---

Suggested next step: `/audit-publish docs/audits/AUDIT_SAVE_2026-07-16.md`
