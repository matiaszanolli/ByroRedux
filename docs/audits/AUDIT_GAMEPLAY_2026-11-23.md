# Gameplay Audit — 2026-11-23

**HEAD**: 04bf89c44 (clean `origin/main` worktree) · **Baseline**: AUDIT_GAMEPLAY_2026-11-22.md (audited 2b40f76b6) · **Audited**: Dims 1, 2, 4, 5, 7 (delta-first) · **Light**: Dim 6 (one commit) · **Skipped**: Dim 3 (no commits on its paths since the baseline)

Read-only run: no repo edits, no engine launched, no issues filed. Every test command used the pinned 1.100.0 toolchain with `TMPDIR=/mnt/data/tmp/cargo-tmp`, never `--ignored`. Baseline guard run: **765 passed, 0 failed, 32 ignored** (`-p byroredux --bin byroredux` over the combat, inventory, interaction, npc_spawn, reference_state, ambient-locomotion, package, crime, force-greet, voice, VATS, save_io, door, sandbox, travel, registry-completeness and scheduler-access filters), plus 233 targeted tests by the dimension agents, all green.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 3 |
| LOW | 6 |
| **Total** | **9** |

The delta since 2b40f76b6 (about 60 commits on these paths) is mostly the fixes for the previous report's findings (#7224–#7234, #7243, #7252–#7254, #7262) plus the scripting-audit fixes that touch gameplay (#7291, #7302, #7305, #7306, #7317, #7281, #7294, #7310). Most hold up. Three are half-applied (GP-02 for #7305, GP-04 for #7317, GP-05 for #7252), and one opens a new path (GP-03 for #7262).

