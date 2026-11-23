---
description: "Deep audit of the M45 save/load subsystem — full-ECS-snapshot capture, type-erased registry, format-version discipline, atomic disk write + ring, save-side refusal/validation gates, and the M45.1 live load-apply (cell reload + FormId-keyed deltas + player-pose restore)"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Save / Load Subsystem Audit (M45 + M45.1)

Audit `crates/save` (full-ECS-snapshot format) and its sole live caller `byroredux/src/save_io/` (`save_io.rs` is a ~133-LOC re-export hub over `registry` / `player` / `commands` / `live_load`, split in `d7c47d741`; it also holds `session_translator` / `translated`, the load-order scope every save and load runs its FormIDs through, see Dim 2)
(+ `save_io/*_tests.rs`) for **data loss and save corruption**. The subsystem exists to remove Bethesda's
slow-corruption tail by making the live ECS the single source of truth; verify the CODE delivers that.
A silently dropped column, a stale schema guard, a torn capture, or a botched FormId remap **loses player
progress** — frame as CRITICAL/HIGH per `_audit-severity.md` (data loss is CRITICAL).

**Architecture**: Orchestrator; each dimension is a Task agent (max 3 concurrent). Read
`.claude/commands/_audit-common.md` (layout, methodology, dedup, finding format) and
`.claude/commands/_audit-severity.md` first; do not duplicate them here.

## Scope

**Crate** `crates/save/src/` (~3.7k LOC — read all): `lib.rs` (design intent docstring, `SaveError`),
`snapshot.rs` (`Snapshot`, container header, `FORMAT_MAJOR` doc = the bump ledger, `encode`/`decode`),
`registry.rs` (`SaveRegistry`; `register_component` / `register_replacing_component` /
`register_component_preserving` / `register_resource` / `register_form_id_component`, and the context-aware portable-pair codecs `register_component_codec` / `register_resource_codec` (OAI-SAVE v133: encode / decode take the `World`, so a key is a `FormIdPair` on disk and resolves through `LoadOrderIdentity`; an unowned key is `SaveError::UnownedFormId`); `ValidateFn`; `schema_fingerprint`), `driver.rs` (`save_world`,
`restore_world`, `validate_snapshot_types`, `restore_resources`, `restore_resources_subset`,
`build_form_id_remap`, `apply_deltas`), `disk.rs` (`write_slot`, `slots_by_recency`, `SaveRing`, `reclaim_stale_staging_files`),
`validate.rs` (`validate_world` + 6 sub-checks). Tests: `crates/save/tests/round_trip.rs`.

**Engine side** (`byroredux/src/save_io/`): `build_save_registry` (the curated type set — a scripting subsystem's
saved columns live in its own module's `register_saved(&mut registry)` (`crates/scripting/src/*`: 56 files define 64 `register_saved*` fns, the hub makes 52 delegate calls into 46 modules; extracted in slices from #5802 `bde3478db` through `93bcda9bc`, then #6705 `15e498420` moved 47 more rows
out of the hub; umbrella #5802 closed 2026-11-07). The hub keeps 44 rows, each with a stated reason: core types and the FormId column (`byroredux-core` sits
below `byroredux-save`; `FactionReputation` #6759 `8fc1a9ef9`, `DismemberRecord` d50fc1153, `CharacterIdentity` (v125, the New Vegas chargen name/sex/race), `Sneaking` (v130), `SkillUseLevel` (v137, #7050) and `EquippedAmmo` (v141, #7055 `e15645976`, the player's arrow type, registered right after `EquippedWeapon`; a *replacing* column since #7228 `c82e4f68f`) and the `SessionProfile` resource (v142, #6670 `d3b208048`, registered right after `CurrentExteriorContext`; see Dim 5) are the newest; three component codecs `ActorValues` / `TimedRestorations` / `ActorVitals` and the `PersistentReferenceStates` / `SessionProfile` resource codecs are registered here too), the physics `CharacterController`, and the binary crate's own state. Each delegate is called at its
rows' former position, because `schema_fingerprint` hashes rows in registration order — moving a delegate call is a
`SchemaMismatch` for every existing save, and `the_schema_fingerprint_is_pinned` now fails it; where a module's columns sit
in separate runs of the order it exports `register_saved_<column>` too),
`MUTABLE_DELTA_COLUMNS` (**second** hardcoded list driving the live overlay), `PRE_RELOAD_RESOURCES`,
`SaveCommand`/`SaveInfoCommand`/`LoadCommand`, `PendingPlayerSaveActions` (F5/F9/pause-menu ingress),
`quickload_latest`, `execute_pending_save_loads` (+ `reload_interior_session`/`reload_exterior_session`),
`capture_player_pose`/`apply_player_pose`, `validate_form_ids`, `validate_cinematic_entity_refs`.
Cross-cut: `byroredux/src/boot/registries.rs` (installs registry/state), `app_events.rs` (per-frame
order: `capture_player_pose` → `step_player_save_actions` → `step_save_loads`), `app_step.rs`,
`byroredux/src/notifications.rs` (`PlayerNotifications`: bounded, transient, unsaved; drained in
`app_frame.rs::render_one_frame`), `byroredux/src/extensions/` (SDK extension-state capture/preflight/
restore layered around the save/reload — the extension *API* is `/audit-tooling`),
`byroredux/src/cell_loader/reference_state.rs` (`PersistentReferenceStates`, `without_parked_state`),
`cell_loader/{transition,spawn,unload}.rs` (`unload.rs`: `purge_cinematic_retention_state`, the
`player_owned_entities` teardown refusal), `crates/core/src/{atomic_file.rs,stable_form_id.rs,string/mod.rs,ecs/world.rs}`,
`crates/physics/src/sync.rs`. Companion doc: `docs/engine/save-load-roundtrip.md` (numbers the extension preflight/restore
steps as 1b/3b).

**Ownership split**: this skill owns schema, atomicity, validation, and load-apply *mechanics*. What loot /
inventory / consumable state *should* persist (gameplay semantics) → `/audit-gameplay`; reconcile-after-
overlay code lives in gameplay files but its **existence per removable column** is Dim 1's.

## Guard ledger (E2) — run these first; aim dimensions at what they cannot see

`cargo test -p byroredux-save` and `cargo test -p byroredux --bin byroredux save_io` (plus
`app_step` / `boot::schedule` sources named below). Only the four real-master tests in
`save_io/consumable_tests.rs` are `#[ignore]`d. Confirm each guard is live: not `#[ignore]`d, its scan
finds >0 items, and it fails when the invariant is broken.

