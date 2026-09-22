# Gameplay Audit — 2026-09-21

**HEAD**: `f97775ca8` · **Baseline**: none (first run) · **Audited**: Dim 1–7 (full run) · **Unchanged since baseline (skimmed)**: none (first run)

(`73aaed7b9`, a docs-only HISTORY/ROADMAP closeout, landed during the run and touches no audited path.)

**Scope**: `/audit-gameplay` default scope, run as one leg of `/audit-suite --preset comprehensive`.
The gameplay layer in `byroredux/src`: inventory and equipment, interaction, locks, containers and
loot persistence, consumables, combat and death, NPC spawn → AI packages → locomotion, player
feedback, and gameplay-state save coverage and stage order. There is no earlier
`AUDIT_GAMEPLAY_*.md`, so every dimension was audited in full. Earlier coverage in the ECS, SAVE,
SCRIPTING, CHARACTER and FNV report families was used for dedup only. No sub-agents were used;
each dimension's notes are in `/tmp/audit/gameplay/dim_<N>.md`.

**Games and cells exercised**: no engine launch; the playable-slice gates belong to the runtime
audit. Evidence comes from:
- the binary and plugin test suites;
- the `#[ignore]`d FO3+FNV Stimpak/Hardcore guard, run explicitly;
- read-only probes that link the engine's own `byroredux-plugin` parser (out-of-repo crate
  `/tmp/audit/gameplay/lvlprobe`). They were run against the installed `FalloutNV.esm`,
  `Fallout3.esm`, `Skyrim.esm` (SE), `Oblivion.esm` and `Fallout4.esm`, alongside two raw-ESM
  Python census scripts.

## Executive Summary

