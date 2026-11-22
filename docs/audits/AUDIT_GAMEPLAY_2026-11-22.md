# Gameplay Audit — 2026-11-22

**HEAD**: 2b40f76b6 · **Baseline**: AUDIT_GAMEPLAY_2026-11-15.md (audited c0a91d8fa) · **Audited**: Dims 1–7 · **Unchanged (skimmed)**: Dim 3 (consumables; its delta window was really `afd260aeb..HEAD`, reviewed in full)

Read-only run: no repo edits, no engine launched, no issues filed. Every test command used the pinned 1.100.0 toolchain with `TMPDIR=/mnt/data/tmp/cargo-tmp`, never `--ignored`.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 5 |
| LOW | 9 |
| **Total** | **14** |

Every dimension guard is green: 0 failures across roughly 1,400 tests in the bin, core, plugin, scripting and debug-server crates. So every finding below is a gap the guards do not cover. The delta since 2026-11-15 is dominated by the Oblivion player-action stack (spellcasting, archery, block, armor rating, fall damage, skill-use leveling, rest/level-up menus) and the M66 door runtime. Most findings sit where that new state meets an in-process load.

- **Two AI-package misroutes (MEDIUM)**:
  - **GP-01:** Must Complete is honoured for procedures that never report done, so 117 FNV and 14 Oblivion NPCs can be pinned to one package.
  - **GP-02:** *Near Editor Location* Travel/Wander anchors on the actor's current position. This is the #5391/#5500 defect, which those issues fixed only for Eat/Sleep.
- **Three load-restore holes (MEDIUM)**:
  - **GP-03:** a door saved mid-swing stays `is_animating` forever.
  - **GP-04:** `FallTracker` survives a teleport or quick-load.
  - **GP-05:** `EquippedAmmo` is an additive column. Dims 1 and 7 found it independently; merged here.
- **LOW**: process-lifetime transients the load drain misses, a NOT_SAVED-reason guard blind spot, two inventory row-convention misses, a stale tree leaf, a vacuous projectile clause, a dead field and two doc placements.

Rejected after verification: an Oblivion ingestible-effect-conditions drop (Dim 3). xEdit's TES4 `wbEffects` (`/mnt/data/tmp/wbDefinitionsTES4.pas:1492`) gives each effect `EFID`, `EFIT`, then an optional `SCIT` + `FULL` and no `CTDA`, so the format has nothing to drop. The code comment saying so is correct.

## Invariant Matrix

| Invariant | State | Evidence |
|---|---|---|
| Single damage path | **holds** | Only `combat_damage_system` writes Health / `Dead` among hit consumers. Defence order is Fallout DT → Skyrim `armor::mitigate` → `target_block` → `weapon_defense`. `ArmorModel` and `ArmorRatingModel` are exclusive per profile. `hit_event_consumers_run_after_every_producer` is green. The `projectile == 0` clause is vacuous (GP-11, latent). |
| Death reconciled once | **holds** | Every production `Dead` insert (combat, scripted kill, water, drowning/fall, SDK batch, `reference_state` restore / starts-dead, lethal spell via `PendingScriptKills`) reaches `reconcile_dead_actor`. No new per-actor runtime component survives a death. |
| Inventory index stability | **holds, two misses** | Rows are zeroed in place. `count_of` / `count_of_any` / `first_row_of` have no hand-spelled copy except `queue_equip_by_form_id` (GP-09). Take-all does not filter zeroed rows (GP-08). |
| Fail-closed consumables vs fail-open packages | **holds** | `consume_item` keeps only `evaluate_modeled` branches and refuses an item on an unreadable effect CTDA (`inventory/actions.rs:138-151`). Packages use `evaluate_unless_refuted` (`npc_spawn/ai_package.rs:52`). |
| Stage order | **holds** | No consumer runs before its producer for the new state (bow/cast → `ActiveProjectiles` → flight → `TrapHit` flush → `combat_damage_system` → Late `skill_use_system`; Physics fall landing → Late consumers; `SleepCompletions` → Late `oblivion_level_system`). The skill text reverses `combat_damage_system` / `vats_sequence_end_system` (skill-doc drift, below). |
| State saved or re-derived | **gaps** | GP-03 (door flag saved, its clearing bookkeeping not). GP-04, GP-06 and GP-07 (process-lifetime resources whose NOT_SAVED reasons hold only for a cold start). GP-05 (an additive column cannot carry a saved absence). |
| Determinism | **holds** | No `rand` / `Instant` / `SystemTime` in gameplay systems except the allowed spawn budget timing. |

