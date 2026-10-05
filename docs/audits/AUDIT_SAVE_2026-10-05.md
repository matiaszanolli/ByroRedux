# Save / Load Subsystem Audit (M45 + M45.1) — 2026-10-05

**HEAD**: `a2c24b16e` · **Baseline**: `docs/audits/AUDIT_SAVE_2026-09-29.md` (HEAD `9fcfdc3fc`) ·
**Audited**: Dimensions 1, 2, 3, 4, 5 (every dimension had commits in the window) · **Unchanged since
baseline (skimmed)**: none at dimension level. Two sub-areas had no production change and were only
spot-checked: the extension-state payload (Dim 2: `byroredux/src/extensions/` and `crates/sdk/src/component.rs`
changed in tests only) and the container header gates (Dim 3: `snapshot.rs` changed only the `FORMAT_MAJOR`
constant and its ledger; `disk.rs` and `tests/round_trip.rs` are unchanged).

This run was part of `/audit-suite --preset comprehensive`. All five dimensions were analysed synchronously,
with no sub-agents. The scratch notes are in `/tmp/audit/save/dim_{1..5}.md`. The window is 313 commits; 12 of
them touch `save_io.rs`. The save paths changed in these places:
- `FORMAT_MAJOR` went from 30 to 32: v31 for #5042 (`ActorValue.base_authored`), v32 for #5017
  (`ActorControlState.unconscious` and `ReferenceState.control`).