| Severity | NEW | Regression | Existing (open) re-confirmed |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 3 | 0 | 0 |
| MEDIUM | 10 | 0 | 3 (#4571, #4232, #4248 stale) |
| LOW | 9 | 0 | 3 (#4574, #4605, #4116) |

There is one more existing open issue, #4414, an enhancement for ambient faction hostility. It is
re-confirmed and not counted above.

The three HIGH findings:

- **GAME-D4-2026-09-21-01**: the frame after an NPC in scripted combat dies, its corpse's placement
  root is teleported to world `(0,0,0)`. `npc_combat_ai_system` writes `Vec3::ZERO` for a dead
  attacker, and `AiCombatState` is not part of the death teardown. The zeroed `Transform` is a
  saved delta column, so after a reload the ragdoll is rebuilt at the origin.
- **GAME-D5-2026-09-21-01**: the shared `resolve_entity_by_global_form_id` can never return the
  player, because the player body's FormID is a sentinel pair with `local = 1`, not `0x14`. As a
  result:
  - every Follow/Escort/Travel/Guard package that targets PlayerRef misroutes. Followers stand
    still, and travel-to-player walks to a hash-picked random point. That is 75 packages on FNV,
    66 on FO3 and 12 on Oblivion.
  - `GetDistance PlayerRef` reads as infinitely far.
- **GAME-D7-2026-09-21-01**: a pickup tombstone for an item picked up in a cell that is resident
  when the save is made is never re-applied on load. The item comes back and can be picked up
  again, duplicating it into the restored inventory. The save allowlist claims the opposite.

The MEDIUM findings:

- **Corpse loot is about ¼ of container loot.** NPC inventories use the equip-oriented leveled
  expander, which ignores LVLO counts and chance-none, so a corpse yields about a quarter of what
  the same list yields in a container. This holds on all five masters, for example FNV 25,708 vs
  105,905 items.
- **Pickup has no input path.** No interaction candidate exists for loose items.
- **Disabled references stay interactive.** A `Disable()`d door still teleports and a disabled
  container can still be looted.
- **The theft rule misclassifies ownership.** It is keyed to PlayerRef `0x14`, but every master
  authors player ownership as NPC_ `0x7`. It also ignores cell ownership.
- **The P2 Draugr combat tail never fires.** Its marker is only inserted on the Oblivion/FO3/FNV
  spawn path.
- **A dead player keeps acting.** The dead player still attacks, opens doors and loots.
- **SDK actor-value writes skip the death transition.** They can take Health to ≤0 without
  producing a death.
- **Scripted combat does not suspend the ambient package.** Two movers drive each actor, so a
  seated actor chases while still in its sit pose.
- **Alias packages are ignored for actors with no base packages.** Skyrim has 19 such
  supported-behavior cases.
- **The save-completeness guard cannot see 39 production types.** It truncates each file at the
  first `#[cfg(test)]`; 5 of the hidden types are unclassified.

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path (HitEvent → `combat_damage_system`; water/drowning are the documented extra producers and both reconcile) | **DRIFTED** | GAME-D4-2026-09-21-04: the SDK actor-value batch writes Health with no `Dead` or reconcile |
| Death reconciled once | **DRIFTED** | GAME-D4-2026-09-21-01 (`AiCombatState` outlives the teardown and zeroes the corpse root); GAME-D4-2026-09-21-05, latent (the death take re-inserts `AnimationPlayer` and replays on every respawn/reload) |
| Inventory index stability (rows zeroed, never removed) | VERIFIED | Dim 1: consume, transfer (Stack), pickup and restore all preserve indices. The `All` path empties the source only together with a full source-equipment reset in the same exclusive call |
| Fail-closed consumables vs fail-open packages | VERIFIED | `restoration_plan` rejects any unknown CTDA or effect; `package_conditions_pass` keeps fail-open. The asymmetry is intact |
| Stage order | VERIFIED, one LOW drift | Update: restoration → interaction → container_loot → combat_input → npc_combat_ai → combat_damage. Ambient packages run before scene packages. Late: water_damage → reconcile_pending_dead → … → event_cleanup. Drift: GAME-D7-2026-09-21-03, fragment `Activate` is flushed after `container_loot_system` |
| State saved-or-rederived | **DRIFTED** | GAME-D7-2026-09-21-01 (`PickedUp`), GAME-D7-2026-09-21-02 (guard blind to 39 types), GAME-D4-2026-09-21-05 (`death_played` latch) |
| Determinism (no RNG/wall clock in gameplay logic) | VERIFIED | `rand::` / `Instant::now` / `SystemTime` / `thread_rng` grep over the 20 gameplay files is empty |

## Findings

### HIGH

### GAME-D4-2026-09-21-01: A dead combatant's corpse root is written to world origin by `npc_combat_ai_system`, and the zeroed `Transform` is saved
- **Severity**: HIGH
- **Dimension**: 4 — Combat & Death Pipeline
- **Location**: `byroredux/src/systems/combat_ai.rs:69-82` (dead-attacker decision), `:207-216` (unconditional translation write); `byroredux/src/combat.rs:515-567` (`reconcile_dead_actor`); `byroredux/src/npc_spawn/ai_package.rs:434-497` (`clear_ambient_behavior`)
- **Status**: NEW
- **Description**:
  - When an `AiCombatState` actor has `Dead`, the read pass pushes a decision with
    `new_translation: Vec3::ZERO` and `state: None`.
  - The write pass then assigns `transform.translation = decision.new_translation` for every
    decision, including that one.
  - `AiCombatState` is not removed at death. Neither `reconcile_dead_actor` nor
    `clear_ambient_behavior` touches it. So each scripted-combat actor that dies reaches this
    branch exactly once, on the next frame, and its placement root is moved to world origin.
  - The skill's contract says the death teardown must grow with each new per-actor runtime
    component. `AiCombatState` (added 09-13, f61ea0447) was never added.
- **Evidence**:
  ```rust
  // combat_ai.rs:69 — dead attacker
  if world.get::<Dead>(entity).is_some() {
      decisions.push(Decision { entity, new_translation: Vec3::ZERO, new_rotation: None, state: None, strike: None });
      continue;
  }
  // combat_ai.rs:207 — every decision, including the one above
  if let Some(transform) = transforms.get_mut(decision.entity) {
      transform.translation = decision.new_translation;
  ```
  - The test `clears_combat_state_when_attacker_dies` spawns the attacker at `Vec3::ZERO`, so the
    wrong write cannot be observed.
  - `Transform` is in `MUTABLE_DELTA_COLUMNS` (`save_io.rs:84-85`).
  - Corpses carry no stream snapshot, because `AmbientPackageRuntime` is removed at death and
    `has_package_state()` is then false.
- **Impact**:
  - Within the session, skinned bodies keep rendering from the ragdoll's world-space palette.
  - The root's `GlobalTransform`, `WorldBound` and non-bone attachments move to the origin, and the
    skinned-mesh bound merge stretches from the origin to the body.
  - A save now stores the corpse root at `(0,0,0)`. On load, `reconcile_dead_actor_runtime_state`
    propagates the hierarchy from that root and `activate_ragdoll` seeds the bodies around the
    world origin. The body is gone from where it fell, sitting inside geometry or in an unloaded
    exterior cell.
- **Trigger**: Skyrim MQ101 keep-escape (or any `Effect::StartCombat` fragment); kill the armed NPC;
  save; load.
- **Related**: GAME-D5-2026-09-21-02 (the same actor is also driven by its package); #4605
  (CONC-D3-2026-09-21-02, the same system's hold stack); #3708 (the precedent that grew the
  teardown for `AmbientPackageRuntime`).
- **Suggested Fix**:
  - Remove `AiCombatState` in `reconcile_dead_actor`.
  - In `npc_combat_ai_system`, carry the actor's current translation, or skip the transform write,
    for decisions with `state: None`.
  - Make the dead-attacker test start away from the origin.

### GAME-D5-2026-09-21-01: PlayerRef (`0x14`) never resolves through the shared FormID resolver — every package, condition and fragment targeting the player misroutes
- **Severity**: HIGH
- **Dimension**: 5 — NPC Spawn → AI Package Selection → Locomotion
- **Location**: `crates/scripting/src/condition.rs:438-450` (`resolve_entity_by_global_form_id`); callers `byroredux/src/systems/follow.rs:82-84`, `systems/escort.rs:103-105`, `systems/travel.rs:99-127` (also Guard through `resolve_near_reference_target`); player identity `byroredux/src/scene.rs:1106-1110` + `crates/core/src/form_id.rs:149-152`
- **Status**: NEW
- **Description**:
  - The resolver returns the entity whose `FormIdComponent` resolves to `pair.local == form_id`.
    The player body's `FormIdComponent` is `PLAYER_FORM_ID_PAIR = { plugin: u128::MAX, local: 1 }`,
    required for the save remap (#1846). No cell places a `0x14` ACHR (probed on `FalloutNV.esm`
    and `Skyrim.esm`), so `resolve_entity_by_global_form_id(world, 0x14)` is always `None`.
  - `scripting::package` knows this and special-cases `0x14 → player` (`package.rs:365`, `:436`).
    The ambient movers do not, and neither do the CTDA or fragment callers.
  - Outcomes on the ambient path:
    - Follow(player) is terminal-idle ("no retry").
    - Travel(NearReference player) and Escort-lead fall back to `pick_wander_target`, a
      deterministic pseudo-random point within 512 u of home.
    - Guard falls back to home.
- **Evidence**: Census with the engine parser (`/tmp/audit/gameplay/lvlprobe`, `playerpk` binary)
  of PACK records whose target or location is PlayerRef:
  - FNV: 75 packages (Follow 52, Escort 8, Travel 13, Guard 2), listed directly by 29 NPC_. Examples:
    `FollowersCassFollowPlayerDEFAULT`, `FollowersRexFollowPlayerDEFAULT`, `VMS18TedFollowPlayer`.
  - FO3: 66 packages (Follow 39, Escort 23, Travel 4) on 40 NPC_. Examples:
    `MQ05Stage80LiEscortPlayerToTunnel`, `MQ11SarahFollowPlayerEndgame`.
  - Oblivion: 12 Travel packages on 4 NPC_ (`MQ12LichAttack`, `SE32FindPC`).

  The same resolver drives `GetDistance` (`condition.rs:542`, where a missing player yields
  `f32::MAX`), `RunOn::Reference` (`:362`), fragment object properties (`fragment/effects.rs:107`)
  and `quest_stages.rs:815`.
- **Impact**:
  - Companion and escort packages never follow or escort the player.
  - Travel-to-player packages send NPCs to arbitrary nearby points with no log.
  - Package, dialogue and quest CTDA gated on player proximity always evaluate as out of range.
  - `PlayerRef` object properties in fragments decline silently.
- **Trigger**: any FO3/FNV/Oblivion NPC whose winning package targets the player (companions,
  MQ05 Li, MQ11 Sarah), any `GetDistance PlayerRef` gate, any game.
- **Related**: #3099 (the `0x14`-vs-`0x7` base confusion, closed); #1664 (GetDistance resolver);
  GAME-D2-2026-09-21-04 (the same `0x14`/`0x7` confusion in the theft rule).
- **Suggested Fix**:
  - Resolve PlayerRef inside `resolve_entity_by_global_form_id` itself: `0x14` → the
    `PapyrusPlayerEntity` / `PlayerEntity` body. Then delete the two local special cases in
    `package.rs`.
  - Add a follow-the-player test through the ambient path.

### GAME-D7-2026-09-21-01: Pickup tombstones of placements resident at save time are never re-applied on load — the item returns pickable and duplicates
- **Severity**: HIGH
- **Dimension**: 7 — Gameplay-State Coverage (plus Dim 2, loot persistence)
- **Location**: `byroredux/src/inventory.rs:913-916` (marker + `mark_picked_up`); `byroredux/src/cell_loader/reference_state.rs:54-72` (`without_parked_state`), `:233-257` (`mark_picked_up`); `byroredux/src/cell_loader/references/synth_child.rs:856-862` (spawn-time restore); `byroredux/src/save_io.rs:1680-1695`, `:1721`; `byroredux/src/save_io/registry_completeness_tests.rs:460`
- **Status**: NEW
- **Description**:
  - `pickup_loot` stamps `PickedUp` on the resident root and immediately parks a
    `picked_up: true` row in `PersistentReferenceStates`, which is saved.
  - `PickedUp` itself is neither serialized nor a delta column.
  - On load, the cell reload runs inside `without_parked_state`, which removes the store for the
    duration. The respawned placement's `reference_state::restore` therefore finds no store and
    does nothing.
  - `restore_resources` then installs the saved store as a plain overwrite (`driver.rs:206-216`).
    Nothing walks resident entities to consume the rows that now match them.
  - The allowlist row justifies not saving `PickedUp` with "reference_state::restore re-stamps the
    marker at load". That holds only for cells streamed in after the reload, not for the saved
    cell itself.
- **Evidence**:
  ```rust
  // save_io.rs:1684 — respawn happens with the store absent
  let outcome = crate::cell_loader::reference_state::without_parked_state(world, |world| { reload_interior_session(...) });
  // save_io.rs:1721 — the saved store arrives after the respawn
  byroredux_save::restore_resources(world, &registry, &snapshot)
  ```
  No test combines pickup with save/load; `container_and_corpse_loot_survive_encoded_live_overlay`
  covers containers and corpses only.
- **Impact**:
  - After save → load in the cell where the item was picked up (the normal case), the item is
    back, interactive and pickable, while the loaded inventory already holds it. The result is a
    duplicate.
  - The stale row stays parked until the cell is evicted and revisited.
  - Per the save rule "a load that silently drops gameplay state (loot)", this is HIGH.
- **Trigger**: any game; pick up a loose item (see GAME-D2-2026-09-21-02 for reachability); save in
  that cell; load.
- **Related**: #4571 (the marker's render skip, separate); #4465 (`picked_up` field and
  FORMAT_MAJOR 25); GAME-D2-2026-09-21-02.
- **Suggested Fix**: pick one of two approaches.
  - Register `PickedUp` as a zero-field delta column, the `Dead` pattern.
  - Or, after `restore_resources`, consume `picked_up` rows for FormId-matched resident placements.

  Either way, add a pickup → save → load round-trip test.

### MEDIUM

### GAME-D2-2026-09-21-01: NPC inventories expand leveled lists without LVLO counts or chance-none — a corpse yields ~¼ of what the same list yields in a container
- **Severity**: MEDIUM
- **Dimension**: 2 — Loot Persistence / leveled loot
- **Location**: `byroredux/src/npc_spawn.rs:1153-1201` (OTFT and CNTO through `expand_leveled_form_id`, CNTO count per leaf); `crates/plugin/src/equip.rs:668-756` vs `:766-820` (`expand_leveled_loot`); `byroredux/src/cell_loader/references/attach.rs:224-244`; player template `byroredux/src/inventory.rs:391-430`
- **Status**: NEW
- **Description**: The two expanders disagree on the same LVLI.
  - `expand_leveled_form_id`, used for NPC and player inventories:
    - drops the LVLO `count` (each leaf gets the CNTO count);
    - ignores `chance_none`;
    - bounds cycles only by depth.
  - `expand_leveled_loot`, used for containers:
    - multiplies nested LVLO counts;
    - returns nothing at chance-none ≥100;
    - carries a cycle `path`.

  The skill's "drops non-item leaves" half is stale: CCRD, CMNY, IMOD, SLGM and CLOT now enter
  `index.items`, and the measured leaf sets are identical. The count half is live.
- **Evidence**: The engine-parser probe (`/tmp/audit/gameplay/lvlprobe`, both sides evaluated at the
  NPC's own `effective_actor_level`):

  | Master | NPCs with leveled seeds | Corpse ≠ container | Items corpse / container | Container-empty, corpse-not seeds |
  |---|---|---|---|---|
  | FalloutNV | 3,311 | 2,612 | 25,708 / 105,905 | 2 |
  | Fallout3 | 1,169 | 849 | 6,872 / 33,369 | 1 |
  | Skyrim SE | 3,934 | 3,098 | 35,932 / 152,180 | 522 |
  | Fallout4 | 2,014 | 1,605 | 13,601 / 36,850 | 2,080 |
  | Oblivion | 2,092 | 1,449 | 8,215 / 16,144 | 0 |

  Examples:
  - FNV `WithAmmo10mmPistolNPC`: Ammo10mm ×2 on the corpse vs ×16 in a container.
  - Skyrim `LootCitizenPocketsCommon`: Gold ×3 vs ×23.
  - Oblivion `LL2NPCWeaponBossCombArrow100`: Arrow1Iron ×1 vs ×20.
- **Impact**: corpses in every game carry a fraction of the authored ammo, gold and consumables.
  The same list gives different loot depending on whether it sits on an actor or in a chest.
- **Trigger**: loot any NPC whose CNTO/OTFT references an LVLI with LVLO count >1 (most ammo and
  gold lists), in any game.
- **Related**: #3217 (0x02 semantics); #4248 (stale-open, containers now expand);
  ESM-2026-09-21-D2-01 (Oblivion LVLO/LVLD decode, same consumers).
- **Suggested Fix**: build NPC and player inventory rows from `expand_leveled_loot`'s
  `(form_id, count)` pairs. Keep `expand_leveled_form_id` only for picking worn gear, or derive
  equip decisions from the same pairs. Add a corpse-equals-container test on one list.

### GAME-D2-2026-09-21-02: Loose-item pickup has no player input path, and the pickup/tombstone flow has no test
- **Severity**: MEDIUM
- **Dimension**: 2 — Interaction
- **Location**: `byroredux/src/interaction.rs:1121-1194` (`populate_candidates`, no pickup arm), `:703-718` (`InteractionKind` has no `Pickup`); `byroredux/src/inventory.rs:836-845`, `:852-940`
- **Status**: NEW
- **Description**:
  - `container_loot_system` routes a player activation of a loose item to `pickup_loot`. Its doc
    says "a loose world item is picked up directly".
  - But the E-key selector only offers loot sources (non-empty Inventory + `is_loot_source`),
    doors and scripted activators. The P3 plan's `InteractionKind::Pickup` arm
    (`.zcode/plans/plan-sess_4f3ff09b…md` Stage 1 item 4) never landed.
  - Fragment and package `Activate()` events are flushed after `container_loot_system`
    (GAME-D7-2026-09-21-03). The only producer that can reach `pickup_loot` is the
    `script.activate` console command.
  - `pickup_loot`, `is_pickup_target` and `mark_picked_up` have no tests. The #4536 render-skip
    tests insert `PickedUp` by hand.
- **Evidence**: `rg 'pickup_loot|is_pickup_target|mark_picked_up'` finds no test caller.
  `populate_candidates` iterates `Inventory`, `DoorTeleport`, `RumbleOnActivate`,
  `QuestAdvanceOnActivate`, `TwoStateActivator` and `MG07LabyrinthianDoor` only.
- **Impact**: in all games, the player cannot pick up weapons, keys, notes, ammo or any other loose
  item through normal input. A P3 goal ("container/corpse/pickup interaction") is unreachable. It
  is the same tested-but-unwired class as #4464.
- **Trigger**: any cell; aim at a loose item; press Activate. No prompt appears and nothing happens.
- **Related**: #4571 (the pickup render skip; it cites this gap only as mitigation);
  GAME-D7-2026-09-21-01; GAME-D2-2026-09-21-05; #4464.
- **Suggested Fix**:
  - Add a `Pickup` candidate kind for `is_pickup_target` placements, with a "Take" verb and the same
    bound, occlusion and lock gates.
  - Add an input-path test: E on a loose item → inventory row + `PickedUp` + tombstone.

### GAME-D2-2026-09-21-03: `Disable()`d doors and containers stay interactive — an invisible working door and a lootable invisible chest
- **Severity**: MEDIUM
- **Dimension**: 2 — Interaction
- **Location**: `byroredux/src/cell_loader/spawn.rs:648-692` (`spawn_placement_root` stamps the teleport and lock before the #3278 disabled gate returns); `byroredux/src/cell_loader/references/synth_child.rs:872` (`attach_container_inventory` unconditional); `byroredux/src/interaction.rs:1146-1152`, `:1262-1277`
- **Status**: NEW (residual of closed #3278)
- **Description**:
  - The #3278 consumer skips meshes, colliders and lights for a disabled REFR, "deliberately"
    keeping the placement root with its `FormIdComponent`, `DoorTeleport` and `Locked` payloads.
  - `populate_candidates` adds every `DoorTeleport` holder, and every loot source with a non-empty
    `Inventory`.
  - `attach_container_inventory` runs for the disabled container too.
  - `interaction_bound` falls back to the root's `GlobalTransform` with a 24 BU sphere. With no
    collider in the way, line of sight passes.
- **Evidence**: `spawn_placed_instances` calls `spawn_placement_root(…, teleport, lock)` at
  `spawn.rs:648` and only afterwards returns early on `placement_is_disabled` (`:684`).
  `synth_child.rs:872` has no `placement_disabled` guard.
- **Impact**: a door disabled by a quest fragment still shows "[E] Open" at its old spot and still
  performs the cell transition. A disabled container can still be looted. #3278's own Impact line
  listed "interactive" among the symptoms it closed.
- **Trigger**: Skyrim/FO4 cells whose references a quest fragment `Disable()`d (saved
  `ReferenceEnableState`), after a reload or revisit.
- **Related**: #3278; #4327 (the actor, trigger and light branches of the same gate).
- **Suggested Fix**: do not insert `DoorTeleport` and skip `attach_container_inventory` for disabled
  placements, or have `populate_candidates` reject entities whose placement is disabled. Pin it with
  a disabled-door candidate test.

### GAME-D2-2026-09-21-04: The P3 theft rule compares ownership to PlayerRef `0x14` (never authored), ignores cell ownership, and the player carries no base factions
- **Severity**: MEDIUM
- **Dimension**: 2 — Containers & Loot
- **Location**: `byroredux/src/inventory.rs:569-590` (`transfer_is_theft`); `byroredux/src/cell_loader/references/synth_child.rs:836-848` (`Owned` stamped from REFR XOWN only); `crates/core/src/ecs/components/owned.rs:7-21`; `byroredux/src/inventory.rs:501-537` (`attach_to_player`: no `FactionRanks`)
- **Status**: NEW
- **Description**: The rule counts a transfer as theft when `owner != player SceneAliasCandidate.reference_form_id (0x14)` and the player has no rank in the owner. Three problems follow:
  1. XOWN names NPC_ or FACT bases, and vanilla authors player ownership as the Player NPC_
     `0x00000007`.
  2. REFR ownership overrides cell ownership (the parser's own docs, `cell/mod.rs:286-297`,
     `:521-527`), but `CellData.ownership` is never stamped. Every container and item that
     inherits only cell ownership is never theft.
  3. `attach_to_player` stamps no `FactionRanks` from the Player record, so the faction exemption
     works only for alias-injected factions.
- **Evidence**: census with the engine parser (`owners` probe):

  | Master | XOWN = 0x7 refs | XOWN = 0x14 refs | CONT own-XOWN / only-cell-owned | item refs only-cell-owned |
  |---|---|---|---|---|
  | FNV | 36 | 0 | 769 / 2,077 | 8,971 |
  | FO3 | 76 | 0 | 377 / 502 | 3,204 |
  | Skyrim SE | 260 (49 are containers) | 0 | 762 / 4,058 | 25,238 |
  | Oblivion | 189 | 0 | 494 / 9,097 | 41,332 |
  | FO4 | 87 | 0 | 619 / 643 | 2,264 |

  Player NPC_ `0x7` authors 22 factions on FNV (incl. PlayerFaction `0x1B2A4`) and 4 on Skyrim.
  None is stamped.
- **Impact**:
  - Looting the player's own property says "Stolen N items" (all 49 Player-owned Skyrim containers,
    e.g. `IvarsteadFellstarFarm`).
  - Emptying a shop or house whose ownership sits on the cell says "Took".
  - `ItemTransfer.stolen` carries the same wrong value. It has no reader today (GAME-D7-2026-09-21-04).
- **Trigger**: any game; loot a player-owned container, or a container in a cell-owned interior.
- **Related**: #692 (ownership parse); GAME-D5-2026-09-21-01 (`0x14` vs `0x7`); CHAR-D4-2026-09-21-03
  (player half-populated, concurrent /audit-character).
- **Suggested Fix**:
  - Compare against the player's base `0x7` (or its `SceneAliasCandidate.base_form_id`).
  - Stamp `Owned` from `CellData.ownership` when the REFR has none, honoring the `XRNK` rank bar.
  - Seed the player's `FactionRanks` from the base record beside its `ActorValues`.

### GAME-D4-2026-09-21-02: The P2 combat tail (Draugr attack/hit/death takes, impact sound, death voice) never fires — its marker is only inserted on the Oblivion/FO3/FNV spawn path
- **Severity**: MEDIUM
- **Dimension**: 4 — Combat & Death
- **Location**: `byroredux/src/npc_spawn/resumable.rs:641-643` + `:1079-1084` (runtime path only); prebaked finalize `:1547-1601` (no insert); `byroredux/src/cell_loader/load.rs:636-640`, `:1305-1327` (the #4551 fix and pin)
- **Status**: NEW
- **Description**:
  - `DraugrCombatAnim` is inserted only in `advance_runtime_unit`'s Finalize, for
    `RuntimeNpcState.combat_anim_draugr`, which is set when the race editor ID contains "draugr".
  - The runtime-FaceGen path serves only `has_runtime_facegen_recipe()` games: Oblivion and
    Fallout3NV.
  - Draugr are a Skyrim race, and every Skyrim actor spawns through the prebaked path, whose
    Finalize never inserts the marker.
  - `combat_feedback_system` gates every take and the impact/death sounds on that marker. Only the
    player swing sound, which is keyed on `CombatState.attacks_started`, can fire.
  - #4551 (closed) installed the clip resource on the `--cell` route. Its comment and pin assume
    "the spawn finalize inserts the DraugrCombatAnim marker regardless", which is false for Skyrim.
- **Evidence**: `rg 'DraugrCombatAnim::default'` has a single production hit, `resumable.rs:1082`,
  inside `RuntimePhase::Finalize`. Every one of the nine `combat_anim` tests inserts the marker by
  hand (`spawn_actor`).
- **Impact**: the P2 "one attack/hit/death animation family and spatial sound family" goal is dead
  in production on its only target content, the frozen Skyrim fixture `000383F7`. The planned
  `p2-combat-feel` assertion ("death take actually started") cannot pass.
- **Trigger**: Skyrim `BleakFallsBarrow01` P2 fixture; hit or kill the Draugr.
- **Related**: #4551; GAME-D4-2026-09-21-05 (latent defects this fix would expose);
  GAME-D7-2026-09-21-02 (the marker is also unclassified for saves).
- **Suggested Fix**: compute `combat_anim_draugr` from the resolved race in `prepare_prebaked_state`
  and insert the marker in the prebaked Finalize beside `AnimationTarget`. Add a spawn test that
  asserts the marker on a Skyrim-shaped Draugr.

### GAME-D4-2026-09-21-03: A `Dead` player still attacks, opens doors, loots and changes equipment
- **Severity**: MEDIUM
- **Dimension**: 4 — Combat & Death
- **Location**: `byroredux/src/combat.rs:106-244` (`combat_input_system`: no aggressor `Dead` check); `byroredux/src/interaction.rs:779-794` (`interaction_system`), `:1301-1347` (`activate_target`); `byroredux/src/inventory.rs:820-846` (`container_loot_system`), `:1140-1240` (`apply_action` ToggleEquip)
- **Status**: NEW
- **Description**:
  - Player death is live: drowning (`systems/character.rs:1399-1419`), NPC combat strikes, and
    zero health through water hazards.
  - After death, `player_controller_system` stops movement (`character.rs:160`) and `consume_item`
    refuses.
  - Attack edges still produce `HitEvent`s. Activate still emits `ActivateEvent`s and queues door
    transitions. Container and corpse take-all and equip toggles still mutate state.
- **Evidence**: none of the four entry points reads `Dead` on the player; `rg 'Dead' combat.rs`
  hits only target checks.
- **Impact**: a drowned or killed player can keep killing NPCs, loot, and walk through doors into a
  new cell while dead. There is no game-over or reload flow to contain it.
- **Trigger**: FNV Lake Mead (drowning) or MQ101 combat on Skyrim; die; press Attack or Activate.
- **Related**: #3119 (water death reconcile, closed).
- **Suggested Fix**: add one `player_can_act(world)` gate (not `Dead`, character mode) used by
  `combat_input_system`, `activate_target`, `container_loot_system` and `apply_action`.

### GAME-D4-2026-09-21-04: SDK actor-value batches can drop Health to ≤ 0 without the death transition — a second Health writer outside `combat_damage_system`
- **Severity**: MEDIUM
- **Dimension**: 4 — Combat & Death (single damage path)
- **Location**: `byroredux/src/extensions/commands.rs:393-478` (`apply_pending_actor_value_writes`: `Damage`, `SetBase`, `ModifyPermanent`)
- **Status**: NEW
- **Description**:
  - The extension host commits `ActorValueOperation::Damage` (and base/permanent reductions)
    straight into `ActorValues`.
  - It validates finiteness only. It never checks the Health AV's resulting current value, inserts
    `Dead`, or queues `reconcile_dead_actor`.
- **Evidence**: the commit loop ends with `*target = values;` and returns `Ok(())`. There is no
  `Dead` or `queue_dead_actor_reconciliation` reference in the file.
- **Impact**: an extension that damages an actor to zero leaves it alive. Its AI keeps running, it is
  not lootable, and it cannot ragdoll until an unrelated `HitEvent` arrives. For the player, the
  same gap means no death at 0 HP.
- **Trigger**: any loaded SDK extension issuing a Health `Damage` command.
- **Related**: GAME-D4-2026-09-21-03; #3119; `/audit-tooling` owns the SDK surface.
- **Suggested Fix**: after commit, for each touched entity whose Health AV (`ActorVitals.health`)
  current value is ≤ 0 and which is not `Dead`, insert `Dead` and call
  `queue_dead_actor_reconciliation`. That is the water-damage pattern.

### GAME-D5-2026-09-21-02: `StartCombat` does not suspend the ambient package — two movers drive one actor, and a seated actor chases in its sit pose
- **Severity**: MEDIUM
- **Dimension**: 5 — Locomotion
- **Location**: `byroredux/src/systems/combat_ai.rs:101-129` (Update-stage chase), `:207-216`; the six PostUpdate movers, e.g. `systems/wander.rs:297-433`; `systems/sandbox.rs` (one-shot seat snap); `systems/walk_anim.rs:124-126` (Seated gate)
- **Status**: NEW
- **Description**:
  - `npc_combat_ai_system` moves an `AiCombatState` actor toward its target in Update.
  - None of the seven ambient systems reads `AiCombatState`, so the actor's Wander, Travel, Follow,
    Escort, Guard or Patrol mover also moves it in PostUpdate, every frame.
  - `sandbox_seat_system` never re-snaps, and `walk_anim` refuses to take over `Seated` actors. A
    seated patron who is `StartCombat`ed slides to the target in the sit pose, and keeps
    `Seated` and its seat reservation.
  - The "one behavior per actor" contract (`AmbientBehavior`) does not account for combat.
- **Evidence**: `rg AiCombatState byroredux/src/systems` finds only `combat_ai.rs`.
- **Impact**: actors in scripted combat jitter between the chase and their package leg, drift, or
  chase while seated.
- **Trigger**: any `StartCombat` fragment on an actor with an ambient package (Skyrim MQ101).
- **Related**: GAME-D4-2026-09-21-01; #4414.
- **Suggested Fix**: have the movers skip `AiCombatState` actors, or have `StartCombat`
  `clear_ambient_behavior` and let `ambient_ai_package_system` re-select when combat ends. Unseat
  on combat start.

### GAME-D5-2026-09-21-03: Quest-alias package overlays are never evaluated for actors whose own resolved package stack is empty
- **Severity**: MEDIUM
- **Dimension**: 5 — AI Package Selection
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:539-541` (early return, no `AmbientPackageRuntime`), `:600-608` (the ambient system iterates runtime holders only), `:658-673` (the only overlay consumer)
- **Status**: NEW
- **Description**:
  - `apply_ai_package_behavior` returns before inserting `AmbientPackageRuntime` when the
    TPLT-resolved `ai_packages` list is empty.
  - `ambient_ai_package_system` only visits entities that carry that runtime, and it is the only
    reader of `QuestAliasInjectedOverlays` packages.
  - An actor whose only packages come from a quest alias (ALPC) therefore never runs them.
- **Evidence**: the `aliaspk` probe on `Skyrim.esm` counts 2,131 aliases with ALPC. 172 are
  forced-reference aliases on NPC_; 44 of those target an actor with an empty resolved PKID stack,
  and 19 of those alias packages resolve to a behavior the engine supports (Sandbox/Patrol tree
  leaf, or an FO3-style procedure). Examples: `dunUstengravQST` entrance warlocks and bandits,
  `MG03CallerAlias`. FO4 has 1 of 2.
- **Impact**: quest-assigned sandboxing or patrolling never starts for those actors; they stand
  idle.
- **Trigger**: Skyrim quests whose alias injects a package onto a package-less actor (e.g.
  `dunUstengravQST` after it starts).
- **Related**: skill known-open: tree packages resolve only Sandbox/Patrol leaves.
- **Suggested Fix**: always insert `AmbientPackageRuntime` for spawned actors (empty candidate
  list), or insert it when an alias overlay lands, so overlay packages go through the same
  selection.

### GAME-D7-2026-09-21-02: The save-registry completeness guard reads only up to each file's first `#[cfg(test)]` — 39 production types are invisible, 5 of them unclassified
- **Severity**: MEDIUM
- **Dimension**: 7 — State Coverage
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:521` (`src.split("#[cfg(test)]").next()`); `byroredux/src/components.rs:663` (mid-file `#[cfg(test)] mod region_ambient_res_tests`)
- **Status**: NEW
- **Description**:
  - The guard assumes test modules sit at file tails. Several files gate an item mid-file:
    - `components.rs:663` opens a test module;
    - `load_order.rs:53` and `skinned_mesh.rs:100` hold test-only fields or fns;
    - `save_io.rs:272` and `extensions/systems.rs:32` also gate items mid-file.
  - Every `impl Component/Resource` after such a line is never classified.
  - The 34 hidden types that are allowlisted are there only because authors added rows
    voluntarily.
- **Evidence**: `/tmp/audit/gameplay/hidden_types.py` strips only test-gated items (brace-matched)
  and finds 39 hidden production impls: 32 in `components.rs`, 1 each in `load_order.rs`,
  `nif_import_registry.rs`, `extensions/systems.rs`, `scene_import_cache.rs` and
  `skinned_mesh.rs`, and 2 in `save_io.rs`. Five are neither registered nor allowlisted:
  - `DraugrCombatAnim` (`components.rs:2053`)
  - `DraugrCombatClips` (`:2023`)
  - `ExposureTuning` (`:1826`)
  - `NavmeshResidency` (`:2134`)
  - `GlobalFormIdResolver` (`cell_loader/load_order.rs:293`)
- **Impact**: the SAVE-D1-12 guard, whose purpose is to catch unclassified gameplay state, is green
  while blind. New gameplay components added to `components.rs` land without a save decision. That
  is exactly how `DraugrCombatAnim`'s latch (GAME-D4-2026-09-21-05) went unexamined.
- **Trigger**: any new `Component`/`Resource` declared below `components.rs:663` (or the other
  cut points).
- **Related**: #2295 / #3166 / #3497 (the guard's history); ECS-2026-09-21-D5-01 (a sibling
  guard-precision gap).
- **Suggested Fix**:
  - Strip `#[cfg(test)]` items by brace matching, the way `hidden_types.py` does, instead of
    truncating the file.
  - Classify the five types.
  - Add a self-test with a mid-file test module.

### LOW

### GAME-D2-2026-09-21-05: Pickup always grants one item — the REFR item count (`XCNT`) is not decoded
- **Severity**: LOW
- **Dimension**: 2 — Pickup
- **Location**: `byroredux/src/inventory.rs:908` (`ItemStack::new(base, 1)`); no `XCNT` arm anywhere in `crates/plugin/src`
- **Status**: NEW
- **Description**:
  - xEdit defines REFR `XCNT` as "Count" (TES4 `itU32`, `wbDefinitionsTES4.pas:3200`) and "Item
    Count" (TES5 `itS32`, `wbDefinitionsTES5.pas:9833`; FNV `:3144`).
  - The parser drops it, and `pickup_loot` hard-codes 1 ("one item per placement, the Bethesda REFR
    convention").
- **Evidence**: raw census (`/tmp/audit/gameplay/xcnt_values.py`): FNV 33, FO3 186, Skyrim 118,
  Oblivion 29 and FO4 97 placed stacks. Every value is 2–50, e.g. FO3 has 47 stacks of 12.
- **Impact**: a placed stack of 20 arrows or 12 rounds would yield 1 (latent while pickup is
  unreachable, GAME-D2-2026-09-21-02).
- **Trigger**: pick up an `XCNT` placement (console path today).
- **Related**: GAME-D2-2026-09-21-02; `/audit-esm` owns the decode half.
- **Suggested Fix**: decode `XCNT` into `PlacedRef.item_count` and use `count.max(1)` in
  `pickup_loot`.

### GAME-D3-2026-09-21-01: A refused native inventory action is silent to the player
- **Severity**: LOW
- **Dimension**: 3 — Consumables
- **Location**: `byroredux/src/main.rs:1266-1270`
- **Status**: NEW
- **Description**: `MutationResult::Unavailable` from Use or Equip reaches only
  `log::warn!("native inventory action was unavailable…")`, while success pushes "Used {name}".
  Refusals do happen at use time: all branches' conditions false, the player lacking the AV, health
  ≤ 0, or the item equipped. They look like a dead button — the shape that hid #4458.
- **Evidence**: `if inventory::apply_action(world, action) == inventory::MutationResult::Unavailable { log::warn!(…) }`
- **Impact**: the player cannot tell why a shown Use/Equip did nothing.
- **Trigger**: FO3/FNV, use an item whose authored branches are all false for the current
  perk/Hardcore state.
- **Related**: #4458.
- **Suggested Fix**: return a reason from `consume_item`/`apply_action`, and push a short
  `PlayerNotifications` line ("Can't use {name} now").

### GAME-D4-2026-09-21-05: (latent) The death take re-inserts an `AnimationPlayer` over the ragdoll, and its once-only latch replays on every respawn and reload
- **Severity**: LOW — latent until GAME-D4-2026-09-21-02 is fixed; must be fixed with it
- **Dimension**: 4 — Death reconciled once
- **Location**: `byroredux/src/systems/combat_anim.rs:158-173`, `:249-272`; `byroredux/src/combat.rs:553-562`; `byroredux/src/npc_spawn/resumable.rs:1079-1084`; `docs/engine/p2-combat-anim-sound-fixture.md:85-90`
- **Status**: NEW
- **Description**:
  - (a) Same frame as the kill: `reconcile_dead_actor` removes the actor's and the skeleton's
    `AnimationPlayer`s (the #3022 teardown) and activates the ragdoll. In PostUpdate,
    `combat_feedback_system` finds no player and inserts `AnimationPlayer::new(death).with_root(skeleton)`.
    The fixture doc asks the take to "mirror, not bypass" walk_anim's Dead guard.
  - (b) `death_played` lives on `DraugrCombatAnim`, which is unsaved and unclassified
    (GAME-D7-2026-09-21-02) and re-inserted as `default()` at every spawn. A respawned or reloaded
    corpse, restored `Dead` through `reference_state::restore` or the save overlay, gets a fresh
    death take and death voice.
- **Evidence**: the death decision fires on `Dead && !death_played`. The spawn inserts
  `DraugrCombatAnim::default()`. Save rows record `Dead` only.
- **Impact**: once wired, every reload or revisit of a Draugr crypt replays each corpse's death
  scream, and the death clip samples over ragdolled skeletons indefinitely.
- **Trigger**: after the D4-02 fix: kill a Draugr, save, load (or leave and re-enter the cell).
- **Related**: GAME-D4-2026-09-21-02; #3022; #3708.
- **Suggested Fix**: derive the latch from `Dead` at spawn and restore (insert with
  `death_played: true` when the actor is already dead). Either play the death clip before
  `activate_ragdoll`, with ragdoll activation deferred until it ends, or skip the clip when a ragdoll
  exists.

### GAME-D4-2026-09-21-06: `BYRO_NO_AI_LOCOMOTION` also unregisters combat feedback
- **Severity**: LOW
- **Dimension**: 4/5 — kill-switch scope
- **Location**: `byroredux/src/boot/schedule/post_update.rs:90-141`; pin `boot/schedule/mod.rs` `locomotion_registers_behind_the_single_kill_switch`
- **Status**: NEW (pointer raised by ECS-2026-09-21)
- **Description**: `make_combat_feedback_system` (combat takes plus swing, impact and death sounds)
  is registered inside the `if locomotion_enabled` block, which is documented as the AI-motion
  debugging switch. The pin checks only the env var and the absence of the old gates, not block
  membership.
- **Evidence**: `scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_combat_feedback_system())`
  sits at `:137` inside the block that opens at `:91`.
- **Impact**: isolating AI motion with the switch silently removes P2 combat feedback, so a P2
  feel gate would fail for an unrelated reason.
- **Trigger**: `BYRO_NO_AI_LOCOMOTION=1` on any combat route.
- **Related**: ECS-2026-09-21 cross-audit pointer.
- **Suggested Fix**: register combat feedback after the block, still after `walk_anim` when it is
  present, and extend the pin to name the gated set.

### GAME-D5-2026-09-21-04: `walk_anim` module docs still describe fixed-speed 100 u/s movement and "never seated and walking"
- **Severity**: LOW
- **Dimension**: 5 — doc
- **Location**: `byroredux/src/systems/walk_anim.rs:34-44`, `:54-58`
- **Status**: NEW
- **Description**: The doc says movement is "the locomotion systems' fixed-speed XZ step, so the
  clip's authored stride can disagree slightly with 100 u/s". Since M42.11 the speed is the clip's
  own stride (`WalkSpeed`). The doc also says "Actors are never both seated and walking", which
  combat breaks (GAME-D5-2026-09-21-02). The `WALK_ANIM_MIN_SPEED` doc repeats "the locomotion walk
  is 100 u/s".
- **Evidence**: see the lines above; `WalkSpeed` has been stamped on both spawn paths since
  774560dce.
- **Impact**: misleading contract text on the take/restore thresholds.
- **Trigger**: n/a (doc).
- **Related**: skill D5 note; GAME-D5-2026-09-21-02.
- **Suggested Fix**: restate both in terms of `WalkSpeed` and the combat chase.

### GAME-D6-2026-09-21-01: `inventory.status` reports a weapon damage that combat does not apply
- **Severity**: LOW
- **Dimension**: 6 — Player feedback (debug frontend)
- **Location**: `byroredux/src/commands/gameplay.rs:80-90`
- **Status**: NEW
- **Description**: The command ("the live player loadout used by combat") prints
  `EquippedWeapon.damage`. Combat applies `attack_damage` = weapon +
  `melee_damage_charal_bonus` (FO3/FNV STR × 0.5), which is active for the player since #4458. That
  is a second, divergent computation behind a debug frontend.
- **Evidence**: `format!(… damage={:.1} source=weapon", …, weapon.damage)` vs `combat.rs:412`.
- **Impact**: operators and smokes read a number combat never deals.
- **Trigger**: FO3/FNV, player with a melee weapon and STR > 0.
- **Related**: #3092; CHAR-D4-2026-09-21-01.
- **Suggested Fix**: print `crate::combat::attack_damage(world, player)` (and reach/cooldown from the
  same helpers).

### GAME-D7-2026-09-21-03: Fragment and package `Activate()` never reaches `container_loot_system` — the flush runs after that consumer
- **Severity**: LOW
- **Dimension**: 7 — Stage order
- **Location**: `byroredux/src/boot/schedule/update.rs:145-160` (`container_loot_system`) vs `:236-239` (`fragment_activation_flush_system`); pin `boot/schedule/mod.rs:61-94`
- **Status**: NEW
- **Description**: The #2654 design flushes queued fragment and package activations "so a fragment
  activation reaches every consumer exactly once on the following frame". The flush is registered
  after `container_loot_system`, and `event_cleanup_system` drains the marker in the same frame's
  Late. A scripted `ContainerRef.Activate(PlayerRef)` or loose-item `Activate` therefore never
  loots or picks up. The pin lists three consumers: it omits this one, and mg07 (open #4116).
- **Evidence**: registration order at `update.rs:145` and `:236`; pin consumer list at `mod.rs:75-79`.
- **Impact**: a scripted container or pickup activation for the player is a no-op. Combined with
  GAME-D2-2026-09-21-02, the console is the only road into `pickup_loot`.
- **Trigger**: any fragment or package `Activate` on a container or item with the player as
  activator.
- **Related**: #4116, #2654, GAME-D2-2026-09-21-02.
- **Suggested Fix**: move the flush ahead of `container_loot_system` (right after
  `interaction_system`), and add both missing consumers to the pin.

### GAME-D7-2026-09-21-04: `ItemEventBatch` is written and drained but read by nothing; its doc names consumers that don't exist
- **Severity**: LOW
- **Dimension**: 7 — written-never-read
- **Location**: `crates/scripting/src/events.rs:208-221`; producers `crates/scripting/src/equipment.rs:74-91` (from `transfer_loot` and `pickup_loot`); `crates/scripting/src/cleanup.rs:110`
- **Status**: NEW
- **Description**:
  - The P3 item-transfer rows (`item_form_id`, `count`, `added`, `stolen`) are emitted on both
    sides of every loot and pickup, then drained at Late.
  - No system reads them. The Papyrus provider and extension dispatchers read
    `EquipmentEventBatch` only, and notifications are pushed directly.
  - The doc says "notification UI, quest fragments, and future crime systems all observe it".
  - `consume_item` emits no row, although the P3 plan listed it.
- **Evidence**: `rg ItemEventBatch` shows definition, emit, cleanup and tests only.
- **Impact**: dead plumbing presented as a live script event. `OnItemAdded`/`OnContainerChanged`
  style quest logic cannot observe loot. The skill names this defect class (compare
  `FactionRelations`, #4414).
- **Trigger**: n/a.
- **Related**: #4414, #4464.
- **Suggested Fix**: wire a consumer, such as a Papyrus provider event or extension dispatch, or cut
  the doc to "no consumer yet" with a tracking issue. Emit from `consume_item` for symmetry.

### GAME-D7-2026-09-21-05: `armor_covers_main_body` / `main_body_bit` are test-only but documented as used by the spawn pipeline
- **Severity**: LOW
- **Dimension**: 7 — tested-but-unwired
- **Location**: `crates/plugin/src/equip.rs:101-137`
- **Status**: NEW
- **Description**: The doc says "Used by the spawn pipeline to skip the base-body NIF … when an
  equipped armor's mesh already covers the torso". No non-test caller exists anywhere. The spawn
  path now uses the race-skin displacement mask (`npc_spawn.rs:1263-1327`). `main_body_bit` (with
  its #4074 PROVISIONAL FO76/Starfield arm) exists only to feed it.
- **Evidence**: `/tmp/audit/gameplay/unwired.py` (strips test items) finds the only uses at
  `equip.rs:833-863` (tests).
- **Impact**: stale API and doc, and a provisional per-game bit table that nothing consumes.
- **Trigger**: n/a.
- **Related**: #4074.
- **Suggested Fix**: delete both functions and their tests, or correct the doc and name the
  consumer that needs them.

## Cross-audit citations (not re-reported here)

- **CHAR-D1-2026-09-21-01** (MEDIUM, concurrent `/audit-character`, not yet filed): the vanilla `--hud` bars read the first `ActorValues` holder rather than `PlayerEntity`, and FO3, FNV and FO4 key on synthetic test FormIDs. This audit independently found the storage-order half (`hud.rs:690-717`, used at `hud.rs:682` and `scaleform_hud.rs:347`). Extra evidence for that issue: cell NPCs receive `ActorValues` during `load_scene_content`, before `spawn_player_body` → `attach_to_player` stamps the player's, so SparseSet dense order puts an NPC first in any populated start cell.
- **CHAR-D4-2026-09-21-03** (LOW, concurrent): the player lacks `CharacterLevel` and `Background`. It is also why container leveled loot always resolves at level 1 (`attach.rs:220-222`). GAME-D2-2026-09-21-04's missing player `FactionRanks` has the same root cause.
- **#4571** (ECS-2026-09-21-D7-01): `PickedUp` render skip on the mesh vs the root. **#4574**: under-declared exclusive access, including `container_loot_system` / `combat_input_system`.
- **#4605** (CONC-D3-2026-09-21-02): `combat_anim` clips guard and the `combat_ai` hold stack.
- **PERF-D1-2026-09-21-04/05** (not yet filed): per-frame HUD objective rebuild and per-frame combat allocations.
- **ESM-2026-09-21-D2-01** (not yet filed): Oblivion 8-byte LVLO / LVLD 0x80 empties 18 LVLIs in both resolvers, the ones in GAME-D2-2026-09-21-01.
- **`/audit-scripting` pointer**: `TwoStateActivator` state is a delta column for resident saves but has no `ReferenceState` row or ledger, so an opened gate or lever reverts on eviction (same class as open #4334).

## Known-Open Register (verified today; cite, don't re-file)

| Issue | State at HEAD |
|---|---|
| #4414 `FactionRelations` never read | Still true. The only binary mention is a doc comment in `combat_ai.rs`; hostility starts only via `StartCombat` |
| #4232 `effective_actor_level` returns 0 | Still true (`crates/plugin/src/esm/records/actor/mod.rs:96-102`, `npc.level.max(0)`) |
| #4248 containers never expand LVLI | **Stale-open.** `attach_container_inventory` calls `expand_leveled_loot` (`attach.rs:233-243`); recommend closing |
| #4116 activation-order pin omits mg07 | Still open. GAME-D7-2026-09-21-03 adds `container_loot_system` as a second omitted consumer |
| 10 of ~17 PACK procedures without runtime; FO4+ walk clips absent; cross-tile pathing blocked | Unchanged; findings above are misroutes of supported cases only |

## Dimension notes (no-finding checks)

- **Dim 1**: `InventoryCatalog`, the two player templates and `PlayerVitals` are written only by
  `install_catalog`, which runs on both loader routes. The player template is copied once, after
  content load. `ItemInstancePool::allocate` is called in production only from
  `reference_state::restore`. `consume_item` preflights instance liveness. Armor toggles never
  touch `EquippedWeapon`, and both NPC spawn paths mirror the weapon into `EquipmentSlots.weapon`.
- **Dim 2**: `PlayerEntity` and `PapyrusPlayerEntity` are written in production only at
  `scene.rs:1153/1168`, both to the same body; the fly-cam branch uses `None` plus an inventory-less
  placeholder. They cannot diverge. Capture runs before instance release. NPC restore runs after the
  authored inventory is attached. `ReferenceState` has no `serde(default)`. `NpcLootAppearance`
  never strips a living actor. `lock_level` has no consumer (documented lockpicking deferral).
- **Dim 3**: the explicit `--ignored` run of
  `real_stimpaks_restore_scaled_health_and_limbs_but_hardcore_only_health` passed. The AV map
  (Skyrim 24/25/26, FO3/NV 16/12) matches the GECK/UESP indices. Hardcore is a CTDA only.
- **Dim 5**: the four-way diff (`from_package` chain, `insert_at_spawn`, `insert_at_runtime`,
  `clear_ambient_behavior`, debug-server registration) agrees for all seven procedures. Only
  `BYRO_NO_AI_LOCOMOTION` is read. Stuck-repick applies to Wander and Patrol only.
  `prune_seat_reservations` keeps furniture and claimant liveness.
- **Dim 6**: notifications drain every frame, and a `--hud` run still displays them (`debug_ui` is
  built whenever a window exists). The loading-screen Phase × Owner machine is covered by six
  passing tests.

## Test verification

| Command | Result |
|---|---|
| `cargo test -j4 -p byroredux --bin byroredux -- combat:: inventory:: interaction:: npc_spawn:: reference_state ambient_locomotion walk_anim save_io::` (skill baseline) | 239 passed, 0 failed, 17 ignored (all installed-data) |
| `cargo test -j4 -p byroredux-core --features inspect --lib no_armor_mask_can_reach_the_weapon_slot` | 1 passed |
| `… --bin byroredux -- reference_state scripted_lock_gate synth_child interaction:: loot_appearance` | 48 passed |
| `… --bin byroredux -- consum restoration timed_` | 25 passed, 4 ignored |
| `cargo test -j4 -p byroredux-plugin --lib consumables` | 9 passed |
| `BYROREDUX_REQUIRE_GAME_DATA=1 … -- --ignored --exact inventory::tests::real_stimpaks_restore_scaled_health_and_limbs_but_hardcore_only_health` | PASS (3.6 s, 1.09 GB RSS) |
| `… --bin byroredux -- ambient_locomotion npc_spawn::ai_package walk_anim locomotion::` | 28 passed, 1 ignored |
| `… --bin byroredux -- notifications loading_screen spawn_tests` | 30 passed |

**Probes** (all read-only, outside the repo):
- `/tmp/audit/gameplay/lvlprobe`, a crate linking `crates/plugin`. Binaries: `lvlprobe` (leveled
  corpse/container), `owners` (XOWN census), `aliaspk` (alias packages on package-less actors),
  `playerpk` (PlayerRef-targeted packages), `playerref` (PlayerRef placements). Target dir:
  `/mnt/data/tmp/gameplay-audit-target`.
- `/tmp/audit/gameplay/xcnt_values.py` and `/tmp/audit/esm/subcensus.py` (REFR `XCNT`).
- `/tmp/audit/gameplay/hidden_types.py` (save-guard blind spot) and `/tmp/audit/gameplay/unwired.py`
  (pub entries with no production use).
- The skill's Phase 4 `rm -rf /tmp/audit/gameplay` was skipped: the suite orchestrator needs the
  per-dimension scratch files.

## Deduplication

Every finding was checked against `/tmp/audit/issues.json` (4,493 issues, refreshed 21:24 with the
suite's #4592–#4605), against `docs/audits/`, and against today's sibling reports and scratch
(ECS, CONCURRENCY, PERFORMANCE, ESM, SAFETY, PARSERS, NIF, NIFAL, RENDERER, CHARACTER):
- Closed issues re-examined: #3278 (Disable consumer; the interactive half is unresolved, see
  GAME-D2-2026-09-21-03), #4551 (clips installed; the marker premise is false, see
  GAME-D4-2026-09-21-02), #4464, #4465, #4458, #3488, #3708, #3112. Their fixes are in place.
- The one overlap with a concurrent leg (the HUD bar source, CHAR-D1-2026-09-21-01) is cited rather
  than filed.
- No reports predating 2026-06-07 were relied on.

Next step: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-09-21.md`. Labels: `gameplay` plus `ai` /
`combat` / `inventory` / `save-load` as fitting; `game:*` only for title-specific findings.
