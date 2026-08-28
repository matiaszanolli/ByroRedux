# Save / Load Subsystem Audit (M45 + M45.1) — 2026-08-27

Scope: `crates/save/src/` (`lib`, `snapshot`, `registry`, `driver`, `disk`,
`validate`, `tests/round_trip.rs`) plus the engine-side consumer
`byroredux/src/save_io.rs` and its six `byroredux/src/save_io/*_tests.rs`
siblings, and the cross-cut ground truth the flow depends on
(`byroredux/src/app_events.rs`, `app_step.rs`, `app_frame.rs`, `boot.rs`,
`cell_loader/transition.rs`, `scene/world_setup.rs`, `combat.rs`,
`inventory.rs`). Audited at HEAD `969d81c8`.

This is the **eleventh** save audit (prior: `2026-06-23`, `2026-07-02`,
`2026-07-03`, `2026-07-16`, `2026-07-25`, `2026-08-03`, `2026-08-07`,
`2026-08-16`, `2026-08-20`, `2026-08-24`). Run solo (no sub-agent fan-out) per
this cycle's explicit instruction; every dimension was covered directly by
Read/Grep/Bash plus live test runs (`cargo test -p byroredux-save` — 42 unit +
14 integration, green; `cargo test -p byroredux save_io::` — 44 tests, green,
including the SAVE-D1-12 completeness guard and both serde/shape baseline
guards).

