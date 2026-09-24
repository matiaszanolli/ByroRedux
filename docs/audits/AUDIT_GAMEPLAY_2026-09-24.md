# Gameplay Audit — 2026-09-24

**HEAD**: `aabd99a05` · **Baseline**: `AUDIT_GAMEPLAY_2026-09-21.md` (`f97775ca8`) · **Audited**: Dim 1–7 · **Unchanged since baseline (skimmed)**: none. Every dimension's paths moved: 163 commits, including fix commits for 13 of the baseline's 22 findings. Two new gameplay features also landed today: faction hostility (#4414) and the magic runtime (#4415).

**Scope**: the default `/audit-gameplay` scope, delta-first. Three agents did the work: Dims 1–3, Dims 4–5 and Dims 6–7. Every HIGH and MEDIUM finding below was re-checked against the code by the merge pass before it was kept. Two findings were raised independently by two legs and are merged here:
- the `SpellList` eviction gap;
- the record-header flag gap, split into Initially Disabled and Starts Dead.

**Games and cells exercised**: no engine was launched. The evidence comes from:
- the binary, plugin and scripting test suites;
- six `#[ignore]`d installed-data tests, run explicitly;
- the real-ESM AIDT decode test;
- raw-ESM Python censuses over `FalloutNV.esm`, `Fallout3.esm`, `Skyrim.esm` (SE), `Oblivion.esm` and `Fallout4.esm`, checked against the engine parser's own counts (the AIDT with-attack-radius counts match exactly);
- one load-order probe test, run in a `git archive` copy of HEAD.

## Executive Summary

| Severity | NEW | Regression | Existing (open) re-confirmed |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 3 | 0 | 1 (#4695) |
| MEDIUM | 5 | 0 | 2 (#4696, #4232) |
| LOW | 7 | 0 | 8 (#4709–#4714, #4742, #4116) |

**Fixes verified.** Every fix commit since the baseline was re-checked: #4571, #4574, #4605, #4613, #4693, #4694, #4697, #4699, #4700–#4708, #4675, #4678, #4674, #4679, #4452 and #4414. Each is correct as far as it goes. Two are incomplete:
- **#4698** (disabled refs not interactive) covers script-disabled refs only.
- **#4457** (TPLT hoist) left two raw-shell reads in place.

**The three HIGH findings:**

- **GAME-D1-2026-09-24-01: templated Skyrim NPCs lose their outfit.** `build_npc_equip_state` reads the default outfit (`DOFT`) and the leveled-expansion level from the placed record instead of its template. As a result, **2,716 Skyrim placed actors spawn without their outfit**: leveled bandits, draugr, Imperial guards and Civil War siege soldiers.
- **GAME-D5-2026-09-24-01: Initially Disabled refs spawn live.** The record-header flag (0x800) is never decoded, so every quest-gated reference spawns live and interactive:
  - items, containers and doors (quest items and keys are takeable early);
  - actors, which since #4414 **start combat**. That is 935 FNV, 212 Skyrim and 579 FO4 aggressive refs with no enable parent.
- **GAME-D4-2026-09-24-01: authored corpses stand up.** The Starts Dead ACHR flag (0x200) is never decoded, so 1,123 Skyrim and 1,178 FO4 authored corpses spawn alive. They cannot be looted, and hostile ones attack. Bleak Falls Barrow, on the MQ102 critical path, has 19.

**What changed in the risk picture.** #4414 turned passive parser gaps into active gameplay faults. Before today, a wrongly-live actor stood idle; now faction hostility arms it. The same feature also brought a lifecycle gap: once combat starts, only death ends it (GAME-D4-2026-09-24-02).

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path (HitEvent → `combat_damage_system`; water, drowning and the SDK batch are the documented extra producers and all reconcile) | VERIFIED | #4702 adds `commit_actor_value_deaths`. Every production `Dead` insert (combat, water, drowning, SDK, `reference_state::restore`) reaches `reconcile_dead_actor` |
| Death reconciled once | VERIFIED; one producer gap | #4693: `AiCombatState` is torn down and the corpse root is kept. #4708: the death take does not replay on a ragdolled or restored corpse. Gap: authored Starts Dead actors never become `Dead` at all (GAME-D4-2026-09-24-01) |
| Inventory index stability (rows zeroed, never removed) | VERIFIED | Consume, `transfer_loot` (Stack and All), `pickup_loot` (now appends `XCNT` stacks) and restore all keep indices |
| Fail-closed consumables vs fail-open packages | VERIFIED | `restoration_plan` and the AV map are unchanged. ALCH keeps its own EFIT decoder, so #4415's shared-EFIT integer fix does not double-convert potions. `package_conditions_pass` is still fail-open |
| Stage order | VERIFIED; one open drift | Update order: restoration → interaction → container_loot → combat_input → **faction_hostility** → npc_combat_ai → combat_damage. Ambient packages run before scene packages, and ambient skips combatants (#4703). Late order: water_damage → reconcile_pending_dead → … → event_cleanup (last). Drift: #4712 is open. No source-order pin covers the combat chain |
| State saved-or-rederived | **DRIFTED** | GAME-D7-2026-09-24-01 (saved package-procedure state discarded on load); GAME-D5-2026-09-24-02 (`SpellList` not parked on eviction); #4695 (pickup tombstone, now reachable with E) |
| Determinism (no RNG or wall clock in gameplay logic) | VERIFIED in code; one load-order dependence | The grep over 20+ gameplay files, including `faction_hostility.rs` and `magic.rs`, is empty. Hostility uses a stable `total_cmp` sort. But the outcome of a load depends on the pre-load session's clock (GAME-D7-2026-09-24-01) |

## Findings

### HIGH

### GAME-D1-2026-09-24-01: Templated NPCs read `DOFT` and the leveled-expansion level from the placed record, not the template — 2,716 Skyrim placed actors spawn without their outfit
- **Severity**: HIGH
- **Dimension**: 1 — Equipment model (also Dim 2, corpse loot)
- **Location**:
  - `byroredux/src/npc_spawn.rs:1158` (`let npc = resolved.shell;`)
  - `:1175` (`let actor_level = effective_actor_level(npc);`)
  - `:1223` (`if let Some(otft_fid) = npc.default_outfit`)
  - For contrast, the correct reads: `npc_spawn.rs:275` (`CharacterLevel` from `resolved.stats`) and `crates/plugin/src/equip.rs:871` (spells from `resolved.stats`)
- **Status**: NEW. The bug predates #4457: the baseline read `npc.default_outfit` the same way. But #4457 claims no consumer at the population boundary still reads raw shell fields, and these two do. That makes #4457 incomplete.
- **Description**: `build_npc_equip_state` takes the carry list (`CNTO`) from the Use Inventory terminal, `resolved.inventory`. It reads two other values off `resolved.shell`:
  - **The outfit.** xEdit places `DOFT` in the Inventory group (`wbDefinitionsTES5.pas:8450`). UESP says TPLT 0x100 "Use inventory" covers the "Inventory tab, including all outfits and geared-up item". The outfit must therefore come from the terminal.
  - **The level used to expand leveled outfits and carry lists.** The stats row (TPLT 0x02) covers level. So one actor reports its `CharacterLevel` and resolves its spell lists from the Use Stats terminal, but expands its leveled gear at the shell's level.
  - `resolved.shell.default_outfit` is the only production reader of an NPC outfit. `inventory.rs:514` is the player seed.
- **Evidence**: census over the raw masters, re-run by the merge pass:
  - Skyrim.esm has 2,490 shells with Use Inventory set. In 775 of them, the shell has no `DOFT` while its terminal has one. There are 0 cases where both have a `DOFT` and the two differ, which is consistent with pure inheritance.
  - After LVLN resolution, 740 shells lose their outfit. 516 of these are placed, across **2,716 ACHRs**. The most-placed are `CWFortSiegeImperial` ×321, `LvlDraugrAmbushMelee1HMale` ×140, `LvlDraugrAmbushMelee2HMale` ×83, `LvlBanditMissile` ×78, `LvlBanditMeleeAny` ×52 and `LvlGuardImperial` ×50. Other examples: `LvlDwarvenCenturion` → `EncDwarvenCenturion03`, and `dunAnsilvundDraugrAmbushMelee2HAggro` → `EncDraugr05Melee2HEbonyHeadM02`.
  - The shell's level differs from its Use Stats terminal's level on Skyrim for 622 records (535 with a leveled `CNTO`), and on FNV for 643 (639).
- **Impact**:
  - Skyrim leveled bandits, draugr, guards and siege soldiers render with only the race skin (visible content missing), and their corpses lack the gear.
  - On Skyrim and FNV, hundreds of templated NPCs expand their leveled carry lists at a level that disagrees with their own `CharacterLevel`.
- **Trigger**: Skyrim; any bandit camp, draugr crypt or `CWFortSiege*` cell.
- **Related**: #1658 (closed; CNTO only); #4457; #4232; #4696; #4137.
- **Suggested Fix**:
  - Read `resolved.inventory.default_outfit`, and the sleep outfit if one is ever consumed.
  - Compute `actor_level` from `effective_actor_level(resolved.stats)`.
  - Add a test with a TPLT shell that has no `DOFT` over a terminal that has one, and a shell-vs-template level mismatch.

### GAME-D5-2026-09-24-01: The REFR/ACHR "Initially Disabled" header flag (0x800) is never decoded — quest-gated items, containers, doors and actors spawn live, and hostile actors now attack
- **Severity**: HIGH
- **Dimension**: 5 (actors) and 2 (items, containers, doors)
- **Location**:
  - `crates/plugin/src/esm/cell/walkers.rs:866`: only `RECORD_FLAG_DELETED` (0x20) is tested.
  - `crates/plugin/src/esm/cell/mod.rs:396-445`: `PlacedRef` has no flags field.
  - `byroredux/src/cell_loader/references/mod.rs:500-509` (inverted-XESP skip only) and `:624` (script `ReferenceEnableState` only).
  - `byroredux/src/interaction.rs:1222-1235`: the #4698 filter reads only the ledger.
- **Status**: NEW. #349 (XESP) and #3278/#4698 (script `Disable()`) are closed and do not cover the ref's own flag. `cell/mod.rs:680-700` documents the flag only as an unknown for XESP *parents* (#471).
- **Description**:
  - A ref's disabled state at spawn comes from two sources only: the inverted-XESP heuristic and the script-disable ledger. The authored initial state (xEdit flag bit 11, `wbDefinitionsTES5.pas:3112`, the same bit on every game) is dropped.
  - Initially disabled items and containers therefore render, collide and, since #4697, show a Take prompt and can be taken or looted. Initially disabled doors open and teleport.
  - Initially disabled actors go through the full NPC job. Since #4414, `stamp_combat_disposition` arms them and `faction_hostility_system` starts combat on sight.
- **Evidence**: census of refs with 0x800 and no XESP, whose state is therefore unambiguous.

  Non-actor refs:

  | Master | Items | Containers | Doors |
  |---|---|---|---|
  | FNV | 105 | 34 | 10 |
  | FO3 | 8 | 13 | 5 |
  | Oblivion | 92 | 43 | 33 |
  | Skyrim | 169 | 249 | 22 |
  | FO4 | 40 | 29 | 33 |

  Examples: FO3 `WeapUniqueMissLauncher` and `MQ04RollerSkate`; Skyrim `MQ106DragonParchment`; FO4 `BoSM02_InitiateClarkeKey`; Oblivion `FGD01BrenusAstisJournal`.

  Actor refs whose base AIDT is Aggressive or worse:

  | Master | Refs | Very Aggressive |
  |---|---|---|
  | FNV | 935 | 111 |
  | FO3 | 101 | 52 |
  | Skyrim | 212 | 54 |
  | FO4 | 579 | 305 |

  Examples: the FNV `Vault11c` ceiling turrets, `NellisGenerator` explosive ants and the HooverDamIntPowerPlant Legion; the Skyrim HelgenKeep01 actors (14), `YsgramorsTomb01` wolf spirits, the GoldenglowEstate TG02 reinforcements, and the MS10 pirates in DawnstarWindpeakInn.
- **Impact**:
  - Content the game hides until a quest stage enables it is present from the first load.
  - Unique weapons, quest items and keys can be taken early, and keys open their locks early.
  - Hidden hostile actors attack the player or other NPCs.
- **Trigger**: FNV `Vault11c`; Skyrim `YsgramorsTomb01`, `HelgenKeep01` or Bleak Falls (MQ106); FO3 MQ04; the FO4 BoS quest cells.
- **Related**: #349, #471, #3278, #4698, #4697, #4414; GAME-D4-2026-09-24-01 (same parser site); GAME-D2-2026-09-24-04. `/audit-esm` owns the decode half.
- **Suggested Fix**:
  - Carry the record-header flags on `PlacedRef`.
  - Seed 0x800 refs that have no enable parent as disabled, through the existing `placement_disabled` branch (identity and scripts only), keyed like `ReferenceEnableState` so a scripted `Enable()` overrides it.
  - Resolve XESP children against the parent's real flag (#471's two-pass plan).

### GAME-D4-2026-09-24-01: The ACHR "Starts Dead" header flag (0x200) is never decoded — authored corpses spawn alive, cannot be looted, and hostile ones attack
- **Severity**: HIGH
- **Dimension**: 4 — Combat & Death
- **Location**:
  - The same parser gap: `walkers.rs:866` and `PlacedRef`.
  - No `Dead` producer for it: there is no `starts_dead` consumer anywhere.
  - `byroredux/src/inventory.rs:687-700`: `is_loot_source` requires `Dead` on actors.
- **Status**: NEW
- **Description**:
  - xEdit's ACHR flag list gives `9, 'Starts Dead'` (`wbDefinitionsTES5.pas:3110`). Skyrim and FO4 use it for every placed corpse (`TreasCorpse*`, `DN027_TheodoreCroupCorpse`).
  - Because nothing reads the flag, the actor spawns as a live NPC with `CombatDisposition`. It stands in an idle pose and is not a loot source.
  - Since #4414, it starts combat if hostile.
- **Evidence**:
  - Skyrim: 1,123 refs with 0x200 (base AIDT: 681 Very Aggressive, 128 Aggressive).
  - FO4: 1,178 refs (428 Very Aggressive, 98 Aggressive).
  - Skyrim `BleakFallsBarrow01` alone has 19: `dunBleakFallsCorpseBretonThomas`, two `TreasCorpseBanditNordMale`, `TreasCorpseSkeleton(Rigid)` and 12 `EncSkeever`.
  - FO3/FNV have no 0x200 actor refs. Their corpse encoding (bases named `DEAD*`/`Loot1*`) was not established, so they are out of scope here.
- **Impact**:
  - Dungeon corpses stand up, and hostile ones attack the player.
  - Treasure corpses can't be looted, so their authored loot is unreachable.
  - This sits on the Skyrim MQ102 critical path.
- **Trigger**: Skyrim `BleakFallsBarrow01`; FO4 `SentinelSite01` or `SuperDuperMart01`.
- **Related**: GAME-D5-2026-09-24-01; #4414.
- **Suggested Fix**: expose the flag. At actor-job completion, insert `Dead` and run `reconcile_dead_actor` (the path `reference_state::restore` already uses), so the ragdoll settles and the corpse becomes a loot source. Pin it with a Starts Dead spawn test.

### MEDIUM

### GAME-D7-2026-09-24-01: Loading a save discards the saved AI-procedure state whenever the pre-load session's clock selects a different package
- **Severity**: MEDIUM
- **Dimension**: 7 — Saved-or-rederived (also Dim 5)
- **Location**:
  - `byroredux/src/save_io.rs:171-175`: `PRE_RELOAD_RESOURCES` holds only the three reference ledgers.
  - `save_io.rs:1713` (reload inside `without_parked_state`), `:1750` (`restore_resources`), `:1779` (`apply_deltas`).
  - `byroredux/src/npc_spawn/ai_package.rs:571-575`: the spawn-time pick reads `GameTimeRes`.
  - `ai_package.rs:684-735`: the first tick re-selects, and calls `clear_ambient_behavior` on a change.
- **Status**: NEW
- **Description**:
  1. The reload spawns NPCs synchronously. `apply_ai_package_behavior` picks each NPC's winner against the **live** `GameTimeRes` and quest state, because neither is a pre-reload resource.
  2. `restore_resources` then installs the saved clock and quest state.
  3. `apply_deltas` overlays the saved `WanderState`, `TravelState`, `Traveled`, `GuardState`, `PatrolState` and `Escorted`.
  4. On the first Update, `ambient_ai_package_system` re-selects against the restored clock. `AmbientPackageRuntime` is unsaved and still holds the spawn-time winner, so `changed` is true whenever the two clocks pick different packages. `clear_ambient_behavior` then deletes the state that was just restored.
  - This breaks the rule in `PRE_RELOAD_RESOURCES`'s own doc: a resource belongs there if a spawn-time path consults it.
- **Evidence**:
  - A probe test in a HEAD copy (`audit_probe_saved_travel_progress_depends_on_live_clock`) replays the load order. It spawns at the live hour, restores the saved hour, overlays a saved `Traveled`, and ticks once. The packages are Sandbox 08:00+12 h and Travel 20:00+2 h.
  - Output: `same_clock_keeps_traveled=true other_clock_keeps_traveled=false`.
- **Impact**:
  - Travel packages that had finished travel again.
  - Patrols restart from waypoint 0.
  - Wander and guard progress resets.
  - The outcome of a load depends on the session state before the load.
- **Trigger**: any game with scheduled PACKs, for example an FNV saloon. Save at night, then load from a freshly booted engine at its default hour.
- **Related**: #2014 (the saved procedure columns), #3789, #4135, #4136.
- **Suggested Fix**: add `GameTimeRes` and the condition resources that package CTDAs read to `PRE_RELOAD_RESOURCES`, after confirming the unload path doesn't need their live values. Alternatively, re-seat each `AmbientPackageRuntime.active_package_form_id` from the restored clock before the first tick. Keep the probe as a regression test.

### GAME-D4-2026-09-24-02: Combat never disengages — a combatant chases a live target forever and its ambient package stays suspended
- **Severity**: MEDIUM
- **Dimension**: 4 — Combat & Death
- **Location**:
  - `byroredux/src/systems/combat_ai.rs:122-135`: the only drop is a dead target or a target with no `Transform`; the drop is applied at `:302`.
  - `byroredux/src/npc_spawn/ai_package.rs:684`: combatants are skipped.
  - No `StopCombat` consumer exists in `crates/scripting` or `byroredux/src`.
- **Status**: NEW (a consequence of #4414)
- **Description**:
  - `faction_hostility` starts combat behind a range and line-of-sight gate, but nothing ends it on range, lost sight or a timeout.
  - The chase is a straight line with no navmesh and no stuck-repick, so a blocked chaser grinds against geometry.
  - #4703 resumes the package only when `AiCombatState` is gone, so while the target lives the package never resumes: no Sandbox, no seat, no schedule.
- **Impact**: an actor that glimpses the player at exterior range (FNV/Skyrim 2,500 × 2.0–2.1 BU) follows them for the rest of the session. Guards and townsfolk engaged by a creature that runs off never return to their packages.
- **Trigger**: FNV or Skyrim exterior; let an Aggressive NPC see you, then walk out of range.
- **Related**: #4414, #4703; GAME-D5-2026-09-24-03.
- **Suggested Fix**: re-check range and line of sight on the existing 0.5 s hostility cadence, and drop ambient-started `AiCombatState` after a grace period. Add a `StopCombat` lowering.

### GAME-D4-2026-09-24-03: `CombatDisposition` is stamped when the actor job starts, but saved death and parked state arrive only at completion — hostility can arm corpses and body-less roots mid-spawn
- **Severity**: MEDIUM
- **Dimension**: 4 (and 5)
- **Location**:
  - `byroredux/src/npc_spawn/resumable.rs:2088-2092`: `spawn_placement_root` stamps `CombatDisposition`, `Transform` and `GlobalTransform` at prepare time.
  - `byroredux/src/cell_loader/references/mod.rs:716-735`: the NPC job returns `Pending` across frames.
  - `byroredux/src/cell_loader/references/synth_child.rs:80-97`: `reference_state::restore`, which inserts the saved `Dead`, runs only at `Complete`.
  - `byroredux/src/systems/faction_hostility.rs:242-260`: the perceiver snapshot is every `CombatDisposition` holder without `Dead` that has a `GlobalTransform`.
- **Status**: NEW (introduced by 9789d8153)
- **Description**: during an async exterior spawn, the root is a valid perceiver frames before its body and its parked `dead` flag land. In that window:
  - `faction_hostility` can start combat for an actor the eviction ledger records as dead;
  - `combat_ai` moves its root;
  - it can strike at once, because the hit cooldown starts at 0.

  Restore then inserts `Dead` and ragdolls it at the displaced position. The same window lets a live hostile root with no mesh chase and strike while invisible.
- **Impact**: a corpse from an earlier visit can hit the player or shift while its cell re-streams. Invisible attacks are possible.
- **Trigger**: FNV exterior. Kill a hostile NPC, walk until its cell evicts, then walk back into range while it re-streams.
- **Related**: #4414; GAME-D4-2026-09-24-05 (no test covers the window).
- **Suggested Fix**: stamp `CombatDisposition` at completion, after `restore`. Or have `faction_hostility` require a completion marker such as the root's `FormIdComponent`.

### GAME-D2-2026-09-24-02: A picked-up item keeps its collision body — an invisible solid that blocks line of sight to the next item
- **Severity**: MEDIUM
- **Dimension**: 2 — Interaction / pickup
- **Location**:
  - `byroredux/src/inventory.rs:1138-1152`: `pickup_loot` marks only `mesh_entities_under`.
  - `byroredux/src/cell_loader/reference_state.rs:215-231`: restore marks the same set.
  - `byroredux/src/cell_loader/spawn.rs:1321-1392`: collision entities are standalone, carry `PhysicsSourceForm` and have no `Parent`.
  - `byroredux/src/interaction.rs:1013-1078`: the line-of-sight check.
- **Status**: NEW. The #4571 fix covers render only.
- **Description**:
  - `PickedUp` hides the mesh descendants, but the placement's colliders are not descendants and stay in Rapier.
  - `cast_ray` excludes only sensors and the player (`crates/physics/src/world.rs:1172`). `collider_belongs_to_target` accepts a hit only on the target's own source form, so the taken item's leftover collider blocks any candidate behind it.
  - A tombstoned placement respawns its colliders (`spawn.rs:716`) before the row is restored (`synth_child.rs:870`), so the ghost survives cell reloads.
  - No system despawns or disables colliders for `PickedUp`.
- **Impact**: every taken item leaves an invisible solid (dynamic for clutter). Items behind it in the aim line can't be selected until the player changes angle, and the ghost body can be bumped or pushed.
- **Trigger**: any game; take the front bottle of a shelf row, then aim at the one behind it.
- **Related**: #4571, #4695, #4697.
- **Suggested Fix**: on pickup and on tombstone restore, remove the collision entities whose `PhysicsSourceForm` matches the root's `FormIdComponent`, or make line of sight and physics skip them. Add a two-items-in-a-row selection test.

### GAME-D5-2026-09-24-02: `SpellList` is not part of the parked `ReferenceState` — after an NPC's cell reloads, its spell membership and its ability actor-value deltas disagree
- **Severity**: MEDIUM
- **Dimension**: 5 / 2 — Persistence (also Dim 3, actor-value layers)
- **Location**:
  - `byroredux/src/cell_loader/reference_state.rs:26-39`: `ReferenceState` has no spell field.
  - `reference_state.rs:96-166`: `capture` stores `ActorValues` verbatim.
  - `reference_state.rs:206-214`: restore.
  - `byroredux/src/npc_spawn.rs:128-150`: `stamp_spell_list` re-derives the list from SPLO.
  - `crates/scripting/src/magic.rs:152-185`: `add_spell`/`remove_spell` gate on list membership.
- **Status**: NEW. Introduced by cd4fc019a (Refs #4415); found independently by two audit legs.
- **Description**:
  - A fragment's `AddSpell` or `RemoveSpell` on an NPC changes both `SpellList` and the permanent AV modifier.
  - On eviction, `capture` keeps the modified `ActorValues` but not `SpellList`. On return, the spawn re-stamps the authored list and `restore` puts back the modified values. After that:
    - an added ability is missing from the list while its bonus remains, so `RemoveSpell` returns false and the bonus becomes permanent;
    - a second `AddSpell` applies the bonus twice;
    - a removed authored ability desyncs the other way.
  - A save taken while the cell is parked loses the list too.
  - Save/load of a *resident* actor is consistent, because both components overlay verbatim.
- **Impact**: silent, permanent AV drift on NPCs whose abilities a quest changes. This is the "load silently drops gameplay state" class, bounded by how often fragments add or remove NPC spells.
- **Trigger**: any fragment `AddSpell`/`RemoveSpell` of a constant spell on an NPC, then leave and re-enter its cell.
- **Related**: #4415, #4465 (the no-`serde(default)` / FORMAT_MAJOR rule); GAME-D7-2026-09-24-03.
- **Suggested Fix**: add `spells: Option<Vec<u32>>` to `ReferenceState`, with a FORMAT_MAJOR bump and no `serde(default)`. Restore it together with `actor_values`, after `stamp_spell_list`.

### LOW

### GAME-D2-2026-09-24-04: `Enable()` at runtime on a ref that was disabled at cell load makes it interactive while it stays invisible and non-solid
- **Severity**: LOW
- **Dimension**: 2
- **Location**: `byroredux/src/cell_loader/spawn.rs:684-691` (the root and its door/lock payloads spawn before the disabled early return); `byroredux/src/interaction.rs:1222-1235` (the #4698 filter is read live)
- **Status**: NEW. This is the mirror of the documented #3278 limitation: there is no live re-spawn.
- **Description**: after `Enable()`, the root passes the live ledger filter, but it never received meshes or colliders. The player gets an invisible Open or Take prompt at the 24 BU fallback sphere, and an invisible door that still transitions.
- **Trigger**: a script-disabled ref is reloaded, then a quest stage `Enable()`s it while its cell is resident.
- **Related**: #3278, #4698; GAME-D5-2026-09-24-01 (its fix makes this path much more common).
- **Suggested Fix**: until live re-spawn exists, also require a spawned-content marker in `populate_candidates`. Or re-run the placement spawn on the enable edge.

### GAME-D7-2026-09-24-02: The `npc_combat_ai_system` access declaration claims to cover every pair `clear_ambient_behavior` removes, but declares none
- **Severity**: LOW
- **Dimension**: 7 — Stage order / access
- **Location**: `byroredux/src/boot/schedule/update.rs:256-264`; `byroredux/src/npc_spawn/ai_package.rs:436-499`
- **Status**: NEW (introduced by #4703, 6c517bc7a)
- **Description**:
  - The comment says the declaration covers "every ambient Behavior/State pair that function removes". Only `AmbientPackageRuntime`, `Seated`, `SeatReservations` and `AnimationPlayer` are declared.
  - The 17 types removed through the generic `remove_component::<T>` go undeclared: seven Behavior/State pairs, plus `Traveled`, `Escorted`, `WalkStuckTimer` and `NavPath`.
  - The `acquired_in` guard can't see through the generic helper.
- **Impact**: diagnostics only. The system runs exclusively, but the reported write set is wrong.
- **Related**: #4574.
- **Suggested Fix**: declare the 17 writes, or share one declaration fragment with `reconcile_dead_actor`'s callers. Teach the guard to follow `remove_component::<T>`.

### GAME-D7-2026-09-24-03: A script's `AddSpell` silently does nothing on an actor spawned with no spells
- **Severity**: LOW
- **Dimension**: 7 — Tested but unwired (#4415)
- **Location**:
  - `byroredux/src/npc_spawn.rs:128-146`: `stamp_spell_list` returns at `:135` without inserting a component.
  - `crates/scripting/src/magic.rs:152-166`.
  - `crates/scripting/src/fragment/effects.rs:709-716`: the `bool` result is dropped and nothing is logged.
- **Status**: NEW
- **Description**: `add_spell` returns false when the actor has no `SpellList`. NPCs and creatures whose own and racial SPLO are empty never get one. The fragment layer accepted the effect, which breaks the translator's decline-on-unmodeled contract, and then drops it with no log. The player always gets a `SpellList`, possibly empty (`inventory.rs:659`).
- **Impact**: quest abilities and diseases added to spell-less actors (mostly creatures) have no effect and are not saved.
- **Suggested Fix**: always insert `SpellList` at spawn (an empty `Vec` round-trips fine), or have `add_spell` insert it. Log when a `false` result is dropped.

### GAME-D7-2026-09-24-04: Save-classification guard drift — the hand-written `NPC_SPAWN_STAMPED` list omits today's spawn stamps, and the `NavmeshResidency` rationale is inaccurate
- **Severity**: LOW
- **Dimension**: 7 — Test gap / doc
- **Location**: `byroredux/src/save_io/round_trip_tests.rs:1417-1435`; `byroredux/src/save_io/registry_completeness_tests.rs:708`
- **Status**: NEW
- **Description**:
  - (a) `NPC_SPAWN_STAMPED` omits the two new stamps in `spawn_placement_root` (`resumable.rs:2089-2091`): `CombatDisposition` (#4414) and `SpellList` (#4415). The general guard classifies both, but this guard's check for a new runtime mutator doesn't run on them.
  - (b) The `NavmeshResidency` row says the counter "restarts at 0 after a load". An in-session load keeps the world, so the counter keeps incrementing. The conclusion (any cached `NavPath` is stale) still holds.
- **Suggested Fix**: add the two names and reword (b). The `AiCombatState` row is covered by GAME-D4-2026-09-24-04.

### GAME-D5-2026-09-24-03: A combatant's ambient runtime is "due" every frame — its candidate list is cloned every frame of a fight
- **Severity**: LOW
- **Dimension**: 5 — Package selection cost
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:522` (suspension sets `last_evaluated_game_minute = None`), `:657` (the due gate), `:672` (the clone), `:684` (the combat `continue` skips the minute stamp)
- **Status**: NEW (#4703 interacting with #4414)
- **Description**: the #3353 "one evaluation per in-game minute" cost model does not hold for combatants. Each frame pays a `package_candidates` clone plus `Dead` and `AiCombatState` lookups per combatant. Ambient hostility makes this common, and GAME-D4-2026-09-24-02 makes it last indefinitely.
- **Suggested Fix**: stamp the current minute for skipped combatants, or filter combatants out before the clone.

### GAME-D4-2026-09-24-04: Docs and the save allowlist still say combat is `StartCombat`-only
- **Severity**: LOW
- **Dimension**: 4 — Doc drift from #4414
- **Location**:
  - `byroredux/src/npc_spawn.rs:189-190`: "Creatures still start no combat of their own — there is no ambient AI aggro".
  - `crates/scripting/src/combat.rs:19`: `AiCombatState` is "forced into combat … by `Effect::StartCombat`".
  - `byroredux/src/save_io/registry_completeness_tests.rs:613`: the `AiCombatState` rationale.
- **Status**: NEW
- **Description**: `faction_hostility_system` is now a second producer. After a load it re-creates ambient combat within about 0.5 s when the pair is still in range and in sight. Only scripted combat against a non-hostile target is lost.
- **Suggested Fix**: name the second producer in all three places, and state the re-derivation in the allowlist row.

### GAME-D4-2026-09-24-05: `faction_hostility` tests don't pin the edges most likely to regress
- **Severity**: LOW
- **Dimension**: 4 — Test gap
- **Location**: `byroredux/src/systems/faction_hostility.rs:399-589`. The four tests cover the aggression table, standing, range and throttle, and the missing GMST.
- **Status**: NEW
- **Description**: nothing covers:
  - a dead perceiver, dead target or dead player (the skip at `:258`);
  - keeping a scripted `StartCombat` target (`:328`, `:380`);
  - excluding the player capsule from line of sight;
  - the mid-spawn window (GAME-D4-2026-09-24-03).

  All of these are correct today except the spawn window.
- **Suggested Fix**: add one fixture test for each.

## Fix verification (since the 2026-09-21 baseline)

| Fix | Verdict | Notes |
|---|---|---|
| #4693 (327d4bddd), corpse root | VERIFIED | The dead-attacker branch carries the current translation (`combat_ai.rs:95-114`), and `reconcile_dead_actor` removes `AiCombatState` (`combat.rs:587`) |
| #4694 (a70b54f14), PlayerRef resolver | VERIFIED | 0x14 resolves to `PapyrusPlayerEntity`. In fly-cam, the placeholder has no transform, and every consumer degrades without a panic |
| #4697 / #4706 (b46fd9d33), pickup and XCNT | VERIFIED | The Take prompt excludes taken items, `Inventory` holders and dead actors. A double activation is harmless. XCNT ≤0 is treated as absent, and the whole stack is granted. Colliders are a gap (GAME-D2-2026-09-24-02) |
| #4698 (b46fd9d33), disabled refs | **INCOMPLETE** | Script-disabled refs are filtered live. The header flag is ignored (GAME-D5-2026-09-24-01), and a runtime `Enable()` gives invisible interactivity (GAME-D2-2026-09-24-04) |
| #4699 (ed52fa5e4), theft rule | VERIFIED | Compares against 0x7. REFR `XOWN` wins over cell ownership (`stamp_cell_ownership` skips `Owned`). The XRNK bar is honoured. Player `FactionRanks` are seeded, and every Player-record rank is 0 on all five masters |
| #4700 / #4708 (3978b5184), Draugr combat marker | VERIFIED | Prebaked insert at `resumable.rs:1993`, with a race resolved through Use Traits. No death replay on ragdolled or restored corpses |
| #4701 / #4707 (d8b849b04), dead player and refusals | VERIFIED | `player_can_act` gates attack, activate, loot and equip; `consume_item` refuses on `Dead`. The "Used" and "Can't … now" notifications are mutually exclusive |
| #4702 (34b53c464), SDK batch death | VERIFIED | `commit_actor_value_deaths` inserts `Dead` and queues reconciliation |
| #4703 / #4704 (6c517bc7a), combat suspends package | VERIFIED, with caveats | Releases the seat and pose, and resumes after `AiCombatState` is gone; alias overlays reach package-less actors. Caveats: combat never ends (GAME-D4-2026-09-24-02); the access declaration is incomplete (GAME-D7-2026-09-24-02) |
| #4705 (66b7eb529), guard stripper | VERIFIED | 23 edge cases were probed: braces in strings, raw strings, byte and char literals; single-line items; nested modules; `cfg(any(test,…))` kept. On the real tree it keeps 344 of 358 impls, and all 14 removed are inside `#[cfg(test)] mod tests`. The 5 newly classified types have true justifications, except the `NavmeshResidency` wording (GAME-D7-2026-09-24-04) |
| #4571 (dcdfafa15), PickedUp render | VERIFIED | The marker reaches mesh descendants at pickup and at restore |
| #4574 (0f0287519), #4605 (fc825a6cd), #4613 (a41202008) | VERIFIED | Access rows match. No guard is held across the physics query. Scratch buffers persist and are cleared per call |
| #4675 (65717ab58), HUD bars | VERIFIED | `hud::fraction` reads only `PlayerEntity`; with no player the bars read full |
| #4678 / #4674 / #4679 | VERIFIED | The real-master player seed test passes |
| #4452 / #4689 / #4690 | VERIFIED | Pin tests pass |
| #4457 (3748f4cb1), TPLT hoist | **INCOMPLETE** | The #4093 AI-package chain is preserved, but the shell `DOFT` and level reads remain (GAME-D1-2026-09-24-01) |
| #4414 (9789d8153), faction hostility | VERIFIED as designed | The AIDT offsets match xEdit `wbAIDT` (FO3/FNV: bool @15, s32 @16; Skyrim/FO4: flag @6, attack u32 @16), and decode is 100% on all four masters. TPLT 0x10 is Use AI Data on all four. XNAM: 0 Neutral / 1 Enemy / 2 Ally / 3 Friend, perceiver faction → target faction. GMSTs: FO3/FNV 2500 ×2.0, Skyrim 2500 ×2.1, FO4 4096 ×1.25. Dead perceivers and targets are skipped. A scripted `StartCombat` target is never overwritten. Line of sight excludes dynamic bodies, bones, sensors and the player capsule. `FactionRelations` keyed by `FormRef` survives plugin-list changes (unmatched rows kept inert). `LoadOrderIdentity` is installed on every load path. Lifecycle gaps: GAME-D4-2026-09-24-02 and -03 |
| #4415 (cd4fc019a), magic runtime (gameplay slice) | VERIFIED; one NEW gap | No double-apply on spawn or save/load (whole-component overlay). `ActorValues::restore` clamps against the modified maximum. ALCH EFIT is unaffected. Eviction gap: GAME-D5-2026-09-24-02 |

## Cross-audit pointers (not re-reported here)

- **`/audit-esm`** owns the decode half of GAME-D5-2026-09-24-01 and GAME-D4-2026-09-24-01 (`PlacedRef` has no record flags). ACHR flag 25 "No AI Acquire" is dropped the same way. Since #4414, it probably should exempt a ref from `faction_hostility`, but this audit did not measure it.
- **`/audit-save`**: GAME-D7-2026-09-24-01 is a pre-reload resource ordering defect in `save_io.rs`. GAME-D5-2026-09-24-02's fix needs a FORMAT_MAJOR bump.
- **`/audit-physics`**: the pickup ghost collider (GAME-D2-2026-09-24-02) is also a Rapier body lifetime question.
- **Carryable lights**: a picked-up LIGH keeps shining (18 Skyrim `Torch01` placements, 4 on Oblivion). Noted, not filed.
- **FO3 corpse bases**: on FO3, same-base infighting occurs in 11 cells, all on `Loot1*`/`DEAD*` corpse bases whose dead encoding is unverified. Not filed until that encoding is known.

## Known-Open Register (verified today; cite, don't re-file)

| Issue | State at HEAD |
|---|---|
| #4414 `FactionRelations` never read | **Closed by 9789d8153.** Remove it from the skill's known-open list |
| #4695 pickup tombstone not re-applied on load (HIGH) | **Still open, now in normal play.** #4697 put pickup on the E key, and #4706 makes the duplicate a whole `XCNT` stack. `save_io.rs:1713/1750` are unchanged. The new `physical_activate_picks_up_a_loose_item_stack` test covers eviction only |
| #4696 NPC leveled counts | Unchanged (`npc_spawn.rs:1250-1266`). The shared leveled walk from #4415 preserves item semantics exactly |
| #4232 `effective_actor_level` 0 | Unchanged (`actor/mod.rs:169-175`). It now also empties LVSP spells in `resolve_actor_spells` |
| #4248 containers never expand LVLI | Stale-open. `attach.rs` expands at the player's `CharacterLevel` (#4678). Recommend closing |
| #4712 fragment `Activate` flushed after `container_loot_system` | Unchanged (`update.rs:153` vs `:288`). It now also blocks scripted loose-item pickup |
| #4713 `ItemEventBatch` has no reader; #4714 `armor_covers_main_body` test-only | Unchanged. `main_body_bit` gained a production caller (`npc_spawn.rs:1131`, bfdc3d3fc), so #4714 is narrowed to `armor_covers_main_body` |
| #4709 kill switch also drops combat feedback | Unchanged. Note that `faction_hostility` and the combat chase are not behind the switch either |
| #4710 `walk_anim` docs; #4711 `inventory.status` damage; #4742 swing sound before the `DraugrCombatAnim` return; #4116 activation pin | Unchanged |
| #4415 magic runtime | Open (Refs). This audit's #4415 findings: GAME-D5-2026-09-24-02, GAME-D7-2026-09-24-03 |
| 10 of ~17 PACK procedures have no runtime; FO4+ walk clips are absent; cross-tile pathing is blocked | Unchanged. The findings above are misroutes of supported cases only |

## Test verification

| Command | Result |
|---|---|
| `cargo test -j8 -p byroredux --bin byroredux -- combat:: inventory:: interaction:: npc_spawn:: reference_state ambient_locomotion walk_anim save_io::` (skill baseline) | 262 passed, 0 failed, 18 ignored (installed data) |
| `… --bin byroredux -- reference_state scripted_lock_gate synth_child interaction:: loot_appearance consum restoration timed_ inventory::` | 102 passed, 9 ignored |
| `… --bin byroredux -- ambient_locomotion npc_spawn::ai_package walk_anim locomotion:: combat faction_hostility follow escort travel guard patrol sandbox wander` | 196 passed, 3 ignored |
| `… --bin byroredux -- notifications loading_screen spawn_tests` | 37 passed |
| `… --bin byroredux -- registry_completeness round_trip_tests::npc_spawn_stamped scheduler_access_tests fragment_activation_order_tests …` | 75 passed, 1 ignored |
| `cargo test -p byroredux-plugin --lib consumables` | 9 passed |
| `cargo test -p byroredux-scripting --lib -- magic combat` | 11 passed |
| `cargo test -p byroredux-core --features inspect --lib no_armor_mask_can_reach_the_weapon_slot` | 1 passed |
| `BYROREDUX_REQUIRE_GAME_DATA=1 … --ignored --exact`: `real_stimpaks_restore_scaled_health_and_limbs_but_hardcore_only_health`, `real_fallout3_bloodpack_selects_live_perk_bonus`, `real_skyrim_healing_potion_runs_through_native_action`, `real_master_player_seed_evaluates_the_player_only_rows`, and the two `save_io::consumable_tests::real_*` tests | All 6 PASS (0.9–2.1 GB RSS each) |
| `cargo test -p byroredux-plugin --test parse_real_esm -- --ignored --exact installed_masters_ai_data_decodes` | PASS (FO3 2180/2180, FNV 5394/5394, Skyrim 5118/5118, FO4 3015/3015) |
| Load-order probe `audit_probe_saved_travel_progress_depends_on_live_clock`, in a HEAD copy | PASS; demonstrates GAME-D7-2026-09-24-01 |

**Probes.** All probes were read-only and outside the repo. They were removed in Phase 4:
- Python censuses of the raw masters:
  - `DOFT` inheritance and ACHR placements;
  - shell vs template level;
  - record-header 0x800 and 0x200 refs, with base AIDT;
  - Player faction ranks;
  - carryable LIGH;
  - LVLI placements.
- A copy of the #4705 stripper, with 23 edge cases.
- The load-order probe test, run in a `git archive` copy.

The `DOFT` census and the xEdit flag tables (`wbDefinitionsTES5.pas:3110-3112`, `:8450`) were re-checked by the merge pass.

## Deduplication

- Every finding was checked against the issue dump (`gh issue list --limit 200`) plus a `gh issue list --state all --search` per topic ("initially disabled", "starts dead", "PRE_RELOAD", "SpellList", "AmbientPackageRuntime", "outfit"). None matched an existing issue.
- Closed issues re-examined, all with their fixes in place: #349, #3278, #1658, #4457, #4571, #4693–#4708.
- No reports dated before 2026-06-07 were relied on.

Next step: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-09-24.md`. Suggested labels: `gameplay` plus `ai`/`combat`/`inventory`/`save-load` as fitting. Add `game:skyrim` for GAME-D1-2026-09-24-01 (title-specific). GAME-D5-2026-09-24-01 and GAME-D4-2026-09-24-01 also take `esm-plugin`.
