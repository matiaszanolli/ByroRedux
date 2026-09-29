# Save / Load Subsystem Audit (M45 + M45.1) — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_SAVE_2026-09-22.md` (HEAD `ee6d3fb39`) ·
**Audited**: Dimensions 1, 2, 4, 5 · **Unchanged since baseline (skimmed)**: Dimension 3 (Container & Disk
Durability: zero commits to `disk.rs`, `atomic_file.rs` or `tests/round_trip.rs`; `snapshot.rs` changed only the
`FORMAT_MAJOR` constant and its doc comments. The guard ledger was re-run and is green.)

This run was part of `/audit-suite --preset comprehensive`. All five dimensions were analysed synchronously, with
no sub-agents; scratch files are in `/tmp/audit/save/dim_{1..5}.md`. The delta window is 272 commits. The save
paths changed in several places:
- `FORMAT_MAJOR` went from 25 to 30;
- three new saved types: `SpellList`, `FactionRelations`, `ReferenceScriptState`;
- `SpellList` joined `MUTABLE_DELTA_COLUMNS`;
- `ReferenceScriptState` joined `PRE_RELOAD_RESOURCES`;
- two new load steps: `restore_resident` (#4695) and `reseat_ambient_packages_after_restore` (#4815);
- the completeness guard was rewritten (#4705);
- the P3 player body, mid-life gear import and P4 dialogue added runtime state.

Dedup covered: open issues (`/tmp/audit/issues.json`), closed issues searched per finding, and today's sibling
reports. `AUDIT_GAMEPLAY_2026-09-29.md` already filed GAME-D7-2026-09-29-01 (a dead player stays `Dead`) and
GAME-D1-2026-09-29-02 (worn meshes are not re-derived from the loaded slots). Both are cross-referenced below, not
re-filed. Per this run's brief, the additive-registration class was then swept across every other overlaid or
registered column on the process-lifetime player.

## Executive Summary

| `lib.rs` / `snapshot.rs` design claim | Status |
|---|---|
| Full ECS snapshot of game state | **DRIFTED.** The registry mechanism is sound, and the completeness guard is fixed (#4705): the 5 previously hidden types are classified with accurate reasons. But the snapshot is only as complete as the load can apply. Exterior rows for cells in the hysteresis band are captured and then dropped on load (SAVE-D5-01). Two registered columns leak the live session into the loaded one on the process-lifetime player (SAVE-D1-01, SAVE-D5-02). |
| "Save size scales with loaded-cell entity count, never with playthrough length" | **DRIFTED (by design since P3).** `PersistentReferenceStates` parks one row per mutated, evicted placement for the whole playthrough. The rows are small, but the claim is no longer literally true. Not filed. |
| Versioned container + CRC32 over the payload | **CODE-CONFIRMED.** Unchanged. The header-gate tests are green. |
| Atomic write (tmp → fsync → re-read+verify → rename → dir-fsync) | **CODE-CONFIRMED.** Unchanged. The three callers use the one helper. |
| Ring never clobbers the last good save | **CODE-CONFIRMED.** `SaveRing::resume`, and the cursor advances only after the commit. |
| Validation gate refuses to persist a poisoned save | **CODE-CONFIRMED.** Nothing new is uncovered: no new registered type carries an `EntityId`. Caveat: a stale-but-below-`next_entity` id (SAVE-D5-02) is invisible by construction. |
| `FORMAT_MAJOR` bump is the only schema-evolution path | **CODE-CONFIRMED for every change this window** (v26–v30, all justified in the same commit as the change). **DRIFTED for the guard's reach:** a scan narrowing in 573170e1c dropped a nested payload of `LightSource`, and `crates/sdk` payloads were never scanned (SAVE-D2-01). |
| Load runs off-frame | **CODE-CONFIRMED.** `step_save_loads` runs after `scheduler.run` (`app_events.rs:921` → `:976`). `restore_world` still has no production caller. |
| Additive overlay + an explicit reconciler per removal | **DRIFTED.** Removals are covered, but *inserts on an entity that survives the reload* are not: `Dead` (GAME-D7-01), `ActorControlState` (SAVE-D1-01), `ActorCinematicState` (SAVE-D5-02). |
| Reproducible CRC at equal state | **Doc corrected.** `snapshot.rs` and `save-load-roundtrip.md:62-63` now disclaim determinism at the resource level (landed in 2f8538334). #4748 is still open and can be closed. The affected set grew with `ReferenceScriptState` and `ReferenceEnableState.enabled`. |

**Findings: 6 NEW — 0 CRITICAL, 3 HIGH, 2 MEDIUM, 1 LOW.**

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 3 | SAVE-D1-2026-09-29-01, SAVE-D5-2026-09-29-01, SAVE-D5-2026-09-29-02 |
| MEDIUM | 2 | SAVE-D1-2026-09-29-02, SAVE-D2-2026-09-29-01 |
| LOW | 1 | SAVE-D5-2026-09-29-03 |

Total: 6 findings (3 HIGH, 2 MEDIUM, 1 LOW), all NEW.

**Prior-cycle findings:**
- **SAVE-D5-2026-09-22-01 (HIGH, #4695): FIXED and verified.** `restore_resident` runs after the post-reload `restore_resources`. Its `release_victim_item_instances` call is a no-op on freshly authored inventories, so the #4135 hazard does not come back.
- **SAVE-D1-2026-09-22-01 (MEDIUM, #4705): FIXED and verified.** The guard now strips test items instead of cutting at the first `#[cfg(test)]`. The pin `test_gated_items_are_stripped_without_hiding_what_follows` is green.
- **SAVE-D1-2026-09-22-02 (MEDIUM, #4748):** the doc is fixed, but the issue is still open. Recommend closing it.
- **SAVE-D6-2026-09-11-01 (LOW):** still not re-checked. Carried forward.

## Data-Loss Class Matrix

| Finding | Class | Dim | Severity | Status |
|---|---|---|---|---|
| SAVE-D1-2026-09-29-01 — `ActorControlState` restraint leaks onto the player across a load | corruption-on-load | 1 | HIGH | NEW |
| SAVE-D5-2026-09-29-01 — exterior load drops every saved row of the hysteresis-band cells | silent-drop | 5 | HIGH | NEW (sibling of closed #3280) |
| SAVE-D5-2026-09-29-02 — cinematic state and retained entities survive the load teardown (player glued to a ghost cart; duplicate FormIDs) | corruption-on-load / reference-break | 5 | HIGH | NEW (related #3817) |
| SAVE-D1-2026-09-29-02 — alias-injected factions on the player become permanent after any load | corruption-on-load | 1 | MEDIUM | NEW |
| SAVE-D2-2026-09-29-01 — shape guard blind to `Emitter` (core `lighting.rs`) and `FormRef`/`ScriptValue` (sdk) nested payloads | none (latent guard gap) | 2 | MEDIUM | NEW |
| SAVE-D5-2026-09-29-03 — `restore_resident`/reseat have no source-order pin | none (test gap) | 5 | LOW | NEW |
| GAME-D7-2026-09-29-01 — dead player stays `Dead` after a load | corruption-on-load | 1 | HIGH | Existing (gameplay report) |
| GAME-D1-2026-09-29-02 — worn meshes not reconciled to the loaded `EquipmentSlots` | none (presentation) | 1 | MEDIUM | Existing (gameplay report) |

## Completeness Ledger (delta from baseline; see `AUDIT_SAVE_2026-09-11.md` for the full table)

This ledger was cross-checked against the #4705-fixed guard's `NOT_SAVED_BY_DESIGN` list, not re-derived.

| Column | Kind | Saved | Overlaid | Notes |
|---|---|---|---|---|
| `SpellList` | Component (#4415, v27) | yes | **yes** | Always present on the player (`inventory.rs:733`) and on every NPC (#4822), so the additive overlay cannot leak. Parked beside `ActorValues` in `ReferenceState.spells` (v30). |
| `FactionRelations` | Resource (#4414, v28) | yes | n/a (wholesale) | `Vec`, `FormRef`-keyed. Nested `FormRef` is outside the shape guard (SAVE-D2-01). |
| `ReferenceScriptState` | Resource (#4334, v26) | yes | n/a (in `PRE_RELOAD_RESOURCES`) | The spawn-time consumer justifies the pre-reload slot. `HashSet` iteration order (#4748 class). |
| `ReferenceEnableState.enabled` | Resource field (#4813, v29) | yes | n/a (in `PRE_RELOAD_RESOURCES`) | Required field, bumped. |
| `ActorControlState` | Component | yes | yes | **Additive, and lazily inserted on the player** (SAVE-D1-01). |
| `ActorCinematicState` / `HorseTetherState` | Component | yes | **no** (EntityId) | The player's live row is neither overlaid nor cleared, and it drives teardown retention (SAVE-D5-02). |
| `Dead` | Component | yes | yes | Additive on the player (GAME-D7-01). |
| `QuestAliasInjectionState` | Resource | yes (`factions` serde-skipped) | n/a | The skipped ledger is safe for respawned NPCs but not for the player (SAVE-D1-02). |
| `FactionRanks` | Component | **no** (REDERIVED_NOT_SAVED) | n/a | Re-derivation assumes a fresh carrier; the player is not one (SAVE-D1-02). |
| P3/P4 types: `PlayerBodyRoot`, `PlayerBodyRootEntity`, `HiddenFirstPerson`, `PlayerCameraView`, `NpcSkeletonBones`, `ActorBodyClass`, `PendingGearImport`, `PendingInventoryActions`, `NpcDialogueTopic`, `DialogueSurfaceState` | Component/Resource | no (allowlisted) | n/a | Reasons spot-checked and accurate. `DialogueSurfaceState.npc` is written but never read in production, so it is harmless. |
| `DraugrCombatAnim`, `DraugrCombatClips`, `ExposureTuning`, `NavmeshResidency`, `GlobalFormIdResolver`, `LoadOrderIdentity`, `AmbientEngagement`, `CombatDisposition` | Component/Resource | no (allowlisted) | n/a | Newly classified by #4705 and #4414 to #4823. Reasons accurate. |

## Findings

### HIGH

#### SAVE-D1-2026-09-29-01: `ActorControlState` is additive and lazily inserted on the process-lifetime player — a restraint from the pre-load session survives loading a save that has no player row

- **Severity**: HIGH
- **Dimension**: Snapshot Completeness (two lists / replacing semantics)
- **Data-Loss Class**: corruption-on-load
- **Location**:
  - `byroredux/src/save_io.rs:135` (in `MUTABLE_DELTA_COLUMNS`) and `:452` (plain `register_component`);
  - `crates/scripting/src/fragment/effects.rs:1239-1262` (`Effect::SetPlayerRestrained` inserts on first use);
  - `byroredux/src/systems/character.rs:107-116` (`player_accepts_movement_input`);
  - `crates/save/src/registry.rs:99-103`.
- **Status**: NEW. The sibling of GAME-D7-2026-09-29-01 (`Dead`); same class as the closed #3488 (`EquippedWeapon`).
- **Description**: the player spawns without `ActorControlState`; no production site pre-stamps it. The first
  `SetRestrained` fragment inserts `{ restrained }` on the player. `save_world` omits an empty additive column, so a
  save from a session that never ran `SetRestrained` has no player row. The player entity survives the load's
  teardown, and `apply_deltas` only writes rows that exist. No reconciler touches the column. A live
  `restrained: true` therefore stays on the player after loading that save.
- **Evidence**: the only production inserts are `effects.rs:1253-1259` (grep across `byroredux/src`,
  `crates/scripting/src` and `crates/core/src`; `character.rs:1825` is inside `#[cfg(test)]`). The overlay is additive
  per `registry.rs:99-103`. The post-overlay reconcilers (`save_io.rs:1799-1815`) cover `Dead` NPCs and the equipped
  weapon only.
- **Impact**: after loading, the player cannot move. `player_accepts_movement_input` returns false until some later
  script calls `SetRestrained(false)`. If the loaded save's quest state never re-runs that fragment, the session
  cannot be recovered short of a restart. Trigger: a save from a session that never used `SetRestrained` (for
  example, booted past Helgen), then a restraining sequence in the current session (Skyrim MQ101 restrains the
  player), then loading the earlier save.
- **Related**: GAME-D7-2026-09-29-01; #2292 (why the column is registered); #3488.
- **Suggested Fix**: register `ActorControlState` with `register_replacing_component`. A saved absence on a
  FormID-matched actor means "not restrained"; NPCs are respawned without it anyway. Better, fix the class once for
  `Dead` and this column together, and add a test: live player restrained → load a save with no row → not restrained.

#### SAVE-D5-2026-09-29-01: exterior load drops every saved row of the hysteresis-band cells — the reload bootstraps `radius_load`, but a save taken after any cell crossing holds resident cells out to `radius_unload = radius_load + 1`

- **Severity**: HIGH
- **Dimension**: Live Load-Apply & Frame Boundary
- **Data-Loss Class**: silent-drop
- **Location**:
  - `byroredux/src/save_io.rs:1514-1523` (`assemble_exterior_streaming(.., ext_ctx.radius_load, exterior_reload_bootstrap_mode())`) and `:1593-1595`;
  - `byroredux/src/streaming.rs:909` (`radius_unload: radius_load + 1`) and `:2020-2026` (evicts only cells beyond `radius_unload`);
  - `byroredux/src/scene/world_setup.rs:904-909` (initial deltas from an empty `loaded` set);
  - `crates/save/src/driver.rs:293-325` (unresolved pairs are logged, then dropped);
  - `byroredux/src/cell_loader/reference_state.rs:1-7` (rows exist only for non-resident placements).
- **Status**: NEW. The sibling of the closed #3280 / SAVE-D6-2026-08-24-01. That fix made the reload wait for every
  cell inside `radius_load`; the hysteresis ring outside it was never considered.
- **Description**: `CurrentExteriorContext.grid` follows the player (`app_step.rs:118-127`). After one crossing, the
  ring at Chebyshev distance `radius_load + 1` is still resident. The state of its placements lives only in component
  columns, because parked rows are consumed on respawn and resident placements are never parked. `save_world`
  captures those rows. The load then streams only `radius_load` around the saved grid, so every band `FormIdPair`
  fails to resolve. `build_form_id_remap` logs one WARN block and `apply_deltas` skips the rows. Nothing turns the
  unresolved rows into `PersistentReferenceStates` entries, so the band cells later respawn from ESM.
- **Evidence**: the reload path passes `radius_load` both to `build_exterior_world_context` (`:1471`) and to the
  bootstrap (`:1520`). `stream_initial_radius` computes its deltas from an empty `loaded` set against
  `state.radius_load`. `restore_resident` consults only the parked store, never the snapshot's unresolved rows.
- **Impact**: after loading an exterior save, the rows below are lost for every band placement. When the player
  returns, killed NPCs are alive and looted containers are restocked while the loot is already in the loaded
  inventory, so items duplicate. Lost columns:
  - `Inventory`, `EquipmentSlots`, `EquippedWeapon`;
  - `Dead`, `ActorValues`, `SpellList`;
  - `Transform` (moved objects);
  - activator/script state and AI procedure markers.

  Trigger: loot or kill something, move so its cell sits exactly `radius_load + 1` cells away (4 at radius 3),
  save, then load.
- **Related**: #3280 (closed), #3499 (pending-cells guard), #4695 (`restore_resident`).
- **Suggested Fix**: at load time, park the snapshot rows the remap could not resolve into
  `PersistentReferenceStates`. Build `ReferenceState` from that pair's
  `Inventory`/`EquipmentSlots`/`EquippedWeapon`/`ActorValues`/`SpellList`/`Dead` rows, the same shape `capture`
  writes. Alternatively, bootstrap the reload to the saved resident set (`radius_unload`). Add a test that feeds a
  snapshot with a band-cell row through an unresolved remap and asserts the row is parked.

#### SAVE-D5-2026-09-29-02: cinematic state survives the save-load teardown — the player stays glued to a retained ghost vehicle, and retained FormID-bearing entities are duplicated by the reload

- **Severity**: HIGH
- **Dimension**: Live Load-Apply & Frame Boundary (GPU/physics teardown completeness)
- **Data-Loss Class**: corruption-on-load / reference-break
- **Location**:
  - `byroredux/src/cell_loader/unload.rs:18-47` (`cinematic_retained_entities`), `:144-176` and `:223-244` (applied by every `unload_cell`/`unload_cells`, including the load path's `unload_current_interior` and `drain_streaming_state`);
  - `byroredux/src/save_io.rs:437-447` (`ActorCinematicState`/`HorseTetherState` registered but excluded from the overlay);
  - `crates/scripting/src/fragment/effects.rs:170-186` (lazy insert) and `:1366-1400` (`SetVehicle` on any resolved actor, including the player);
  - `byroredux/src/systems/cinematic.rs:778`, `:860-904` (`vehicle_attachment_system`, scheduled at `boot/schedule/update.rs:442`);
  - `byroredux/src/save_io.rs:822-855` (`validate_cinematic_entity_refs`);
  - `crates/save/src/driver.rs:282-291` (`pair_to_live` collected into a `HashMap`).
- **Status**: NEW; related to #3817 (open: the cinematic states never terminate, so retention has no re-adoption
  path). #3817 covers permanence across ordinary unloads; it does not cover the load path's teardown, the stale state
  left on the player, or the duplicate FormIDs the reload creates.
- **Description**:
  1. **Player state leaks.** The process-lifetime player's `ActorCinematicState` is neither overlaid nor cleared on
     load. Loading during the MQ101 cart ride keeps `vehicle = Some(cart)`: saving is chargen-disabled then, but
     loading is not.
  2. **The teardown retains instead of despawning.** Because of that state (and `HorseTetherState`, which is never
     removed), the teardown keeps the cart, the horse, every rider and their subtrees. It strips `CellRoot` from them
     rather than despawning them. After the convoy has started once in a process, cart and horse are retained on
     every later load.
  3. **The reload duplicates them.** The reload spawns fresh copies of the same REFR/ACHR records, so two live
     entities share a `FormIdPair`. `build_form_id_remap`'s `HashMap` collect keeps one arbitrarily, so saved deltas
     can land on the ghost.
  4. **The player is pinned to the ghost.** Every frame, `vehicle_attachment_system` overwrites the player's
     `Transform` and kinematic body from the ghost cart, defeating `apply_player_pose`. The restored quest state will
     not fire an ExitCart if the save is past MQ101.
  5. **Validation cannot see it.** The stale ids pass post-load validation and every later pre-save gate, because
     entity ids are never reused and the dangling test is `>= next_entity`.
- **Evidence**: `unload_cell_inner` computes `victims.retain(|e| !retained.contains(e))`. The load path calls
  `unload_cell` through `unload_current_interior` (`transition.rs:418-428`) and the exterior drain. No production
  site removes either cinematic component (#3817, re-confirmed by grep). `vehicle_attachment_system` writes the
  `Transform` and `set_kinematic_translation` for every `ActorCinematicState` whose `vehicle` still has a `Transform`.
- **Impact**: loading during or after the Helgen convoy in the same process has two effects. The player can be
  pinned to a moving ghost cart. The world can hold duplicate cart, horse and (mid-ride) rider entities, with
  ambiguous remap targets. There is no in-session recovery if the loaded quest state is past MQ101.
- **Related**: #3817, #3254, #2380, #2535; ECS-2026-09-29-D1-02 (a lock-order hygiene issue in
  `cinematic_trio_survives_save_load_round_trip`, owned by `/audit-ecs`).
- **Suggested Fix**: before the load's teardown, clear cinematic retention by removing
  `ActorCinematicState`/`HorseTetherState` from live entities, at least the player's, so that `unload_cell` retains
  nothing. After the overlay, clear the player's `vehicle` when the saved row is absent, or register the pair as
  replacing for FormID-matched entities. Pin it with a test: a live player `vehicle` plus a tether, after load, leaves
  no entity outside `CellRootIndex` and no player `vehicle`.

### MEDIUM

#### SAVE-D1-2026-09-29-02: `QuestAliasInjectionState.factions` is serde-skipped — after any load, an alias-injected faction on the process-lifetime player becomes permanent

- **Severity**: MEDIUM
- **Dimension**: Snapshot Completeness (stale allowlist reasoning)
- **Data-Loss Class**: corruption-on-load
- **Location**:
  - `crates/scripting/src/scene/quest_alias.rs:89-98` (`#[serde(skip, default)] factions: HashMap<(EntityId, u32), _>`);
  - `crates/scripting/src/scene/quest_alias.rs:530-565` (the refresh's release and record logic);
  - `byroredux/src/save_io.rs:516-520` (the registration comment: "re-derived from static QUST data");
  - `byroredux/src/scene.rs:1198-1212` (the player carries `SceneAliasCandidate { reference_form_id: 0x14 }`);
  - `byroredux/src/save_io/registry_completeness_tests.rs:472` (`FactionRanks` is `REDERIVED_NOT_SAVED`).
- **Status**: NEW. The closed #2534/#2670 re-keyed the *inventory* half of this ledger; the faction half was left as
  "re-derived".
- **Description**: the wholesale `restore_resources` installs the ledger with `factions` empty. For NPCs this is
  correct, because they respawn with their record `FactionRanks` and the refresh re-injects them. The player is not
  respawned and keeps the pre-load session's injected rank. On the next refresh, `ranks.get_mut(player)` already
  holds the rank, so it is recorded as `original_rank = Some(0)` (an authored membership). The release branch only
  strips memberships whose `original_rank` is `None`, so the injected faction is never removed again. If the loaded
  save's quest no longer binds the alias, the faction is never even visited.
- **Evidence**: a raw QUST census of Skyrim SE `Skyrim.esm` (1811 records) finds 7 player-forced (ALFR `0x14`)
  aliases carrying ALFC factions:

  | Quest | Faction |
  |---|---|
  | MQ101 | `0x4E1E1` |
  | MQ104 | `0x4E1E1` |
  | DGIntimidateQuest | `0x4CFA6` |
  | C06 | `0xFDEC5` |
  | CWFinale | `0xF7630` |
  | WE34, WE36 | `0xBA0B9` |

  The script is `/tmp/audit/save/alfc_census.py`.
- **Impact**: the player keeps faction memberships the loaded save does not have. They feed
  `faction_hostility_system`, the `GetInFaction`/`GetFactionRank` CTDA and dialogue conditions. The membership
  outlives every later alias release in the process.
- **Related**: #2534, #2670; SAVE-D1-2026-09-29-01 (the same process-lifetime-player class).
- **Suggested Fix**: after the wholesale restore, strip alias-injected factions from the player before the first
  refresh. For example, record the player's pre-injection ranks in a saved, FormID-keyed form, or snapshot the
  player's record `FactionRanks` at attach and reset to it on load. Alternatively, save the faction ledger keyed by
  reference FormID the way #2534 re-keyed inventory grants.

#### SAVE-D2-2026-09-29-01: `save_type_sources()` is blind to nested payloads of two registered types — 573170e1c dropped `crates/core/src/lighting.rs` (`LightSource.emitter`), and `crates/sdk` payloads (`FormRef` in the new `FactionRelations`, `ScriptValue` in `PapyrusProviderContinuationQueue`) were never scanned

- **Severity**: MEDIUM
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: none today (latent guard gap)
- **Location**:
  - `byroredux/src/save_io/serde_default_guard_tests.rs:88-114` (the filter keeps a file only if it contains `cfg_attr(feature = "save"` or defines a registered name; the explicit list has four files);
  - `crates/core/Cargo.toml:9-13` (core's serde derives are gated on `inspect`; `save = ["inspect"]`);
  - `crates/core/src/ecs/components/light.rs:26` (`pub emitter: Emitter`);
  - `crates/core/src/lighting.rs` (`Emitter`, `AttenuationModel`, `VisibilityMask`, `RadiantIntensityRgb`, `Meters`);
  - `crates/scripting/src/combat.rs:68-81`;
  - `crates/sdk/src/identity.rs:237-241` (`FormRef`, plain derive);
  - `crates/sdk/src/script_function.rs:30,49` (`ScriptValueType`, `ScriptValue`);
  - `crates/scripting/src/papyrus_provider/ir.rs:98-102`.
- **Status**: NEW. The fifth recurrence of the #2015 / #2537 / #3025 / #3167 class.
- **Description**: commit 573170e1c (misleadingly titled "feat(tests): add new test for concurrent initialization of
  OnceLock") removed the `cfg_attr(feature = "inspect"` arm from the file filter. Its reasoning was that
  "feature-gated save derives cover standalone serialized payloads". But every `byroredux-core` derive is
  `inspect`-gated, not `save`-gated. A Python reproduction of the filter shows 16 core files dropped. Only
  `lighting.rs` holds a nested payload of a registered column, `LightSource`, which is also in
  `MUTABLE_DELTA_COLUMNS`. The `crates/sdk` payloads of `FactionRelations` (new at v28) and
  `PapyrusProviderContinuationQueue` were never selected at all.
- **Evidence**: `/tmp/audit/save/scan_sel.py` reproduces the filter. `crates/sdk/src/identity.rs` contains 0
  `cfg_attr(feature = "save"` occurrences. The `lighting.rs` diffs since the narrowing (b978bb5a1) change only
  default values and const fns, and `FormRef` is unchanged since v28, so no shape change has slipped through yet.
- **Impact**: a new field or retyped field in `Emitter`, `FormRef` or `ScriptValue` would ship without a
  `FORMAT_MAJOR` bump. Old saves would then fail `serde_json` decoding after the version gate passed them, or the
  typed preflight would reject them with a confusing error.
- **Related**: #2015, #2537, #3025, #3167, #4141.
- **Suggested Fix**: add `crates/core/src/lighting.rs`, `crates/sdk/src/identity.rs` and
  `crates/sdk/src/script_function.rs` to the explicit nested-payload list. Better, derive the list from the field types
  of registered types, or restore the `inspect` sweep and allowlist its known false positives.

### LOW

#### SAVE-D5-2026-09-29-03: `restore_resident` (#4695) and `reseat_ambient_packages_after_restore` (#4815) are order-critical load steps with no source-order pin

- **Severity**: LOW
- **Dimension**: Live Load-Apply & Frame Boundary
- **Data-Loss Class**: none (test gap)
- **Location**: `byroredux/src/save_io.rs:1764`, `:1796`; `byroredux/src/save_io/live_reload_tests.rs:412-470`.
- **Status**: NEW
- **Description**:
  - If `restore_resident` moved above the post-reload `restore_resources`, it would be a silent no-op, because the
    store is absent. That would re-open #4695's item duplication.
  - `reseat_ambient_packages_after_restore` must precede `apply_deltas` (#4815).
  - Only unit tests of the two functions exist (`reference_state.rs:757`, `ai_package.rs:1033`). The existing
    source-order pin covers only the pre-reload subset and the post-reload `restore_resources`.
- **Suggested Fix**: extend `saved_resources_are_restored_before_the_cell_reload` (or add a sibling test) to assert
  this order inside the drain body:
  `restore_resources` < `restore_resident` < `reseat_ambient_packages_after_restore` < `build_form_id_remap` < `apply_deltas`.

## Cross-referenced, not re-filed

- **GAME-D7-2026-09-29-01** (HIGH): `Dead` is additive on the player. Confirmed at `save_io.rs:110`/`:418`. Sweeping
  the rest of the class on the player found:
  - **leaks**: `ActorControlState` (SAVE-D1-01), `ActorCinematicState` (SAVE-D5-02) and alias-injected
    `FactionRanks` (SAVE-D1-02);
  - **cleared**:
    - `SpellList`, `Inventory`, `EquipmentSlots`, `ActorValues` and `CharacterController` are always present;
    - `EquippedWeapon` has a reconciler;
    - `Perks` and `TimedRestorations` are replacing columns;
    - the AI procedure columns only have NPC producers;
    - `ScriptTimer` has no production insert.
- **GAME-D1-2026-09-29-02** (MEDIUM): worn meshes are not re-derived after the overlay. Confirmed. Extension: a
  player `PendingGearImport` in flight at load time (not saved, and it survives on the player) keeps attaching the
  pre-load session's gear.
- **#4748**: the doc half is fixed at HEAD (see the Executive Summary). Recommend closing it.
- **#4758** (the atomic-write doctrine in `BootRequest::save`) belongs to `/audit-tooling`.

## Guards Verified

| Guard | State |
|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | **green**, and now structurally sound (#4705 test-item stripping, pinned by `test_gated_items_are_stripped_without_hiding_what_follows`). Scan roots are discovered, pinned by `discover_scan_roots_finds_every_workspace_crate_and_byroredux`. Reasons spot-checked: 18 rows (P3/P4 + #4705 + #4414..#4823), all accurate. |
| `serde_default_on_saved_struct_requires_format_major_bump` | green |
| `saved_type_shape_changes_require_format_major_bump` | green. `BASELINE_MAJOR = 30` = `FORMAT_MAJOR`. Every refresh this window carries a justification; the two refreshes without a bump (#4612, #4816) are correct. Reach gap: SAVE-D2-01. |
| `set_in_chargen_renames_still_decode_v23_keys`, `quest_revision_keys_never_reach_a_save` (new) | green |
| `delta_columns_carry_only_session_stable_fields` | green (now includes `SpellList`) |
| `delta_columns_removed_at_runtime_have_a_load_reconciler` | green. The new `RigidBodyData` NoReconcilerNeeded row (player-body strip) is accurate. Structural blind spot: runtime *inserts* on reload survivors (GAME-D7-01, SAVE-D1-01). |
| `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` | green (`CombatDisposition`, `SpellList` added, #4823) |
| `saved_resources_are_restored_before_the_cell_reload`, `pre_reload_restore_must_not_install_saved_item_instance_pool_early` | green. Neither covers the new steps (SAVE-D5-03). |
| `command_queue_tests::{a_save_taken_mid_cell_transition_is_refused_not_written, a_save_taken_while_chargen_disables_saving_is_refused_not_written, quicksave_ring_cursor_does_not_advance_on_validation_abort, player_save_actions_wait_for_the_quiescent_fifo_drain, quickload_empty_errors_and_corrupt_newest_falls_back}` | green |
| `app_step.rs::the_save_drain_publishes_the_transition_flag_before_draining` | green |
| `crates/save` unit and integration tests (header gates, CRC, atomic write, ring, replacing semantics, typed preflight) | green: 40 + 15 |
| `reference_state::tests::save_reload_window_tombstone_is_applied_by_restore_resident` (#4695) | green (unit level) |

Commands run, all with `TMPDIR=/mnt/data/tmp` and no engine or GPU process:
- `cargo test -p byroredux-save`: 55 passed.
- `cargo test -p byroredux --bin byroredux save_io`: 69 passed, 4 ignored. The 4 ignored tests are exactly the
  real-master `consumable_tests`.
- `cargo test -p byroredux --bin byroredux the_save_drain_publishes`: 1 passed.

## Summary per Dimension

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & the Two Lists | 2 (1 HIGH, 1 MEDIUM) + 2 cross-refs | The #4705 guard fix is verified. A new sweep of the process-lifetime-player class found `ActorControlState` and the alias faction ledger. |
| 2 — Format & Schema Discipline | 1 (MEDIUM) | Bumps v26–v30 are all sound; the extension-state payload is unchanged. The guard's reach regressed in 573170e1c and never covered `crates/sdk`. |
| 3 — Container & Disk Durability | 0 | Unchanged; the guard was re-run. |
| 4 — Save-Side Gates | 0 | Gate order is unchanged. No new registered `EntityId` field. New transients are on the allowlist. |
| 5 — Live Load-Apply & Frame Boundary | 3 (2 HIGH, 1 LOW) | #4695 is verified fixed. Exterior hysteresis-band rows are dropped. Cinematic retention survives the load teardown. The new steps lack an order pin. |

Suggested next step: `/audit-publish docs/audits/AUDIT_SAVE_2026-09-29.md`. Use domain label `save-load`; add
`test-gap` for SAVE-D5-03, and `gameplay` for SAVE-D1-01/02 and SAVE-D5-02.