- `Dead` and `ActorControlState` became replacing columns (#5027/#5052).
- Three load steps are new:
  - the #5054 park of unresolved snapshot rows;
  - the #5056 cinematic purge before teardown;
  - the #5034 worn-gear reconcile and #5058 faction reset on the player.
- Saves now use the LSCR loading cover (`e60911864`).
- `atomic_write` was hardened (#5143, #5163, #5164).

Dedup covered:
- open issues (`/tmp/audit/issues.json`, 97 rows);
- closed issues searched for each new finding (#4138, #3472, #3166 matched as relatives);
- today's sibling reports.

`AUDIT_GAMEPLAY_2026-10-05.md` routed GAME-D7-2026-10-05-01 (the completeness guard's reach) to this audit, so
it is owned here as SAVE-D1-2026-10-05-01. Its GAME-D1-2026-10-05-01 and GAME-D4-2026-10-05-01 are
cross-referenced, not re-filed.

## Executive Summary

| `lib.rs` / `snapshot.rs` design claim | Status |
|---|---|
| Full ECS snapshot of game state | **CODE-CONFIRMED for the registry, with residue.** No registered column changed this window. Every new runtime type is allowlisted with an accurate reason, except 8 fully-qualified impls the guard cannot see; all 8 are correctly transient (SAVE-D1-01). The load-side gaps from the baseline are closed: #5054 parks the hysteresis-band rows, #5056 stops cinematic retention, and #5052/#5027 clear saved absence on the player. |
| "Save size scales with loaded-cell entity count, never with playthrough length" | **DRIFTED (by design since P3).** Unchanged from the baseline. `PersistentReferenceStates` grows with mutated, evicted placements. Not filed. |
| Versioned container + CRC32 over the payload | **CODE-CONFIRMED.** Header gates are unchanged and their tests are green. |
| Atomic write (tmp → fsync → re-read+verify → rename → dir-fsync) | **CODE-CONFIRMED.** #5163 now also removes the temp on any failure. Residue: `write_slot` keeps a fixed temp name and has no no-clobber pin (SAVE-D3-01). |
| Ring never clobbers the last good save | **CODE-CONFIRMED.** `SaveRing::resume`; the cursor advances only after the commit. |
| Validation gate refuses to persist a poisoned save | **DRIFTED for the operator ingresses.** The mid-transition refusal (#4138) reads a once-per-frame flag. That flag is stale for the remote and overlay consoles when a door transition tears down without a loading cover (SAVE-D4-01). |
| `FORMAT_MAJOR` bump is the only schema-evolution path | **CODE-CONFIRMED** for v31, v32 and the one refresh without a bump (P4 `serde(skip)` deque). The reach gap #5059 is still open; no file in its blind spot changed this window. |
| Load runs off-frame | **CODE-CONFIRMED.** `step_save_loads` runs post-scheduler, now behind the LSCR save cover. `restore_world` has no production caller. |
| Additive overlay + an explicit reconciler per removal | **CODE-CONFIRMED.** `Dead`/`ActorControlState` are replacing. Player worn gear and factions are re-derived after the overlay. The new drain steps have no source-order pin (#5060, extended below). |
| Reproducible CRC at equal state | Doc disclaims it. #4748 is still open; `Globals` is still a `HashMap`. Cite, don't re-file. |

**Findings: 3 NEW — 0 CRITICAL, 0 HIGH, 1 MEDIUM, 2 LOW.**

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 1 | SAVE-D4-2026-10-05-01 |
| LOW | 2 | SAVE-D1-2026-10-05-01, SAVE-D3-2026-10-05-01 |

**Prior-cycle findings:**
- **SAVE-D1-2026-09-29-01 (HIGH, #5052): FIXED and verified.** `ActorControlState` is replacing (`save_io.rs:471`). The test `dead_and_restraint_cleared_on_live_player_by_saved_absence` is green.
- **SAVE-D5-2026-09-29-01 (HIGH, #5054): FIXED and verified.** Rows that do not resolve are parked as `ReferenceState` (`reference_state.rs:504-621`), and `unresolved_snapshot_rows_are_parked_for_the_next_respawn` is green. The park covers the eviction subset only. Band-cell `Transform`, activator and AI-procedure rows are still dropped, but in-session eviction drops them the same way, so this is parity rather than a regression.
- **SAVE-D5-2026-09-29-02 (HIGH, #5056): FIXED and verified.** `purge_cinematic_retention_state` runs after the preflight and before the teardown on both reload arms and both debug-load paths. Four `purge_wiring_tests` are green.
- **SAVE-D1-2026-09-29-02 (MEDIUM, #5058): FIXED and verified.** `reset_player_factions_to_record` runs after the overlay (`save_io.rs:1874`). `load_reset_strips_stale_alias_injected_player_factions` is green.
- **SAVE-D2-2026-09-29-01 (MEDIUM, #5059): still OPEN, unchanged.** The explicit list is still the four files (`serde_default_guard_tests.rs:106-111`). No commit touched `lighting.rs` or the sdk payload files in the window, so nothing has slipped through.
- **SAVE-D5-2026-09-29-03 (LOW, #5060): still OPEN, and its scope grew.** See "Existing, extended" below.
- **#4748:** still open. The doc is fixed; recommend closing it, as the baseline did.

## Data-Loss Class Matrix

| Finding | Class | Dim | Severity | Status |
|---|---|---|---|---|
| SAVE-D4-2026-10-05-01 — mid-transition save refusal is stale for the console ingresses (no-cover door transitions) | irrecoverable-write | 4 | MEDIUM | NEW (residual of closed #4138) |
| SAVE-D1-2026-10-05-01 — completeness guard blind to fully-qualified impls; `PendingGearRelease` outlives a load on the player | none (guard gap; latent presentation) | 1 | LOW | NEW (owns GAME-D7-2026-10-05-01) |
| SAVE-D3-2026-10-05-01 — `write_slot` left on the pre-#5143 shape (fixed temp name, no no-clobber pin) | irrecoverable-write (multi-process only; CRC catches it on load) | 3 | LOW | NEW |
| #5060 (extended) — new order-critical drain steps without a source-order pin | none (test gap) | 5 | LOW | Existing |
| #5059 — shape guard reach | none (latent) | 2 | MEDIUM | Existing |
| GAME-D1-2026-10-05-01 — loaded player's root-less armor re-imports at random | none (presentation) | 1/5 | MEDIUM | Existing (gameplay report) |
| GAME-D4-2026-10-05-01 — `restore` seeds a returning corpse's ragdoll before propagation; on the load path this is reached through `restore_resident` | corruption-on-load (physics pose) | 5 | MEDIUM | Existing (gameplay report) |

## Completeness Ledger (delta from baseline; see `AUDIT_SAVE_2026-09-11.md` for the full table)

Cross-checked against the guard's `NOT_SAVED_BY_DESIGN`, not re-derived.

| Column / type | Kind | Saved | Overlaid | Notes |
|---|---|---|---|---|
| `Dead` | Component | yes (**replacing**, #5027) | yes | A saved absence clears a live player death. Parked as `ReferenceState.dead` by both eviction and #5054. |
| `ActorControlState` | Component (v32 `unconscious`) | yes (**replacing**, #5052) | yes | `update_actor_control` keeps a row with `false` bools, so a woken #5017 robot carries a row and the overlay wins over the spawn-time `apply_starts_unconscious`. Parked as `ReferenceState.control`. |
| `ActorValues` | Component (v31 `base_authored`) | yes | yes | Shape change, bumped. |
| `PersistentReferenceStates` | Resource | yes | n/a (wholesale) | New load-time producer: `park_unresolved_snapshot_rows` (#5054). |
| `FactionRanks` | Component | no (REDERIVED_NOT_SAVED) | n/a | The player is now reset to the record after the overlay (#5058). |
| `PendingGearImport` | Component | no (allowlisted) | n/a | Removed from the player after the overlay (#5034). |
| `PendingGearRelease` | Component (#5028) | no (**unclassified**, guard-invisible) | n/a | Not removed on load (SAVE-D1-01). It has no production producer today (GAME-D1-2026-10-05-02). |
| `LoadingModelStage` | Component (LSCR) | no (allowlisted) | n/a | Reason accurate. Stage NIFs carry no `FormIdComponent`, so they cannot shadow a remap target. |
| `AmbientEngagement` | Component (moved to scripting, #5046) | no (allowlisted) | n/a | Reason accurate. |
| `TriggerOccupancyState`, `GracefulExitRequested`, `SceneEffectSoftCache`, `ScaleformHudDiag`, `ScriptProvider`, `HudControl`, `CellLoadPhaseTimings` | Resource | no (**unclassified**, guard-invisible) | n/a | Classified by hand: all are transient. See SAVE-D1-01. |

## Findings

### MEDIUM

#### SAVE-D4-2026-10-05-01: The mid-transition save refusal reads a flag published once per frame before `step_cell_transition` — a console `save` in the frame a cover-less door transition tears down writes a permanently unloadable save

- **Severity**: MEDIUM
- **Dimension**: Save-Side Gates
- **Data-Loss Class**: irrecoverable-write
- **Location**:
  - `byroredux/src/app_step.rs:847-861` (`step_player_save_actions` publishes `CellTransitionInFlight` from `interior_transition.is_some() || loading_screen.active()`);
  - `byroredux/src/app_events.rs:968`, `:1031`, `:1042`, `:1093` (frame order: `scheduler.run` → `step_player_save_actions` → `step_cell_transition` → `render_one_frame`);
  - `byroredux/src/app_step.rs:1001-1009` (a failed `loading_screen.begin` falls through to an immediate teardown), `:1031-1041` (`unload_current_interior` in the same call);
  - `byroredux/src/cell_loader/transition.rs:104-111` (the doc claims the flag "cannot go stale"), `:433-436` (teardown removes `CurrentCellContext`);
  - `byroredux/src/save_io.rs:891-939` (`SaveCommand::execute`: the gate is the only context check);
  - `byroredux/src/main.rs:1661-1668` (native-overlay console executes `CommandRegistry` inside `render_one_frame` via `app_frame.rs:276`).
- **Status**: NEW. Residual of the closed #4138 (HIGH), whose fix and pin (`the_save_drain_publishes_the_transition_flag_before_draining`) cover only the player-action drain.
- **Description**:
  - `CellTransitionInFlight` is derived state, synced once per frame at the head of `step_player_save_actions`. That is correct for the queued F5/pause-menu drain, which runs immediately after the sync.
  - `SaveCommand`'s own lock comment (`save_io.rs:892-903`) names two other production callers:
    - the remote console, which runs in `DebugDrainSystem` inside `scheduler.run`, *before* the sync;
    - the native-overlay console, dispatched from `render_one_frame`, *after* `step_cell_transition`.
  - The no-cover case: a door transition whose loading cover cannot start, because `begin_artwork` finds no `LoadedCellIndex`, no load screen or missing art. In that case `step_cell_transition` tears down in the same call:
    - `drain_streaming_state` clears the exterior context;
    - `unload_current_interior` removes `CurrentCellContext`;
    - `InteriorCellApply::begin` starts the budgeted apply.
  - The flag published earlier in that frame is still `false`. Two windows follow:
    - an overlay `save` later in frame N;
    - a remote `save` in frame N+1's scheduler, before N+1 republishes.

  Both pass every gate. No gate checks for a missing context, so the save commits with neither context. It consumes the ring slot or overwrites an explicit slot. `LoadCommand` then refuses it forever as a "loose save", which is exactly #4138's outcome.
- **Evidence**: with a cover, `loading_screen.begin` returns `Ok` and the teardown moves to the next frame's `step_cell_transition`, after that frame's sync publishes `true`. That path is safe. The cover-less `Err(pending) => pending` arm continues straight into `unload_current_interior` within the same `step_cell_transition` call. The flag has exactly one writer (`rg CellTransitionInFlight`).
- **Impact**: an operator or smoke script that runs `save [slot]` right after a door transition can write a slot that can never be loaded, destroying that slot's previous save, with a success message. It needs no loading cover and the one-frame window. Player input is not affected.
- **Related**: #4138 (closed); `docs/smoke-tests/p5-door-transition.sh` (cross-cell door saves; uses the remote console).
- **Suggested Fix**: make the refusal read the invariant itself rather than a derived flag. Refuse when a `LoadedCellIndex` is installed but neither `CurrentCellContext` nor `CurrentExteriorContext` exists. Alternatively, publish `CellTransitionInFlight` wherever `interior_transition` is assigned. Extend the pin to the console ingress: a world with an index and no context refuses `save`.

### LOW

#### SAVE-D1-2026-10-05-01: The registry-completeness guard only sees literal `impl Component for X` / `impl Resource for X` — 8 fully-qualified impls are unclassified, hand-added rows are never checked for staleness, and `PendingGearRelease` (the sibling #5034 drops) survives a load on the player

- **Severity**: LOW
- **Dimension**: Snapshot Completeness & the Two Lists
- **Data-Loss Class**: none (guard gap; one latent presentation path)
- **Location**:
  - `byroredux/src/save_io/registry_completeness_tests.rs:128-137` (`impl_target_type`), `:526` (stale `ItemEventBatch` reason);
  - `byroredux/src/npc_spawn.rs:1166` (`PendingGearRelease`);
  - `byroredux/src/save_io.rs:1869` (only `PendingGearImport` is removed);
  - `byroredux/src/app_events.rs:1036` vs `:1062` (`step_save_loads` precedes `gear_import_loader.step`).
- **Status**: NEW. This audit owns GAME-D7-2026-10-05-01 (Related), as `AUDIT_GAMEPLAY_2026-10-05.md` routed it here. It is the same class as the closed #3166 (guard reach).
- **Description**:
  1. `impl_target_type` strips only the two literal prefixes. `rg 'impl [a-z_:]+::(Resource|Component) for'` finds 13 fully-qualified impls. Five are classified only because a row was added by hand (`NpcEquipmentPart`, `NpcSkeletonBones`, `ActorBodyClass`, `PendingGearImport`, `DialogueSurfaceState`). Eight are classified nowhere: `TriggerOccupancyState`, `GracefulExitRequested`, `SceneEffectSoftCache`, `ScaleformHudDiag`, `PendingGearRelease`, `ScriptProvider`, `HudControl`, `CellLoadPhaseTimings`. Classified by hand today, all eight are correctly unsaved:
     - `TriggerOccupancyState` is `EntityId`-keyed detector scratch;
     - `GracefulExitRequested` is a process flag;
     - `SceneEffectSoftCache` is a render cache;
     - `ScaleformHudDiag` and `CellLoadPhaseTimings` are diagnostics;
     - `ScriptProvider` is an asset provider;
     - `HudControl` holds HUD debug pins;
     - `PendingGearRelease` is handoff scratch.
  2. The guard does not check the reverse direction (allowlisted name ↔ discovered impl). A hand row survives its type being renamed or deleted, and the hand rows above would stay green even if a type's impl were rewritten.
  3. `ItemEventBatch`'s reason ("no reader yet (#4713)") is stale: #5028's `queue_gear_releases` reads it (`loot_appearance.rs:306`). The classification is still right.
  4. Concrete consequence: after the overlay, #5034 removes the player's in-flight `PendingGearImport` because it would attach the pre-load session's gear. The sibling `PendingGearRelease` is not removed. It is queued in Late and drained by `gear_import_loader.step` *after* `step_save_loads`. A release queued in the load's frame would therefore despawn a gear root that `reconcile_worn_gear` just revealed for the loaded slots. Nothing re-imports it without a new equip event. The path is latent: GAME-D1-2026-10-05-02 shows the queue has no production producer yet.
- **Impact**: the next fully-qualified component or resource that does need saving passes the guard silently. Once a player-side item removal lands, a load in the same frame as a worn-item removal leaves the restored armor invisible.
- **Suggested Fix**:
  - Match any path prefix, e.g. `impl\s+(?:[\w:]+::)?(Component|Resource)\s+for\s+(\w+)`.
  - Allowlist the eight types with reasons.
  - Add the reverse check (every allowlisted name is discovered).
  - Fix the `ItemEventBatch` reason.
  - Remove `PendingGearRelease` from the player beside the `PendingGearImport` removal at `save_io.rs:1869`.

#### SAVE-D3-2026-10-05-01: `write_slot` is the one durable writer #5143/#5164 left on the old shape — a fixed `save_<slot>.ess.tmp` temp shared across processes and no `assert_no_clobber_fallback` pin

- **Severity**: LOW
- **Dimension**: Container & Disk Durability
- **Data-Loss Class**: irrecoverable-write (multi-process only; the CRC rejects the result on load)
- **Location**:
  - `crates/save/src/disk.rs:38-46` (`write_slot`: `final_path.with_extension("ess.tmp")`), `:5-8` (module doc: "a stray `.tmp` that the next save overwrites");
  - `crates/core/src/atomic_file.rs:19-25` (`atomic_temp_path`), `:93-121` (`assert_no_clobber_fallback`, used by `settings-io`, `boot-request` and `game-detect` only).
- **Status**: NEW. Related to #5143, #5163 and #5164 (closed), which hardened the other three writers, and #3472 (closed).
- **Description**:
  - #5143 gave every durable writer a unique hidden `.{name}.{pid}.{n}.tmp`. #5164 pinned each writer's production text against a clobbering fallback.
  - The save ring is the most valuable file these writers handle, yet `write_slot` still stages through a fixed sibling name and carries no pin.
  - Two engine processes that share a save directory (the default `<cwd>/saves`, with both ring cursors resumed independently from the same mtimes) can quicksave into the same slot and share one temp path:
    - A's `stage_and_rename` can pass read-back, then rename the inode that B has just re-created and is still writing;
    - the slot then holds B's bytes, or a torn file if B fails mid-write.

  The CRC refuses a torn file on load, and quickload falls back past it. The previous good save in that slot is gone either way.
- **Impact**: a lost slot when two instances run against one save directory. The memory note *no parallel engine launch* records that this happens in practice; the smoke gates avoid it only through `BYROREDUX_SAVE_DIR`. A future fallback branch in `write_slot` would also go uncaught.
- **Suggested Fix**: stage through `atomic_temp_path(&final_path)` and add an `assert_no_clobber_fallback(include_str!("disk.rs"))` test beside the other three. Update the module doc, since the temp is now removed on failure and no longer overwritten by the next save.

## Existing, extended (not re-filed)

- **#5060 (LOW, open)**: order-critical drain steps have no source-order pin. Since the issue was filed, three more steps landed in the same drain body, each covered only by unit tests:
  - the #5054 park, which must follow `restore_resources` (it releases instances from the *restored* pool) and precede `apply_deltas`;
  - the #5034 `PendingGearImport` removal and `reconcile_worn_gear`, which must follow the overlay;
  - the #5058 faction reset, which must follow the overlay and precede the first alias refresh.

  Only the #5056 purge has a source pin (`purge_wiring_tests`). Suggest widening #5060's pin to the full chain:
  `restore_resources` < `restore_resident` < `reseat_ambient_packages_after_restore` < `build_form_id_remap` < `park_unresolved_snapshot_rows` < `apply_deltas` < `reconcile_worn_gear` < `reset_player_factions_to_record` < `apply_player_pose`.
- **GAME-D4-2026-10-05-01**: `restore` runs `reconcile_dead_actor` (ragdoll) without the transform propagation that `reconcile_dead_actor_runtime_state` performs first (`combat.rs:660-667`). On the save-load path, `restore_resident` (`save_io.rs:1796`) reaches it too, for a parked dead row applied to a placement resident after the reload, for example a cell that was pending at save time. The overlaid-`Dead` reconcile itself is safe.
- **GAME-D1-2026-10-05-01**: the player's worn-gear re-derivation after the overlay is lossy when a weapon is equipped. Confirmed at the call site (`save_io.rs:1870`).
- **#4748**: open; recommend closing it (the doc half is fixed).

## Notes (not filed)

- A replacing `Dead` lets a saved absence override a newly recognised spawn-derived corpse. #5223 shipped at v32 without a bump, so a v32 save taken before it revives those placements on load (about 88 FO3 placements and 12 FNV placements). This is arguably the save's truth, and the window is dev builds only.
- `build_form_id_remap`'s WARN ("their saved deltas will not overlay") and the `apply_deltas` doc predate #5054. Unresolved rows are now parked.
- A parent-directory fsync failure after a successful rename returns `Err`. `SaveCommand` then reports "write failed" without advancing the ring, although the slot was replaced. This is conservative, and the next quicksave re-targets the same slot.
- An `extension_state` restore failure after the reload returns before `restore_resources`, leaving a fresh ESM world with the outgoing session's resources. This is pre-existing and unchanged; the preflight makes it unexpected.

## Guards Verified

| Guard | State |
|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | **green, reach gap** (SAVE-D1-01). Reasons spot-checked (10 rows): `CharacterLevel`, `AfflictionStatus`, `AiCombatState`, `AmbientEngagement`, `CombatDisposition`, `PickedUp`, `PlayerNotifications`, `TriggerVolume`, `FactionRanks`, `LoadingModelStage`. All are accurate; `ItemEventBatch` is stale. |
| `serde_default_on_saved_struct_requires_format_major_bump` | green |
| `saved_type_shape_changes_require_format_major_bump` | green. `BASELINE_MAJOR = 32 = FORMAT_MAJOR`. Both bumps landed in the same commit as the baseline refresh. The P4 refresh without a bump (`serde(skip)` `events`) is justified in a comment. Reach gap #5059 still open. |
| `set_in_chargen_renames_still_decode_v23_keys` | green |
| `delta_columns_carry_only_session_stable_fields`, `delta_columns_removed_at_runtime_have_a_load_reconciler`, `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` | green |
| `saved_resources_are_restored_before_the_cell_reload`, `pre_reload_restore_must_not_install_saved_item_instance_pool_early` | green. They do not cover the new steps (#5060). |
| `command_queue_tests::{a_save_taken_mid_cell_transition_is_refused_not_written, …chargen…, quicksave_ring_cursor_does_not_advance_on_validation_abort, player_save_actions_wait_for_the_quiescent_fifo_drain, quickload_empty_errors_and_corrupt_newest_falls_back}` | green. The mid-transition test sets the flag by hand, so it cannot see SAVE-D4-01. |
| `app_step::tests::the_save_drain_publishes_the_transition_flag_before_draining` | green (player drain only) |
| `dead_and_restraint_cleared_on_live_player_by_saved_absence`, `load_reset_strips_stale_alias_injected_player_factions`, `unresolved_snapshot_rows_are_parked_for_the_next_respawn`, `purge_wiring_tests` (4), `reference_state::tests` (12) | green |
| `crates/save` unit and integration tests (header gates, CRC, atomic write incl. `failed_rename_removes_the_temp` path, ring, replacing semantics, typed preflight) | green: 40 + 15 |

Commands run, all with `TMPDIR=/mnt/data/tmp`; no engine binary or GPU process was launched:
- `cargo test -p byroredux-save`: 55 passed.
- `cargo test -p byroredux --bin byroredux save_io` (toolchain 1.96.0): 70 passed, 4 ignored. The ignored tests are exactly the 4 real-master `consumable_tests`.
- `cargo test -p byroredux --bin byroredux -- the_save_drain_publishes purge_wiring reference_state load_reset_strips dead_and_restraint`: 18 passed, 1 ignored (the FO3/FNV corpse census needs game data).

## Summary per Dimension

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & the Two Lists | 1 LOW (+ cross-refs) | No two-list drift. Replacing semantics verified. Guard reach and staleness gap (owns GAME-D7-01). |
| 2 — Format & Schema Discipline | 0 (#5059 still open) | v31 and v32 are sound and recorded in the bump ledger. Extension payload unchanged. |
| 3 — Container & Disk Durability | 1 LOW | `atomic_write` hardening verified. `write_slot` was left on the old temp/pin shape. |
| 4 — Save-Side Gates | 1 MEDIUM | The #4138 refusal is stale for the console ingresses on cover-less transitions. |
| 5 — Live Load-Apply & Frame Boundary | 0 NEW (#5060 extended; GAME-D4/D1 cross-refs) | #5054 and #5056 verified fixed. Sequence matches the skill. |

Suggested next step: `/audit-publish docs/audits/AUDIT_SAVE_2026-10-05.md`. Use the domain label `save-load`. Add `test-gap` for SAVE-D1-01 and `gameplay` for SAVE-D4-01, which concerns cell transitions. Map SAVE-D3-01 to `save-load` alone.