| Guard | Pins |
|---|---|
| `registry_completeness_tests::every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | every `Component`/`Resource` impl in any `crates/*/src` (roots **discovered**, not listed) + `byroredux/src` is registered XOR in `NOT_SAVED_BY_DESIGN` with a reason; `PlayerNotifications`, `PickedUp` etc. are classified there |
| `serde_default_guard_tests::serde_default_on_saved_struct_requires_format_major_bump` | any `#[serde(default)]` on a save-participating type (skip-only fields exempt) |
| `serde_default_guard_tests::saved_type_shape_changes_require_format_major_bump` | hash of every `#[derive(..Serialize..)]` struct/enum body in `save_type_sources()` — discovered files that carry a `cfg_attr(feature = "save"` derive or define a registered type, plus every codec's wire-type module (the module of its save / load function path, derived by `codec_registrations`, and its column's type by name; D2-01) plus eight explicit non-turbofish / nested-payload files (`form_id.rs` ×2, `string/mod.rs`, `script_instance.rs`; since #5059 `945be7dc6`: `lighting.rs`, `crates/sdk` `identity.rs` / `script_function.rs`; since #6509 `c72ecafad`: `collision.rs`, whose `MotionType` is the payload of the saved `ReferenceMotionTypeState` and had silently left the scan when #6237 unregistered `RigidBodyData`); registered names are read from `save_io/registry.rs` and from every scanned file that defines `fn register_saved(` (#5802) — vs `BASELINE_SHAPE_FINGERPRINT`/`BASELINE_MAJOR`; the attribute scan reads the whole contiguous attribute block, so a save derive above a plain `#[derive(..)]` is seen (#5403 `e19cb0491`) |
| `serde_default_guard_tests::every_codec_registration_reaches_the_scan` | the codec-derived scan (D2-01 `01d390583`) fails on a `register_*_codec` call its parser cannot read |
| `serde_default_guard_tests::set_in_chargen_renames_still_decode_v23_keys` | `serde(alias)` rename path stays decodable |
| `round_trip_tests::delta_columns_carry_only_session_stable_fields` | no `FixedString`/`EntityId`/session handle in a `MUTABLE_DELTA_COLUMNS` type |
| `round_trip_tests::delta_columns_removed_at_runtime_have_a_load_reconciler` | every delta column with a production removal site has a declared reconciler or "NoReconcilerNeeded" reason |
| `round_trip_tests::npc_spawn_stamped_components_are_saved_or_intentionally_rederived` | spawn-stamped components registered or in `REDERIVED_NOT_SAVED` |
| `crates/save` `snapshot.rs`/`disk.rs`/`registry.rs` unit tests | header gates (`rejects_bad_magic`/`_truncated`/`_payload_truncation`/`detects_crc_corruption`/`rejects_schema_mismatch`/`rejects_major_version_skew`), atomic write, slot parsing, ring resume, recency tie-break, `form_id_column_resolves_the_flagged_entry`, replacing-column semantics |
| `tests/round_trip.rs::typed_snapshot_preflight_rejects_bad_column_without_world_mutation` | typed preflight precedes `clear_entities` |
| `live_reload_tests::{saved_resources_are_restored_before_the_cell_reload, pre_reload_restore_must_not_install_saved_item_instance_pool_early, pre_reload_restore_installs_the_relocation_ledger}` | pre-reload resource subset, #4135; `ActorRelocations` joined it in `7250362df` (the spawn path's `skipped_at_home` reads it mid-reload) |
| `live_reload_tests::{an_aborted_load_hands_the_live_ledgers_back, the_drain_captures_before_the_pre_reload_restore_and_reinstates_on_abort}` | `LiveReloadLedgers` (one field per `PRE_RELOAD_RESOURCES` entry, kept in step by a source guard) captured before the pre-reload restore and reinstated on every abort (#6081 `b5a205715`) |
| `live_reload_tests::live_load_drain_discards_the_outgoing_sessions_script_events` | the teardown's queued `OnCellDetach`/`OnUnload` deliveries are discarded and `ActorWatch` re-baselined after the session replacement (#6083 `4b8e62d87`) |
| `live_reload_tests::{live_load_drain_discards_the_cell_resets_its_reload_raised, live_load_drain_discards_the_outgoing_sessions_requests}` | the reload's own `note_player_entered_cell` must not leave `PendingCellResets` queued against the outgoing clock (#6238 `078df54e1`, `discard_pending_cell_resets`), and the value-keyed request queues (`PlayerMoveRequest`, `PendingPlayerXp`, `PendingSkillAdvances`, `PendingSleep`, `SleepCompletions`, the Oblivion bed/level-up menus' `RestSession` (reset to `Closed`, `250a5ba67`), `PendingMusic`, `PendingScriptMessages`, `MessageBoxQueue`, and the activate-choice box's `PendingActivateChoice`, PERK-HOOK-007) straddling the drain are cleared (#6239 `337bf25c8`, `discard_outgoing_session_requests`); the same sweep now also clears `PendingEvidenceDeposits` (#6573 `81332fc38`) and `PendingMineHits` (#6540 `bdaf75c66`), ends `VatsState` mode via `end_vats_for_world_change` (`systems/vats.rs`: `exit_all()` keeping `roll_state`; #6508 `5a423316f` — an F9 inside VATS left the loaded world frozen on despawned targets; #6625 `6186ab77e` made it the one helper shared with the player-death and cell-transition exits) and drops the crafting session, `CraftingRequest` and `PendingDroppedItems` (#6574 `375d48c6d`); later additions to the same sweep: the M65 detection transients `PendingDetectionEvents` / `PlayerActionNoise` (SAVE-6), the XCT queues `PendingRelocations` / `PendingActorMoves` / `TravelReports` (#6927 `75d0fe6a2`), `PendingScriptStoryEvents` / `PendingNonArmorEquips`, the scripted-speech queues and any `SpeakingLine` (the loaded save's own lines come back through the delta column, #6944), and the surviving player's `Seated` (INC4-1 `a69331ded`: `Seated` holds a furniture `EntityId`, not a delta column); #7229 `9e650b5c0` `ActiveProjectiles` + `PendingTrapHits` (shots in flight named the outgoing shooter and physics bodies; the test seeds an arrow in flight and a queued hit); #7230 `LevelNotices`, `CastState`, `BowState`, `StanceTimers`, `MineClock`; #7253 `RandomConversationState`, `DeadBodyState`, `AttractionState`, `JailEscapeWatch`, `ObScriptQuestTimers` (only boot ever built them; the guard's wording check caught them) — all through the `reset_resource` helper, pinned by the NOT_SAVED-reason guard rather than per type |
| `live_reload_tests::the_day_blocks_restart_before_the_reload_and_survive_an_aborted_one` | `PackageDayBlocks` (Once Per Day) is cleared by `reload_with_fresh_day_blocks` *before* the cell reload — the reload's spawns note their own blocks via `apply_ai_package_behavior`, which a post-reload reset in the drain erased — and an aborted reload ("keeping the current session") gets the live blocks back; source pin that the reload runs inside the wrapper (#7252 `9f31ab66f`) |
| `registry_completeness_tests::every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` (tail) | #6574: a `NOT_SAVED_BY_DESIGN` reason that says "a load discards" / "The load drain ends" its type must be named in the body of `discard_outgoing_session_requests` (scanned with comments stripped; `ActiveCrafting` is credited through `end_crafting` via `RESET_THROUGH_A_CALL`) — a claim the sweep does not back fails (the generic guard the earlier audits asked for). #7230 `48c6ea693` + #7253 `bf977bfb3`: the same promise in any wording — `a load <verb…>` or `<verb…>` within three words before `on [live] load`, the verb matched by root (`LOAD_RESET_ROOTS`; the matcher is itself asserted on wordings it must and must not catch) — is either reset by the drain or listed in `RESET_OUTSIDE_THE_DRAIN` with the mechanism that does it (`place_character_body`, `reload_with_fresh_day_blocks`, the cell reload's respawn, the sky install, ...); a listed row that no longer promises a reset fails too. A new reason with a load-reset promise whose mechanism you cannot find is the residue |
| `registry_completeness_tests::the_schema_fingerprint_is_pinned` | `BASELINE_SCHEMA_FINGERPRINT` vs `build_save_registry().schema_fingerprint()`: a registration-row reorder (the #5802 slices' risk) or a column add/remove/rename fails CI instead of rejecting every save (#6296 `5a3f571f4`) |
| `registry_completeness_tests::every_delta_column_is_parked_or_explicitly_not` | the **third** list: every `MUTABLE_DELTA_COLUMNS` entry is parked in a named `ReferenceState`/`ScriptState` field (`cell_loader/reference_state.rs`), carried by `ActorStreamSnapshot`, or on `NOT_PARKED` with a reason; every parked field maps to a column or `FIELDS_WITHOUT_A_COLUMN` (#6085 `3b9060701`); no `NOT_PARKED` reason may read `UNAUDITED` — the marker's last row, `Escorted`, is parked now (#6294 `f3ab49f55`) |
| `command_queue_tests::{a_save_taken_mid_cell_transition_is_refused_not_written, a_save_taken_while_chargen_disables_saving_is_refused_not_written, quicksave_ring_cursor_does_not_advance_on_validation_abort, player_save_actions_wait_for_the_quiescent_fifo_drain, quickload_empty_errors_and_corrupt_newest_falls_back}` | save-side refusal gates, ring, quiescent drain |
| `command_queue_tests::{a_save_with_a_non_finite_float_is_refused_not_written, quickload_falls_back_past_a_slot_whose_columns_do_not_decode, a_save_with_an_unowned_actor_value_key_is_refused_before_the_snapshot, the_console_save_refuses_in_the_tick_of_a_scripted_move, a_quicksave_waits_for_queued_actor_moves_to_reach_their_ledger}` | the 2026-11-17 audit's save-side gates: encoded bytes must decode + typed-validate before `write_slot` (SAVE-3 `8cfb70c44`), `LoadCommand` runs the same preflight so F9 falls back past an undecodable slot, `validate_actor_value_identities` (SAVE-5), console `save` refuses while `PendingRelocations`/`PendingActorMoves` are non-empty and the player drain waits at most `MAX_SAVE_DEFERRALS` (3) ticks (SAVE-4/-7 `d72e1f45e` / `110790dce`) |
| `live_reload_tests::a_live_load_rebaselines_the_players_perk_bodies`; `app_step.rs::a_save_load_forgets_the_respawn_step_keys`; `cell_loader/load_order.rs::{a_changed_load_order_discards_the_load_order_resolved_caches, the_reference_loader_discards_stale_caches_before_building_them}` | post-overlay `PerkBodyState` removal (`734bcb1fa`, a load re-ran perk Quest entries); `step_save_loads` forgets `created_step_key` / `relocation_step_key` / `enabled_step_key` after the drain (`c4d1ea3f2`); a live load that changes the plugin set drops the load-order-resolved rulesets before the reference loader builds them (SAVE-2 `0e8053225`) |
| `live_reload_tests::post_reload_drain_steps_keep_their_order` | source-order pin inside `execute_pending_save_loads`: `restore_resources` → `restart_day_blocks_at_loaded_clock` → `restore_resident` → `build_form_id_remap` → the `ScriptPackage` pre-seat `apply_deltas` → `reseat_ambient_packages_after_restore` → `drop_day_blocks_of_packages_not_running` → the full `apply_deltas(.. MUTABLE_DELTA_COLUMNS)` (#5060 `02fa8358c`; the remap and pre-seat moved ahead of the re-seat in #6087 `c7463c04d`; the `reconcile_worn_gear` single-call pin joined it, #5460 `bda5a1167`; the two Once Per Day steps are #7341 `48266eed6` (pinned by #7351 `f2ea5b7de`) and #7350 `7b3b9b6c2`) |
| `app_step.rs::the_save_drain_publishes_the_transition_flag_before_draining` | `CellTransitionInFlight` published before the drain |
| `crates/save/src/snapshot.rs` `the_bump_ledger_is_contiguous_and_current` | the `FORMAT_MAJOR` doc ledger has every `vN -> vN+1` step with no hole and ends at the constant — a bump that edits only the constant fails (#5459 `a4ccf3b27`) |
| `stable_form_id_guard_tests::{every_saved_column_is_classified_for_stable_form_ids, slice_two_shapes_follow_their_plugin_across_a_changed_load_order, script_variable_references_follow_their_plugin, pre_reload_ledgers_follow_their_plugin_across_a_changed_load_order}`; `round_trip_tests::{slice_one_form_ids_follow_their_plugin_across_a_changed_load_order, a_character_identitys_race_and_class_follow_their_plugin_across_a_changed_load_order}` | every registered column sits in exactly one of the guard's audited lists (`STABLE` / `NO_FORM_IDS` / ...; Rust has no field reflection, so a new column fails until classified), and representative columns of each shape (adapter field, wire type, codec, ledger) follow a shifted plugin and drop an absent one's entries (#6676 `49f58723c`, `070e124d2`, `c50456a03`) |
| `command_queue_tests::{a_quit_waits_for_the_quicksave_it_was_batched_with, a_new_save_state_reclaims_the_staging_files_a_crash_left}`; `disk::stale_staging_files_are_reclaimed_and_nothing_else`; `app_events.rs::the_quit_poll_asks_whether_a_save_is_still_queued_before_shutting_down` | `engine.quit` held up to `QUIT_SAVE_WAIT_TICKS` polls for a queued player save (#7353 `0ee5a68ac`); `SaveState::new` reclaims dead writers' staging files (#7354 `099740238`) |
| `live_reload_tests::{a_profile_difference_is_logged_and_never_shown_as_a_failed_load, a_load_reports_against_the_profile_captured_before_the_reload}`; `live_load.rs` unit tests `{absent_inventory_stacks_are_removed_in_place, the_removed_mod_report_names_each_plugin_once}` | the load's profile report goes to the log, never `notify_player`'s "Load failed." (#7347 `04c2c6b7f`), and compares against the profile captured *before* the reload (#7349 `5a8947a19`); removed-plugin inventory sweep and report (Phase 7 slice 3) |
| `*_survives_save_load*` (`round_trip_tests.rs`, `consumable_tests.rs`) | per-type round trips incl. lock/enable ledgers, fragment/provider queues, cinematic trio, `Perks`/timed restorations, SM node state + `StoryEventAliasFill` reference FormIds, the script-set player class (`a_script_set_player_class_survives_save_load`, v144) |

Not covered by any guard (the audit's real work): the **two-list drift** between registry and
`MUTABLE_DELTA_COLUMNS`; staleness of `NOT_SAVED_BY_DESIGN` *reasons*; whether the `NOT_PARKED` reasons of the eviction-row
partition are *true* (seeded from the delta list by #6085; #6294 `8fc521650` / `f3ab49f55` retired the `UNAUDITED` marker —
the guard now rejects it, and `Escorted` is parked; a delta column reverting on every cell exit is the GAME-SD-01/02 class); whether a baseline refresh
without a bump was *justified*; manual `impl Serialize` types (invisible to both serde guards);
the extension-state payload's own versioning (Dim 2); an absent package's own saved state (STATE-008: a removed plugin's rows are dropped and counted, never preserved opaquely for its return); semantic correctness of load-apply ordering beyond
the two pinned pre-reload cases. (Path-qualified `impl byroredux_core::ecs::Resource for X` impls were
invisible to `impl_target_type` until #5255 `6b494d002`: it now parses the trait path's final segment, the nine
formerly unclassified types are allowlisted, and the guard also asserts every `NOT_SAVED_BY_DESIGN` row matches a
discovered production impl — a renamed/deleted type can no longer keep a stale row. Still judge the *reasons*.)

## Parameters / Extra Fields

`--focus <dims>` (default all 5) · `--depth shallow|deep` (`shallow` = container/API contracts; `deep` =
trace capture → encode → disk → decode → reload → delta-apply). Finding fields: **Dimension**: Snapshot
Completeness | Format & Schema Discipline | Container & Disk | Save-Side Gates | Live Load-Apply ·
**Data-Loss Class**: silent-drop | corruption-on-load | irrecoverable-write | reference-break | none
(required for any finding that can lose progress).

## Phase 1: Setup

`mkdir -p /tmp/audit/save`; dedup: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/save/issues.json`;
read the newest `docs/audits/AUDIT_SAVE_*.md` (fixed findings are regression checks — verify the fix is
in place); read the `crates/save/src/lib.rs` docstring and `snapshot.rs` container doc — each design claim
must be CODE-CONFIRMED or DRIFTED; run the guard ledger. Delta-first: for each dimension
`git log --since=<last report> --format='%h %cs %s' -- <Paths>`, skim unchanged ones.

## Phase 2: Dimensions

### Dimension 1: Snapshot Completeness & the Two Lists (highest risk — silent-drop)
Paths: `byroredux/src/save_io/registry.rs` (`build_save_registry`, `MUTABLE_DELTA_COLUMNS`), `byroredux/src/save_io/live_load.rs` (`PRE_RELOAD_RESOURCES`), `save_io/registry_completeness_tests.rs`, `crates/save/src/{registry,driver}.rs`
First step: `git log -p -S'register_' --since=<last report> -- byroredux/src/save_io`; then `git log --since=<last report> --format='%h %cs %s' -- 'crates/*/src' byroredux/src | grep -i 'component\|resource\|state'` to find new persistent state.
- **The registry IS the completeness contract**; start from the guard's `NOT_SAVED_BY_DESIGN` list, not from a
  hand enumeration. A persistent, player-mutable component/resource that is neither registered nor
  allowlisted is a HIGH silent-drop; a *stale allowlist reason* (the guard checks a reason exists, not that
  it is true) is the auditable residue — spot-check a sample every run, and re-check any reason that names a
  "no production mutator" claim after new gameplay commits land (P3 loot/consumables/pickups, perks, timed
  restorations, hardcore mode, persistent reference state were all new saved state; `PlayerNotifications`
  and `PickedUp` are correctly transient — the durable half of a pickup is the `PersistentReferenceStates`
  tombstone). Recent false reasons, since fixed: `StoryEventAliasFill` ("quests restart
  through fresh events" — now registered, #5394 `03e51dfad`), `EatSleepState` ("`resolve_destination` is
  idempotent" — restated per PLDT anchor, #5391 `42aab4c09`), `CinematicReAdoption` ("the teardown handles
  them" — the purge now despawns them, #5379 `203be9ed4`); plus the Eat/Sleep rows' "registered Seated restore"
  (`Seated` is not overlaid — `restore_world`-only; restated and pinned by the `seated_without_path` check at the tail
  of the completeness guard, #5458 `0aa844dbb`) and `StoryLocationCursor` ("a load legitimately re-fires" —
  `execute_pending_save_loads` now resets it on every in-process load, #5435 `557ead21d`, pinned in
  `live_reload_tests.rs`); the 2026-11-17 audit's SAVE-6 (detection transients) and SAVE-9 (`Background` named the wrong writer, true for the race only until its class half was saved with the identity, #7348) and
  `PendingScriptedLines` (cfff182ea). The guard also catches a *missing* row: `PathGridTile` (Oblivion PGRD residency, the `NavmeshTile` posture)
  landed unclassified and went red until `45ad4ab69` allowlisted it with no format bump.
- **Removed-from-allowlist stays registered**: `CharacterController` (fractional breath/drowning carry),
  `Material`, `RumbleOnActivate`, `FragmentExecutionQueue`, the cinematic trio — verify none
  regressed out of `build_save_registry`, and no re-added `NOT_SAVED_BY_DESIGN` entry re-excludes them.
- **Two lists, one truth**: the live load overlays only columns in BOTH the registry and
  `MUTABLE_DELTA_COLUMNS`. A registered mutable column absent from the overlay is captured but never
  replayed → HIGH silent-drop, unless deliberately excluded and documented at its `register_component`
  site: `Material` (blast radius), `AnimationPlayer`/`AnimationStack` (stale `root_entity`/`clip_handle`),
  `FollowState`/`EscortState`/`Seated`/cinematic pair (`EntityId`, registry handles). `ActorVitals`
  is **in** the overlay since SAVE-1 `9d837aa3d` (its key is slot-composed since v133 and a live load can move the engine
  slot under the process-lifetime player; the old "write-once FormID key" exemption is the regression to look for). A column with an `EntityId`/handle must also be covered by
  `validate_saved_entity_references` (Dim 4).
- **Replacing columns**: `Perks`, `CharacterLevel`, `LevelUpPoints`, `SkillUseLevel` (v137), `ReadiedSpell` (v140, #7079 `9feaee8c5`, registered by `crates/scripting/src/magic.rs` after `SpellList`), `EquippedAmmo` (v141 column, replacing since #7228 `c82e4f68f`: the slot is created lazily by the first arrow equip, so a save from before it has no row and an additive overlay left the outgoing arrow equipped on the process-lifetime player; pinned by `round_trip_tests::a_saved_absence_clears_the_live_equipped_ammo`), `FactionReputation` (#6759), `TimedRestorations` (a codec row, `replace_missing`), `Dead`, `Sneaking` (v130), `ActorControlState`, `DismemberRecord`, `CharacterIdentity`, `SpeakingLine` (v132) use
  `register_replacing_component` (`Dead`/`ActorControlState` since #5027/#5052 `7aa1d7741` — the process-lifetime player
  outlives the reload, so an additive `Dead`/restraint row survived a load into an alive save; pinned by
  `dead_and_restraint_cleared_on_live_player_by_saved_absence`) — saved absence is
  authoritative for FormID-matched entities (empty column kept in the snapshot; a *missing* column is not a
  tombstone). Ordinary columns are additive-only, so **every runtime removal of an overlaid component needs a
  reconciler** (`reconcile_dead_actor_runtime_state`, `reconcile_player_equipped_weapon`); the guard enforces
  disposition, this dimension judges whether "NoReconcilerNeeded" reasons are true (carrier destroyed by
  the cell reload?) and whether a new persisted fact (disable, pickup, lock) chose the marker-plus-
  reconciler or FormID-keyed-ledger model (`ReferenceEnableState`, `ReferenceLockState`,
  `PersistentReferenceStates` — resources keyed by FormID survive cell unload; a component would not). A saved
  resource that needs session `EntityId`s takes the `StoryEventAliasFill` shape (#5394): `StoryEventFill`
  `serde(skip)`s its `EventDataSlots` and saves the R1/R2 reference FormIds, which the alias refresh re-resolves.
  The overlay emits no `EquipmentEventBatch`: presentation derived from an overlaid column is re-derived by
  `reconcile_worn_gear` (`byroredux/src/npc_spawn/loot_appearance.rs`, #5034 `20717d5b7`) — run on the player after
  `apply_deltas` and on NPCs from `reference_state::restore`; it diffs live roots against restored slots
  (reveal / hide / queue `PendingGearImport`) and synthesizes no event batch. A new presentation fact derived
  from an overlaid column needs the same treatment. Re-derived-not-saved player state: `FactionRanks` is reset
  to the record by `reset_player_factions_to_record` after the overlay (#5058 `be3cd9468` — alias-injected
  ranks otherwise outlived the load).
- **P3/P4 runtime state (2026-09-29)**: player body (`PlayerBodyRoot`, `PlayerBodyRootEntity`, `HiddenFirstPerson`,
  `PlayerCameraView`), mid-life gear import (`NpcSkeletonBones`, `ActorBodyClass`, `PendingGearImport`,
  `PendingGearRelease` — both dropped together on load by `clear_player_gear_handoff_scratch`, #5255 `6b494d002`,
  `PendingInventoryActions`) and NPC dialogue (`NpcDialogueTopic`, `DialogueSurfaceState`) are all
  `NOT_SAVED_BY_DESIGN`; `ObjectiveHudCache` is an `App` field, not a Resource. Verify each reason (body subtree
  entities have no `FormIdComponent`: captured by `save_world`, never remapped). `RigidBodyData` is
  `NOT_SAVED_BY_DESIGN` since v116 (#6237 `6765e60eb`): every carrier is a `PhysicsSourceForm` collision entity the live
  overlay can never resolve, so #2379's column only ever round-tripped through `restore_world`; a script's
  `SetMotionType` lives in the `ReferenceMotionTypeState` ledger (`PRE_RELOAD_RESOURCES`), applied by
  `cell_loader::spawn::scripted_motion_type` to each collider. A re-registered `RigidBodyData` column is the
  regression.
- **Determinism** (the `Snapshot` doc claims reproducible CRCs at equal state): `Snapshot` maps are
  `BTreeMap` and component rows are sorted by entity id (`registry.rs`), but saved **resources** serialize
  as-is — at 2026-09-19 `Globals(HashMap<u32, f32>)`, `QuestStageState`, `ReferenceEnableState` (`HashSet`),
  `ReferenceLockState` and `PersistentReferenceStates::pair_rows` all emit hash-iteration order (since joined by
  `DialogueSpokenInfoForms` (`HashSet`) and `StoryEventAliasFill` (`HashMap`)), so two saves
  of equal state can differ in bytes/CRC. #4748 is closed as a doc fix: the `Snapshot` doc in `snapshot.rs` now
  says the payload "is not guaranteed deterministic for equal logical state" (`Globals` is still a `HashMap`, by
  design of that resolution). A new claim of reproducible CRCs, or a consumer that diffs/hashes saves, reopens
  it (MEDIUM doc/contract mismatch, not data loss). Do not claim determinism at the row level without checking this.
- **`next_entity`** is saved verbatim and replayed via `set_next_entity` before inserts; `insert_batch`'s
  `entity < next_entity` is a `debug_assert` only (release inserts at an unspawned id silently — MEDIUM);
  `StringPool::dump`/`from_dump` preserves symbol order (a reordered dump = every `Name` wrong = CRITICAL).
**Output**: `/tmp/audit/save/dim_1.md`

### Dimension 2: Format & Schema Discipline (registry fidelity + `FORMAT_MAJOR`)
Paths: `crates/save/src/{snapshot,registry}.rs`, `save_io/serde_default_guard_tests.rs`, `crates/save/Cargo.toml`, `crates/core/Cargo.toml`
First step: `cargo test -p byroredux --bin byroredux serde_default_guard` ; `git log --since=<last report> --format='%h %cs %s' -- crates/save/src/snapshot.rs`
- **Bump rule** (read the `FORMAT_MAJOR` doc comment in `snapshot.rs`: 147 as of 2026-11-23, with one `vN -> vN+1`
  entry per bump from v22 on, contiguous since #5459 `a4ccf3b27` restored the v32→v33 entry (#5367 `14cff35ae`, new saved
  resource `DialogueSpokenInfoForms`) and pinned it with `the_bump_ledger_is_contiguous_and_current`. Recent anchors: v36 Wave 4 `RigidBodyData.surface_material`, v37 #5542
  widened it `u8`→`u32`, v38 #5300 re-keyed `DialogueSpokenInfoForms` to a (speaker ref, INFO) pair, and v39–v104
  are almost entirely M47.2 scripting state — new saved resource columns, `Effect` / `FragmentGuard` / `ValueExpr`
  variants and fields reachable through queued fragment continuations (`FragmentExecutionQueue`), `CharacterLevel`
  v46 (kill XP ended #2947's exemption), `Bleedout` v51, `LevelUpPoints` v53; v105–v132 are gameplay/ObScript
  fields and columns (`Bleedout.remaining_secs` → `Option` v105, parked `ReferenceState.bleedout` v106, M47.3 ObScript
  columns v109–v112, `Material.source_shader_type` v113 #4256, parked `ReferenceState` script package / corpse pose +
  `PersistentReferenceStates` script rows v114 #6071–#6073 `611769b16`, parked `ReferenceState` magic effects /
`weapon_drawn` / `ignore_crime` + `ScriptState` timer / rumble v115 #6085 `3b9060701`; `RigidBodyData` out,
`ReferenceMotionTypeState` in v116 #6237 `6765e60eb`; v117 #6190 `ObScriptLedger` lost four unread columns; v118 Oblivion
jail — `ObScriptLedger.stolen_goods` / `evidence` / `jail`; v119 the `CrimeKnowledge` resource joined the registry; v120 the `KnownWords` resource joined it and `Effect` gained `ObtainWord`; v121 `SmNodeRuntime.last_node_fire_hours`; v122 `ItemInstance.quality`; v123 #6759 `8fc1a9ef9` the `FactionReputation` replacing column (the stale "no mutation site" exemption); v124 d50fc1153 the `DismemberRecord` replacing column plus a parked `ReferenceState.dismember` field, VATS-LIMB-008/-009; v125 the `CharacterIdentity` replacing column plus the required `ActiveEffect.suspended`; v126 the `ActorRelocations` resource, XCT-LOC-003 #6852, and v127 its required `journeys` map, #6854; v128 `Effect::PlayBink` + `FragmentResumeCondition::MovieEnded`, #6858; v129 #6778 `9405138e9` `EquipmentSlots::occupants` 32 → 64 biped slots for Starfield's `BO64` mask — `MAX_BIPED_SLOTS`, serialized as a sequence of exactly 64 because serde derives stop at 32, so a v128 save's array is refused, not padded; v130 M65 #6910 the `Sneaking` replacing column, the player's sneak stance, SNK-SAVE-001; v131 M65 #6913 the required `ActorControlState.not_shown_on_stealth_meter` plus `Effect::SetNotShowOnStealthMeter`, SNK-DET-014; v132 #6944 the `SpeakingLine` replacing column, an ObScript `Say` / `SayTo` line in flight, delta column, not parked; v133 OAI-SAVE #7021 the portable `FormIdPair` actor-value codecs, a wire-shape change to six existing columns with no registry change; v134 M48 #6668 the `MenuEventRegistrations` resource; v135 M48 #6667 `Effect::ShowAppearanceMenu`; v136 #7044 the required `ActorValue.stored` layer (Oblivion's per-level Health gain); v137 #7050 the `SkillUseLevel` replacing column; v138 #7052 its required `pending`; v139 #6941 the required `ActiveEffect.effect_index`; v140 #7079 PSC-RDY-004 the `ReadiedSpell` replacing column, the player's readied spell; v141 #7055 BOW-AMM-002 the `EquippedAmmo` column, the player's equipped arrow type; #7228 then made it replacing with no bump — the policy is hashed into `schema_fingerprint`, so older saves are refused with `SchemaMismatch` and `BASELINE_SCHEMA_FINGERPRINT` moved, the serialized shape unchanged; v142 #6670 `d3b208048` (LMC-ID-007 / LMC-SAV-001) the `SessionProfile` resource codec column, the profile the save was made with; v143 #7309 `ed8c4c968` `ObScriptLedger` lost its unread `forms` column; v144 #7348 `CharacterIdentity` gained the required `class_form_id`, the player's script-set class, which `player_identity_sync_system` writes back onto the unsaved `Background` and a live load clears to the record's class when the saved identity carries none; v145 #6676 `49f58723c` / `013d128a4` (LMC-SAV-002..004) the first stable-FormID slice: `Inventory`, `EquippedWeapon`, `EquippedAmmo`, `Perks`, `FactionReputation`, `CharacterIdentity` (race and class), `SpellList` and `ReadiedSpell` save plugin-and-object identities instead of session-global numbers; v146 `070e124d2` slice 2: every other saved FormID the post-reload restore reads (quest / story / alias / ledger / crime / registration / cinematic / reference-state columns, the active-effect and parked-reference codecs, script-instance properties, reference variables kept as `f32` halves; `ScriptVariables` also lists its reference pairs); v147 `c50456a03` slice 3: the six `PRE_RELOAD_RESOURCES` ledgers, now that a load reloads the session's own plugins. `df5db4b46` moved `CharacterIdentity.class_form_id` onto the adapter with no bump: it still reads a bare number); do not hardcode the number elsewhere). Precedent: v112
  (#6050 `5f8bbf454`) — `register_saved` columns landed after a baseline refresh with no bump and the shape guard sat
  red on main; a new registered column's defining file joins the shape scan, so it is a bump here too. Intra-type shape changes need a bump because
  `schema_fingerprint` hashes only column keys (+ replacing policy). A new required field, retyped field, or new `Option` in a saved
  type bumps; `#[serde(default)]` is forbidden as a compatibility mechanism (guard) — even where the default
  would be correct for every old save. A saved map keyed by a tuple/struct cannot be a JSON object (`serde_json` refuses the key once it holds a row, failing the
  whole snapshot — the FNV `SetAlly` quicksave failure): such maps go through `byroredux_scripting::serde_pairs` as
  `[key, value]` lists, which also read an empty object as the empty map; `4156e68cd` moved the shape baseline with no bump
  on that ground (a non-empty map never wrote, so no old save can hold one) — judge any similar refresh against it.
  Read-compatible changes move the shape baseline **without** a bump:
  adding an enum variant to an externally-tagged `serde_json` enum (keyed by variant *name*, never ordinal —
  #4140 corrected the old "discriminant shift" rationale), removing a field (no `deny_unknown_fields` on any
  saved type), renaming with `serde(alias)`. Changing an existing variant's field shape is not compatible.
  Adding/removing a registered column changes the fingerprint and rejects old saves via `SchemaMismatch`
  with no bump. An audit proposing to relax the blanket rule for a "safe" default reopens #1714.
- **Baseline refresh triage** (the guard cannot judge this): every `BASELINE_SHAPE_FINGERPRINT` change must
  carry a justification comment stating whether a registered column's shape changed. Known false-positive
  classes: (a) the guard hashes *every* serialized-derive type in a scanned file, including `NOT_SAVED_BY_DESIGN`
  types sharing it (e.g. `AnimatedTextureFlip`; inspect-only files such as `WaterMaterial`'s left the scan in
  573170e1c) — those move the hash with no snapshot impact; (b) a **tuple struct** sweeps the following `impl` block into its span (`VisibilityMask`) — method
  edits move the hash; (c) the hash includes the file-relative path, so moving a save-participating type
  between files moves it. Recent no-bump refreshes of class (a): #7225 (`TravelBehavior` / `WanderBehavior`), #7254 `bfcca10be` (`GuardBehavior.near_editor_location`) and #7262 `5ede7b032` (`SandboxBehavior.near_editor_location`; `BASELINE_SHAPE_FINGERPRINT` = `0x372c_7a70_dcaa_a604` after the v145-v147 bumps; the #6667 refresh moved three shapes with no bump — `FragmentResumeCondition::MessageAnswered`, `VmStep::Replay` and a new `Option` field `VmResume.answer`, which an old save decodes only through serde's `Option` default-fill, safe there because v136 refuses every v135 save; #7352 `7c26406cb` made the comment say so, and the next new `Option` needs its own bump or argument), each a `NOT_SAVED_BY_DESIGN` sibling of a registered `*State` in the same file. Verify baseline and `FORMAT_MAJOR` were updated in the same commit, and that a
  refresh-without-bump was not hiding a real field change to a registered type.
- **Guard blind spots**: manual `impl Serialize` (no derive) types; a nested payload type of a plain-`derive`
  registered type (the `byroredux/src` pattern) defined in another file is outside `save_type_sources()` unless
  added to its explicit list (#5059 fixed in `945be7dc6` for `LightSource.emitter` and the `crates/sdk`
  `FormRef` / `ScriptValue` payloads — the next such nested payload is the residue; the extension-manifest
  declaration types sharing `script_function.rs` are exempt from the `serde(default)` scan, with a staleness
  test); the shape scan once skipped a save derive that was not the *last* `#[…]` attribute (#5403 fixed in
  `e19cb0491`, coverage markers assert `StoryManagerNodeState` / `SmNodeRuntime` contribute shapes); a new `Option<T>` field is hashed but
  looks like a routine baseline bump — confirm it got the bump; `serde(alias)` must never coexist with a
  dropped value semantic change. Neither guard sees a change to how a saved *id* is derived: `PluginId::from_filename` now hashes `plugin_key` (case-folded basename, full Unicode `to_lowercase`; 285633f27, #7249 `6438f700a` documents that a name with a non-ASCII capital changed identity, so ids saved against the old ASCII-only key do not resolve) with no `FORMAT_MAJOR` bump — judge any such change by hand.
- **Second payload — extension state** (`Snapshot.resources["ByroExtensionState"]`, written by
  `extensions::capture_extension_state`, type `ExtensionStateSnapshot` in `crates/sdk/src/component.rs`):
  it is outside the registry, the completeness guard, and both serde guards (sdk types carry a plain
  `derive`, so `save_type_sources()` drops them), and follows its **own** policy — a `format_version` range
  (`MIN_EXTENSION_STATE_FORMAT_VERSION..=EXTENSION_STATE_FORMAT_VERSION`) with `#[serde(default)]` fields, which
  the blanket rule forbids for registry types. Verify: the range gate runs in `preflight_extension_state`
  (before teardown); every shape change to its rows bumps `EXTENSION_STATE_FORMAT_VERSION` (nothing enforces
  it — no shape baseline); rows without stable authored identity refuse the save rather than dropping;
  `decode_saved_state` treats a missing key as empty but a present-and-malformed one as an abort.
- **`ValidateFn` parity**: every `register_*` variant builds a `validate` closure that decodes the SAME type
  `load` does (`Vec<(u32, T)>` for columns, bare `T` for resources) — a missing/wrong one exempts a column
  from the preflight so it fails only after teardown.
- **Feature chain**: `crates/save` → `byroredux-core` `features=["save"]` (→ `inspect`) must still hold, or a
  non-default build compiles away serde impls and columns serialize to `null`.
- **Stable FormID scope** (`crates/core/src/stable_form_id.rs`, v145-v147): a saved FormID field marked `#[serde(with = "stable_form_id")]` (or `::option` / `::vec` / `::map`, or carried through a wire type / codec) is written as a `FormIdPair` while a `FormIdTranslator` is installed (`byroredux/src/save_io.rs::session_translator` = the session's `LoadOrderIdentity`, `translated(..)` wraps `save_world` in `SaveCommand::execute` and each restore / apply step of the drain), and read back through the *current* load order: a pair whose plugin is absent reads as `0` for a field and is dropped from a list or map, counted in the `TranslationReport`. A FormID with no owner (0, an engine-made reference) stays a bare number, and a bare number reads unchanged, so a save without a translator (a unit-test world) is the plain integer form. A new saved FormID field not marked, or a decode outside `translated(..)` (the typed preflight deliberately has none: pairs read as null there and count nothing), is the regression; `stable_form_id_guard_tests` classifies every column but cannot see a field inside a classified one. Moving a field onto or off the adapter moves the shape hash; the adapter's own untagged `Repr` is not in `save_type_sources()`.
- **FormId handle vs pair**: `register_form_id_component` saves the stable `FormIdPair` (skips unresolvable
  handles with WARN, never panics); load re-interns to fresh handles and errors cleanly without a pool.
  `form_id_column()` is keyed by the explicit `is_form_id` flag, never "first `apply: None` entry".
- `FnvHasher` uses canonical FNV-1a 64 constants and hashes only names/order/policy (no TypeId/address).
**Output**: `/tmp/audit/save/dim_2.md`

### Dimension 3: Container & Disk Durability
Paths: `crates/save/src/{snapshot,disk}.rs`, `crates/core/src/atomic_file.rs`, `crates/save/tests/round_trip.rs`
First step: `cargo test -p byroredux-save disk:: snapshot::`
Guarded (see ledger): header gate order (length → magic → major → schema fingerprint → `payload_len`
checked bounds → CRC over the payload only → `serde_json::from_slice`), advisory `minor`, slot-name strictness
(`save_42.ess.tmp` rejected), `slots_by_recency` newest-first with slot-number tie-break, ring wrap/resume (`SaveRing::resume` uses `slots_by_recency`'s tie-break, higher slot wins at equal mtimes — SAVE-8 `110790dce`, `resume`'s old `max_by_key` followed arbitrary `read_dir` order).
Residual checks:
- **`atomic_write`** (`crates/core/src/atomic_file.rs`; shared with `crates/settings-io`,
  `crates/boot-request`, `crates/game-detect/src/overrides.rs`, the terrain-height cache
  `byroredux/src/asset_provider/terrain_height.rs` and the texture-upscale manifest writer
  `tools/texture-upscale/src/lib.rs`) order is exactly: create tmp → `write_all` →
  `flush` → `sync_all` → byte-exact read-back → `rename` (all inside `stage_and_rename`) → parent-directory
  fsync. **Any** failure up to the rename removes the temp (#5163 `69fd54fb2`; `failed_rename_removes_the_temp`),
  and no caller may fall back to copying/renaming the temp over the target on error — the old "Windows rename"
  fallback clobbered good files on disk-full (#5143 `2e95f0bbf`). The five non-save writers pin that with
  `assert_no_clobber_fallback` (production text only, via `core::source_scan`; the last two since #7355 `080788891`, the
  guard needs the staging path bound as `temp_path`); `write_slot` is pinned by
  `write_slot_has_no_clobber_fallback` and stages through the unique `atomic_temp_path` (#5247 `eb08e14fd`;
  the old fixed `save_<slot>.ess.tmp` raced two processes sharing a save dir) after `create_dir_all`. A crash between creating the staging file and renaming it leaves a `.save_<slot>.ess.<pid>.<n>.tmp` the
  slot scans ignore; `SaveState::new` reclaims the ones at least ten minutes old (`disk::reclaim_stale_staging_files` / `STALE_STAGING_AGE`,
  #7354 `099740238`; removes only a regular file of exactly that name shape, never a slot, another writer's temp or a near-miss name; `stale_staging_files_are_reclaimed_and_nothing_else`; the panic-unwind guard and exclusive create the finding also named stay with the shared helper, #7158 / #7143). Rename-before-fsync, a length-only read-back, or a new fallback branch is a HIGH
  durability hole.
- **Ring never clobbers the last good save**: `SaveState::new` must build the ring via `SaveRing::resume`
  (not `new`), and the cursor advances only after a committed write (`quicksave_ring_cursor_does_not_advance_on_validation_abort`).
- **Minor-version + default-fill**: `decode` accepts newer minors and serde default-fills; that is the exact
  path `#[serde(default)]` would exploit — cross-check Dim 2's guard is live.
- `SAVE_DIR_ENV` (`BYROREDUX_SAVE_DIR`) redirects the ring; smoke gates depend on the spelling.
**Output**: `/tmp/audit/save/dim_3.md`

### Dimension 4: Save-Side Gates (refusal + validation)
Paths: `byroredux/src/save_io/commands.rs` (`SaveCommand::execute`, `validate_form_ids`, `validate_cinematic_entity_refs`), `crates/save/src/validate.rs`, `byroredux/src/app_step.rs` (`step_player_save_actions`)
First step: `grep -n 'fn validate_\|validate_[a-z_]*(world' crates/save/src/validate.rs byroredux/src/save_io/commands.rs`
- **Order in `SaveCommand::execute`**: refusals first (`CellTransitionInFlight` — a mid-transition save would be
  written and permanently unloadable; `CinematicPresentationState.disable_saving` — `Game.SetInChargen`
  `abDisableSaving`; plus the context-invariant refusal #5253 `27562f8ae` — `LoadedCellIndex` present but neither
  `CurrentCellContext` nor `CurrentExteriorContext` means an unloadable snapshot even when the synced flag still
  reads `false`, as in the console ingresses that dispatch outside the drain), then `validate_world` + `validate_form_ids` + `validate_cinematic_entity_refs` + `validate_actor_value_identities`
  (SAVE-5: dry-runs the portable codecs, so an unowned actor-value key aborts naming its entity / column / parked reference instead of failing inside `save_world`)
  (abort, up to 20 lines, nothing written), then `save_world` (inside `translated(session_translator(world), ..)`, LMC-SAV-002..004: marked FormIDs are written as stable identities) → `capture_extension_state` → `encode` → a `decode` + `validate_snapshot_types` of the encoded bytes
  (SAVE-3: `serde_json` writes a non-finite `f32` as `null`, which no `f32` field decodes — a NaN saved through `setav`/`modav` used to commit an unloadable slot; refuses with no write and no ring advance) →
  `write_slot`; ring advance and the `SaveComplete` session event only after the commit. The console `save` additionally refuses (SAVE-7) while `PendingRelocations` / `PendingActorMoves` hold entries — they reach `ActorRelocations` only when `step_actor_relocations` drains later in the tick — and the queued player quicksave waits up to `MAX_SAVE_DEFERRALS` = 3 ticks for the same (SAVE-4); `engine.quit` / a script *QuitGame* in the same batch no longer drops that deferred save: `graceful_exit_due` (`app_events.rs`, the shutdown poll's one decision) holds while `quit_waits_for_a_queued_save` sees a queued player save action (`save_io::player_save_actions_pending`), at most `QUIT_SAVE_WAIT_TICKS` (`MAX_SAVE_DEFERRALS + 2`) polls counted in `GracefulExitRequested.1`, so a queue that never empties cannot keep the process alive (#7353 `0ee5a68ac`); `LoadCommand` runs the typed preflight when it queues a slot, so `quickload_latest` falls back past one that does not decode (SAVE-3). Any alternate save
  path that bypasses the gates (console, player action, SDK) is HIGH; input adapters must enqueue through
  `queue_player_save_action` (post-scheduler drain — `save_world` takes ~30 storage + resource read locks, so
  a live scheduler lane needs the ABBA analysis re-derived; `/audit-concurrency`). Current ingresses: the
  winit F5/F9 intercept, the pause menu, and the debug `input.press quicksave|quickload`, which resolves
  through `ActionBindings` to the same F5/F9 edges (`queue_debug_action_press`, `byroredux/src/interaction.rs`,
  `bae84e54b`; `quicksave_press_joins_the_player_save_queue_through_the_f5_binding`).
- **Coverage**: `validate_world` currently runs hierarchy, equipment, saved-entity references (session-local
  `EntityId` fields of columns excluded from the overlay — grep the function for the live column list),
  animation (`AnimationPlayer`, `AnimationStack` root + layer clips, `Seated.animation_restore`),
  inventory instances vs `ItemInstancePool`, and `Material` finiteness (the progression check
  `validate_progression_state` retired in `dbe562c1f` when `CharacterLevel` became a saved replacing column, v46);
  the binary adds FormId resolvability and cinematic `EntityId` refs. Do
  not restate the list elsewhere — enumerate **inter-entity reference fields in any newly registered type
  not covered** (MEDIUM defense-in-depth gap) and any check that flags a legitimately sparse-but-spawned id
  (dangling = `>= next_entity`, not "no components").
- **Post-load validation is diagnostic-only** (`log_validation_warnings`, WARN, no abort) in both
  `restore_world` and `execute_pending_save_loads`; the **typed preflight aborts** (before teardown).
- **Transient state**: saves must not depend on transients — `PlayerNotifications`, pending action queues,
  `CellTransitionInFlight`, parked `StreamStateSnapshots` are unsaved by design; a save taken while one is
  non-empty must still round-trip (check new transient resources land on the allowlist, not the registry).
**Output**: `/tmp/audit/save/dim_4.md`

### Dimension 5: Live Load-Apply & Frame Boundary
Paths: `byroredux/src/save_io/live_load.rs` (`execute_pending_save_loads`, `reload_*_session`), `byroredux/src/save_io/player.rs` (`apply_player_pose`), `crates/save/src/driver.rs`, `byroredux/src/{app_events,app_step,app_frame}.rs`, `byroredux/src/cell_loader/{transition,reference_state,unload}.rs`
First step: read `execute_pending_save_loads` top to bottom against the sequence below; `git log --since=<last report> --format='%h %cs %s' -- byroredux/src/save_io`
- **Strict apply sequence**: drain slot → `validate_snapshot_types` (abort, session kept) →
  `preflight_extension_state` (abort) → `restore_resources_subset(PRE_RELOAD_RESOURCES)` (read through `translated(..)` like every later decode; only resources
  read by spawn-time code during the reload: `ReferenceEnableState`, `ReferenceLockState`, `ReferenceScriptState`,
  `ScriptedFactionChanges` — #5919 `aa14d3e89`, replayed by `stamp_quest_reference` as NPCs respawn —
  `ReferenceMotionTypeState`, #6237, read by `scripted_motion_type` as each collider is built, and `ActorRelocations`,
  XCT-LOC-002 `7250362df`, read by `cell_loader::relocated::skipped_at_home` as each home-cell reference is placed;
  `without_parked_state` also installs the `SaveReloadTeardown` marker so the teardown's `depart_unwatched` starts no offscreen journey in that ledger;
  `pre_reload_restore_installs_the_scripted_faction_ledger`, `pre_reload_restore_installs_the_relocation_ledger`; **never**
  `ItemInstancePool` — teardown's `release_victim_item_instances` needs the live pool, #4135; the live values are
  captured first as `LiveReloadLedgers` and reinstated if this restore or the reload below fails/refuses, so a
  "keeping the current session" abort does not leave the other timeline's ledgers live — #6081 `b5a205715`) →
  `without_parked_state` wraps the reload (outgoing session's `PersistentReferenceStates` and
  `StreamStateSnapshots` are set aside and restored on failure) → teardown + reload of the **session's own plugins** (LMC-SAV-002/003, `c50456a03`: `session_plugin_list` overwrites the snapshot context's `masters` / `esm_path`, so a mod removed since the save no longer refuses the load and one added since is loaded; only the cell or worldspace comes from the save; run inside `reload_with_fresh_day_blocks`, which takes the outgoing `PackageDayBlocks` first so the reload's own Once Per Day notes survive and restores them if the reload aborts, #7252 `9f31ab66f`; `validate_cell_loadable`
  preflight first, so a missing ESM/cell keeps the live session; then `purge_cinematic_retention_state`
  drops every `ActorCinematicState`/`HorseTetherState` row so the convoy is not retained across the teardown
  as a ghost twin sharing a `FormIdPair` — #5056 `a197e8563`, both reload arms + both debug-load paths, never
  ordinary cell transitions (`purge_wiring_tests`) — and despawns + clears the unsaved `CinematicReAdoption`
  pending list, whose un-rooted entities the `CellRootIndex` teardown cannot enumerate (#5379 `203be9ed4`).
  Since #5384 that list holds parentless roots only, so the purge (now taking the `VulkanContext`) collects each
  root's `Children` closure and hands it to `release_entities` — subtrees no longer orphan (which made
  `validate_world` refuse every later save) and GPU/Rapier handles are released, #5490 `99cd9fd6c`. The teardown never
  despawns the player rig: `unload_cell_inner` refuses `player_owned_entities`, #5483 `5a68e983e`)
  → `reset_story_location_cursor` + `discard_pending_story_events` (the unsaved CLOC cursor and any straddling
  `StoryEvent` markers describe the outgoing session; every load behaves like a fresh boot, #5435 `557ead21d`)
  → `discard_package_reports` (outgoing actors' package transitions must not run PF_ Begin/Change fragments in the
  loaded session, #5864 `001de8741`) → `discard_pending_unload_deliveries` (the teardown's own `OnCellDetach`/
  `OnUnload` queue; Bethesda raises no unload event for a load, #6083 `4b8e62d87`) →
  `discard_pending_cell_resets` (the reload's cell-root `note_player_entered_cell` calls ran against the outgoing
  `CellVisits`/clock; the restored `CellVisits` is authoritative, #6238 `078df54e1`) → `discard_outgoing_session_requests`
  (value-keyed `PlayerMoveRequest` / XP / skill / sleep / music / message-box queues that would otherwise apply in the
  loaded world and be kept by the next save, #6239 `337bf25c8`; plus jail-evidence deposits, mine hits, VATS mode (`end_vats_for_world_change`), the crafting session
  and pending dropped items, #6573 / #6540 / #6508 / #6574; the test pins all but the mine queue; plus the `reset_resource` resets of #7229 / #7230 / #7253 — projectiles in flight, trap hits, level notices, cast / bow / stance / mine timers, the M43 ambient schedulers and ObScript quest cadence; `PackageDayBlocks` is deliberately *not* here, see above)
  → `restore_extension_state` → wholesale
  `restore_resources` **again** (idempotent; re-asserts `CurrentCellContext`/`PlayerPose`, after which the session's plugin list is stamped back onto both contexts so the next save records what is running; the second call is
  not redundant) → `restart_day_blocks_at_loaded_clock` (the reload's Once Per Day blocks were noted under the outgoing clock, #7341 `48266eed6`) → `restore_resident` (parked rows for resident placements, #4695; followed by `combat::release_held_dead_actor_reconciliations`, which hands a corpse's held reconciliation to the Late-stage sink now that its placement is resident, #6133 `1e2c1d30e`) → `drop_entity_bound_continuations` on `PapyrusProviderContinuationQueue` (session-local
  `EntityRef` handles must not resume, #4139) → `build_form_id_remap` → a `ScriptPackage`-only `apply_deltas` pre-seat (the ambient selection honours a running
  script package over the schedule, so re-seating without it stamped the scheduled winner and the next minute's tick
  cleared the freshly overlaid `TravelState`/`Traveled` markers and reported a spurious package-change fragment; the column
  is replacing and idempotent, so the full overlay re-applies it harmlessly — #6087 `c7463c04d`) → `reseat_ambient_packages_after_restore` (re-pick packages
  against the restored clock *before* the overlay, #4815; `drop_day_blocks_of_packages_not_running` then keeps a restarted block only for the package its actor ended up running, #7350 `7b3b9b6c2`; its order against `restore_resident`, the remap, the pre-seat and the
  overlay is pinned by `post_reload_drain_steps_keep_their_order`, #5060 `02fa8358c`) → park the snapshot rows
  whose `FormIdPair` did not resolve, minus those of a removed plugin (`removed_plugins` partitions them out: a reference of a plugin this load order lacks never streams in, so its rows are dropped and counted, not parked; `unresolved_form_id_pairs` → `park_unresolved_snapshot_rows` into
  `PersistentReferenceStates`, #5054 `17e720430` — the exterior reload streams only `radius_load`, so the
  save's hysteresis-band rows were dropped and their loot duplicated; `unresolved_snapshot_rows_are_parked_for_the_next_respawn`)
  → `apply_deltas(MUTABLE_DELTA_COLUMNS)` → `sweep_absent_inventory_stacks` (stacks whose base form read back as absent are zeroed in place, instances freed, slots released) + `removed_mod_report` (per removed plugin: absent forms, left-out references, removed stacks; WARN + `notify_player`, which draws only `LOAD_FAILED`: the #7347 class, check it) + `profile_change_report` (references an added plugin edits whose saved state masks the edit, references a reorder gave another winner, Start Game Enabled quests the save had not seen; log only) → dead/equipped-weapon reconcilers, `reconcile_worn_gear` (called
  once; the duplicate was dropped in `a8b3d1902` and the count pinned by #5460 `bda5a1167`), `reset_player_factions_to_record` → `reapply_scripted_faction_changes` (restored scripted memberships on top); the player's `PerkBodyState` is dropped after the overlay so the next `perk_body_system` pass baselines on the loaded perks instead of re-running every extra rank's Quest entries (`734bcb1fa`) →
  `rebaseline_actor_watch` (outgoing bleedout/combat sets and player location must not deliver across timelines,
  #6083) → diagnostic `validate_world` + `validate_form_ids` + `validate_cinematic_entity_refs` → `report_profile_difference` (LMC-SAV-001 #6670: the save's `SessionProfile` record vs the one the session ran *before* the load — plugins with content hashes, load order, mod roots, extensions, grants, record policy — captured ahead of the reload as `profile_before`, #7349 `5a8947a19`; logged WARN only, never `notify_player`, whose one HUD line is "Load failed.", #7347 `04c2c6b7f`; informational only: the `load_profile` decoder keeps the *live* record, so a load never imports the save's profile (`profile_fingerprint::tests::a_save_carries_its_profile_and_a_load_keeps_the_sessions`), and `save.info` prints `save_profile_lines` against the running session, or "profile: none recorded" when the snapshot has no parseable record) → `end_sting_layer` (a sting from the outgoing session ends, `2419fb27c`) → `sync_body_after_load` (INC4-2 `a0b255b77`: `CharacterIdentity` is a replacing delta column, so a load restoring another sex/race raises `PlayerBodyRebuildRequest`; `identity_before` is noted at the top of the drain) →
  `apply_player_pose` LAST (after the
  overlay of `CharacterController`, whose motion fields pose-restore then zeroes; a new field on either
  side needs an explicit decision). Any failure after the reload returns immediately, never falling through
  into pose-restore on a partial overlay. After the drain returns, `step_save_loads` forgets `created_step_key` / `relocation_step_key` / `enabled_step_key` (`c4d1ea3f2`: an exterior reload onto the same grid set left the keys equal, so created references and relocated-in actors stayed missing), and the load runs under the loading-screen cover (`begin_save` / `take_presented_save`). Process-lifetime *rulesets* resolved against the load order (`CharacterRuleset`, `PoolRegenConfig`, `VatsModel`, …) are dropped by `discard_stale_load_order_caches` in `load_references_budgeted` when the reload's plugin set differs (SAVE-2 `0e8053225`). Idempotency: teardown is unconditional, so a second load of the same
  slot does not stack deltas.
- **Two restore paths**: live load uses `restore_resources` + `apply_deltas` (saved ids remapped to the
  reloaded cell's fresh ids); `restore_world` (clear + repopulate at saved ids) is test/loose only and must
  never be reachable from `execute_pending_save_loads` (id collision = CRITICAL). `clear_entities` does not
  free GPU/physics handles — the live path's `unload_current_interior`/`drain_streaming_state` must.
- **Spawn-time ledger check (resolved 2026-11-17)**: `PRE_RELOAD_RESOURCES` holds only ledgers the spawn path reads
  mid-reload; `ActorRelocations` was the open case (restored after the reload while `skipped_at_home` consults it as the home cell is
  built) and `7250362df` moved it in, with a `LiveReloadLedgers` field and the `SaveReloadTeardown` hold-off. A *new* resource a spawn-time
  consumer reads is the same question: check it against this list, not the post-reload restore.
- **Remap**: `build_form_id_remap` matches saved `FormIdPair` → live entity; rows without a form id are
  skipped (respawned by the loader); unresolved pairs are logged (WARN, first 20) — a moved object vanishing
  must stay diagnosable; the player body carries `PLAYER_FORM_ID_PAIR` from spawn so it participates.
  Persistent-reference rows: resident rows arrive via the component overlay, nonresident via the resource
  restore; rows are consumed on respawn (semantics → `/audit-gameplay`).
- **Frame boundary**: capture runs post-scheduler (`step_player_save_actions` drains a FIFO after
  `Scheduler::run`; remote console runs in the exclusive `DebugDrainSystem`); `capture_player_pose` reads
  post-propagation `Transform` before the drain; `PlayerNotifications` drain in `render_one_frame` is
  unconditional (bounded ring of 8, oldest dropped). The load drain lives in `step_save_loads` where the
  App owns `&mut World` + `&mut VulkanContext` and is skipped while an interior transition or foreign
  loading screen is active; the single `PendingSaveLoadSlot` is last-writer-wins and reports supersession.
- **Pose**: yaw/pitch always to `InputState`; Character mode + live body → `place_character_body`
  (`byroredux/src/systems/character.rs`, the one teleport helper since #5591 `946310eb9`: body `Transform` +
  `GlobalTransform` + `CharacterController::clear_momentum` — `vertical_velocity`/`is_grounded`/`wants_jump` —
  + `FallTracker` reset (#7227 `6eb1729fe`: a quick-load closer than `TELEPORT_DISTANCE` kept the pre-placement fall seconds and the first landing charged them) + kinematic Rapier sync, no-op without a handle); restore branches on the *live* mode (#2018), so
  FlyCam-saved/Character-loaded relocates the body and the reverse repositions the camera. `apply_player_pose`
  returns `bool` and refuses the whole pose (non-finite or out-of-world position — `PhysicsWorld::translation_is_sane`,
  the keyframe bound — or non-finite yaw/pitch), leaving the reloaded cell's spawn (#5530 `d28545728`,
  `a_corrupt_saved_pose_is_refused_and_leaves_the_player_where_the_cell_spawned_them`).
- **Context guards**: `LoadCommand` accepts interior OR exterior context and rejects only neither;
  `SaveInfoCommand` mirrors the three arms (#3500); `quickload_latest` walks `slots_by_recency` and falls back
  past a corrupt newest slot; `command_output_is_failure` scans every output line.
**Output**: `/tmp/audit/save/dim_5.md`

## Phase 3: Merge

Combine `/tmp/audit/save/dim_*.md` into `docs/audits/AUDIT_SAVE_<TODAY>.md`: **Executive Summary** (each
`lib.rs` design claim CODE-CONFIRMED/DRIFTED; findings by severity and Data-Loss Class) · **Data-Loss
Class Matrix** · **Completeness Ledger** (registry × `MUTABLE_DELTA_COLUMNS`: SAVED-only / SAVED+OVERLAID /
structural; cross-check against the guard's allowlist rather than re-deriving) · **Findings** (CRITICAL
first, deduplicated; two-list drift owned by Dim 1, GPU/physics teardown by Dim 5) · **Guards
Verified** (which ledger guards were confirmed live, which reasons were spot-checked).

## Phase 4: Cleanup

`rm -rf /tmp/audit/save`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_SAVE_<TODAY>.md` (domain label `save-load`; `test-gap` for coverage
findings, `doc-rot` for drifted save/load docs).