## Findings

### GP-01: Must Complete is honoured for procedures that never report done, so an Eat/Sleep/Patrol/Follow package outlives its window forever

- **Severity**: MEDIUM
- **Dimension**: 5 (NPC spawn → AI package selection)
- **Domain**: ai · **Game**: FNV, Oblivion
- **Trigger**: FNV and Oblivion (same PKDT bit 2). An actor whose first-open Eat, Sleep, Patrol, Follow, Use Item At, Accompany or Wander package carries *Must Complete* and has a lower-priority package that should take over when its window closes. Vanilla example: FNV `VSpecialViperShadow*` (`NPC_` 0x168bd7, 0x168bd6, …):
  - `VSpecialViperShadowSleepAny22x10` (Sleep 22:00–08:00, flags 0x81406)
  - `...SupperCampAny20x2` and `...LunchCampAny11x2` (Eat, bit 2 set)
  - `...BreakfastCampAny8x2`
  - `...SandboxLinked`

  Any save state. A normal game day ends the window.
- **Location**: `byroredux/src/npc_spawn/package_flags.rs:34-45` (`procedure_done`, the `_ => false` arm), `:76-82` (`kept_past_window`), `:106-117` (`scan_with_flags`); caller `byroredux/src/npc_spawn/ai_package.rs:991-1005` (`select_active_package`)
- **Status**: NEW
- **Description**: AIP-FLG-002 keeps a Must Complete package winning past its window "until its procedure reports done: Travel arrived, Find count met, UseWeapon bursts fired". `procedure_done` implements those three and answers `false` for every other procedure, so `kept_past_window` makes the running package win with no end. Because it is the earlier entry in the priority list, nothing below it can win again. The Viper Shadow NPC enters Sleep at 22:00 and never leaves it, so breakfast, lunch, supper and the sandbox package never run.

  The GECK pages in the local reference say Must Complete means nothing for several of these procedures:
  - Sandbox: "Invalid".
  - Patrol: "a repeatable patrol has no notion of 'Must Complete' (like a wander)".
  - Follow: "Disabled unless End Location is checked".
  - Guard: "can never complete".

  For Eat and Sleep no source says either way. Keeping them forever is an invented rule, not a declined case: nothing is logged and `pack.ambient` shows no decline.
- **Evidence**: A read-only scan of `FalloutNV.esm` covered all 3,816 NPCs, using PKID order and PSDT hour windows and ignoring conditions and day-of-week, so it is an upper bound. 117 NPCs have a Must Complete package of a no-signal procedure that wins in some hour, followed by a lower-priority package that would win after its window closes. By procedure: Eat 166 pairs, Sleep 114, Follow 16, Patrol 11. `Oblivion.esm`: 14 NPCs (Sleep 6, Use Item At 5, Eat 3, Accompany 2, Wander 1). GECK text: `/mnt/data/src/reference/geck-uesp-wiki/(main)/{Sandbox,Patrol,Follow} Package.wiki`.
- **Impact**: The actor is pinned to its first Must Complete package for the rest of the session: it loses its day/night cycle and stays seated or asleep through the day. The wrong package also reports as the running one to package fragments and `GetIsCurrentPackage`.
- **Related**: AIP-FLG-002 (`docs/prd/ai-packages-completion.md` §5.7), #6656
- **Suggested Fix**: Honour Must Complete only for procedures that report done (Travel, Find, UseWeapon) and treat it as unset elsewhere. Show the ignored flag in `pack.ambient`. Add a test that a Must Complete Sleep package yields to the next open package when its window ends.

### GP-02: A Travel or Wander package to *Near Editor Location* anchors on where the actor stands, not on its editor placement