- **GP-01 (MEDIUM):** a script's `DisablePlayerControls` does not stop the player fighting or activating. `fighting_enabled` is read only by the ObScript getter, and `activation_enabled` by nothing.
- **GP-02 (MEDIUM):** #7305 stopped parking a door's `TwoStateActivator`, but the door script's `::isOpen_var` / `::isAnimating_var` are still parked, so a scripted door comes back with its variables contradicting its state.
- **GP-03 (MEDIUM):** a Near Editor Location Sandbox now picks a seat around the editor placement and `apply_seat_assignments` snaps the actor there, with no walk and no arrival test: a displaced sandboxer teleports across the cell (the Eat/Sleep twin was #5726).
- **LOW:** two Once Per Day half-fixes (GP-04, GP-05), exterior-ring doors leaving a permanent parked row (GP-06), a dropped tempered item losing its quality (GP-07), the add-side sibling of #7231 (GP-08), and `stance_use_system`'s undeclared acquisitions after #7281 (GP-09).

## Invariant Matrix

| Invariant | State |
|---|---|
| Single damage path (every `HitEvent` producer ahead of `combat_damage_system`, `ignores_resistance` set) | verified |
| Death reconciled once (every production `Dead` insert reaches `reconcile_dead_actor`; `spare_from_death` and the Story KILL where the skill says) | verified |
| Inventory index stability (rows zeroed in place; `Inventory::add` folding; `count_of` / `first_row_of`) | verified; GP-08 is a zero-count add path |
| Fail-closed consumables vs fail-open packages | verified (Dim 3 unchanged) |
| Stage order (built schedule vs the skill's order; door motion before the activator; flush before `ActivateEvent` consumers) | verified |
| AmbientBehavior arms = `from_package` = `AMBIENT_COMPONENT_REMOVERS` = debug-server registration | verified (14 procedures) |
| State saved or re-derived | drifted: GP-02, GP-04, GP-05, GP-06 |
| Determinism grep (no `rand::` / `Instant::now` / `SystemTime` in gameplay systems) | verified, empty |
| Player control locks honoured by every action system | **drifted: GP-01** |
| Declared access matches acquisitions | drifted: GP-09 |

## Findings

### GP-01: `PlayerControlState.fighting_enabled` / `activation_enabled` have no consumer, so a script's `DisablePlayerControls` does not stop the player attacking, blocking, firing, casting or activating
- **Severity**: MEDIUM
- **Dimension**: 4 (combat input) / 6 (player flow)
- **Location**: `byroredux/src/combat.rs:199-203` (`can_swing` = Character mode + `player_can_act`), `byroredux/src/combat.rs:241` (`state.blocking = block_held`), `byroredux/src/systems/character.rs:91-103` (`player_can_act`), `byroredux/src/bow.rs:255`, `byroredux/src/player_cast.rs:36`, `byroredux/src/interaction.rs:1755`; writer `crates/scripting/src/player_control.rs:105-125`; the only reader `crates/scripting/src/obscript_more.rs:445-446`
- **Status**: NEW (no open or closed issue; #2292 only saved the state; not in the 2026-11-15 / 11-22 reports)
- **Trigger**: any game, any cell where a script calls `DisablePlayerControls` (Skyrim `Game.DisablePlayerControls()` with its CK defaults, ObScript `DisablePlayerControls`, FO4/SF `InputEnableLayer.DisablePlayerControls`). Examples named in the repo: Skyrim `QF_MQ101_*` (`SetInChargen; DisablePlayerControls; ...`), FNV VCG chargen. Save state: any, the control state is saved.
- **Description**: `PlayerControlState::apply_selection` stores every control domain the script selects, and the module header says "lets the app input/controller layer consume the same state". Only `movement_enabled` (`player_accepts_movement_input`, `player_seat`), `looking_enabled` (`app_events.rs:1507`) and `sneaking_enabled` (M65 `systems/stance.rs`) have a consumer in the binary. `fighting_enabled` and `activation_enabled` (and `menu_enabled`) are read only by ObScript `GetPlayerControlsDisabled`. `DISABLE_PLAYER_CONTROLS_DEFAULT` sets `fighting: true, activation: true`, so a bare `Game.DisablePlayerControls()` is meant to lock attack, block, ready-weapon and activate.
- **Evidence**:
  - `rg 'fighting_enabled|activation_enabled' byroredux crates --glob '!*test*'` returns the field declaration, `apply_selection`, and `obscript_more.rs:445-446` (`GetPlayerControlsDisabled`) only.
  - `combat_input_system`: `let can_swing = world.try_resource::<PlayerMode>()...== Character && crate::systems::player_can_act(world);` and `player_can_act` is `Dead` + VATS only.
  - `state.blocking = block_held;` is written from the raw key every frame, and `target_block` reads `MeleeState.blocking`, so a locked-out player still resists damage and banks Block.
  - Same shape in `player_bow_system`, `player_cast_system`, `iron_sights_system` (all gate on `player_can_act`) and `interaction_system` (`interaction.rs:1755`).
  - `InputLayers` (`Control::Fighting`, `Control::Activate`) folds into the same fields (`input_layers.rs:147`), so the FO4/Starfield layer path is equally unread. `docs/engine/scripting.md:1352` lists only VATS/favorites/running/jumping/sprinting/ZKey/FastTravel as "nothing reads them yet", so this is not a declared gap.
- **Impact**: during every scripted sequence that locks the controls, the player can still swing, shoot, cast, block and press activate. The sequence can break (an NPC killed or turned hostile mid-scene, a quest object activated out of order), and `GetPlayerControlsDisabled` answers 1 while the player demonstrably can fight, which tutorial scripts poll.
- **Related**: M65 wired `sneaking_enabled` the same way (PRD sneak-and-detection row 2, "read only by GetPlayerControlsDisabled" then); #6469 / #6350 wired `Control::Vats`; routed to /audit-ui: `menu_enabled`.
- **Suggested Fix**: add one `player_may_fight(world)` helper (PlayerControlState.fighting_enabled and the `Control::Fighting` layer) beside `player_can_act` and apply it to `combat_input_system` (swing, ready-weapon, `blocking`), `player_bow_system`, `player_cast_system`, `iron_sights_system`; gate `interaction_system`'s activate edge on `activation_enabled`. Add one guard test per consumer.
- **Dimension agent ID**: GP-D4-01

### GP-02: A scripted door's `::isOpen_var` is still parked and restored while its `TwoStateActivator` is not (#7305 left the half)
- **Severity**: MEDIUM
- **Dimension**: 2 (Interaction, doors, reference_state persistence)
- **Location**: `byroredux/src/cell_loader/reference_state.rs:523-543` (`capture_script_states`), restore at `:560-587` (`restore_script_state`); stamp at `crates/scripting/src/translate/recognizers/two_state_activator.rs:42-48`
- **Status**: NEW (sibling of #7305 / 7a7cc9aa7, closed)
- **Trigger**: a placed DOOR whose model authors `Open`/`Close` sequences (so it gets a `Door`) and whose base or REFR carries a Papyrus script recognised as `default2StateActivator` (Skyrim+ script on a door with NiControllerSequences). The player opens the door, leaves the cell (eviction), returns in the same session. Not reachable through a save-load (the `TwoStateActivator` and `ScriptVariables` snapshot columns are both saved and re-applied together).
- **Description**: #7305 made `capture_script_states` skip `TwoStateActivator` for an entity carrying `Door`, so the door returns to its `Open by Default` state on re-entry (DRT-STATE-003). The same function still parks `ScriptVariables` unconditionally. The recognizer's variable set is `{::isOpen_var, ::isAnimating_var, ::doOnce_var}`, so a scripted door always has `variables: Some(..)`, its row survives `states.retain(!is_empty)`, and `restore_script_state` writes the old `::isOpen_var` / `::isAnimating_var` back over the fresh values the recognizer just stamped from the default state. After re-entry the door is at its default pose and `TwoStateActivator.is_open` is the default, while `::isOpen_var` still holds what it was when the player left.
- **Evidence**:
  ```rust
  // reference_state.rs (#7305): only the two-state half is skipped for a Door
  for (((entity, _, _), (_, state)), door) in rows.iter().zip(&mut states).zip(&doors) {
      if !door { state.two_state = two_state_q.get(*entity).copied(); }
  }
  // the next block, unconditional:
  state.variables = variables_q.get(*entity).cloned();
  // two_state_activator.rs:45-47, the variables that mirror the state
  variables.set_by_name("::isOpen_var", f32::from(is_open));
  variables.set_by_name("::isAnimating_var", ...); variables.set_by_name("::doOnce_var", ...);
  // restore_script_state, over the fresh stamp
  if let Some(variables) = state.variables { world.insert(entity, variables); }
  ```
  `::isOpen_var` is the CTDA-visible mirror (`vm_state.rs` `IS_OPEN_VARIABLE`, rewritten on every toggle and on `SetOpen`; read by `GetVMScriptVariable` conditions, `condition/tests.rs:602`). The guard `a_doors_open_state_is_not_parked_but_a_levers_is` puts no `ScriptVariables` on its door, so it cannot see this.
- **Impact**: any condition (quest, package, scene, dialogue) that reads `::isOpen_var` of such a door sees the stale open/closed answer until the next toggle, while the door itself is at default. Narrow reach today (Skyrim+ doors that author sequences, with the recognised script), hence MEDIUM not HIGH.
- **Related**: #7305, #7306, #6071.
- **Suggested Fix**: for a `Door` entity, also reset the three two-state-mirror names in the parked `ScriptVariables` (or skip `variables` when those are its only entries), or re-derive the mirror from the surviving `TwoStateActivator` after `restore_script_state`. Extend the guard with a `ScriptVariables` row on the door.

---
- **Dimension agent ID**: GP-D2-01

### GP-03: A Near Editor Location Sandbox now snaps the actor into a chair near its editor placement, from anywhere in the cell
- **Severity**: MEDIUM (the same class, #5726, was filed LOW; keep that calibration if preferred)
- **Dimension**: 5 (NPC spawn, AI package selection, locomotion)
- **Location**: `byroredux/src/systems/sandbox.rs:352-365` (the new `centre`), `:405` (assignment), `:202-219` (`apply_seat_assignments` snap); introduced by #7262 5ede7b032
- **Status**: NEW (regression-class sibling of closed #5726, which bounded the same seat-snap teleport for Eat/Sleep In-Cell; #7262 closed without this check)
- **Domain**: ai, gameplay · **Game**: FNV, FO3 (424 + 72 type-3 Sandbox packages, per the #7262 commit)
- **Trigger**: FNV or FO3, any cell with sit markers. A sandboxing NPC whose Sandbox package has PLDT type 3 (Near Editor Location) and whose position is far from its `EditorPlacement` when its seat pass first runs. The common displacements:
  - a fight or flee that ends, after which `suspend_ambient_behavior_for_combat` resets the winner and the first tick re-installs the Sandbox;
  - a handover from Travel, Follow, Wander or Flee to the Sandbox package.
  Any save state. An actor still standing at its authored spot is unaffected.
- **Description**: #7262 centres the seat search on `travel::package_home(.., near_editor_location, npc_g.translation)`, i.e. on the `EditorPlacement`. The pass then assigns the picked seat with no arrival test, and `apply_seat_assignments` writes the placement root's `Transform` straight to the seat. Sandbox has no walk leg. So an actor N units from its editor placement is teleported to a chair within `radius` of that placement, a jump of up to N + radius, instead of the old jump of at most `radius` (512 default) from where it stood. Eat/Sleep, which share the same snap, guard this exactly: "a picked marker beyond `ARRIVE_RADIUS` becomes the next KCC walk goal instead of a snap" (#5726). Travel, Wander and Guard, the other #7225/#7254 consumers, all walk to the placement. Sandbox is the only one that teleports.
- **Evidence**:
  ```rust
  // sandbox.rs:358-365
  let centre = super::travel::package_home(world, npc, behavior.near_editor_location, npc_g.translation);
  if let Some((seat_id, seat)) = pick_nearest_seat(centre, &scratch.seats, &reservations.0, radius) {
  // ... :405  scratch.assignments.push((npc, furn_e, seat));  -> :417 apply_seat_assignments
  // sandbox.rs:215  t.translation = seat.translation;   // unconditional snap
  ```
  The #7262 test `a_near_editor_location_sandbox_seats_near_the_editor_placement` puts the actor 4000 units from the chair that gets claimed and asserts only the reservation. It never asserts that the actor walks or that the jump is bounded.
- **Impact**: A visible NPC pop across the cell (up to the cell's extent), in front of the player, for every displaced type-3 sandboxer. It is also a mismatch with the GECK's own rule, since Sandbox's Wander Location is where the actor goes and then idles.
- **Related**: #7262, #5726, #7225, #7254, AIP-SIT-001/-002.
- **Suggested Fix**: Seat at the picked marker only when the actor is within the arrival radius of it (as `seat_at_marker` does). Otherwise leave the actor to walk toward the `EditorPlacement` with the same `cached_walk_step` plumbing Wander and Guard use, then seat. Add a test with the actor displaced beyond `ARRIVE_RADIUS` that asserts its `Transform` is not moved to the far chair.
- **Dimension agent ID**: GP-D5-01

### GP-04: The spawn-time package scan cannot see a reference-keyed Once Per Day block, so a stream respawn can re-run the package inside its 24 hours (#7317 half-applied)
- **Severity**: LOW
- **Dimension**: 5
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:1223-1236` (spawn scan and `note_selected`), `byroredux/src/npc_spawn/package_flags.rs:56-62` (`block_owner`), `:143-160`; `byroredux/src/cell_loader/references/mod.rs:873` (the stamp happens after the job); `byroredux/src/cell_loader/stream_snapshot.rs:129` / `:324` (`has_package_state`)
- **Status**: NEW (follow-up to closed #7317 c99ccdb23)
- **Domain**: ai, gameplay · **Game**: FNV, FO3, Oblivion
- **Trigger**: An actor with a Once Per Day package P (PKDT bit 10) and no always-open fallback package. P is selected at 08:10 on day 0. The cell unloads at 13:00 with nothing running. The player returns at 08:05 on day 1, when P's window is open again but its block (until 08:10) is live.
- **Description**: #7317 re-keys the block by the actor's placed reference (`FormIdComponent`). But `finish_actor` calls `apply_ai_package_behavior` inside the spawn job, and the job's placement root has no `FormIdComponent` yet. It is stamped later by `stamp_quest_reference` (mod.rs:873, then `adopt_entity_day_blocks`). So the spawn scan (`select_active_package(world, placement_root, .., None, ..)`) computes `block_owner` = `Entity(new id)` and never sees the `Reference` block. P wins, its behavior is installed, and `active_package_form_id = P`. On the first tick P is the running package (`current == Some(P)`), which `once_per_day_blocked` exempts (`current != Some(package.form_id)`). The block is bypassed for the whole window. `adopt_entity_day_blocks` only merges timestamps afterwards ("earlier start wins"); it does not undo the install. The #7317 test `a_block_noted_before_the_reference_stamp_moves_onto_it_keeping_the_earlier_start` even notes "a respawn whose spawn job re-selected it".
  The stream snapshot rescues the common case. When the actor was running some other package at eviction, `restore_actor_snapshot` calls `install_package_now(fallback)` and replaces P. The snapshot row exists only when `has_package_state()` (an active package, travel state or a seat). So the bypass survives exactly for actors with nothing running at eviction.
- **Evidence**:
  ```rust
  // ai_package.rs:1223  let active = select_active_package(world, placement_root, game_hour, None, false, ..);
  // package_flags.rs:56 fn block_owner: world.get::<FormIdComponent>(actor).map(Reference).unwrap_or(Entity(actor))
  // package_flags.rs:158-160  once_per_day_blocked: .. && current != Some(package.form_id) && ..
  ```
- **Impact**: A Once Per Day package re-runs after a stream round trip inside its 24-hour block, for actors with no fallback. This is the AIP-FLG-001 defect #7317 set out to close, remaining for one entry path.
- **Related**: #7317, #7252, AIP-FLG-001.
- **Suggested Fix**: Give the spawn scan the placed reference. Either stamp `FormIdComponent` before `apply_ai_package_behavior` runs, or pass the reference into `select_active_package` as the owner. Alternatively, when `adopt_entity_day_blocks` finds a pre-existing live `Reference` block for the spawn winner, re-select once after the stamp.
- **Dimension agent ID**: GP-D5-02

### GP-05: Once Per Day blocks the reload's spawns note carry the OUTGOING session's clock, so loading an earlier save suppresses the package for days (#7252 half-applied)
- **Severity**: LOW
- **Dimension**: 7 (gameplay-state coverage, load drain)
- **Location**: `byroredux/src/save_io/live_load.rs:628-641` (`reload_with_fresh_day_blocks`), `:836` (post-reload `restore_resources`), `:43-54` (`PRE_RELOAD_RESOURCES` has no `GameTimeRes`); `byroredux/src/npc_spawn/package_flags.rs:139-143` (`now_hours`), `:190-205` (`note_selected`)
- **Status**: NEW (follow-up to closed #7252 9f31ab66f)
- **Domain**: ai, save-load · **Game**: FNV, FO3, Oblivion (and Skyrim+, since the flag is a PKDT bit on every game)
- **Trigger**: An in-process load (quickload or the load menu) of a save whose clock is EARLIER than the live session's, in a cell holding actors that spawn on a Once Per Day package, with that package's window open at the outgoing hour.
- **Description**: #7252 moved the block clear to before the reload so the reload's own spawns keep their blocks. The reload runs `apply_ai_package_behavior` under the OUTGOING `GameTimeRes`: `ensure_game_time` inserts only when absent, and `GameTimeRes` is restored by the later wholesale `restore_resources`, not pre-reload. So `note_selected` stamps `first = T_out` (outgoing day*24+hour). After the restore the clock is `T_saved`. `reseat_ambient_packages_after_restore` re-notes only winners that changed, and `note_selected` uses `or_insert`, which does not restart a block whose `now - first` is negative. With `T_saved < T_out` the check `now - first < 24` stays true for `(T_out - T_saved) + 24` game hours. Once the package's window closes it cannot win again until the clock reaches `T_out + 24`. The same `first` is wrong in the other direction after loading a later save, but harmlessly (the block just lapses early). The cold-start path has the same wrong-clock stamp, but its boot clock is near zero so the block is already expired; only the in-process path produces a future-dated block.
- **Evidence**:
  ```rust
  // live_load.rs:774-791  without_parked_state(.. reload_with_fresh_day_blocks(.., reload_*_session))   // clock = outgoing
  // live_load.rs:836      byroredux_save::restore_resources(..)                                         // GameTimeRes -> saved clock, after
  // package_flags.rs:204  let first = blocks.0.entry((owner, package)).or_insert(now);  if now - *first >= 24.0 { *first = now }
  ```
- **Impact**: A player who plays 10 days and quickloads a day-4 save finds every Once Per Day actor selected at spawn suppressed for about 8 in-game days after its first window ends. The visible effect is an NPC skipping a daily visit.
- **Related**: #7252, #7317, #7230, AIP-FLG-001.
- **Suggested Fix**: Rebase after the restore: when `GameTimeRes` is restored, set every surviving block's `first` to the restored `now_hours` (a block restarts at load, as the NOT_SAVED reason states). Or re-note the unchanged winners in `reseat_ambient_packages_after_restore`. Add a test that reloads with the live clock ahead of the saved one.

---
- **Dimension agent ID**: GP-D7-01

### GP-06: Doors in the exterior streaming ring park their saved open state in `scripts`, where only a script attach ever consumes it
- **Severity**: LOW
- **Dimension**: 2 (reference_state, save-load park)
- **Location**: `byroredux/src/cell_loader/reference_state.rs:1135-1148` (`park_unresolved_snapshot_rows`, #5054); consumers `:1034` (`restore_resident`) and `references/attach.rs:220` (only inside `if attached`)
- **Status**: NEW
- **Trigger**: an exterior save (Oblivion/FO3/FNV worldspace) with an unscripted sequence door in the hysteresis ring (`radius_load + 1`, not respawned by the reload) left open; load; walk to it.
- **Description**: #7305 reasoned that an unscripted door never consumes a parked `TwoStateActivator` because `restore_script_state` runs only after a script attaches. The save-reload park path builds the same kind of row from the snapshot (`two_state: two_states.remove(&old)`) for every unresolved reference with no `Door` test (the entity is not live, so there is no `Door` to read). For an unscripted door that row is never consumed: the door streams in at `Open by Default` instead of its saved state (a resident door gets the saved state through `apply_deltas`), and the row is never removed, so it is re-saved forever (the leak #7305 fixed for eviction).
- **Evidence**: `restore_script_state` is called only from `reference_state.rs:1034` (`restore_resident`, resident entities at load time) and `attach.rs:220` inside `if attached { .. }`. `parked_scripts` keeps any non-empty `ScriptState` whether or not the reference will ever attach a script.
- **Impact**: a saved open (or closed, for an open-by-default door) unscripted door state in the ring is lost on load, and one dead row per such door sits in `PersistentReferenceStates.scripts` and in every later save.
- **Related**: #5054, #7305.
- **Suggested Fix**: consume the parked `two_state` where `attach_door` stamps the `Door` (restore after `attach_door`), or park it in `rows` with a consumer on the door spawn path.

---
- **Dimension agent ID**: GP-D2-02

### GP-07: Dropping an improved (instanced) item silently destroys its quality
- **Severity**: LOW
- **Dimension**: 1 (Inventory & equipment, ItemInstancePool)
- **Location**: `byroredux/src/inventory/actions.rs:296-306` (`drop_item`), `:311` (`create_scripted`)
- **Status**: NEW
- **Trigger**: Skyrim; `crafting::improve_item` at a grindstone/armor table (the improved copy gets `ItemInstance { quality }`), then `inv.drop` or the menu's Drop on that row, then pick it back up.
- **Description**: `drop_item` allows an instanced row (it refuses only an *unresolvable* instance), decrements the count, and at 0 does `row.instance.take()` plus `ItemInstancePool::release`. The created reference it spawns (`create_scripted(reference, form_id, PLAYER_REFERENCE, false)`) carries only the base form id; there is no instance field on a created reference, and `take_placed_item` adds `ItemStack::new(base, count)`. The tempered armor or weapon comes back plain. Barter documents the same rule ("per-instance items are traded as their base", `barter.rs:41`), but nothing documents or tests it for drop.
- **Evidence**:
  ```rust
  let released = { let row = &mut inventory.items[index as usize]; row.count -= 1;
      if row.count == 0 { row.instance.take() } else { None } };
  if let Some(instance) = released { world.resource_mut::<ItemInstancePool>().release(instance); }
  ```
- **Impact**: silent loss of a crafted upgrade on drop-and-repickup. Low: Skyrim only, player-initiated.
- **Related**: #6544 (removal order for improved copies), M43 crafting.
- **Suggested Fix**: refuse the drop for an instanced row until a created reference can carry an instance, or document "dropped items revert to base" beside the barter note and pin it with a test.

---
- **Dimension agent ID**: GP-D1-01

### GP-08: Sibling of #7231, a scripted or console add of count 0 still adds a zero row and emits a zero-count `ItemTransfer`
- **Severity**: LOW
- **Dimension**: 1 (Inventory::add paths, item events)
- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:299-335` (`add_items`), `byroredux/src/inventory/mod.rs:84-102` (`add_item`, console)
- **Status**: NEW
- **Trigger**: a Papyrus fragment `AddItem` whose `ItemCount` resolves to 0 (`resolve_item_count` accepts `0.0..=u32::MAX`, e.g. a `Global` or `Value` count that is 0), or the console `inv.add <form> 0`.
- **Description**: #7231 (2e535ba49) established that a zero-count transfer is phantom: it runs the item's `OnAdd`/`OnDrop`, and gives the receiver a zero-count row for a form it does not hold. `add_items` does the same on the add side: `Inventory::add(ItemStack::new(item, 0))` appends a (form, 0) row when the container has none, then `emit_item_transfers` publishes `ItemTransfer { count: 0, added: true }` unconditionally (it filters `count > 0` only for the challenge counter, not for the `ItemEventBatch`). `obscript_item_system` then runs `BLOCK_ON_ADD`. The ObScript caller clamps with `.max(1.0)` (`obscript_more/actions.rs:170`), so only the fragment and console doors are open.
- **Evidence**: `add_items` has no `count == 0` early return; `remove_items` does (`if removed == 0 { return; }`), so the two directions are asymmetric.
- **Impact**: a spurious item `OnAdd` (re-running scripted-item side effects) and a zero row when a script computes a 0 count. Low reach.
- **Related**: #7231.
- **Suggested Fix**: `if count == 0 { return; }` at the head of `add_items` (after the leveled-list expansion, which recurses per entry) and in `add_item`.

---
- **Dimension agent ID**: GP-D1-02

### GP-09: `stance_use_system`'s `Access` row was not updated by #7281 and no acquisition guard covers the skill-use exclusives
- **Severity**: LOW
- **Dimension**: 4 (Oblivion skill-use seam) / 7 (stage order and access)
- **Location**: `byroredux/src/boot/schedule/late/scripted_combat.rs:38-52` (row), `byroredux/src/skill_use_emit.rs:195-203` (body), `byroredux/src/systems/character.rs:164-197` (`player_accepts_movement_input`, `player_movement_input_drives_body`); guard list `byroredux/src/boot/schedule/mod.rs:1226-1260`
- **Status**: NEW (sibling of closed #7319, whose fix named a fixed list of exclusives and did not drive the guard from every `add_exclusive_with_access` row)
- **Trigger**: Oblivion (the only game with skill-use leveling), any cell; static declaration only, no runtime effect today.
- **Description**: 6e6fdb58b (#7281) made `stance_use_system` call `player_movement_input_drives_body`, which acquires resource `PlayerMode`, component `Dead`, resource `PlayerControlState`, component `ActorControlState` and resource `VatsState` (through `player_accepts_movement_input`). The row declares none of them (it lists StanceTimers, PendingSkillUse, CharacterRuleset, PlayerEntity, ActionState, PlayerDetection, Sneaking, CharacterController, WaterContact). The 8 acquisition guards in `system_access_declaration_tests` (`detection_tick`, `sneak_stance`, `player_cast`, `player_bow`, `player_seat`, `combat_input`, ...) do not include `stance_use_system`, `skill_use_system` or `oblivion_level_system`, so the test stayed green (verified: `system_access_declaration` filter passes).
- **Evidence**: row at `scripted_combat.rs:41-52` ends in `.reads::<byroredux_physics::CharacterController>().reads::<...WaterContact>()`; `rg stance_use_system byroredux/src/boot/schedule/mod.rs` has no hit; `skill_use_emit.rs`: `let drives_body = crate::systems::player_movement_input_drives_body(world, player);`.
- **Impact**: exclusives are not paired by the analyzer, so nothing misbehaves today; but `sys.accesses` reports a hold stack that is short by five types, and promoting the system to a parallel lane would be unsound. The same drift will recur for any later edit to the three skill-use exclusives.
- **Related**: #7319, #6546, #7281.
- **Suggested Fix**: add the five types to the row and add `stance_use_system` / `skill_use_system` / `oblivion_level_system` to the acquisition guard (ideally derive the guard from every exclusive row, as #7319 suggested).
- **Dimension agent ID**: GP-D4-02

## Per-dimension notes

### Verified invariants (checked, found intact)

Dim 1
- `Inventory::add` folds a plain stack into the form's existing plain row (reusing a zeroed one) and appends only instanced stacks. Every production `items.push` outside tests is spawn/authored/instanced (`npc_spawn/equip.rs:354`, `attach.rs:302`, `crafting.rs:404`); no hand-spelled plain-row append path found.
- `count_of` / `count_of_any` saturate and sum every row. No hand-spelled `items.iter().filter(base_form_id).map(count).sum()` in production code (remaining `.map(|s| s.count).sum()` hits are test assertions).
- `first_row_of` is the single first-non-empty-row lookup: `queue_by_form_id` serves `inv.equip` / `inv.use` / `inv.drop`, and `drain_script_weapon_equips` uses it too. #7232 (c834ddb5e) is complete: no `position(|stack| stack.base_form_id == ..)` is left in `inventory/`. The smoke-gate greps (`queued equip toggle`, `queued use`, `queued drop`) still match the new reply text.
- #7231 take-all: `outcome.rows` and the player hand-over skip zero-count rows and keep a zero row that still holds an instance (pool entry stays owned). `source_form_ids` is computed from the unfiltered stacks, so the equipment-index to form mapping stays positionally correct. The `item_count == 0` early return happens after the source was drained, but only for sources with no live rows; no equipped row can be zero (consume/drop refuse an equipped row, `remove_items` releases the slot).
- Rows zeroed in place: `consume_item`, `drop_item`, `LootSelection::Stack`, `remove_items` all keep the row and release the instance at count 0.
- Armor toggles never re-derive `EquippedWeapon` (`weapon_changed = equip_target == Weapon`); `toggle_equipped_ammo` touches neither `EquipmentSlots` nor `EquippedWeapon`; `EquipTarget::Ammo` returns before the equipment query.
- #7228: `EquippedAmmo` is registered with `register_replacing_component` and is still in `MUTABLE_DELTA_COLUMNS`. An empty replacing column is serialized as `[]` (`component_keeps_empty`), so a save with no arrow row does clear the live player's choice (a wholly missing column would not). Only the player ever carries `EquippedAmmo` (no NPC spawn or park site), so the replacing policy cannot strip an NPC's choice.
- 64-slot masks: the only `1u32 << bit` sites left are the two deliberate ones (`npc_spawn/equip.rs:425,520`), both under `.take(32)` for the NIF skin-partition mask; no `u32` mask on an equip path.

Dim 2
- #7226 door mid-swing: the new `idle` pass runs after `starts` consumption and the `remaining > 0` branch (`continue`), so a freshly started clip is never cleared the same tick; a zero-duration clip sets `is_animating = duration > 0.0`; the `query_2_mut` and the later `query_mut` scopes are sequential, not nested (door lock-order guard unaffected).
- #7305 capture: the `Door` read guard is dropped (end of the `match`) before `TwoStateActivator` is read; `doors` is indexed 1:1 with `rows`/`states`; a lever still parks (`a_doors_open_state_is_not_parked_but_a_levers_is`); `is_empty()` drops a row whose only content was the skipped door state.
- #7306 recognizer: an authored `isOpen` wins, then the `attach_door` seed, then the script default; `::isOpen_var` is written from the resolved value.
- #7317 Once Per Day: `adopt_entity_day_blocks` runs from `stamp_quest_reference` (the single stamper; the persistent-worldspace stub loop goes through `spawn_logical_quest_reference_with_scripts`), takes the earlier start, and holds no `FormIdComponent` guard while taking the resource. The spawn job on a respawn still selects with an entity key (it runs before the stamp), but `restore_actor_snapshot` re-installs the snapshot's active package right after, so the block-bypass window does not surface. A load clears the whole map (`reload_with_fresh_day_blocks`, #7252).
- #7308 `FollowUpTopics`: keyed by `Weak<EsmIndex>` + `ptr_eq` (a live Weak pins the allocation, so a reused address cannot alias); the missing-resource path warns once.
- #7307 pin scans production text (`source_scan::production_text`), not the whole file.
- `unlock_with_carried_key`: the refusals precede the key spend, `commit == false` is side-effect free, the outcome is recorded in `ReferenceLockState`, and the unlock aborts when that resource is absent.
- `transfer_loot` validates source, lock and both inventories before mutating (`inventories.get_mut(player)?` precedes `mem::take`); source equipment is cleared with one unequip event per index; `container_loot_system` skips script-blocked and non-player activations.
- `capture`, `restore` and `park_unresolved_snapshot_rows` are symmetric over every `ReferenceState` field (inventory, equipment, weapon, AVs, spells, perks, control, bleedout, dismember, script_package, dead + corpse_pose, picked_up, magic_effects, weapon_drawn, ignore_crime); `restore` removes `EquippedWeapon` when the row has none; instance payloads are resolved after the `Inventory` guard drops.
- Door lock order: `populate_candidates` snapshots `TwoStateActivator` before reading `Door` (`interaction.rs` ~1550-1573); `door_motion_system` takes the pair via `query_2_mut`.

### Not filed (considered and dropped)
- `set_two_state_open` clears `is_animating` while a clip is still running (`remaining > 0`), which would let an activation through mid-swing: the mid-swing behaviour for a script `SetOpen` is unsourced (#7062), so it is not a defect on documented sources.
- `transfer_loot` `item_count == 0` early return after `mem::take` when every row is a zero row still holding an instance: unreachable with a live equipment index (see invariants), cosmetic otherwise.

### Observations (not filed)

- **VATS producer vs the #7302 comments.** The #7302 comments (`combat.rs` ~310, `combat_ai.rs` ~265) and commit text say the flag is resolved first "as the VATS producer does". `vats_confirm` (`systems/vats.rs:970-981`) does not resolve the weapon flag at all; it hard-codes `ignores_resistance: false`. No behavioural effect: `ArmorRatingModel` exists only on the Oblivion profile (`profile.rs:423`), VATS only on FO3/FNV (`:459`, `:506`), so the two never coexist. Becomes a defect only if a profile ever carries both; the lock-order claim (flag resolved outside the guard) is true for the two melee producers.
- **#7297 sibling, unreachable today.** `route_script_messages` refuses a full queue (16) for a `MessageAnswer::ActivateChoice` box without clearing `PendingActivateChoice.held` (`perk_activate.rs:86-100`), so that activation would be lost silently. A waiting box opens the modal page and releases world input (`app_frame.rs:1170-1190`), so the player cannot press Activate while 16 boxes wait; only a script raising 16+ boxes inside one frame reaches it.
- **Debug-drain `HitEvent` overwrite.** `combat_input_system` inserts its `HitEvent` unconditionally (`combat.rs:317`), while the two other direct producers hold when a row exists. A `vats.confirm` hit raised in the Late debug drain survives one frame, so a same-frame player melee on the same target would replace it. Console-only; VATS has no UI confirm yet.
- `fall_landing` queues Acrobatics (`on_fall`) even when the landing killed the player. Matches the UESP rule quoted at `character.rs:~1712` ("each time you take damage"); not a defect.

### Verified invariants

- **Death producers.** Every production `Dead` insert reaches `reconcile_dead_actor`: `combat_damage_system` (`combat.rs:758` -> `reconcile_new_death`), `scripted_kill_system` (`:1785` -> `queue_dead_actor_reconciliation`), NPC drowning (`water.rs:78-80`), player drowning and fatal fall (`character.rs:1744-1746`), SDK batch (`extensions/commands.rs:565-567`), `reference_state` restore and `apply_starts_dead` (`reference_state.rs:681/697`, `760/764`, held until cell load). All other `Dead` inserts under `byroredux/src` and `crates/scripting/src` are `#[cfg(test)]`.
- **`spare_from_death` and Story KILL.** `combat_damage_system` (`combat.rs:728`), `scripted_kill_system` (`:1780`), `water.rs:76`, `commit_actor_value_deaths` (`commands.rs:561`) ask `spare_from_death` before marking `Dead`; the player is exempt by design (`bleedout.rs:198`). KILL is raised by combat (`:771`), water (`:82`), SDK batch (`:569`), player drowning/fall (`:1748`) and non-silent script kills (`:1793`); `KillSilent` raises only `OnDeath`. `spare_from_death` is idempotent for an actor already bleeding out.
- **HitEvent producers set `ignores_resistance`.** `combat_input_system` and `npc_combat_ai` (both resolved before the events guard, #7302), `TrapHit`-flushed hits (`projectile_flight` from the `Projectile`/`ArrowSpec` flag, script `UseWeapon` from `item_ignores_resistance`, #7234), mines and VATS (constant `false`, no actor aggressor / no resistance model). The other `HitEvent {` literals are in tests or SDK `HitEvent`.
- **Producer order.** `hit_event_consumers_run_after_every_producer` and `console_raised_hits_are_queued_after_the_frame_cleanup` pass against the built schedule.
- **#7227.** `place_character_body` resets `FallTracker` (`character.rs:1039`); its callers are the save restore (`save_io/player.rs:246`), door arrival grounding (`:1153`), the camera snap (`:989`) and `commands/view.rs:457`; the controller early-outs on `Dead` (`:246`), so a dead player is not charged or re-killed.
- **#7281 / #7294.** `player_movement_input_drives_body` mirrors the controller's gates (Character mode, not `Dead`, `player_accepts_movement_input`); Sneak needs `PlayerDetection.detector.is_some()`, and `fold` keeps the worst counted detector so `level < Noticed` means nobody noticed. The `NoSprint` over-encumbrance rule is Skyrim-only, so the sprint key not being gated on `sprint_allowed` cannot reach Oblivion skill-use.
- **#7228.** `EquippedAmmo` is registered with `register_replacing_component`; `set_equipped_ammo` writes `Some/None`.
- **#7310 / #7331.** `raise_skill_ranks` has no surplus path left; callers updated (`obscript_more/state.rs`).
- **#7297.** The refused Papyrus ticket is answered with 0 after the queue guard drops (`notifications.rs:45-75`), and the ticket is in `waiting` by `reserve_papyrus_box`, so `record_papyrus_answer` takes effect.
- **#7315.** `live_actor_bodies` drops the `ActorVitals` guard before reading `RapierHandles`; no sibling `ActorVitals` iterator in `byroredux/src` holds it across a `RapierHandles` read.
- **Teardown.** `reconcile_dead_actor` still clears ambient behavior, `AmbientPackageRuntime`, `EvaluatePackageRequest`, `AiCombatState`, `HeadLook`, the graph/locomotion/console-walk runtimes, animation players, ragdoll, voice line; `AmbientEngagement` self-cleans in `disengage_lost_contact` (entity with no `AiCombatState` is dropped on the next pass). New per-actor components in the delta (`TreeLeafRun`, `ReleasePackageHold`) are Dim 5; neither affects a corpse (ambient pass skips `Dead`).
- **Dim 6.** `notifications::drain` runs every frame regardless of `debug_ui` (`app_frame.rs:401-403`); `present_script_messages` dismisses boxes with no native UI (`:1183-1186`); `MessageBoxQueue` and `PapyrusMessageAnswers` are in the load drain (`live_load.rs:529`).

### Verified invariants (re-checked at 04bf89c44)

- **Three-way arm diff.** All 14 `AmbientBehavior` arms (Sandbox, Wander, Travel, Follow, Escort, Guard, Patrol, Eat, Sleep, Flee, Ambush, Find, UseWeapon, Dialogue) are present in `from_package`, `install` (one match, `SpawnSink` / `RuntimeSink`) and `clear_ambient_behavior`'s `AMBIENT_COMPONENT_REMOVERS`. Ambush installs `GuardBehavior` + `AmbushBehavior`, and both are removed. `CrossSpaceHop`, `FindProgress`, `UseWeaponPackageStarted`, `EatSleepState`, `ForceGreetDirective`, `WalkStuckTimer`, `NavPath` and `Seated` are removed, and `UseWeaponRun` only when the package started it.
- **Debug-server registration** carries every `*Behavior`, `*State`, `Traveled`, `Escorted`, `FindProgress`, `UseWeaponPackageStarted`, `Seated`, `EatSleepState` and `EditorPlacement`. The roster test enforces `*Behavior`. `ForceGreetDirective` stays unregistered, as in earlier reports. The `near_editor_location` fields are absent from the descriptors' `field_names` lists but remain visible in the JSON (cosmetic).
- **`TreeLeafRun`** (#7233) is `NOT_SAVED`, registered at boot and re-adopted by `rerun_tree_leaf` on the first pass. `record_tree_leaf` is set or cleared on every `changed` handover, including `rearm_ai` after a revive. A stale component on a corpse is inert because the ambient pass filters `Dead`.
- **`ReleasePackageHold` / `AmbientPackageRequestsSeen`.** `request_package_evaluation` raises the hold and the request, and `queue_package_evaluation` raises the request alone. The ambient pass takes the hold before any early return and records `seen` before the early returns. `scene_package_system` removes only the seen requests. Both systems are unconditional Update exclusives in that order (`package_fragments.rs`, then `scene_actions.rs`). `event_cleanup_system` leaves both markers alone (`survives("ReleasePackageHold")`). A stale `seen` after a load is harmless: entity ids are never recycled, and `reseat_ambient_packages_after_restore` runs the ambient pass, which rewrites it.
- **Near Editor Location** (#7225/#7254/#7262). Wander, Travel, Guard and Ambush walk to the `EditorPlacement`. `package_home` is a transient `world.get` (no held `EditorPlacement` guard, so no reverse lock edge). Find reaches the placement through `resolve_anchor`. Patrol is declined with its GECK citation. `ace99c164`'s `CreatureActor`-first order holds in the seat pass. The #7262 call to `package_home` inside the seat pass adds no cycle.
- **Block and load wiring.** `adopt_entity_day_blocks` is called from `stamp_quest_reference`, and the earlier-start `min` is safe (`note_selected` purges expired entries first). Relocated (XCT) actors spawn through `load_references`, so they stamp their reference identity and the block follows them across cells. `reload_with_fresh_day_blocks` restores the live blocks on an aborted reload. The NOT_SAVED reason shape check passes (`RESET_OUTSIDE_THE_DRAIN` lists `PackageDayBlocks`, `FallTracker` and the others). The #7229/#7230/#7253 resets (`ActiveProjectiles`, `PendingTrapHits`, `LevelNotices`, `CastState`, `BowState`, `StanceTimers`, `MineClock`, `RandomConversationState`, `DeadBodyState`, `AttractionState`, `JailEscapeWatch`, `ObScriptQuestTimers`) are all in the drain body. The reset resources hold only cadence or diagnostic state.
- **Save wiring.** `SessionProfile` is a codec column whose load keeps the live record. `FORMAT_MAJOR` is 143 and `saved_type_shape_changes_require_format_major_bump` is green. `near_editor_location` on the Behavior components is NOT_SAVED, so the shape fingerprint was refreshed without a bump, as the commits state.
- **Stage order** (built-schedule guards exist and pass): detection, restoration, bleedout, encumbrance, interaction, activation flush, `door_activation_system`, `container_loot_system`, `bed_use_system`, `jail_bed_system`, `player_seat_system`, `evidence_deposit_system`, the combat segment, the `quest_scene` file order (story manager, aliases, scene playback, ambient with the package fragments, forcegreet, ObScript quests, `scene_package_system` → `door_motion_system` → `two_state_activator_system`, ObScript objects, dialogue speech). `activation_flush_is_scheduled_before_every_activate_event_consumer` now lists the four #7321 consumers. The story-event producers sit after `story_manager_dispatch_system`, so their events reach it one frame later, by the #5366 design (the event lives until the dispatch drains it).
- **Determinism.** `grep -rnE 'rand::|Instant::now|SystemTime|thread_rng'` over combat, inventory, interaction, `npc_spawn/`, the dialogue, forcegreet, mines, attraction, dead_body and random_conversation systems, and all Dim 5 systems, is empty. The only `Instant` hits are the spawn-budget timers in `npc_spawn/resumable/mod.rs`. `adopt_entity_day_blocks` and `AmbientPackageRequestsSeen` iterate hash containers, but only order-independent (set, min) work comes of it.
- **Written-never-read.** Every new field and resource has a production reader (`near_editor_location` on Sandbox, Wander, Travel and Guard, `TreeLeafRun`, `ReleasePackageHold`, `AmbientPackageRequestsSeen`).

### Notes (not filed; premise unverified or a design call)

- **#7233's leaf swap covers only leaves with a Done state.** `tree_leaf_done` is `false` for Sandbox, Patrol, Sleep, Eat, Wander, Guard, Follow and Escort, so a leaf-condition flip never replaces an installed open-ended leaf (the original GP-10 scenario). The commit cites the CK ("a swap mid-run is not described") and chooses to make none. The CK text is the only source, so this is not filed. If the owner wants GP-10 fully closed, that is a decision, not a code slip.
- **Flee's `PLDT` "Flee To" location.** The GECK page says the Flee To location is Standard Location data. The engine reads only a `NearReference` refuge (`location_reference`), so types 2, 3 and 6 are silently ignored. There is no census of such packages in the PRD, so it is unfiled pending one.
- **`AmbientPackageRequestsSeen`** holds `EntityId`s and has no drain line, which the Dim 7 "new transient holding entity ids" rule asks for. There is no consequence: ids are never recycled and the reseat pass rewrites it before any scene pass. Rule-literal only.
- **Late `EvaluatePackage` for an actor already in `seen`** (a second request raised between the ambient pass and the scene pass, for an actor whose first request the ambient pass did read) is drained with the first, so its `ReleasePackageHold` is taken at the next pass without a request. This is a one-in-many edge and has no filed premise.

## Known-Open Register (cited, not re-filed)

- #6350 VATS partial (FO3/FNV only; no limb-condition routing, gore, cinematics, overlay); #6646 per-part damage Phase 2 open.
- #6657 M24.2 Phase 3 condition subjects and perk hooks (perk activation is a slice of it).
- #7057 / #7059 / #7062 door runtime PRD and Phase 1 (doors with no authored clips stay scenery; no actor opens a door; door `BNAM` unplayed).
- #6662 CastMagic (procedure 11) has no runtime; declined `unsourced-semantics`.
- #6656 package decline reasons; #6670 legacy mod compatibility (the fold provenance commits touch cell loading).
- Save-load revival of a ragdolled player never calls `stand_up` (gap ledger, no issue); no pickpocket system (#5017's unconscious clauses have no hook).
