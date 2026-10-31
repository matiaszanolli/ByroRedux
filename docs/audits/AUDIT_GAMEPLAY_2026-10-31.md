**HEAD**: 670f81917 · **Baseline**: `AUDIT_GAMEPLAY_2026-10-27.md` (`fa6be4a6b`, 136 commits ago) · **Audited**: Dim 1 (crafting, smithing, armor, loot/AIPL/REMP, inventory split), Dim 2 (lockpick/HACK, Craft candidate, dialogue-page delta), Dim 4 (mines, iron sights, armor mitigation, ASSU, VATS fixes, #6133), Dim 5 (random conversation / hello / hold, dead body, attraction, 10-27 package fixes), Dim 7 (stage order, access rows, determinism, save classification of the new transients) · **Unchanged since baseline (skimmed)**: Dim 3 (0 commits on its paths), Dim 6 (one comment-only commit, #6194)

# Gameplay Audit, 2026-10-31

This audit ran inside the comprehensive suite, solo, with no sub-agents. I made no source edits, created no issues, and did not launch the engine. The delta is 136 commits. Path-scoped counts: Dim 1 15, Dim 2 9, Dim 3 0, Dim 4 14, Dim 5 20, Dim 6 1, Dim 7 21. Most of the delta is M43: mines, attraction, dead body, random conversation and hello, iron sights, lockpick and hack, crafting and improving, Skyrim armor, jail (`SendPlayerToJail`, escape) and the new Story Manager producers (`LEVL`, `ASSU`, `AIPL`, `REMP`, `CRFT`, `OAAT`, `ARRT`, `ESJA`).

## Scope and evidence

- **Baseline tests**: I ran `cargo test -p byroredux --bin byroredux -- combat:: inventory:: interaction:: npc_spawn:: reference_state ambient_locomotion walk_anim eat_sleep find_package flee_package use_weapon_package crime:: forcegreet dialogue_voice vats save_io:: consum restoration timed_ notifications loading_screen spawn_tests revive knockdown flee crafting smithing armor lockpick mines iron_sights attraction dead_body random_conversation scheduler_access hit_event activation_flush` in the shared repo `target/` with `CARGO_INCREMENTAL=0`. Result: **678 passed, 0 failed, 25 ignored**. No test exercises any of the four MEDIUM findings below. I did not re-run the `BYRO_LOCK_ORDER_CHECK=1` lane; the ECS and concurrency audits cover it (red on ECS-2026-10-31-01).
- **Prior findings**: all five 10-27 findings are fixed in the tree and I re-read each fix. #6466 (Find activate queues through `PendingFragmentActivations`, `find_package.rs:378-382`), #6467 (`use_weapon_system` takes `collect_unconscious`, `script_requests.rs:65`), #6468 (Flee/Ambush gated on `package_conditions_fully_modeled`, `ai_package.rs:191,390,396`), #6469 (`vats_entry_refusal` shared, queue/confirm/scan refuse outside VATS), #6470 (`Inventory::count_of_any`).
- **Dedup**: against `/tmp/audit/open_issues_snapshot.txt` (98 open), the 10-27 report, and suite section 6 (safety, renderer, ecs, concurrency, nif, performance, regression, nifal, parsers, physics, esm, character, exterior). Overlaps are cited, not re-filed.
  - ECS-2026-10-31-01 / CONC-2026-10-31-01 (`improve_plan` holds `Inventory` across the `ItemInstancePool` read; lock-order lane red).
  - ECS-2026-10-31-02 / CONC-2026-10-31-02 (`mine_system` is missing from the `hit_event_consumers_run_after_every_producer` pin).
  - CHAR-2026-10-31-M-01 / L-01 (`armor.rs` hardcodes GMSTs and tests `GameKind::Skyrim`), CHAR-2026-10-31-L-02 (`send_player_to_jail` falls back to `CrimeModel::SKYRIM`).
  - REG-2026-10-31-04 (nine `m43-*.sh` smokes in no contract loop).
- **Not checked**: smoke scripts, anything that needs Vulkan, `--ignored` real-data tests, and vanilla counts (for example how many FO4 mines are initially disabled). All findings are static; unmeasured counts are marked as such.

## Executive Summary

| Severity | NEW | Regression | Existing |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 4 | 0 | 0 |
| LOW | 2 | 0 | 0 |

The new M43 producers are mostly well shaped: they are idle without their Story Manager root, draw only from the seeded `StoryManagerRng`, snapshot before they write, and sit in the right stage (`quest_scene` ahead of `script_runtime`, with `combat_damage_system` last). The four MEDIUM defects share one pattern: **a new runtime that skips a rule an older gameplay path already enforces.**

- The world-object scanners (`mine_system`, `attraction_system`) ignore the reference enable ledger that the loader and `random_conversation` already honour (GAME-D4-01).
- The mine blast writes `HitEvent` with a replacing `insert`, where every other producer holds a colliding hit (GAME-D4-02).
- The random-conversation hold has an owner-only release, so a partner can be orphaned (GAME-D5-01).
- Improving a single plain copy puts an instanced row ahead of future plain siblings, and form-keyed removal takes rows in index order (GAME-D1-01).

Games reasoned about (statically): Skyrim (crafting, improving, armor, jail, random conversations), Fallout 4 (mines, attraction, lockpick, hack, iron sights, dead body), Oblivion (jail escape side effects).

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path | **DRIFTED**, one residual | `mine_system` is a new `HitEvent` producer registered ahead of `combat_damage_system` (`quest_scene.rs:19` before `player_animation.rs:24`). It replaces a colliding hit instead of holding it (GAME-D4-02). `armor::mitigate` sits inside `combat_damage_system`, so the single consumer is intact. |
| Death reconciled once | **VERIFIED** | No new production `Dead` insert. `DEAD` discovery only reads `Dead`. `PendingDeathReconciliations.held` is released by `complete_reference_load` and the live-load sweep (#6133). |
| Index stability | **DRIFTED** | `improve_item` appends the split-off improved copy with `Inventory::push` (correct: instanced stacks are always appended) and gives a single copy an instance in place (rows never reorder). The residual is removal order, not stability (GAME-D1-01). |
| Fail-closed consumables vs fail-open packages | **VERIFIED** | Dim 3 untouched. Recipe conditions (`crafting::conditions_met`) are fail-open for unmodelled functions, the package rule. They are not a consumable; the CK hides a recipe on a failed condition, so the policy is consistent with the ambient rule. |
| Stage order | **VERIFIED** | `interaction_and_crime` -> `combat` -> `quest_scene` (M43 producers, then `story_manager_dispatch`) -> `script_runtime` -> `player_animation` (`combat_damage_system`). Lockpick/Craft run inside `interaction_system`, so they precede every `ActivateEvent` consumer. The producer pin omits `mine_system` (ECS-02, dup). |
| State saved or re-derived | **DRIFTED**, one residual | `ArmedMine`, `SpentMine`, `ConversationHold`, `DeadBodyState`, `AttractionState`, `RandomConversationState`, `JailEscapeWatch`, `IronSightsState`, `ActiveCrafting` all carry allowlist reasons and `registry_completeness_tests` is green. The spent-mine reason ("disabled in the persistent enable ledger so it does not come back") is true of the loader but not of the trip scan (GAME-D4-01). |
| Determinism | **VERIFIED** | `grep -E 'rand::\|Instant::now\|SystemTime\|thread_rng'` over combat, inventory, interaction, crafting, smithing, armor, lockpick, crime, `ai_package`, faction_hostility, npc_dialogue, forcegreet, dialogue_voice, vats, the M43 producers and every Dim 5 system is empty. `attraction_system` and `random_conversation_system` draw from `StoryManagerRng` in `Vec` order. The `#[allow(dead_code)]` sites are the two #4464 rows. |

## Findings

### MEDIUM

### GAME-D4-2026-10-31-01: `mine_system` and `attraction_system` ignore the reference enable ledger, so a detonated mine comes back with its cell and disabled mines and objects still react
- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4, M43 mines); NPC producers (Dim 5, attraction); state saved or re-derived (Dim 7)
- **Location**: `byroredux/src/systems/mines.rs:229-248` (the `placed` scan) and `:261-331` (`detonate`); `byroredux/src/systems/attraction.rs:173-190` (the object/actor gather)
- **Status**: NEW (`c36ab4125`, `ccccbaf03`, 2026-10-30)
- **Trigger**: Fallout 4. (a) Detonate a placed mine, then leave and re-enter its cell, or save and load. (b) Enter a cell whose mine or attraction object is "Initially Disabled", has an `XESP` parent that is off, or was `Disable()`d by a script.
- **Description**: A disabled REFR still gets a placement root. The loader (`spawn.rs:787-797`) spawns the root with no mesh or collider, marks it `PlacementContentWithheld`, and `synth_child.rs:83` still stamps `SceneAliasCandidate` on it. Both scanners enumerate every `SceneAliasCandidate` whose base is in their book and read `GlobalTransform`; neither asks `byroredux_scripting::reference_enabled(world, reference_form_id)` or checks the withheld marker. Compare `random_conversation::complex_scene_active` (`random_conversation.rs:94-107`), which fixes the same trap for the complex-scene marker ("the vanilla marker beside Whiterun is parented to a quest-driven reference, and counting it while dormant held back every conversation"). `detonate` records the mine as spent only by inserting the session component `SpentMine` and by `ReferenceEnableState::set_enabled(form, false)`. The ledger is saved and honoured on the next load, which withholds the root's content, but `SpentMine` is not saved. After a reload the root is a fresh "waiting" mine, so the next actor within the record's proximity re-trips and re-detonates it. The module doc says the ledger entry means the mine "does not come back with its cell". The invisible, non-solid mine does.
- **Evidence**:
  ```rust
  .filter(|(entity, _)| !world.has::<SpentMine>(*entity) && !world.has::<ArmedMine>(*entity))
  .filter_map(|(entity, spec)| Some((entity, spec, world.get::<GlobalTransform>(entity)?.translation)))
  ```
  There is no `reference_enabled` or `PlacementContentWithheld` test in `mines.rs` or `attraction.rs` (`grep` finds none).
- **Impact**: A spent mine explodes again on every cell reload, hurting the player and re-raising `TMEE`, which can restart its quests. An authored dormant mine field (quest-staged, enable-parented) detonates before the quest enables it. `attraction_system` raises `AOBJ` for disabled workshop and comment objects that nothing renders. The number of vanilla `Fallout4.esm` mines or attraction objects that are initially disabled or enable-parented was not measured.
- **Related**: #3278, #4698, #4820, #6077 (the loader and interaction honour the same ledger); `complex_scene_active` (005aa8a08) is the in-tree model for the fix.
- **Suggested Fix**: In both scans, skip a candidate when `candidate.reference_form_id != 0 && !reference_enabled(world, candidate.reference_form_id)` (or when it carries `PlacementContentWithheld`). Add a case to `mines.rs` tests for a disabled reference and for a spent mine after a simulated reload.

### GAME-D4-2026-10-31-02: A mine blast replaces a colliding `HitEvent`, so two mines tripped together land one blast
- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4, single damage path)
- **Location**: `byroredux/src/systems/mines.rs:280-297` (`detonate`)
- **Status**: NEW (`c36ab4125`). ECS-2026-10-31-02 / CONC-2026-10-31-02 flag the missing pin entry and call the overwrite "a design note and not a defect"; the multi-mine case below is why it is one.
- **Trigger**: Fallout 4. Two or more mines within 500 BU of each other, tripped by the same approaching actor, or a mine blast on a target that takes a melee or projectile hit the same frame.
- **Description**: `HitEvent` is one `SparseSet` row per target and `insert` replaces (the #4324 contract stated in `combat_ai.rs:259-264`). Every other producer holds a collision instead: `npc_combat_ai_system` keeps the attacker ready and strikes next frame (`combat_ai.rs:268-275`), and `pending_trap_hit_flush_system` waits a frame (#5860, pinned by `a_same_frame_melee_hit_survives_a_projectile_hit`). `detonate` calls `events.insert(target, ...)` for every actor in the outer radius without looking. A minefield trips in groups: `mine_system` scans every 0.25 s and arms every mine within `proximity` of the approaching actor in the same pass, each with the same record fuse (vanilla `fragMineExplosion`, 1.5 s), so adjacent mines reach `fuse_left <= 0` in the same frame. Their blasts overlap (outer radius 500) and the second `insert` replaces the first on every shared target, the player included.
- **Evidence**:
  ```rust
  if let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() {
      for (target, damage) in hits {
          events.insert(target, byroredux_scripting::HitEvent { aggressor: mine, ... damage, ... });
  ```
  There is no `events.get(target).is_some()` check, unlike `combat_ai.rs:268`.
- **Impact**: The player takes one mine's damage where the data says N. A melee hit that lands on the blast frame is silently dropped. No crash or save damage. Not measured: how many vanilla mines sit within one outer radius of a neighbour.
- **Related**: #4324, #5860 (closed, same contract); ECS-2026-10-31-02 (the pin).
- **Suggested Fix**: Make `detonate` stage its hits and keep the colliding ones for the next frame (a `PendingMineHits` resource drained by `mine_system`'s next pass, or defer the whole blast a frame), the way the other producers hold. Add a two-mines-one-frame case to the `mines.rs` tests. Fold the pin change from ECS-02 into the same commit.

### GAME-D5-2026-10-31-01: A random-conversation partner is orphaned in `ConversationHold` when its initiator is despawned or double-booked, and stays frozen
- **Severity**: MEDIUM
- **Dimension**: NPC Spawn -> AI Package (Dim 5, M43 random conversations)
- **Location**: `byroredux/src/systems/random_conversation.rs:446-496` (`random_conversation_system` / `begin_conversation` / `release_pair`), `:548-700` (`conversation_hold_inner`); consumer `byroredux/src/npc_spawn/ai_package.rs:1160-1162`
- **Status**: NEW (`b0591ba88`, 2026-10-30)
- **Trigger**: Skyrim and Fallout 4 (an `ADIA` root). (a) The initiator's cell is unloaded by streaming while the partner stays resident (exterior radius 2000 BU). (b) Two actors in one social radius come due in the same one-second pass, so one is both a target and an initiator.
- **Description**: Only the initiator's hold is driven. `conversation_hold_inner` collects `hold.initiator` entries and, per initiator, decides the step for both; `release_pair` removes the holds of the two actors it is given. The partner's `ConversationHold { initiator: false }` is never examined on its own. (a) When the initiator's entity is despawned, its hold goes with it and the pass has nothing to release; the partner's hold stays. (b) `collect_participants` filters held actors once, at the start of the pass, and `pick_target` is run over that fixed list for every due initiator with no "already claimed in this pass" set (only the `(initiator, target)` cooldown pair). If A picks B and B (also due) picks C, `begin_conversation(A, B)` then `begin_conversation(B, C)` overwrites B's hold. When A's hold later releases (`Waiting` with no scene, `MAX_APPROACH_SECS`), `release_pair(A, B)` removes B's initiator hold, which leaves C's partner hold without a driver.
- **Evidence**: `release_pair` is `holds.remove(a); holds.remove(b);` and nothing else clears a `initiator: false` hold. `ambient_ai_package_system` does `due.retain(|&actor| held.get(actor).is_none())`, and `collect_participants` drops held actors, so the orphan is never re-selected a package and never talks again. `grep` finds no other `ConversationHold` consumer outside `random_conversation.rs` and `ai_package.rs:1160`.
- **Impact**: The orphaned NPC stands with its package cleared (`suspend_ambient_behavior_for_combat` ran in `begin_conversation`) for the rest of the session, until a load drops the unsaved component. It still fights and dies normally, since combat does not read the hold. A town NPC standing idle forever is visible. Not measured: how often the double-booking case occurs. The despawn case needs only a stream boundary.
- **Related**: GAME-D4 family in the 10-27 report (a new runtime missing a rule the older paths enforce); `ConversationHold` allowlist reason says "a load drops the conversation".
- **Suggested Fix**: Add a partner-side sweep in `conversation_hold_inner` that removes a non-initiator hold whose `partner` lacks the reciprocal hold (or is not alive/placed). Claim actors during the due loop so one pass cannot book an actor twice. Add both cases to the tests.

### GAME-D1-2026-10-31-01: Form-keyed removal consumes an improved item ahead of a plain sibling when the improved row has the lower index
- **Severity**: MEDIUM
- **Dimension**: Inventory & Equipment (Dim 1, M43 improving)
- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:47-74` (`remove_items`); producers `byroredux/src/crafting.rs:394-398` (in-place instance), `crates/core/src/ecs/components/inventory.rs:123-135` (`Inventory::add`); callers `byroredux/src/barter.rs:427-446`, `DropObject`, scripted `RemoveItem`
- **Status**: NEW (`2c55546f4`, 2026-10-30)
- **Trigger**: Skyrim. The player holds one plain copy of an item, improves it at a grindstone, then acquires another plain copy, and then sells, drops or spends one copy by form.
- **Description**: `improve_item` improves a single-copy row in place, so that row now carries an `ItemInstance` (`(true, None)` arm). `Inventory::add` folds a plain stack into an existing plain row of the form and otherwise appends, so the later plain copy lands in a new row after the improved one. `remove_items(world, source, form, count, ...)` walks `items` in index order and takes from the first row of the form with `count > 0`. Selling "one Iron Sword" through `barter::apply_action` calls it by form only, and so do `DropObject` and a scripted `RemoveItem`. The improved row is first, so it goes: its instance moves to the merchant (or its `ItemInstancePool` entry is freed on a drop), and the plain copy stays. Barter prices a per-instance item "as its base", as documented, so the player also gets the unimproved price. When the improved copy is appended later (the split-off `(false, _)` arm) the plain row precedes it and is consumed first, so the loss only appears when the improved row is the lower index.
- **Evidence**:
  ```rust
  for (index, stack) in inventory.items.iter_mut().enumerate() {
      ...
      if stack.base_form_id != item_form_id || stack.count == 0 { continue; }
  ```
  There is no preference for an uninstanced row. `Inventory::add` gives an instanced row no plain twin to merge with.
- **Impact**: An improved weapon or armor piece is silently lost, with its quality, when its plain sibling is sold, dropped or removed by a script. The player cannot choose the copy: `BarterAction` carries only a form. This is not a crash and the loss is bounded, but it is irreversible and only appears once improved items exist, which began with M43.
- **Related**: #5871 (the add-fold rule), #5863 (`remove_items` unequip), barter doc "per-instance items are traded as their base".
- **Suggested Fix**: In `remove_items`, take plain rows before instanced ones (a two-pass walk over the matching rows), then instanced rows in descending quality or index. Add a case to the `crafting.rs` or `script_requests.rs` tests that improves one copy, adds a plain copy, removes one, and expects the improved one to survive.

### LOW

### GAME-D7-2026-10-31-01: The `Access` rows of `interaction_system`, `container_loot_system` and `combat_damage_system` omit what M43 made them acquire
- **Severity**: LOW
- **Dimension**: Stage Order & Declared Access (Dim 7)
- **Location**: `byroredux/src/boot/schedule/update/interaction_and_crime.rs:75-117` and `:140-175`; `byroredux/src/boot/schedule/update/player_animation.rs:21-84`
- **Status**: NEW (M43 commits 851636a13, 2c55546f4, ccccbaf03, 9e17ebce1, 31aca901d, 9c6c37cd0)
- **Description**: The three exclusives keep a declared `Access` row that the comments call the report the analyzer uses. `interaction_system` now also reads `CraftingBook`, `LockpickItem`, `TerminalBases`, `SmTree` and `ObScriptLedger` (the jail bed candidates), writes `CraftingRequest`, and writes `StoryEvent` and `PendingScriptStoryEvents` through `unlock_with_carried_key` (`LOCK`/`HACK`). `container_loot_system` writes `StoryEvent` (`AIPL`, `loot.rs:270,534`). `combat_damage_system` reads `ArmorBook`, `ItemInstancePool`, `ActorValueNames` (`armor::mitigate`) and writes `StoryEvent` (`ASSU`; `KILL` since #5434). A grep of `boot/` finds none of those types in those three rows. Exclusives are not paired by the analyzer, and `assert_declares_everything_it_acquires` covers only the parallel systems and four named exclusives, so nothing fails.
- **Impact**: The `sys.accesses` report under-describes the busiest gameplay systems, which is the exact complaint #3473 fixed for the first P2 set. No behavioural effect today.
- **Suggested Fix**: Add the missing types. Add `interaction_system`, `container_loot_system` and `combat_damage_system` to a declares-everything test next to `npc_combat_ai_system_declares_everything_it_acquires`.

### GAME-D7-2026-10-31-02: `raise_actor_attach_story_event` was inserted between `raise_kill_story_event` and its doc block
- **Severity**: LOW (doc-rot)
- **Dimension**: Player Feedback & Flow / Story Manager producers (Dim 7)
- **Location**: `byroredux/src/systems/story_events.rs:370-381`
- **Status**: NEW (`4e6132a73`)
- **Description**: The `#5434 KILL` doc comment (lines 370-377) now runs straight into the `M43 OAAT` doc comment, so rustdoc attaches both to `raise_actor_attach_story_event` and `raise_kill_story_event` (line 408) has no documentation. The `KILL` text states the slot contract (R1 victim, R2 killer, L1 session LCTN) that the five death producers rely on.
- **Impact**: Wrong generated docs on two functions. Same shape as ESM-2026-10-31-04.
- **Suggested Fix**: Move the `OAAT` function above the `#5434` doc block, or re-attach the block to `raise_kill_story_event`.

## Known-Open Register (cited, not re-filed)

- **Gameplay**: #4232 (`effective_actor_level` returns 0), #6350 (VATS tracker; #6422 still open for Concentrated Fire only), #6440 / #6441 (VATS Damage Threshold fold, skill fallback), #6429 (`vats_mode_system` declared access), #6313 (encumbrance / head-look per-frame cost), #6259 (Starfield bleedout), #6061 (dt2 force-greet open leg), #6065 (FO4 XRGD poses on non-human rigs), #6435 (player-body gender vs race source).
- **Suite duplicates**: ECS-2026-10-31-01 / CONC-2026-10-31-01 (HIGH, `improve_plan` lock order; the crafting guard test fails under `BYRO_LOCK_ORDER_CHECK=1`), ECS-2026-10-31-02 / CONC-2026-10-31-02 (`mine_system` pin; its "design note" about the overwrite is superseded by GAME-D4-02), CHAR-2026-10-31-M-01 / L-01 / L-02 (armor GMST hardcoding, duplicated coefficients, `CrimeModel::SKYRIM` fallback in `send_player_to_jail`), REG-2026-10-31-04 (nine `m43-*.sh` smokes in no contract loop), PERF-2026-10-31-D7-01 / D7-02.
- **Documented gaps, not defects**: barter prices per-instance items as their base; `armor.rs` omits the Custom Fit / Matching Set perk terms; improved-item crafting has no animation; `SpentMine` / `ArmedMine` / `ConversationHold` / `DeadBodyState` are session-only by design (the stated reasons are accurate except for the enable-ledger claim in GAME-D4-01); attraction rules with *Combat Target* are left out unsourced.
- **Observed, not filed**: `dead_body_discovery_system` and `attraction_system` let an unconscious, restrained or `AiSuspended` actor discover a body or react to an object (they do not call `collect_unconscious`). Both only raise a Story Manager event, which a quest condition can filter, so I did not rate it as an AI-consumer miss. Skill drift: the Dim 4 text still says `hit_event_consumers_run_after_every_producer` omits `vats_confirm`; the pin is a registration-order check and `vats_confirm` runs in the debug drain after cleanup, which this audit confirms is harmless.

Next step: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-10-31.md` (labels `gameplay` + `combat` / `ai` / `inventory`; `game:fo4` on GAME-D4-01, GAME-D4-02; `game:skyrim` on GAME-D1-01).