**Both 2026-08-24 findings are fixed and verified line-by-line at HEAD** (see
*Prior-Cycle Disposition*). The delta since `048a8bd8` is large but mostly
outside this subsystem; the two save-relevant changes are `0f651aba` (#3113 —
`PendingPlayerSaveActions`, deferring F5/F9/pause-menu save actions past the
scheduler join) and `98eea9b3` (the exterior-reload bootstrap-mode refactor
that closed last cycle's HIGH). `FORMAT_MAJOR` has moved 5 → 8 in four
separate, correctly-guarded bumps.

This cycle's one HIGH lives in the newest code in the blast radius: the P2
gameplay slice's native inventory menu **removes** a saved component
(`EquippedWeapon`) from the process-lifetime player body, and the live
load-apply overlay is documented as unable to undo a removal — with no
companion reconciler, unlike the one `Dead` already has.

## Executive Summary

`crates/save/src/lib.rs` docstring claims verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot (curated game-state set) | **CODE-CONFIRMED, with two ledger caveats.** `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` is green over its six `SCAN_ROOTS`. But the guard has no mechanism to notice a *new crate* (`crates/sdk`, added `21a840d5`, is outside every root — SAVE-D1-2026-08-27-03), and one allowlist reason cites a guard that does not exist (SAVE-D1-2026-08-27-02). |
| Atomic write (tmp → fsync → read-back-verify → rename → dir-fsync) | **CODE-CONFIRMED.** `disk::write_slot` (`crates/save/src/disk.rs:34-69`) has zero diff this cycle; the ordering is exactly `create_dir_all` → tmp write → `flush` → `sync_all` → byte-exact `readback != bytes` → `rename` → parent-dir `sync_all`. The concurrent ECS audit's `settings_io::save_to_path` fsync LOW has **no counterpart here** — the real save path fsyncs both the file and the parent directory. |
| Ring never clobbers the last good save | **CODE-CONFIRMED.** `SaveState::new` still calls `SaveRing::resume` (not `new`), and `SaveCommand::execute` still takes `peek()` and only `advance()`s *after* validation passes (`byroredux/src/save_io.rs:751-789`). |
| Validation gate refuses to persist an inconsistent world, and is player-visible | **CODE-CONFIRMED.** Seven core checks (`validate_world`) + two binary-side (`validate_form_ids`, `validate_cinematic_entity_refs`) all run **before** `save_world`/`encode`/`write_slot`, with an unconditional early `return` on a non-empty result. `surface_save_load_output` still routes every player-facing output to `push_player_message`/`push_console_line`. |
| Typed-decode preflight rejects a bad snapshot before any live-world teardown | **CODE-CONFIRMED.** `validate_snapshot_types` is the first thing `execute_pending_save_loads` does after draining the slot (`byroredux/src/save_io.rs:1329-1338`), before the interior/exterior branch is even chosen; `restore_world` runs it before `clear_entities`. |
| `FORMAT_MAJOR` bump is the only sanctioned schema evolution path | **CODE-CONFIRMED.** Now `8` (was `5` at last sync), with per-version rationale in the doc comment for v6 (#2571 NIFAL clamp/blend fields), v7 (BGEM glass optics + Bethesda lighting response) and v8 (#3112, `EquipmentSlots.weapon` out of biped bit 31). `saved_type_shape_changes_require_format_major_bump`'s `BASELINE_MAJOR = 8` / `BASELINE_SHAPE_FINGERPRINT = 0xa6ba_8f20_d6fe_4907` were regenerated in the same commit (`a5ed4bf5`) that last moved `FORMAT_MAJOR` — verified via `git log -S`. |
| Off-frame load, never inside the scheduler | **CODE-CONFIRMED, and strengthened this cycle.** `restore_world` still has zero production callers. #3113 added a second deferral hop: F5/F9/pause-menu now enqueue `PlayerSaveAction` and `step_player_save_actions` executes them in `about_to_wait` *after* `Scheduler::run` joins and *after* `capture_player_pose`, immediately before `step_save_loads` — so a quickload queued this frame drains this frame. |
| Exterior saves can live-reload | **CODE-CONFIRMED, and last cycle's race is closed.** `exterior_reload_bootstrap_mode()` (`byroredux/src/save_io.rs:1293-1295`) now hard-returns `ExteriorBootstrapMode::FullRadius` with a doc comment naming exactly the hazard: *"foreground-first would permanently drop saved rows belonging to still-pending peripheral cells (#3280)"*. One narrow residual remains (SAVE-D6-2026-08-27-05). |
| Additive-only overlay + explicit reconciler for removals | **DRIFTED.** `apply_deltas`' own doc (`crates/save/src/driver.rs:310-317`) requires that *"runtime removals that are consequences of a persisted fact must be rebuilt by the binary after this call"*, naming `Dead`/`reconcile_dead_actor` as the model. The P2 inventory slice added a second runtime removal — `EquippedWeapon` — with no reconciler. See SAVE-D1-2026-08-27-01. |

**Findings this cycle: 5. 0 CRITICAL, 1 HIGH, 2 MEDIUM, 2 LOW.**

By Data-Loss Class: **silent-drop / corruption-on-load — 1** (HIGH);
**latent silent-drop (no live loss today) — 2** (MEDIUM);
**reference-break (narrow window) — 1** (LOW); **none — 1** (LOW).

## Data-Loss Class Matrix

| Finding | Class | Dimension | Severity |
|---|---|---|---|
| SAVE-D1-2026-08-27-01 — `EquippedWeapon` removal has no reconciler, so the additive-only overlay cannot clear it from the surviving player body | silent-drop → corruption-on-load | 1 (owner) / 6 | HIGH |
| SAVE-D1-2026-08-27-02 — `Perks`' allowlist reason cites a `validate_progression_state` guard that never inspects `Perks` | latent silent-drop | 1 | MEDIUM |
| SAVE-D1-2026-08-27-03 — the completeness guard's `SCAN_ROOTS` cannot notice a new crate; `crates/sdk` is unscanned | latent silent-drop | 1 | MEDIUM |
| SAVE-D6-2026-08-27-04 — the `FullRadius` bootstrap's worker-disconnect `break` re-opens a narrow #3280-shaped window; `apply_deltas` runs regardless of `state.pending` | reference-break | 6 | LOW |
| SAVE-D3-2026-08-27-05 — `save.info` still calls every exterior save `<none — loose/exterior save>` and never prints `CurrentExteriorContext` | none (operator diagnostic) | 3 | LOW |

## Per-Dimension Coverage

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & Determinism | **3** (1 HIGH, 2 MEDIUM) | SAVE-D1-12 guard re-run live (green). Consumed its `NOT_SAVED_BY_DESIGN` list rather than re-deriving; spot-checked `Locked` (still zero runtime mutators — `interaction.rs:934` only *reads* it), `Perks` (reason false — finding 02), `CharacterLevel` (reason true: `npc_spawn.rs:164-167` still stamps `xp: 0` unconditionally and `effective_actor_level` only writes `level`, with no `get_mut::<CharacterLevel>` anywhere), `WaterContact` (reason correctly updated to point at the now-saved `CharacterController`), `PendingPlayerSaveActions`/`SaveLoadNotifications` (both newly allowlisted with accurate one-line reasons). Verified none of the seven 2026-08-05 regressions (`RigidBodyData`, `RumbleOnActivate`, `Material`, `FragmentExecutionQueue`, the MQ101 cinematic trio) drifted back out, and that `CharacterController`'s allowlist entry stays removed. Determinism: both `save` closures still `rows.sort_by_key(|(entity, _)| *entity)` (`registry.rs:108`, `:253`), so the reproducible-CRC claim holds at row level, not just column level. |
| 2 — Registry & (De)serialization Fidelity | **0** | All three `register_*` variants build a matching `ValidateFn` decoding the *same* target type `load` does (`Vec<(u32, T)>` / `Vec<(u32, FormIdPair)>` / bare `R`). `FnvHasher` constants are canonical 64-bit FNV-1a (`0xcbf2_9ce4_8422_2325` / `0x0000_0100_0000_01b3`) and the fingerprint depends only on names + order + kind tag. `form_id_column()` still keys off the explicit `is_form_id` flag with the registration-time double-registration assert. `register_form_id_component` still saves the `FormIdPair`, warns-and-skips an unresolvable handle at save, and returns `SaveError::MissingResource("FormIdPool")` rather than panicking at load. Both shape guards green with a baseline regenerated in the same commit as the last `FORMAT_MAJOR` move. |
| 3 — Disk Format & Durability | **1** (LOW) | `write_slot`'s dance verified byte-for-byte, including the read-back-verify's tmp cleanup + `SaveError::Io`, and the post-rename parent-directory `sync_all` (SAVE-D3-01, still present). `decode`'s gate order is length → magic → major → schema_fpr → `checked_add` payload bounds → CRC over `[HEADER_LEN..payload_end]` → `from_slice`; every gate precedes any parse. `minor` is still advisory. `parse_slot_filename` still rejects `save_42.ess.tmp` and `save_x.ess`. `slots_by_recency` orders newest-first with a slot-number tie-break and skips unreadable metadata via `?`. Only finding is the cosmetic `save.info` exterior gap. |
| 4 — Validation Gates | **0** | The write-path gate is unconditional and precedes `save_world`. Coverage is now **nine** checks, not five: `validate_hierarchy`, `validate_equipment` (which #3112 correctly extended to span `EquipmentSlots.weapon` via `equipped_indices()`, plus the `EquippedWeapon` base-FormID cross-check), `validate_saved_entity_references`, `validate_animation`, `validate_inventory_instances`, `validate_progression_state`, `validate_material_finiteness`, plus the binary's `validate_form_ids` and `validate_cinematic_entity_refs`. Dangling semantics are still `>= next_entity`, not "has no components". `log_validation_warnings` stays diagnostic-only on both restore paths. |
| 5 — Frame-Boundary Capture & Off-Frame Apply | **0** | `save_world` is `&World`-only. Every production `SaveCommand::execute` call site is post-scheduler-join: the `DebugDrainSystem` exclusive, `apply_panel_outputs` inside `render_one_frame`, and the new `step_player_save_actions` drain — the last drops its `PendingPlayerSaveActions` guard via `mem::take` *before* entering the registry's wide lock surface (`save_io.rs:978-983`). `capture_player_pose` → `step_player_save_actions` → `step_save_loads` ordering is intact in `app_events.rs:700-710`. `SaveLoadNotifications` is still drained unconditionally by `mem::take` in `app_frame.rs:104-107`, `if let Some(ui)` gating only the *display*, so a headless run cannot leak the `Vec`. The live path never calls `restore_world`. |
| 6 — M45.1 Live Load-Apply | **1** (LOW; the HIGH is filed under Dim 1, which owns removal semantics) | Ordering re-verified: drain → typed preflight → context resolve → per-branch preflight (`validate_cell_loadable` / `build_exterior_world_context`) → teardown → reload → lighting + temporal discontinuity + `LoadedPluginSet` → `restore_resources` → `build_form_id_remap` → `apply_deltas` → `reconcile_dead_actor_runtime_state` → post-load `validate_world` → `apply_player_pose`. `restore_resources` still precedes `apply_deltas`; pose-restore is still last and still zeroes only `vertical_velocity`/`is_grounded`/`wants_jump` on top of the `CharacterController` column's `breath_remaining`/`drowning_damage_accumulator` overlay. The `Err` arm of `apply_deltas` still reconciles then `return`s. `build_form_id_remap` now logs unresolved pairs (take(20) + "… and N more", `driver.rs:266-291`), closing last cycle's "at minimum, log it" recommendation. `PLAYER_FORM_ID_PAIR` is still attached at spawn (`scene.rs:1364-1371`). |

## Completeness Ledger

`build_save_registry` (`byroredux/src/save_io.rs:267-449`) × `MUTABLE_DELTA_COLUMNS`
(`:84-135`, twenty-one entries). Cross-checked against the SAVE-D1-12 guard's
`NOT_SAVED_BY_DESIGN` allowlist rather than re-derived.

| Column | Kind | Saved | Overlaid | Status |
|---|---|---|---|---|
| `Transform`, `Inventory`, `EquipmentSlots`, `LightSource`, `LightFlicker`, `ScriptTimer`, `TwoStateActivator`, `ScriptVariables`, `ActorValues`, `EquippedWeapon`, `Dead`, `WanderState`, `TravelState`, `Traveled`, `GuardState`, `PatrolState`, `Escorted`, `ActorControlState`, `CharacterController`, `RigidBodyData`, `RumbleOnActivate` | Component | yes | yes | SAVED+OVERLAID, pinned by `delta_columns_carry_only_session_stable_fields`. **`EquippedWeapon` is overlay-safe but not removal-safe — see SAVE-D1-2026-08-27-01.** |
| `Name`, `Parent`, `Children`, `FormIdComponent` | Component | yes | no | structural identity — correct by design |
| `AnimationPlayer`, `AnimationStack` | Component | yes | no (deliberate) | #1696 `root_entity`/`clip_handle` hazard, unchanged |
| `FollowState`, `EscortState`, `Seated` | Component | yes | no (deliberate) | `EntityId` hazard, covered by `validate_saved_entity_references` |
| `ActorCinematicState`, `HorseTetherState` | Component | yes | no (deliberate) | #2380 `EntityId` hazard; also covered by `validate_cinematic_entity_refs` |
| `Material` | Component | yes | no (deliberate) | #2378 blast-radius; carries the v6/v7 fields. `restore_world`'s `sanitize_finite` sweep is unreachable in production (that path has no production callers) but the *prevention* half — `validate_material_finiteness` — runs on every save. |
| `ActorVitals` | Component | yes | no (documented) | **#3027 CLOSED, verified**: `save_io.rs:311-322` now carries the twelve-line reason explaining `ActorVitals.health` is a per-game AVIF **FormID key**, not an HP value. |
| `ItemInstancePool`, `CurrentCellContext`, `CurrentExteriorContext`, `PlayerPose`, `GameTimeRes`, `QuestStageState`, `QuestObjectiveState`, `Globals`, `QuestAliasInjectionState`, `PlayerControlState`, `FragmentExecutionQueue`, `ReferenceEnableState`, `CinematicPresentationState` | Resource | yes | n/a | replaced wholesale by `restore_resources`, before `apply_deltas` — correct. Verified `Globals` is re-installed fresh from ESM by `cell_loader/load.rs:177` on the reload and *then* overwritten by the saved value, so a scripted `GameHour` write wins over the ESM default. |

Ledger note (informational, not a finding): `ItemInstancePool` has **zero
production installation sites** — `grep` finds `ItemInstancePool::new()` only
in test fixtures, and `ItemInstancePool::allocate` has no production caller.
The registration and the `restore_resources`-before-`apply_deltas` ordering
that exists to serve it are therefore correct but currently inert. Nothing is
lost (`validate_inventory_instances` aborts any save whose `Inventory` rows
reference a pool that isn't installed), so this is forward plumbing, not a gap.

## Findings

### HIGH

#### SAVE-D1-2026-08-27-01: `EquippedWeapon` is removed at runtime with no reconciler, and the additive-only live overlay cannot clear it from the process-lifetime player body — loading an unarmed save leaves the player wielding the current session's weapon against the restored inventory

- **Severity**: HIGH
- **Dimension**: 1 — Snapshot Completeness & Determinism (removal semantics); manifests in 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: silent-drop → corruption-on-load
- **Location**: `byroredux/src/inventory.rs:493` (the removal), `crates/save/src/driver.rs:310-317` (the contract it violates), `crates/save/src/registry.rs:144-147` (the silent `filter_map` that makes it invisible), `byroredux/src/save_io.rs:102` (`"EquippedWeapon"` in `MUTABLE_DELTA_COLUMNS`), `byroredux/src/save_io.rs:1367-1384` (the one-shot `build_form_id_remap` + `apply_deltas` + `reconcile_dead_actor_runtime_state` tail — no equipment reconciler), `byroredux/src/combat.rs:414-437` (`reconcile_dead_actor`, the contract-compliant counterexample)
- **Status**: NEW. The removal site landed with the P2 gameplay slice (2026-08-15/16) and was reshaped by `a5ed4bf5` (#3112) this cycle; no prior save audit has reviewed `byroredux/src/inventory.rs`, which sits in an explicitly **un-owned** subsystem per `.claude/commands/_audit-common.md`'s coverage-gap table.
- **Description**: `apply_deltas`' own doc comment states the contract plainly:

  ```rust
  /// This overlay is **additive-only** — it can update or insert a row via
  /// `ApplyFn`, never remove one. Runtime removals that are consequences of a
  /// persisted fact must therefore be rebuilt by the binary after this call.
  /// Death uses that model: `Dead` is overlaid here, then the shared combat
  /// reconciler removes respawned AI/animation state and reactivates ragdoll
  /// (#3022). Future disable/delete persistence needs the same explicit
  /// marker-plus-reconciler contract […]
  ```
  (`crates/save/src/driver.rs:310-317`)

  The native inventory menu now performs exactly such a runtime removal. Unequipping the wielded weapon routes `apply_action` → `reconcile_equipped_weapon`, whose `else` arm is:

  ```rust
  if let Some(weapon) = candidate {
      world.insert(player, weapon);
  } else {
      let _ = world.remove::<EquippedWeapon>(player);
  }
  ```
  (`byroredux/src/inventory.rs:489-494`)

  This is production-reachable, not a debug path: `main.rs:813-817` drives it straight off the pause-menu overlay's `outputs.inventory_actions`.

  Three facts combine into the bug:

  1. **The player body outlives the reload.** `unload_cell_inner` draws its victim set from `CellRootIndex` (`byroredux/src/cell_loader/unload.rs:139-142`); the player body is spawned in `scene::setup_scene` and never stamped with a `CellRoot`, which the SAVE-D1-12 allowlist itself states — *"`PlayerEntity`: points to the process-lifetime player body, which deliberately outlives cell unload; the entity remains valid across live reload"*. So on a live load the player body keeps every component the current session gave it.
  2. **A removed component leaves no trace in the snapshot.** `save_world` serialises the live `EquippedWeapon` column; an unarmed player simply has no row (and if no NPC has one either, `save_world` omits the column entirely, `driver.rs:37-45`).
  3. **`apply_deltas` cannot express a removal.** The component `ApplyFn` `filter_map`s saved rows through the remap and calls `insert_batch`; a saved id with no row is a no-op, and a *live* row with no saved counterpart is never touched (`crates/save/src/registry.rs:144-149`).

  Net: quicksave while unarmed, equip a weapon, quickload that save — the player is still holding the weapon. Worse, the *sibling* columns **do** overlay: `Inventory` and `EquipmentSlots` are both in `MUTABLE_DELTA_COLUMNS` (`save_io.rs:86-87`), so the restored `EquipmentSlots.weapon == None` now contradicts the surviving `EquippedWeapon`, whose `inventory_index` indexes an inventory that was just wholesale-replaced.
- **Evidence**: `byroredux/src/inventory.rs:493` (quoted above). `byroredux/src/main.rs:813-817` — the production driver: `for action in outputs.inventory_actions { if inventory::apply_action(world, action) == …`. `byroredux/src/save_io.rs:1367-1384` — the entire post-`apply_deltas` tail is `reconcile_dead_actor_runtime_state(world)` and nothing else; `grep -rn "reconcile_equipped_weapon" byroredux/src` returns exactly one call site, inside `apply_action`, never from the load drain. `combat.rs:414-419` documents the correct pattern for the analogous case (*"Live-load deltas are intentionally additive, so absence of AI and animation components is not serialized as a second, generic tombstone format. Both the combat transition and save-load drain call this one reconciler"*) — `EquippedWeapon` has no such shared reconciler. The gates do not catch it: `validate_equipment` (`crates/save/src/validate.rs:196-240`) checks `EquippedWeapon.inventory_index` bounds and its `base_form_id` against `inventory[index]`, but a stale weapon whose index is in range and whose form id matches the restored item at that index passes cleanly, and the whole pass is post-load **diagnostic only** anyway (`log_validation_warnings`, no abort). No test covers the direction: `crates/save/tests/round_trip.rs:333-382` (`player_body_inventory_survives_live_load`) proves only that an *added* `Inventory` overlays onto the surviving player; the removal direction is untested for every column.
- **Impact**: Fires on the ordinary path — equip/unequip through the pause menu is the P2 slice's headline interaction, and quickload is a one-keystroke action. The player's wielded state silently desynchronises from the save they just loaded, and because `combat.rs` reads `EquippedWeapon` (not `EquipmentSlots.weapon`) to resolve melee damage, the reloaded session deals the *current* session's weapon damage rather than the saved unarmed damage — a gameplay-visible divergence with no log line anywhere. Blast radius is one component today, but the defect is contract-shaped, not instance-shaped: every future `world.remove::<T>()` on a `MUTABLE_DELTA_COLUMNS` type inherits it, and `EquippedWeapon` is the first to land since the contract was written.
- **Related**: #1847 / SAVE-04 (the additive-only overlay contract); #3022 (`reconcile_dead_actor`, the model the contract names); #3112 (`a5ed4bf5`, which reshaped `reconcile_equipped_weapon` this cycle without noticing the save interaction); `.claude/commands/_audit-common.md`'s "Gameplay slice (P2) — **NO owner audit skill**" row, which predicted exactly this class of miss.
- **Suggested Fix**: Add an equipment reconciler alongside `reconcile_dead_actor_runtime_state` in `execute_pending_save_loads`'s tail that re-derives `EquippedWeapon` from the just-overlaid `EquipmentSlots.weapon` + `Inventory` — `inventory::reconcile_equipped_weapon` already *is* that function; call it for the player after `apply_deltas` succeeds (it removes the component when `EquipmentSlots.weapon` is `None`, which is precisely the missing behaviour). Extend `crates/save/tests/round_trip.rs` with the removal-direction case (`player_body_unequipped_weapon_survives_live_load`), and add a tripwire beside `delta_columns_carry_only_session_stable_fields` asserting that any `MUTABLE_DELTA_COLUMNS` type with a production `world.remove::<T>` site is named in an explicit reconciler allowlist.

### MEDIUM

#### SAVE-D1-2026-08-27-02: the `Perks` completeness-allowlist reason cites a `validate_progression_state` guard that never inspects `Perks` — the ledger's only line of defence against a future perk-granting runtime does not exist

- **Severity**: MEDIUM
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: latent silent-drop (no loss today — `Perks` has zero production mutators)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:108`, `byroredux/src/save_io/round_trip_tests.rs:765-777`, `crates/save/src/validate.rs:410-442`
- **Status**: NEW
- **Description**: The SAVE-D1-12 allowlist entry reads:

  ```rust
  ("Perks", "known progression gap guarded by validate_progression_state: saves are refused once perks exist (#2947)"),
  ```

  and `round_trip_tests.rs`'s `REDERIVED_NOT_SAVED` preamble repeats the claim for both types: *"`crates/save/src/validate.rs::validate_progression_state` aborts any save where a `CharacterLevel.xp != 0` slips through **with these two still unregistered**, so the exemption fails loudly rather than silently discarding progress."*

  `validate_progression_state` reads `Perks` nowhere. Its whole body is:

  ```rust
  fn validate_progression_state(world: &World, errors: &mut Vec<ValidationError>) {
      let Some(q_level) = world.query::<CharacterLevel>() else { return; };
      for (entity, level) in q_level.iter() {
          if level.xp != 0 { … }
      }
  }
  ```

  Its own doc comment (`validate.rs:410-422`) is scrupulously accurate and only ever discusses `CharacterLevel.xp` — the over-claim exists solely in the two allowlist reasons. The stated trigger ("once perks exist") is also already false in the component sense: `scene.rs:1393` inserts `Perks::default()` on the player body unconditionally (#3158), and `npc_spawn.rs:178-186` inserts a populated `Perks` for any NPC with `PRKR` entries. Saves are not refused, correctly, because `Perks` is genuinely write-once from ESM today — but that is a *different* argument from the one the allowlist makes.
- **Evidence**: `crates/save/src/validate.rs:424-442` (quoted, complete function body — the only `Perks` reference anywhere in `crates/save` is absent). `grep -rn "get_mut::<Perks>\|query_mut::<Perks>\|resource_mut::<Perks>" byroredux/src crates/` returns zero hits, confirming the exemption is *substantively* safe today. `byroredux/src/scene.rs:1393` — `world.insert(body, byroredux_core::character::Perks::default());`.
- **Impact**: The completeness ledger is this subsystem's primary silent-drop defence, and the skill's own Dimension 1 instruction is to consume its reasons rather than re-derive them — so a wrong reason propagates directly into every future audit and every future reviewer's mental model. The day an `AddPerk` effect or a perk-selection UI lands (`docs/engine/charal.md`'s perk work, #3004/#2986), the author will read "guarded by `validate_progression_state`", conclude the gate will catch them, and ship a runtime that silently discards every granted perk on save — with the guard test still green, because a green `NOT_SAVED_BY_DESIGN` entry only asserts that *a* reason exists.
- **Related**: #2947 (the `CharacterLevel` half, which is correctly implemented); #3158 (the unconditional player `Perks` stub); the skill's own note that the guard "enforces a reason exists, not that it's still true".
- **Suggested Fix**: Either extend `validate_progression_state` to also flag a non-empty `Perks` on any entity (making the claim true and giving the future perk runtime the loud failure the ledger promises), or rewrite both reasons to state the real justification — "`Perks` is stamped verbatim from `NPC_.PRKR` at spawn with no production mutator (`grep` for `get_mut::<Perks>`); register it the moment an `AddPerk` effect lands." Prefer the former: it costs four lines and makes the ledger self-enforcing rather than self-describing.

#### SAVE-D1-2026-08-27-03: the SAVE-D1-12 completeness guard's `SCAN_ROOTS` is a hardcoded six-entry list with no mechanism to notice a new crate — `crates/sdk` landed this cycle carrying an `impl Resource` outside every root

- **Severity**: MEDIUM
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: latent silent-drop (no loss today — `StudioSession` is authoring-tool state, correctly not save-worthy)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:362-369`, `crates/sdk/src/studio.rs:120`
- **Status**: NEW — made possible by `21a840d5` ("feat: introduce byroredux-sdk"), the first new workspace crate to define ECS state since the guard was written
- **Description**: The guard's scan set is:

  ```rust
  const SCAN_ROOTS: &[&str] = &[
      "../crates/core/src",
      "../crates/scripting/src",
      "../crates/physics/src",
      "../crates/audio/src",
      "../crates/plugin/src",
      "../byroredux/src",
  ];
  ```

  It has a strong self-defence against a root *moving* (`collect_rs_files` panics on an unreadable directory, and a `!found.is_empty()` assert catches the impl-line shape changing) — but none at all against a root that was never added. `crates/sdk/src/studio.rs:120` declares `impl Resource for StudioSession {}`, and `StudioSession` is neither registered in `build_save_registry` nor listed in `NOT_SAVED_BY_DESIGN`. The guard is green because it simply never looks there.

  `StudioSession` itself is correctly excluded on the merits — it is a Studio authoring document holding `Vec<EntityId>` / `Option<EntityId>` / a `BTreeMap<EntityId, TransformValue>`, all session-local identity, installed only when the Studio host is active (`byroredux/src/app_events.rs:163-168` opens the Studio panel when it is present). So there is no live data loss. The defect is that the ledger's *coverage* silently shrank relative to the workspace, in exactly the way the ledger exists to prevent.
- **Evidence**: `grep -rn --include='*.rs' "^impl Component for \|^impl Resource for " crates/ | grep -vE "^crates/(core|scripting|physics|audio|plugin|save)/"` returns four hits: `crates/sdk/src/studio.rs:120` (`StudioSession`), `crates/debug-ui/src/lib.rs:179` (`DebugUiState`), and `crates/renderer/src/vulkan/allocator.rs:49,70` (`AllocatorResource`, `GpuMemoryBudget`). The last three are unambiguously renderer/overlay infrastructure and predate the guard; `StudioSession` is the new one, and it is the only one of the four that carries a *document* rather than a device handle. `_audit-common.md`'s crate list is 25 entries against the guard's six roots.
- **Impact**: The SDK is described in `_audit-common.md` as *"the first tooling API surface"* and has no owner audit skill of its own. If Studio grows a document field that is genuinely game state (a persisted scene edit, a per-asset material override the engine should reload), it will land unnoticed by the one guard whose job is to notice exactly that. The cost of the miss compounds: the guard's green run is cited in this report and every prior one as "the completeness ledger", so an unscanned crate is not merely unchecked, it is affirmatively reported as checked.
- **Related**: #2295 / #3166 (the guard and its last `SCAN_ROOTS` widening); `21a840d5`; the "ByroRedux SDK — no dedicated owner" row in `_audit-common.md`'s un-owned-subsystems table.
- **Suggested Fix**: Replace the hardcoded list with a discovery step — enumerate `crates/*/src` from the workspace root and subtract an explicit, reasoned `NOT_SCANNED` set (`renderer`, `debug-ui`, `ui`, `save` itself, the parser-only crates) — so adding a crate forces a deliberate classification instead of silently widening the blind spot. Failing that, add `"../crates/sdk/src"` now and give `StudioSession` a `NOT_SAVED_BY_DESIGN` entry ("Studio authoring document holding session-local `EntityId`s; the edited world state it describes is saved through the normal component columns").

### LOW

#### SAVE-D6-2026-08-27-04: the `FullRadius` bootstrap's worker-disconnect escape hatch re-opens a narrow #3280-shaped window — `execute_pending_save_loads` runs `apply_deltas` without ever checking `state.pending`

- **Severity**: LOW
- **Dimension**: 6 — M45.1 Live Load-Apply
- **Data-Loss Class**: reference-break
- **Location**: `byroredux/src/scene/world_setup.rs:837-847`, `byroredux/src/save_io.rs:1258-1264` (the `count_label` that *does* report the pending count), `byroredux/src/save_io.rs:1367-1368`
- **Status**: NEW — residual of the fix for last cycle's SAVE-D6-2026-08-24-01 (#3280)
- **Description**: `exterior_reload_bootstrap_mode()` returns `FullRadius` specifically so `state.pending` is drained before `build_form_id_remap` scans the world, and `bootstrap_waiting(FullRadius, …)` is `!pending.is_empty()` — correct. But the wait loop has one non-`pending`-driven exit:

  ```rust
  let payload = match state.payload_rx.recv() {
      Ok(p) => p,
      Err(_) => {
          log::error!(
              "Streaming worker disconnected mid-bootstrap with {} pending cells",
              state.pending.len(),
          );
          break;
      }
  };
  ```

  On that break, `stream_initial_radius` returns with a non-empty `pending`, `reload_exterior_session` reports it honestly in `count_label` (`"{} cells streaming ({} pending)"`) — and `execute_pending_save_loads` then calls `build_form_id_remap` + `apply_deltas` unconditionally anyway, silently dropping every saved delta row belonging to a cell that never arrived. This is the identical mechanism #3280 fixed, on a narrower trigger.
- **Evidence**: `world_setup.rs:837-847` (quoted). `byroredux/src/save_io.rs:1367-1368` — `let remap = byroredux_save::build_form_id_remap(…); match byroredux_save::apply_deltas(…)` with no guard on `state.pending` between them; the only condition guarding the tail is `outcome`'s `Some`/`None`, and `reload_exterior_session` returns `Some` unconditionally after `assemble_exterior_streaming`. Mitigation already present: `build_form_id_remap` now warns per unresolved `FormIdPair` (`driver.rs:278-291`), so the loss is at least diagnosable — which is why this is LOW rather than a repeat HIGH.
- **Impact**: Requires the streaming worker thread to die mid-bootstrap, which is already an engine-broken state; but the consequence is silent, permanent save-state loss layered on top of it, and the next save re-records the reverted state as truth. The `log::error!` names the pending count but not the resulting delta loss, so an operator reading the log would not connect the two.
- **Related**: #3280 / SAVE-D6-2026-08-24-01 (the primary fix); #2019 / SAVE-D6-04 (the unresolved-pair warning that limits the blast radius).
- **Suggested Fix**: Have `reload_exterior_session` return `None` (with a `notify_player` message) when `state.pending` is non-empty after a `FullRadius` bootstrap, so the load aborts loudly instead of half-applying — the same posture `validate_snapshot_types` and `validate_cell_loadable` already take for their own failure modes.

#### SAVE-D3-2026-08-27-05: `save.info` still reports every exterior save as `<none — loose/exterior save>` and never prints its `CurrentExteriorContext`

- **Severity**: LOW
- **Dimension**: 3 — Disk Format & Durability (operator diagnostics)
- **Data-Loss Class**: none
- **Location**: `byroredux/src/save_io.rs:869-878`
- **Status**: NEW
- **Description**: `SaveInfoCommand::execute` was not updated when exterior save/load shipped (`0a847910`, EX-09/17 item 4):

  ```rust
  match snapshot_cell_context(&snap) {
      Some(ctx) => lines.push(format!("  cell: {} (esm {}, {} master(s))", …)),
      None => lines.push("  cell: <none — loose/exterior save>".to_string()),
  }
  ```

  `snapshot_exterior_context` exists (`save_io.rs:464-471`) and `LoadCommand` already uses it to build a `"worldspace '{}' @ ({},{})"` destination label — `save.info` is the one consumer that never got the second arm. An operator inspecting an exterior quicksave is told it is a loose save that cannot be live-loaded, which is the opposite of the truth.
- **Evidence**: `byroredux/src/save_io.rs:869-878` (quoted) versus `:1022-1037` (`LoadCommand`'s three-arm `match (snapshot_cell_context(…), snapshot_exterior_context(…))`). The resource *is* listed later by the generic `for name in snap.resources.keys()` loop, but only as a bare `resource CurrentExteriorContext` line with no worldspace or grid.
- **Impact**: Diagnostic only — no save or load behaviour changes. It matters because `save.info` is the operator's only pre-load inspection tool over `byro-dbg`, and it now actively contradicts `load`'s own classification of the same file.
- **Related**: `0a847910` (EX-09/17 item 4); SAVE-D6-2026-08-24-02 / #3028, the doc-side instance of the same omission, now fixed in `5458522d`.
- **Suggested Fix**: Mirror `LoadCommand`'s three-arm match — print the worldspace key, grid, and load radius for the exterior arm, and reserve `<none — loose save>` for a snapshot carrying neither context.

## Prior-Cycle Disposition (2026-08-24)

Both findings re-checked at HEAD `969d81c8`, line-by-line, not taken on the fix
commits' word:

| 2026-08-24 finding | Issue | State at HEAD |
|---|---|---|
| SAVE-D6-2026-08-24-01 — exterior live-load's one-shot delta overlay races the streaming worker; every non-arrival cell's saved state silently dropped | #3280 | **FIXED, verified.** `98eea9b3` introduced `exterior_reload_bootstrap_mode()` (`save_io.rs:1288-1295`), which hard-returns `ExteriorBootstrapMode::FullRadius` with a doc comment naming the hazard by issue number. `bootstrap_waiting(FullRadius, …)` is `!pending.is_empty()` (`world_setup.rs:748`), so the wait loop drains the whole radius before `build_form_id_remap` scans. The report's secondary recommendation also landed: `build_form_id_remap` now logs every unresolved `FormIdPair` (`driver.rs:266-291`). One narrow residual filed as SAVE-D6-2026-08-27-04. |
| SAVE-D6-2026-08-24-02 — `save-load-roundtrip.md` asserts exterior saves cannot live-reload | #3028 | **FIXED, verified.** `5458522d` refreshed the doc: it now documents `CurrentExteriorContext` (`:53`, `:124`), the `FullRadius` reload branch (`:158`), the `validate_cell_loadable`/`build_exterior_world_context` preflight split (`:144`), the typed preflight (`:13`), and the #3021 interior/exterior mutual-exclusion fix (`:24`). The false §5 claim is gone. |

Also closed and verified this cycle: **#3027** (`ActorVitals`'s undocumented
`MUTABLE_DELTA_COLUMNS` exclusion — `5458522d` added the twelve-line reason at
`save_io.rs:311-322`, matching exactly the semantics last cycle's *Disproved
Candidates* section derived) and **#2687** (non-finite `Material` scalars —
`sanitize_finite` is now wired into both halves, `driver.rs:145-159` repairing
on restore and `validate.rs:450-466` preventing on save; `59b85565`/#3373
closed the BGEM glass-optics hole in the field list).

## Regression Guards Verified This Cycle

All run live (`cargo test -p byroredux-save`, `cargo test -p byroredux save_io::`),
not re-derived from a prior report:

| Guard | Location | Invariant pinned | State |
|---|---|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | `save_io/registry_completeness_tests.rs` | every `impl Component`/`impl Resource` under six scan roots is registered XOR allowlisted with a reason | **green** — but see SAVE-D1-2026-08-27-03 for what the roots miss |
| `saved_type_shape_changes_require_format_major_bump` | `save_io/serde_default_guard_tests.rs:289-301` | any field add/remove/retype on a saved struct requires a `FORMAT_MAJOR` bump; `BASELINE_MAJOR = 8`, `BASELINE_SHAPE_FINGERPRINT = 0xa6ba_8f20_d6fe_4907` | **green**, baseline regenerated in `a5ed4bf5` — the same commit that moved `FORMAT_MAJOR` 7 → 8 |
| `serde_default_on_saved_struct_requires_format_major_bump`, `serde_guard_handles_bare_and_cfg_attr_forms`, `serde_guard_ignores_skipped_fields_and_non_keys` | same file | no bare/`cfg_attr` `#[serde(default)]` on any save-participating type | **green** |
| `source_discovery_follows_registry_and_nested_save_modules` | same file | scan set derived from the registry plus the four explicit non-turbofish edges | **green** |
| `delta_columns_carry_only_session_stable_fields` | `save_io/round_trip_tests.rs` | `MUTABLE_DELTA_COLUMNS` == hand-audited set, no `FixedString`/`EntityId` | **green** — pins *membership*, not removal semantics (SAVE-D1-2026-08-27-01) |
| `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` | `save_io/round_trip_tests.rs:740-803` | every NPC-spawn-stamped type is registered XOR in `REDERIVED_NOT_SAVED` | **green** — its `Perks` preamble is the second site of SAVE-D1-2026-08-27-02 |
| `character_controller_breath_state_survives_live_delta_overlay` | `save_io/round_trip_tests.rs:806+` | the v5 `CharacterController` column overlays *before* pose-restore's momentum clear | **green** |
| `typed_snapshot_preflight_rejects_bad_column_without_world_mutation` | `crates/save/tests/round_trip.rs` | `validate_snapshot_types` runs before `clear_entities` and never touches the world on failure | **green** |
| `player_body_inventory_survives_live_load` | `crates/save/tests/round_trip.rs:333-382` | `PLAYER_FORM_ID_PAIR` resolves saved → live and the player's `Inventory` overlays | **green** — covers the *additive* direction only |
| `anim_player_root_entity_not_clobbered_by_delta_apply`, `delta_apply_skips_unresolvable_form_id_without_disturbing_others`, `delta_apply_reroutes_by_form_id_after_cell_reload` | `crates/save/tests/round_trip.rs` | #1696 exclusion; remap-miss isolation; form-id re-targeting | **green** |
| `restore_world_rejects_snapshot_with_out_of_bounds_entity_id`, `restore_world_does_not_abort_on_referentially_broken_snapshot` | `crates/save/tests/round_trip.rs` | `EntityIdOutOfBounds` is a real (non-`debug_assert`) gate; post-restore validation is diagnostic-only | **green** |
| `material_with_non_finite_scalar_trips_the_gate`, `material_with_only_finite_scalars_is_clean`, `sanitize_finite_leaves_no_non_finite_float_anywhere` | `crates/save/src/validate.rs`, `crates/core/src/ecs/components/material.rs` | #2687/#3373 NaN prevention + repair, field-list parity between the two halves | **green** |
| `form_id_column_resolves_the_flagged_entry`, `form_id_column_is_none_without_registration`, `registering_a_second_form_id_column_panics` | `crates/save/src/registry.rs:408-447` | #1845's explicit `is_form_id` flag, not the old `apply.is_none()` heuristic | **green** |
| Container gates (`rejects_bad_magic` / `rejects_truncated` / `rejects_payload_truncation` / `detects_crc_corruption` / `rejects_schema_mismatch` / `rejects_major_version_skew`) | `crates/save/src/snapshot.rs` | every header gate precedes `serde_json::from_slice` | **green** |
| Disk atomicity (`write_read_round_trip_and_atomic_rename`, `latest_slot_ignores_newer_tmp_and_empty_directory`, `recency_tie_breaks_by_slot_number`, `cursor_after_newest_points_past_latest_mtime`, `parse_slot_names`) | `crates/save/src/disk.rs` | atomic rename, tmp exclusion, deterministic recency, ring resume | **green** |
| `quickload_empty_errors_and_corrupt_newest_falls_back`, `startup_load_parser_queues_valid_slot_and_surfaces_invalid_value`, `save_then_load_command_queues_with_exterior_context`, `load_command_rejects_a_save_with_no_cell_or_exterior_context`, `validation_aborted_quicksave_is_classified_for_player_feedback` | `save_io/command_queue_tests.rs` | the player-facing entry points (F5/F9/menu/`--load`) and `LoadCommand`'s two-context accept | **green** — queueing only; the exterior *drain* still has no test |
| `validate_progression_state` (#2947) | `crates/save/src/validate.rs:424-442` | save aborts once `CharacterLevel.xp != 0` | **green** — and scoped to `CharacterLevel` alone (SAVE-D1-2026-08-27-02) |

## Disproved Candidates (investigated, not filed)

- **"`validate_material_finiteness` is an over-aggressive abort gate: one NaN material anywhere in a cell makes the game permanently unsaveable, with no repair path on the save side."** Investigated and rejected on reachability. The three producers of a renderer-bound `Material` all guard finiteness at their own boundary: `translate_material` clamps and `is_finite`-checks (`byroredux/src/material_translate.rs:105-173`), `mat.set` explicitly rejects non-finite input (`byroredux/src/commands/scene.rs:838-842`, with a comment noting `"NaN".parse::<f32>()` succeeds), and the SDK's `StudioCommand::SetMaterial` runs `valid_material` (`byroredux/src/studio_host.rs:172-177`) before writing. The gate is therefore reachable only by a hand-edited-but-CRC-valid file, which is exactly the case it exists for.
- **"Two live entities can share a `FormIdPair`, so `build_form_id_remap`'s `pair_to_live` collapses them last-wins and `apply_deltas` writes the delta onto an arbitrary one."** Rejected — `is_primary_synth` (#2541) gates every synth-child identity stamp so at most one entity per REFR carries `stamp_quest_reference`'s pair (`byroredux/src/cell_loader/references/synth_child.rs:662-674`, `:720-732`), and `exterior.rs:1027` carries an explicit assert against a second construction site. The player's `PLAYER_FORM_ID_PAIR` is a reserved sentinel with one insertion site.
- **"A registered resource absent from the snapshot leaves the live resource untouched, so quest/global state from the discarded session survives a load."** Rejected for every registered resource. `QuestStageState` is installed unconditionally in *both* branches of `scene::setup_scene` (`scene.rs:1433`, `:1460`), so a save always carries the column; `Globals` is re-installed fresh from ESM by `cell_loader/load.rs:177` during the reload *before* `restore_resources` overwrites it with the saved value; `CurrentCellContext`/`CurrentExteriorContext` are handled explicitly and are mutually exclusive by construction (#3021, `transition.rs:141` + `:397-399`). The remaining lazily-installed resources default to empty, so an absent column and a default column are indistinguishable.
- **"`ItemInstancePool`'s absence breaks the `restore_resources`-before-`apply_deltas` ordering guarantee."** Rejected — the ordering is correct and the resource is simply never installed in production yet (`ItemInstancePool::allocate` has zero production callers). `validate_inventory_instances` aborts any save whose `Inventory` rows would reference a missing pool, so the inert plumbing cannot lose data. Noted in the Completeness Ledger as informational.
- **"`SaveCommand::execute` running inside `render_one_frame` (the debug-UI console path) is a mid-frame capture and can snapshot torn state."** Rejected — `apply_panel_outputs` runs after `Scheduler::run` has joined for the frame (`app_events.rs:690-745`), so no system holds a storage write lock; the widest-hold reasoning is documented at the call site (`save_io.rs:724-737`, #3113/#2154) and the new `PendingPlayerSaveActions` hop moves the *input-driven* callers onto the same quiescent lane.
- **"A `PlayerPose`-less save reaches `apply_player_pose`."** Rejected again this cycle (unchanged code shape): `PlayerPose` is installed at boot (`boot.rs:1596`) and refreshed every frame before any save command can run, so `snapshot_player_pose` returning `None` requires a save predating the resource's registration — which the schema fingerprint rejects in `decode` first.

## Deduplication

`gh issue list --repo matiaszanolli/ByroRedux --limit 300 --state open` (139
open issues) searched for `save`, `load`, `snapshot`, `equip`, `weapon`,
`perk`, `registry`, `delta`, `overlay`, `sdk`, `corrupt`, `formid`, `serde`,
`quicksave`, `quickload`, `ring`, `validate`, `exterior`, `streaming`. No open
issue overlaps any of the five findings; the nearest neighbours (#3299 —
"actor/package state snapshot/restore across ordinary stream-tile boundaries",
#3254 — cinematic unload-retention orphaning) are streaming/ECS-domain and do
not touch the save registry or the delta overlay. `docs/audits/` scanned: no
prior `AUDIT_SAVE_*` report mentions `EquippedWeapon`, `inventory.rs`,
`SCAN_ROOTS` coverage, or the `Perks` guard claim.
