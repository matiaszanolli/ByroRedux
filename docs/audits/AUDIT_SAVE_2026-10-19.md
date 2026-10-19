# Save / Load Subsystem Audit (M45 + M45.1) — 2026-10-19 — AREA-SCOPED (streaming-deep)

**HEAD**: 41f9c2c6c · **Baseline**: `docs/audits/AUDIT_SAVE_2026-10-12.md` (HEAD `5d31fd637`) · **Audited**: Dimensions 1, 2 (area slice), 5 (live load-apply, streaming/cell_loader/npc_spawn contract); scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: Dimension 3 (Container & Disk: `crates/save/src/{disk,driver,registry,validate}.rs` and `atomic_file.rs` have no commits since 10-12; `snapshot.rs` changed only `FORMAT_MAJOR` + ledger doc) and Dimension 4 (Save-Side Gates: out of area except the quiescence/transition gates, re-read in `save_io/commands.rs:122-215`, unchanged).

**This report is AREA-SCOPED to `streaming-deep`** (`byroredux/src/{streaming,npc_spawn,cell_loader}/` plus the direct callers/callees needed to follow save/load flows). It is NOT a full-codebase save audit; the next full run must not treat it as a baseline. Everything was analysed synchronously, no sub-agents, no engine launch, no source edits. Only `cargo test` was run (toolchain 1.96.0, repo `target/`, `TMPDIR=/mnt/data/tmp`). Scratch notes were kept under `/tmp/audit/streaming-deep/save/`.

Dedup sources: `/tmp/audit/issues.json` (121 open), closed-issue searches (`PRE_RELOAD_RESOURCES`, `OnUnload OnCellDetach save load`, `discard_package_reports`, save-abort), sibling reports of 2026-10-19 (gameplay GAME-SD-01..04, concurrency CONC-SD-01..03, exterior, performance, safety) and the forwarded IDs from the coordinator.

## Executive Summary