- **Severity**: MEDIUM
- **Dimension**: 5 (locomotion / package install)
- **Domain**: ai · **Game**: Oblivion, FNV (FO3 shares the PLDT type)
- **Trigger**: An actor moved away from its authored position by an earlier package, then handed a Travel or Wander package whose PLDT is *Near Editor Location* (type 3). Vanilla Oblivion:
  - `HorseGoHome` (0xc55fd, radius 1024, 13 users)
  - `SE45zReturnHome` / `SE45mReturnHome` (radius 512)
  - `MS38GlarthirGoHome`
  - `HirtelFleeFromKvatch`

  Totals: FNV 394 type-3 Travel packages (271 package edges); Oblivion 767 edges (157 Travel packages plus 415 Wander ones).
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:462-483` (`from_package` Travel arm: `NearEditorLocation` leaves `target_form_id: None`); `byroredux/src/systems/travel.rs:148-159` (`resolve_destination`), `:352-357` (called with `home = current`); `byroredux/src/systems/wander.rs:390-393` (`home = transform.translation`)
- **Status**: NEW. A sibling of closed #5391 / #5500, which fixed exactly this for Eat/Sleep through `EditorPlacement`.
- **Description**: `TravelBehavior` carries only a radius and a reference FormID, so the PLDT *type* is lost at the `AmbientBehavior` boundary. For type 3 there is no reference: `resolve_near_reference_target` returns `None`, and `resolve_destination` falls back to `pick_wander_target(home = current, radius)`. A "go home" Travel therefore picks a point near the actor's *current* position (with radius 0, its current position exactly), arrives at once and stamps `Traveled`. The comment from f09ba4ee6 assumes "the actor is placed at its editor location". That holds only until an earlier package moves it.
- **Evidence**: `EditorPlacement` (`crates/core/src/ecs/components/eat_sleep.rs:55-66`) is documented as "the anchor of a 'near editor location' package, which must not follow the actor wherever an earlier package walked it". `spawn_placement_root` stamps it on every actor (`npc_spawn/resumable/mod.rs:381-388`), but only `systems/eat_sleep.rs:473-475` and `forcegreet.rs` read it; `travel.rs` and `wander.rs` never do. `from_package` already computes `location_anchor(package.location)` for the Travel arm, then discards it except for two radius special cases.
- **Impact**: ReturnHome / GoHome packages are silent no-ops: horses never return and quest NPCs do not go back to their posts. A type-3 Wander with a radius wanders around wherever it was installed.
- **Related**: #5391, #5500, f09ba4ee6, 61a1da1cd, #6662
- **Suggested Fix**: Carry the `EatSleepLocation` anchor `from_package` already derives into `TravelBehavior` and `WanderBehavior` (and Patrol/Guard/Sandbox as needed). Resolve `NearEditorLocation` to `EditorPlacement.translation` in `resolve_destination` and the wander `home`, with the same unresolved-anchor idle rule Eat/Sleep uses. Add a test that moves the actor and then installs a type-3 Travel.

### GP-03: A door saved or parked mid-swing keeps `is_animating = true` forever, so it can never be activated again

- **Severity**: MEDIUM
- **Dimension**: 2 (interaction, in-cell doors M66, persistence)
- **Domain**: gameplay · **Game**: Oblivion, FO3, FNV (any door whose model authors `Open` + `Close` sequences)
- **Trigger**: A save (quicksave, autosave) taken within about 1–2 s of activating a door, while its clip plays, then a live load of that save. Also: a scripted door whose cell unloads mid-swing, so its parked `scripts` row is consumed on re-entry.
- **Location**: `byroredux/src/systems/door.rs:143-173` (`door_motion_system`: the `settled` / `stale` loops), `crates/scripting/src/vm_state.rs:166-170` (`two_state_activator_system` skip), `byroredux/src/interaction.rs:1556-1560` (prompt filter), `byroredux/src/save_io/registry_completeness_tests.rs:1034` (the `Door` NOT_SAVED reason)
- **Status**: NEW (#7059 DRT-SAVE-001/-002 specify only the resting open pose)
- **Description**: `TwoStateActivator.is_animating` is saved: it is a live-save column, and `ScriptState.two_state` carries it on eviction. The paired bookkeeping, `Door.remaining`, is re-derived as `0.0` at spawn and is NOT_SAVED. `door_motion_system` clears `is_animating` only in the `settled` path, which needs `remaining` to step from `> 0` to `<= 0`. A restored door has `remaining == 0` and `posed == None`, so it takes the `stale` branch. That branch only re-poses the `AnimationPlayer` and never clears the flag. The registry reason ("presentation bookkeeping that the first tick re-establishes") is true for the pose and false for the flag.
- **Evidence**:
  - The `stale` loop calls `set_player` only.
  - `two_state_activator_system` does `if state.is_animating { continue; }`.
  - The interaction snapshot filters out an animating `TwoStateActivator`.
  - `restore_resident` runs before `apply_deltas`, so the saved `true` is the final value.
  - Every door test builds `is_animating: false`.
  - ObScript `GetOpenState` (`obscript_effects/getters.rs:425`) reports such a door as permanently opening or closing.
- **Impact**: The door is stuck for the rest of that save, with no prompt and every activation ignored. A door saved while closing stays shut, which can block quest paths. An unscripted door recovers on cell re-entry. A scripted door keeps the stuck state through eviction, and only a script `SetOpen` clears it.
- **Related**: #7059, #7062, #6071
- **Suggested Fix**: In `door_motion_system`, enforce the invariant the system itself creates (`is_animating` only while `remaining > 0`): clear the flag for a door with `remaining == 0`, in the `stale` branch or before the `posed` test. Add a test beside `a_door_at_rest_in_a_state_its_pose_does_not_show_snaps` with `TwoStateActivator { is_open: true, is_animating: true, .. }` and a fresh `Door`.

### GP-04: `place_character_body` clears the controller's momentum but not `FallTracker`, so a quick-load or teleport mid-fall applies fall damage on landing

- **Severity**: MEDIUM
- **Dimension**: 7 (state re-derived after a load)
- **Domain**: combat · **Game**: Oblivion (the only profile with a `FallModel`)
- **Trigger**: An in-process load (F9 / `save.load`), or any `place_character_body` caller (door arrival, `combat.approach`, `cam.pos` with the body pinned), while the player is airborne below their takeoff height, with the new pose within `TELEPORT_DISTANCE` (400 BU) of the previous frame's. Typical case: quicksave at a ledge, jump off, F9 mid-fall.
- **Location**: `byroredux/src/systems/character.rs:611-613` (`FallTracker::step`), `:712-714` (`fall_landing`), `:991-1022` (`place_character_body`); `crates/core/src/character/fall.rs:74,97-125`; `byroredux/src/save_io/live_load.rs:480` (drain); `registry_completeness_tests.rs:463`
- **Status**: NEW (#7056, closed, introduced the tracker)
- **Description**: `FallTracker` is a process-lifetime resource holding the takeoff height and the seconds spent below it. It forgets a fall only on a one-frame move over 400 BU, a swim or a landing. `place_character_body` is the documented single teleport seam and calls `clear_momentum()`. But it never resets `FallTracker`, and neither does the load drain or the pose restore (`save_io/player.rs:246`). The allowlist reason ("a load starts with the body unplaced and no fall in flight") holds only for a cold start.
- **Evidence**: The first grounded frame after the placement returns `Some(self.seconds)`, which `fall_landing` turns into `FallModel::damage` and applies through `apply_player_drowning_damage`. At Oblivion's defaults (`time_min` 0.2 s, 80/s, skill factor 1.25), 0.5 s of pre-load fall costs about 50 Health. `grep FallTracker` finds no reset anywhere.
- **Impact**: Unearned damage, plus an Acrobatics skill use, right after a quick-load of a save taken on solid ground. It can kill a low-Health character.
- **Related**: #7056, #5591
- **Suggested Fix**: Reset the tracker inside `place_character_body`, next to `clear_momentum()`, so every teleport path shares it. Reword the allowlist reason. Add a test beside `place_character_body_moves_both_transforms_and_clears_momentum`.

### GP-05: `EquippedAmmo` is an additive column, so a load cannot clear a live arrow choice

- **Severity**: MEDIUM
- **Dimension**: 1 (equipment model) / 7 (what a load restores); the registration kind belongs to `/audit-save`
- **Domain**: inventory · **Game**: Oblivion, Skyrim (the profiles with a `BowModel`)
- **Trigger**: Equip an arrow type, then load in-process a save written while the player had no `EquippedAmmo` row (before their first arrow equip), whose inventory still carries that arrow type.
- **Location**: `byroredux/src/save_io/registry.rs:97-102,340` (`register_component::<EquippedAmmo>`, additive); `byroredux/src/save_io/live_load.rs:864-876` (the post-`apply_deltas` player block re-derives only `EquippedWeapon`); `byroredux/src/bow.rs:120-141` (`equipped_ammo`, `set_equipped_ammo`, `reconcile_equipped_ammo`)
- **Status**: NEW (found independently by Dims 1 and 7)
- **Description**: The row is created lazily by the first `toggle_equipped_ammo`; neither `attach_to_player` nor the boot player build stamps one. A save from before that has no row. `apply_deltas` removes a live row on a saved absence only for `register_replacing_component` columns (`crates/save/src/driver.rs:369-375`), and `EquippedAmmo` is additive, unlike its siblings `ReadiedSpell`, `SkillUseLevel`, `Sneaking` and `Dead`. The registry comment's "cleared by writing None" covers only a clear made in the saving session. `reconcile_equipped_ammo` clears only a form the inventory no longer carries. This is the shape of #3488 (`EquippedWeapon`) and #5027 (`Dead`).
- **Evidence**: `rg EquippedAmmo byroredux/src/save_io` hits only the registry and tests. `equipped_ammo_round_trips` checks a saved `None` and a saved `Some` through cold `restore_world`, where an absent row cannot occur.
- **Impact**: The loaded game fires, and the menu shows as equipped, an arrow type the saved game never chose. It would otherwise refuse with "no arrows". Re-toggling the slot fixes it.
- **Related**: #7055 (BOW-AMM-002), #3488, #5027
- **Suggested Fix**: Register `EquippedAmmo` with `register_replacing_component`, keeping the `None` writes. Add a live-overlay test where the live player holds `Some` and the snapshot has no row. Dims 1 and 7 disagree on whether the registration kind enters the registry fingerprint. Check it against the shape guard before deciding whether a `FORMAT_MAJOR` bump is needed.

### GP-06: `ActiveProjectiles` survives an in-process load, so the outgoing session's arrows and spell bolts land in the loaded world

- **Severity**: LOW
- **Dimension**: 7 (session transient not dropped by the load drain)
- **Domain**: combat · **Game**: all with projectiles; Oblivion arrows and Target spells make the window common
- **Trigger**: An in-process load while a shot is in flight.
- **Location**: `byroredux/src/systems/projectiles.rs:48-76` (`Projectile`, `ActiveProjectiles`), `:408-546` (`projectile_flight_system`); `byroredux/src/save_io/live_load.rs:480-624` (`discard_outgoing_session_requests`, not named); `registry_completeness_tests.rs:561`
- **Status**: NEW
- **Description**: Each `Projectile` carries `shooter: EntityId`, `launcher_bodies` (Rapier handles of the outgoing physics world), `spell`, `arrow` and `damage`. Neither the drain nor the cell reload touches the vector. The allowlist reason ("a saved game restarts with none in the air") holds only for a cold start. After the load, the old shots keep flying through the new physics world. A hit becomes a `TrapHit` against a loaded-world actor, and a spell bolt calls `player_cast::land_target`, which starts its effects and feeds the school. The player's id survives the load, so the player's own shot lands as the loaded player's.
- **Evidence**: `rg ActiveProjectiles byroredux/src/cell_loader byroredux/src/save_io` finds nothing.
- **Impact**: Phantom damage, a Marksman or school skill use, and spell effects for the remaining second or two of flight. This is the class #6239/#6508/#6574/#6927 closed for other transients.
- **Related**: #6574, #6508, #6927, #7055, #7080
- **Suggested Fix**: Clear `ActiveProjectiles` in `discard_outgoing_session_requests` and reword its NOT_SAVED reason to "a load discards", so the drain guard covers it.

### GP-07: Several NOT_SAVED reasons promise a load reset that an in-process load never does, and the drain guard matches only two literal phrases

- **Severity**: LOW
- **Dimension**: 7 (state-restore claims; guard blind spot)
- **Domain**: gameplay · **Game**: Oblivion
- **Trigger**: An in-process load in an Oblivion session.
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:463-465` (`FallTracker`, `CastState`, `BowState`), `:802` (`LevelNotices`), `:805` (`StanceTimers`), the guard at `:1263-1285`; `byroredux/src/skill_use_emit.rs:249-257,321-338`; `crates/scripting/src/player_cast.rs:181-189,422-424`; `byroredux/src/bow.rs:204-212`
- **Status**: NEW (GP-04 and GP-06 are the consequential cases; this is the remainder and the guard)
- **Description**: These resources are process-lifetime, and the drain does not touch them. Their cold-start-only reasons go unchecked because the guard looks only for "a load discards" and "The load drain ends". Effects:
  - `LevelNotices.announced` keeps the outgoing session's pending-level count, so loading a save with more pending levels fires a spurious "rest and meditate" notice on the first frame. `a_loaded_pending_level_is_not_announced_again` covers only a cold start.
  - `CastState.impact` keeps a stale target `EntityId`, which `cast.status` prints.
  - `BowState.held` / `cooldown`, `CastState.cooldown_remaining` and `StanceTimers` carry sub-second timers across the load.