| `lib.rs` / `snapshot.rs` / area design claim | Status |
|---|---|
| Full ECS snapshot of game state | **CODE-CONFIRMED** for the registry; the area's `Component`/`Resource` impls are all classified (guard green). The residue is the *eviction/park* row (`ReferenceState`), which is a third list that lags the registry (SAVE-SD-03). |
| Versioned container + CRC; atomic write; ring | **CODE-CONFIRMED (unchanged since baseline)**; 39 + 15 `byroredux-save` tests pass. |
| `FORMAT_MAJOR` bump discipline | **CODE-CONFIRMED.** 38 → 113 since the last full ledger read; every bump refreshed `BASELINE_MAJOR` with it (`BASELINE_MAJOR == FORMAT_MAJOR == 113`, both serde guards green). The ledger has one gap (v32→v33), already open as #5459; v33→v113 is contiguous. |
| "Keeping the current session (on-disk save intact)" on a refused load | **DRIFTED** (SAVE-SD-01): the pre-reload restore has already overwritten four live ledgers. |
| "Every load behaves like a fresh boot" (#5435/#5864 reset the outgoing session's CLOC cursor, story markers, package reports) | **DRIFTED** (SAVE-SD-02): the teardown's own `OnUnload`/`OnCellDetach` deliveries survive into the loaded session. |
| Ambient-package re-seat before the overlay reproduces the saved winner (#4815) | **DRIFTED for actor-local winners** (SAVE-SD-04). |
| `stream-boundary-state-continuity.md` §4: "script-local state — canonical save/load already owns this boundary" | **DRIFTED** for the eviction boundary (SAVE-SD-03). |

**Findings: 4 (3 NEW + 1 extension of sibling findings). 0 CRITICAL, 0 HIGH, 3 MEDIUM, 1 LOW.**

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 3 | SAVE-SD-01, SAVE-SD-02, SAVE-SD-03 |
| LOW | 1 | SAVE-SD-04 |

Prior-cycle items touching the area, re-verified in place: #5490 (`purge_cinematic_retention_state` closure release), #5483 (`player_owned_entities`), #5435 (CLOC reset, `live_load.rs:550-551`), #5054 (`park_unresolved_snapshot_rows`, `live_load.rs:627`), #4695 (`restore_resident`, `live_load.rs:588`), #3499 (`exterior_reload_overlay_is_safe`), #5859 (`Bleedout` in `ReferenceState`, `reference_state.rs:54,181,322,713`). All present and pinned by tests. Open and unchanged: #5459, #5403, #5059, #6018 (ModEvent registrations never pruned on load; scripting-owned, not re-reported).

## Data-Loss Class Matrix

| Finding | Severity | Class |
|---|---|---|
| SAVE-SD-01 | MEDIUM | silent-drop (live session's four FormID ledgers replaced by an aborted load's, then re-saved) |
| SAVE-SD-02 | MEDIUM | none (state contamination: the outgoing session's script events mutate the loaded state) |
| SAVE-SD-03 | MEDIUM | silent-drop (eviction round trip and the #5054 hysteresis-ring park) |
| SAVE-SD-04 | LOW | none (derived behavior reset; spurious package fragments) |

## Completeness Ledger (area slice)

The registry × `MUTABLE_DELTA_COLUMNS` comparison for the area is unchanged in structure. The residue found this run is orthogonal: the **third list**, the fields of `ReferenceState` (`cell_loader/reference_state.rs:27-62`).

| Per-placement fact | Registry column | Overlaid on live load | Parked at eviction / ring park (`ReferenceState`) |
|---|---|---|---|
| Inventory, EquipmentSlots, EquippedWeapon, ActorValues, SpellList, Perks, ActorControlState, Bleedout, Dead, PickedUp | yes | yes | **yes** |
| Transform / Traveled / TravelState / package id / Seated | yes | Transform, Travel*, yes | `StreamStateSnapshots` (unsaved scratch, ordinary eviction only, by design) |
| `TwoStateActivator`, `ScriptVariables` | yes | yes | no (GAME-SD-01) |
| `ScriptTimer`, `RumbleOnActivate` | yes | yes (`RumbleOnActivate`) | **no — new fact, SAVE-SD-03** |
| `ScriptPackage` | yes | yes | no (GAME-SD-02) |
| `ActiveMagicEffects`, `WeaponDrawn`, `IgnoreCrime` | yes | yes | **no — new fact, SAVE-SD-03** |

Spot-checked `NOT_SAVED_BY_DESIGN` reasons for the area's types (`CreatedReferenceEntities`, `ObjectSequenceStore`, `SharedToneCache`, `GlobalFormIdResolver`, `ActiveLoadOrder`, `PrebakedHeadPart`, `ScriptKilledCorpseForms`, `LoadedCellIndex`, `LoadedPluginSet`, `CellTransitionInFlight`, `CurrentCellRoot`, `NifImportRegistry`, `StreamStateSnapshots`): all true. `CreatedReferenceEntities` holds stale `EntityId`s after a load, but `unspawned_in` re-checks `world.has::<CellRoot>` and ids are never recycled, so a stale entry reads as "gone" and the record respawns (`cell_loader/created.rs:178-197`). `StreamStateSnapshots`' reason holds: a save is a drain by the doc's own §5 bound. Resources saved by the scripting crate are `init_resource`d at boot (`crates/scripting/src/registration.rs`), so the "snapshot lacks the resource, so the live value survives" path in `restore_resources` (`crates/save/src/driver.rs:211-216`) does not leak in practice.

## Findings

### SAVE-SD-01: A load refused at its pre-teardown preflight has already replaced four live ledgers with the save's
- **Severity**: MEDIUM
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: silent-drop
- **Location**: `byroredux/src/save_io/live_load.rs:512-522` (pre-reload restore) versus `:85-97` and `:220-238` (the preflights) and `:540-546` (`None` → `return`)
- **Status**: NEW
- **Description**: `execute_pending_save_loads` restores `PRE_RELOAD_RESOURCES` (`ReferenceEnableState`, `ReferenceLockState`, `ReferenceScriptState`, `ScriptedFactionChanges`, `live_load.rs:37-42`) before the reload. The two reload helpers then run their non-destructive preflights (`validate_cell_loadable`, `build_exterior_world_context`) and, on failure, `return None` with the message "keeping the current cell/session (on-disk save intact)". The ledgers were already overwritten by the save's values (`restore_resources_subset` → `World::insert_resource`) and nothing puts the live values back. The same holds for the "snapshot lost its cell/exterior context" arm (`:533-538`) and for a post-teardown reload failure.
- **Evidence**: order in the drain is `restore_resources_subset(PRE_RELOAD_RESOURCES)` (`:512`) → `without_parked_state(... reload_*_session ...)` (`:528`); `reload_interior_session` preflight failure returns `None` at `:96`; the exterior one at `:235`. `live_reload_tests.rs` pins the restore-before-reload order (`saved_resources_are_restored_before_the_cell_reload`) but nothing covers the abort path. `without_parked_state` protects only `PersistentReferenceStates` and `StreamStateSnapshots`.
- **Impact**: after a refused load (ESM moved, cell editor id renamed, mod set changed) the player keeps playing in the live world, but the Papyrus `Enable`/`Disable` ledger, scripted lock overrides, once-only trigger parking and scripted faction memberships are now the *other timeline's*. They govern the next cell respawn, and the next quicksave persists them, so the live session's own entries are lost for good. Reachable only when a preflight fails, hence MEDIUM.
- **Related**: #3789, #4135, #5919 (which added `ScriptedFactionChanges` to the pre-reload set).
- **Suggested Fix**: run the two preflights before `restore_resources_subset`, or snapshot the four live values and reinstate them when the outcome is `None` before teardown. Add a test that aborts a load and asserts the ledgers.

### SAVE-SD-02: The load's own cell teardown raises `OnUnload` / `OnCellDetach` deliveries that fire inside the loaded session
- **Severity**: MEDIUM
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: none (state contamination of the freshly loaded save)
- **Location**: `byroredux/src/cell_loader/unload.rs:437` (raise) · `crates/scripting/src/fragment/reference.rs:233-281` (queue and `raise_unload_events`) · `:301-310` (`reference_script_event_system` drains it) · `byroredux/src/save_io/live_load.rs:548-555` (the reset block that omits it)
- **Status**: NEW
- **Description**: M47.2 `5ea89f3be` made `unload_cell_inner` call `raise_unload_events(world, &victims)` for every unloaded cell. The save-load reload tears the outgoing cell down through `unload_current_interior` / `drain_streaming_state` → `unload_cells` → `unload_cell_inner`, so every scripted reference (and alias script owner) in the outgoing session queues an `OnCellDetach` and an `OnUnload` `Delivery` (keyed by owner FormID, not entity) in `PendingUnloadDeliveries`. The load drain resets the outgoing session's CLOC cursor (#5435), story-event markers (#5435) and package reports (#5864) but not this queue. The first `reference_script_event_system` run after the load drains it and interprets the handlers against the **restored** `QuestScriptVariables` / `QuestScriptObjects` / quest state.
- **Evidence**: `PendingUnloadDeliveries` is a plain `Vec<Delivery>` resource, `init_resource`d at boot (`registration.rs`), allowlisted as "transient" (`registry_completeness_tests.rs:726`), and never cleared by `execute_pending_save_loads`; `without_parked_state` does not gate `raise_unload_events`. The same class: `ActorWatch` (`reference.rs:527`) keeps the outgoing session's bleedout/combat sets and the player's pre-load location, so the first post-load pass can deliver `OnEnterBleedout` / `OnCombatStateChanged` / `OnLocationChange` (old→new across timelines).
- **Impact**: a handler that writes script variables, calls `Disable()`, sets a stage or moves a quest runs against the loaded state for every scripted reference of the cell the player loaded from. A normal F9 in the same cell reproduces it. Bethesda does not fire unload events for a load; here it corrupts, in handler-dependent ways, the state the save was meant to restore. MEDIUM because the effect depends on handler content.
- **Related**: #5435, #5864 (same class, different queues), #6018 (ModEvent registrations never pruned on load).
- **Suggested Fix**: add a `discard_pending_unload_deliveries` (and an `ActorWatch` reset) beside `discard_package_reports` in the load drain, after the reload and before `restore_resources`; a source-order test like `load_discards_story_event_markers_pending_from_the_outgoing_session` would pin it.

### SAVE-SD-03: `ReferenceState` (eviction and #5054 ring park) lags the registry — extends GAME-SD-01 / GAME-SD-02 with new columns and an asymmetric permanent-stat hazard
- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness (third list)
- **Data-Loss Class**: silent-drop
- **Location**: `byroredux/src/cell_loader/reference_state.rs:27-62` (struct), `:119-264` (`capture`), `:682-802` (`park_unresolved_snapshot_rows`); consumers `crates/scripting/src/magic_effects.rs:313-359`
- **Status**: Existing sibling findings **GAME-SD-01** (`TwoStateActivator`, `ScriptVariables`) and **GAME-SD-02** (`ScriptPackage`) from `AUDIT_GAMEPLAY_2026-10-19.md`; this entry adds only the facts below and does not re-report those two.
- **Description (new facts)**:
  1. **`ActiveMagicEffects` is asymmetric with the parked `ActorValues`.** A *Recover* value effect applies `mod_permanent(+amount)` once at start and `mod_permanent(-amount)` when it finishes (`magic_effects.rs:325-326`, `:356-358`). `capture` parks the actor's `ActorValues` (with the modification in it) but not the `ActiveMagicEffects` list that owns the pending revert. After an eviction round trip the respawned actor gets the modified values back and no effect to undo them, so the buff or debuff is permanent. The ring park of #5054 does the same from the snapshot columns. This is the hazard #4819 fixed for `SpellList` vs `ActorValues`, reappearing for timed effects.
  2. **`WeaponDrawn` and `IgnoreCrime`** (registered replacing columns, `obscript_state::register_saved`) are not parked: a scripted `SetAlert` / `IgnoreCrime` reverts on cell exit.
  3. **`ScriptTimer` and `RumbleOnActivate`** (registered columns on placed references, the latter in `MUTABLE_DELTA_COLUMNS`) are not parked: a spent trap/lever timer or `Busy` rumble state re-arms on re-entry. GAME-SD-01 names only `TwoStateActivator` and `ScriptVariables`.
  4. **The ring park cannot save the actors' scripted state either.** `park_unresolved_snapshot_rows` builds `ReferenceState` from the same ten columns, so every item above (and GAME-SD-01/02) is also dropped for the `radius_load + 1` hysteresis band at save-load time.
  5. **Doc drift**: `docs/engine/stream-boundary-state-continuity.md:130-135` justifies dropping script-local state at the stream boundary with "canonical save/load already owns this boundary (`ScriptVariables`/`ScriptTimer` are registered)". Those are *components*: they survive a save of the resident cell but are despawned with the entity on every ordinary unload, so save/load does not own the eviction boundary.
- **Evidence**: `ReferenceState` fields are exactly `inventory, equipment, weapon, actor_values, spells, perks, control, bleedout, dead, picked_up`. `grep` for `ActiveMagicEffects|WeaponDrawn|IgnoreCrime|ScriptTimer` in `cell_loader/` finds no hit. `ScriptTimer` is registered at `save_io/registry.rs:60`. Every spawned non-player actor gets an `Inventory` (`npc_spawn/resumable/runtime.rs:387,510`), so `capture`'s keep rule (`reference_state.rs:197`) does not itself hide these actors; the fields are simply absent.
- **Impact**: timed buffs on streamed-out NPCs become permanent; scripted alert/ignore-crime flags and spent trap/lever timers reset on every cell exit. All of it is new M47.3 content, so the blast radius is limited to scripted FNV/FO3/Oblivion content, hence MEDIUM. Adding fields bumps `FORMAT_MAJOR` per GAME-SD-01/02's proposed fix.
- **Related**: GAME-SD-01, GAME-SD-02, GAME-SD-03 (corpse position, unsaved `StreamStateSnapshots` scratch cleared on drain — confirmed consistent with this audit's reading of `stream_snapshot.rs` and `streaming_helpers.rs:601-605`), GAME-SD-04, #4819, #5859.
- **Suggested Fix**: fold the new columns into the same `ReferenceState` extension (one `FORMAT_MAJOR` bump), and park `ActiveMagicEffects` together with `ActorValues` as one unit. A guard that diffs `ReferenceState` fields against registered per-placement columns, with an explicit allowlist, would stop the third list drifting again.

### SAVE-SD-04: The post-load ambient re-seat runs before `ScriptPackage` is overlaid, so a script-package winner is not reproduced
- **Severity**: LOW
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: none (derived procedure state is cleared and re-installed; spurious package fragments)
- **Location**: `byroredux/src/save_io/live_load.rs:620` (re-seat) before `:638` (`apply_deltas`); `byroredux/src/npc_spawn/ai_package.rs:892-894`, `:1036-1040` (script package precedence), `:1088-1099` (changed-handover clears behavior)
- **Status**: NEW (the alias-overlay half is the load-side face of sibling GAME-SD-04)
- **Description**: #4815's contract is "re-seat the winner against the restored clock and quest state *before* the overlay, so the overlay lands on the saved winner". `ScriptPackage` (M47.3, `register_replacing_component`, overlaid) changes the winner in `ambient_ai_package_system` (`script_package_of`), but it is restored by the overlay after the re-seat. The re-seat therefore stamps the scheduled winner W; the overlay then restores the script package's `TravelState` / `Traveled` markers and the `ScriptPackage` slot on an actor whose `AmbientPackageRuntime` names W. At the next in-game minute the tick selects the script package, sees `changed`, calls `clear_ambient_behavior` (deleting what the overlay restored) and installs the behavior fresh, reporting `W → script` package transitions (PF_ fragments) for an actor that was already on the script package. The test comment at `round_trip_tests.rs:1715-1719` ("persisting its cached winner is unnecessary") assumes winners depend only on the clock and quest state; the alias-injected overlays are re-derived after the re-seat too.
- **Impact**: a saved script-package actor loses its Travel progress markers within about a game minute of loading and fires an extra package-change fragment. Cosmetic-to-minor, hence LOW.
- **Related**: #4815, #5864, GAME-SD-04.
- **Suggested Fix**: overlay `ScriptPackage` (a plain-data replacing column) before the re-seat, or run the re-seat after `apply_deltas` and re-assert the markers; pin with a round-trip test that includes a running script package.

## Sibling dedup (forwarded IDs)

Checked against the forwarded ECS findings: ECS-SD-01 (misplaced `World::init_resource` doc), ECS-SD-02 (= CONC-SD-03, `tag_descendants_as_actor` Children walk), ECS-SD-03 (misleading "reused entity id" comments at `ai_package.rs:862` and `fragment/package.rs:182`) and ECS-SD-04 (`Material::source_shader_type` has no reader and cost the v112→v113 `FORMAT_MAJOR` bump). None overlaps SAVE-SD-01..04 and none is re-reported. For ECS-SD-04 the save-side facts hold: the bump and baseline refresh landed together in `2266f7778`, so the save guards are satisfied; whether the field earns a format bump is the ECS audit's call. ECS-SD-03 is relevant context for SAVE-SD-02: entity ids are not recycled on the live load path, which is why the stale `CreatedReferenceEntities` and `ActorWatch` entries read as gone, but the owner-FormID `Delivery` entries do not depend on that. Gameplay GAME-SD-01..04 are cited as siblings in SAVE-SD-03 / SAVE-SD-04.

## Dimension notes (out of area / unchanged)

- **Dimension 2 (format & schema)**: area slice only. `FORMAT_MAJOR` 113; every bump since 10-12 carried its baseline refresh (guards green). The v32→v33 ledger gap is #5459. New saved types introduced by the area's neighbours (`ActiveEffect`, `ValueEffect`, `ScriptPackage`, ObScript ledger and challenge columns) live in files that carry the `cfg_attr(feature = "save"` derive, so `save_type_sources()` discovers them.
- **Dimension 3 (container & disk)**: out of area; unchanged since the baseline.
- **Dimension 4 (save-side gates)**: out of area; `CellTransitionInFlight` and the context-invariant refusals re-read, unchanged. Not checkable statically: a save taken mid-`StreamingCellApplyJob` (partially applied cell). Reading the code, rows of unspawned references stay in the saved `PersistentReferenceStates` and a partial NPC carries only identity and `Transform`, so the load reconstructs it; this was not exercised.
- **Dimension 5 (area contract)**: the drain order, `without_parked_state`, the exterior `FullRadius` bootstrap (`consume_streaming_payload` is the synchronous driver, so NPC jobs complete before the parked store is reinstated), the `CurrentExteriorContext.grid` mirror update at grid crossings (`app_step.rs:143-150`) and the #5054 park were traced and hold.

## Guards Verified

- `cargo test -p byroredux-save`: 39 + 15 passed, 0 failed.
- `cargo test -p byroredux --bin byroredux save_io`: 91 passed, 4 ignored (the four `consumable_tests` real-master legs, need installed game data), 0 failed. Confirmed live by name in the run: `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`, `serde_default_on_saved_struct_requires_format_major_bump`, `saved_type_shape_changes_require_format_major_bump`, `delta_columns_carry_only_session_stable_fields`, `delta_columns_removed_at_runtime_have_a_load_reconciler`, `post_reload_drain_steps_keep_their_order`, `quickload_empty_errors_and_corrupt_newest_falls_back`.
- Not re-run this audit: `app_step.rs::the_save_drain_publishes_the_transition_flag_before_draining` (outside the `save_io` filter; the file has no commits touching it since the baseline).
- Not checkable here: any engine, smoke or Vulkan leg (`p5-*`, `p6-loading-model.sh`, `m41`, live save/load of an exterior with in-flight streaming); game-data-dependent `#[ignore]`d tests. SAVE-SD-01..04 are static findings with no live reproduction.