- **Evidence**: None of these names appears in the drain body.
- **Impact**: Cosmetic or sub-second. The real cost is that the guard cannot catch the next one; it already missed `ActiveProjectiles` and `FallTracker`.
- **Related**: #6574, #6508, GP-04, GP-06
- **Suggested Fix**: Reset `LevelNotices`, `CastState`, `BowState` and `StanceTimers` in the drain. Widen the guard: a NOT_SAVED reason that says "a load" followed by a reset verb (starts / restarts / re-seeds / resets / forgets) must name a drain line or a restore step, or carry an explicit "cold start only" marker.

### GP-08: Take-all over a source with zeroed rows publishes zero-count item transfers (phantom OnAdd/OnDrop) and a zero-count theft row

- **Severity**: LOW
- **Dimension**: 1 (inventory)
- **Domain**: inventory · **Game**: all
- **Trigger**: A corpse or container with an emptied row (rows are zeroed in place by `remove_items`, `RemoveItem`, an NPC's Find/Eat use, or a single-stack loot), taken with take-all while another row still holds items. Only visible when the emptied form carries an `OnAdd`/`OnDrop` ObScript block.
- **Location**: `byroredux/src/inventory/loot.rs:192-210` (`LootSelection::All`), feeding `:238-262` (`emit_item_transfers`) and `:277` (`crime::report_theft`)
- **Status**: NEW
- **Description**: The `Stack` arm filters `count > 0` (`loot.rs:216`); the `All` arm does not. A zeroed source row yields `(form, 0)` in `outcome.rows`, and `emit_item_transfers` pushes `ItemTransfer { count: 0 }` onto both sides' `ItemEventBatch`. `obscript_item_system` (`obscript_objects.rs:604-620`) runs `OnAdd` / `OnDrop` with no count check. `report_theft` notes a stolen count of 0, and `destination.add` appends a zero-count row to the player.
- **Evidence**: No `inventory::tests` case runs `transfer_loot` over a zeroed row; every fixture has positive counts.
- **Impact**: A scripted item can run its `OnAdd` again on the player at corpse loot, which can repeat quest-stage side effects.
- **Related**: #5871, #5268
- **Suggested Fix**: Skip `count == 0` stacks in the `All` arm. Add a `transfer_loot` test over one zeroed and one live row, asserting the batch holds only the live one.

### GP-09: `inv.equip` takes the first row of the form even when it is zeroed, so it refuses while a live row exists

- **Severity**: LOW
- **Dimension**: 1 (inventory)
- **Domain**: inventory · **Game**: all
- **Trigger**: The console or smoke command `inv.equip <form>` after the player's first row of that form was emptied while a later row (an instanced copy, or a re-add after instancing) still holds it.
- **Location**: `byroredux/src/inventory/mod.rs:112-119` (`queue_equip_by_form_id`)
- **Status**: NEW
- **Description**: This is the last hand-spelled "first row of form X" left in the inventory code. `Inventory::first_row_of` (#6965) requires `count > 0`; this site takes the first row of the form regardless of count. It queues `ToggleEquip`, which then refuses the zero row ("Can't equip …"), and the command still reports success.
- **Evidence**: `inv.use` (`mod.rs:155`) and `drain_script_weapon_equips` (`mod.rs:191`) both use `first_row_of`. The only test has a single positive row.
- **Impact**: Console and smoke scripts only.
- **Related**: #6965, #6363
- **Suggested Fix**: Use `inventory.first_row_of(form_id)`. Add a two-row test (zeroed, then live).

### GP-10: A tree package's leaf is chosen once per package win; leaf conditions are never re-read while that package keeps winning

- **Severity**: LOW
- **Dimension**: 5 (package selection)
- **Domain**: ai · **Game**: Skyrim, FO4, Starfield
- **Trigger**: A tree package whose leaves carry their own conditions (116 of 1,627 vanilla Skyrim leaves, per the `tree.rs` module doc), with a condition that flips while the package stays the winner.
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:1463-1500` (behaviour recomputed every pass, installed only on `changed`); `byroredux/src/npc_spawn/ai_package/tree.rs:93-110`
- **Status**: NEW (low confidence on the original's re-evaluation cadence; a candidate, not a proven misroute)
- **Description**: `ambient_ai_package_system` re-runs `from_package`, and therefore `tree_decision`, each in-game minute, but installs the result only when the winning PACK FormID changes. A leaf whose conditions flip is never swapped. A package whose leaves all failed at install stays with no behaviour after they later pass.
- **Evidence**: `if !changed { … continue; }` at `:1491-1497` discards the recomputed behaviour.
- **Impact**: A stale leaf choice until the next package change.
- **Related**: aee4fab7a, #6660
- **Suggested Fix**: When the winner is unchanged and its template is a tree with conditioned leaves, compare the freshly chosen leaf with the installed one and re-install if it differs.

### GP-11: `weapon_defense`'s "no projectile" clause is vacuous, and the `UseWeapon` hit producers hard-code `ignores_resistance: false` (latent)

- **Severity**: LOW
- **Dimension**: 4 (combat)
- **Domain**: combat · **Game**: Oblivion (latent)
- **Trigger**: A projectile or `UseWeapon` hit by an actor with no `EquippedWeapon` on an Oblivion load. Nothing reaches it today: Oblivion has no UseWeapon procedure, no NPC archery and no scripted projectile fire.
- **Location**: `byroredux/src/combat.rs:556` (`unarmed = event.projectile == 0 && !event.arrow && no EquippedWeapon`); `crates/scripting/src/trap_hit_requests.rs:68-80` (`projectile: 0`); `byroredux/src/systems/script_requests.rs:135,182` (`ignores_resistance: false`)
- **Status**: NEW
- **Description**: #7195 defines a natural creature attack as "a `CreatureActor` blow with no weapon and no projectile". But the `TrapHit` flush, which lands every projectile and every UseWeapon strike, always writes `projectile: 0`, so the clause cannot exclude a projectile. A shooter whose launcher is not its `EquippedWeapon` would count as unarmed, and a creature or a Journeyman Hand-to-Hand actor would bypass Resist Normal Weapons. The UseWeapon producers know `run.request.weapon` but pass `false` for `ignores_resistance`.
- **Evidence**: `HitEvent.projectile` is non-zero only at `mines.rs:157`, whose aggressor has no `ActorVitals`.
- **Impact**: None on a shipped path. The "no projectile" guard looks like protection it does not give.
- **Related**: #7195, #7199
- **Suggested Fix**: Carry a real `from_projectile` marker on `TrapHit` → `HitEvent` in place of the `projectile == 0` test, or reword #7195's docs to say the test rests on `EquippedWeapon` alone. Answer `ignores_resistance` from `run.request.weapon` in the UseWeapon producers.

### GP-12: `CastState.interval` is written on every cast and never read

- **Severity**: LOW
- **Dimension**: 7 (written-never-read)
- **Domain**: combat · **Game**: Oblivion
- **Trigger**: Any Oblivion player cast.
- **Location**: `crates/scripting/src/player_cast.rs:184-185,424`; `byroredux/src/commands/spell.rs:139`
- **Status**: NEW
- **Description**: The field's doc says "The interval the last cast armed, for the report". But `cast.status` prints the constant `CAST_INTERVAL_SECONDS`, and nothing else reads the field.
- **Evidence**: `rg '\.interval'` over `byroredux/src` and `crates/scripting/src` finds only the write.
- **Impact**: A dead field. `cast.status` would report the wrong value once the interval stops being constant.
- **Related**: PSC-DX-001
- **Suggested Fix**: Print `state.interval` in `cast.status`, falling back to the constant before the first cast, or drop the field.

### GP-13: `install_block_model` has no doc comment; its doc sits stacked on `install_fall_model`

- **Severity**: LOW
- **Dimension**: 4 (combat)
- **Domain**: combat · doc-rot · **Game**: all (static)
- **Trigger**: Reading the code.
- **Location**: `byroredux/src/combat.rs:618-639`
- **Status**: NEW
- **Description**: #7056 (787cdf19c) inserted `install_fall_model` between #7054's doc comment for `install_block_model` and that function. The block line now reads as the first paragraph of `install_fall_model`'s doc, and `install_block_model` has none.
- **Evidence**: Lines 618-619, "Install (or, for a game without block rules, remove) the [`BlockModel`] …", sit directly above the fall paragraph on `install_fall_model`. `install_block_model` at line 631 has no `///`.
- **Impact**: rustdoc and reader confusion; no behaviour change.
- **Related**: closed GAME-5 (006770e3a, same class), #7203
- **Suggested Fix**: Move the block line above `install_block_model`.

### GP-14: Two comments still say the Oblivion level-up surface is the console until a native page exists

- **Severity**: LOW
- **Dimension**: 6 (player feedback)
- **Domain**: gameplay · doc-rot · **Game**: Oblivion
- **Trigger**: Reading the code.
- **Location**: `byroredux/src/commands/gameplay.rs:665` (`LevelUpCommand::execute`, the `attr` arm) and `:615` (the `LevelUpCommand` doc)
- **Status**: NEW
- **Description**: The `attr` arm says "OSL-API-005 (Q3): the level-up surface until a native page exists". The page now exists (`oblivion_rest::snapshot` / `menuxml_rest.rs`, #7086), and the console picks remain only for scripted and jail sleep (PRD `game-ui-integration` rev 16, `docs/engine/skill-use.md:181-185`). The struct doc ("the level-up menu's work, until a menu exists") is right for the Fallout reward spend but reads as covering Oblivion too.
- **Evidence**: As above.
- **Impact**: It suggests the page is still outstanding, or invites deleting the still-needed console path.
- **Related**: #7086, #7052, #7217–#7220
- **Suggested Fix**: Reword `:665` to say the console picks serve scripted and jail sleep while a bed sleep offers the page, and qualify the `:615` doc.

## Known-Open Register (cite, do not re-file)

| Issue | State | Note |
|---|---|---|
| #6943 | OPEN | Unarmed swings skip the crippled-arm penalty (`attack_damage`'s `UNARMED_DAMAGE` arm); still true (Dim 4). |
| #6350 | OPEN | VATS umbrella: limb-condition routing, gore and playback absent (declared). |
| #7022 | OPEN | Reload spawns read the pre-load session's player level before `apply_deltas` (Dim 5; same shape as stats). |
| #6662 | OPEN | M42 package tracker; CastMagic declines as `unsourced-semantics`. |
| #4137 | OPEN | Two `template_flags` bits with no consumer. |

Regression checks from 2026-11-15: GAME-1 (7a969f688), GAME-2 (b6315be1c), GAME-3 / #6942 and GAME-5 (006770e3a) all hold. This session's #7193, #7211 and #7212 hold in every dimension that touches them.

## Observations (not filed)

- **Fewer than three eligible attributes (Oblivion level-up).** `apply_level` needs exactly three eligible picks unless every attribute is 100 (`crates/scripting/src/oblivion_skill_use.rs:418-433`), so a player with one or two attributes under 100 gets a page whose Confirm never becomes ready. The PRD and `skill-use.md` cover only the all-100 case. Route to `/audit-character` for a sourced answer (UESP Leveling) before changing it.
- **Notification banner is one slot for 4 s** (`crates/debug-ui/src/lib.rs:178-184`). A second notice within 4 s replaces the first on screen; both stay in the log. Route to `/audit-ui`.
- **Scripted `EquipItem` of an arrow** is dropped by `drain_script_weapon_equips`. No vanilla script evidence either way.
- **Player-template leveled lists** (`inventory/catalog.rs:715`) still read `effective_actor_level`. The behaviour is identical on vanilla: Oblivion's Player `NPC_` has ACBS flags 0x0 and level 1, and no player CNTO in FNV/FO3/Oblivion/Skyrim carries an LVLI. The comment at `:709-712` is stale.
- **Parked `scripts` rows of unscripted doors** are never consumed (DRT-STATE-003 default on re-entry), so they accumulate in saves at one small row per evicted door.
- **`MeleeState.blocking`** follows the held key even while the player cannot act. No source says what the original does.
- **Chargen `apply`** still does not enforce the SPECIAL total or the trait count; console-only. Unchanged from 2026-11-15.

## Routed to other audits

- `/audit-save`: the `EquippedAmmo` registration kind and its fingerprint effect (GP-05).
- `/audit-concurrency`: `door_motion_system` is registered with plain `add_exclusive` and has no `Access` row. `player_cast_system`'s declared access omits `PhysicsWorld`, `ActiveProjectiles`, `ActiveCamera`, `ActorColliderOwner`, `Transform`/`GlobalTransform` and `Inventory`, which its Touch ray and Target launch use.
- `/audit-character`: the `crates/core/src/combat.rs:1-35` module header is stale: it still calls the zero-caller state "unresolved", but the bow is now a consumer and PRD Q6 decided melee keeps base damage. Also the fewer-than-three-attributes question above.
- `/audit-physics`: KCC grounded-flag behaviour after `place_character_body` (GP-04).

## Skill-text drift (for the next `/audit-sync`)

- Dim 7's order list says the player_animation segment "opens with `vats_sequence_end_system` → `combat_damage_system`". The code registers `combat_damage_system` first.
- The Dim 7 list also omits `quest_aliases` / `scene_playback`, the `vitals_upkeep.rs` file name, the Late `diagnostics` / `extension_*` / `script_hosts` systems, `player_stats` (`pool_regen_tick_system`) and the Physics-stage fall damage.
- Dim 1 calls `main_body_mask` "bit 3, or bit 35 on Starfield". On Starfield the code returns both bits as one mask.
- Dim 3's "paths unchanged" premise was false: `byroredux/src/inventory/` had about 20 commits since 2026-11-15.
