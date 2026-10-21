**HEAD**: 4a3e89d56 · **Baseline**: `AUDIT_GAMEPLAY_2026-10-19.md` (`41f9c2c6c`, area-scoped) for the halves of Dims 2/5/7 it covered; `AUDIT_GAMEPLAY_2026-10-15.md` (`9e11eb1fb`, the last full run) for everything else · **Audited**: Dim 1 (inventory, delta), Dim 2 (dialogue refusal and eviction delta), Dim 3 (consumables), Dim 4 (combat and death, including the M47.3 knockdown / revive / flee / head-look / magic-effect runtime), Dim 5 (spawn → package → locomotion delta), Dim 6 (feedback), Dim 7 (stage order, save coverage, determinism) · **Unchanged since baseline (skimmed)**: none. Every dimension's paths had commits (402 since `9e11eb1fb`). Dim 6 got the lightest pass: its delta is notifications and level-up wording, and no finding came out of it.

# Gameplay Audit, 2026-10-21

Comprehensive suite, solo run (no sub-agents). I made no source edits and launched no engine.

## Scope and evidence

- **Delta read.** The 2026-10-18 M47.3 ObScript wave added most of the new gameplay code: `8d32b3fe9` (`knockdown.rs`), `d1b298921` (`revive.rs`, `head_look.rs`, `magic_effects.rs`, and ObScript/caster effects on ingestibles in `consume_item` / `install_catalog`), and `07bd22a6b` (flee). I also read the fixes for the last two reports' findings: #5859 / #5860 / #5861 / #5862 / #5863 / #5864 / #5865 / #5866 / #5867 / #5868 / #5869 / #5870 / #5871 / #5872 / #5874 / #5875 / #5876, and #6071–#6074 (`611769b16`) and #6085 (`3b9060701`) for the 10-19 stream-boundary findings. Other commits read: #6079 (PSDT day of week), #6078 / #6100 (creature and Oblivion idle clips), #6128 / #6132, and #6007 / `8daa24105` (topic-click INFO reuse).
- **Baseline tests**: `cargo test -p byroredux --bin byroredux -- combat:: inventory:: interaction:: npc_spawn:: reference_state ambient_locomotion walk_anim eat_sleep forcegreet dialogue_voice save_io:: consum restoration timed_ notifications loading_screen spawn_tests revive knockdown flee`, built in the shared repo `target/`: **500 passed, 0 failed, 24 ignored**. I also ran the skill's 23 named guard tests by name (`hit_event_consumers_run_after_every_producer`, `every_delta_column_is_parked_or_explicitly_not`, `locomotion_kill_switch_gates_exactly_the_motion_systems`, `walk_movers_share_one_pass_two_write_back`, `post_reload_drain_steps_keep_their_order`, and the rest). All 23 are green. **No test exercises any finding below.**
- **Not checked**: smoke scripts, anything that needs a Vulkan device, real-data `--ignored` tests. All findings are static: code read and call paths traced. I loaded no cell. Where a finding depends on how often vanilla content takes a path, the count was not measured, and the finding says so.
- **Dedup**: `/tmp/audit/issues.json` (119 open), `/tmp/audit/SUITE_FINDINGS.md` (esm / renderer / scripting / nif), the 10-21 scripting report, and closed-issue searches (`stand_up`, `Resurrect`, `PushActorAway`, `knockdown`, `#5869`, `#5870`, `#5864`). None of the findings below overlaps SCR-2026-10-21-01…12, ESM-2026-10-21-01…05 or the REN / NIF lists. SCR-2026-10-21-12 (ResetInventory pool release) and ESM-2026-10-21-01 (PSDT duration unit) are cited, not re-filed.

## Executive Summary

| Severity | NEW | Regression | Existing (open) re-confirmed |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 4 | 0 | 0 |
| LOW | 4 | 0 | 2 (#5495 with new consumers, #5721) |

Every closed finding from the 10-15 and 10-19 reports is fixed in the tree and stayed fixed. The new defects all come from the M47.3 caster / script-actor-state runtime added on 10-18, and they share one theme: **the new runtime bypasses gates that the older gameplay paths enforce.**

- Ingestibles now reach the caster runtime around the fail-closed consumable gate (GAME-D3-01).
- Lethal magic damage skips the death edge (GAME-D4-03).
- A refused ragdoll activation leaves an actor half torn down (GAME-D4-01).
- The shared stand-up restores a humanoid clip on creatures (GAME-D4-02).
- The new "out of the AI" state split the two dialogue-refusal forms (GAME-D2-01).
- Death does not tear down two of the new per-actor components (GAME-D4-04, GAME-D4-05).

Games and paths reasoned about (statically): FNV / FO3 / Oblivion (ObScript `PushActorAway`, `TrapUpdate`, `Resurrect`, `Cast`, `Look`, ingestibles), and Skyrim (potions and poisons through `install_catalog`).

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path | **VERIFIED** for hits | Projectile and trap hits queue on `PendingTrapHits` and land in `pending_trap_hit_flush_system` (`update.rs:604`). The hit consumers run after it (`:608-618`), then `combat_damage_system` (`:649`). `hit_event_consumers_run_after_every_producer` is green. **Drift outside the hit path:** the caster runtime writes Health damage directly (GAME-D4-03). |
| Death reconciled once | **DRIFTED** | Every listed producer still reaches `reconcile_dead_actor`, and ObScript `SetAV` / `DamageAV` / `ModAV` / `ForceAV` reach `queue_health_death` (`evaluate.rs:636`). But a lethal magic value effect never reaches it (GAME-D4-03), and the reconciler did not grow for `HeadLook` / `ActiveMagicEffects` (GAME-D4-04, GAME-D4-05). |
| Index stability | **VERIFIED**, one residual | `Inventory::add` (`core/.../inventory.rs`) folds plain stacks. `add_items`, the `remove_items` destination, `transfer_loot` and `pickup_loot` use it. The quest-alias grant still appends (GAME-D1-01). `ResetInventory` replaces a container's inventory wholesale, but only for `CONT` bases, which have no equipment (`script_requests.rs:186-197`). |
| Fail-closed consumables vs fail-open packages | **DRIFTED** | `restoration_plan` is still all-or-nothing and fail-closed. `install_catalog` now sends every plan-less ALCH through `consumable_cast_effects`, which ignores the effect CTDAs and `simple_consumption_header` (GAME-D3-01). Package CTDA stays fail-open as designed. |
| Stage order | **VERIFIED** | Update: `restoration` → `bleedout` → `interaction` → … → `ambient_ai_package_system` (`:465`) → force-greet → … → trap flush (`:604`) → hit consumers → `magic_effect_system` (`:622`) → `revive` / `push_actor` / `get_up` / `flee` (`:630-636`) → `combat_damage_system` (`:649`). The pinned order tests are green. |
| State saved or re-derived | **VERIFIED** for the 10-19 drift | `ReferenceState` now carries `script_package`, `corpse_pose`, `magic_effects`, `weapon_drawn` and `ignore_crime`. The `scripts` map carries `TwoStateActivator` / `ScriptVariables` / `ScriptTimer` / `RumbleOnActivate`. `install_package_now` fixes #6074. `KnockedDown` / `FleeState` / `AiSuspended` / `HeadLook` are `NOT_SAVED_BY_DESIGN` and transient. Their loss on save leaves an actor standing with its package re-selected, because suspension already reset the winner. |
| Determinism | **VERIFIED** | `rand::|Instant::now|SystemTime|thread_rng` over `combat.rs`, `inventory.rs`, `interaction.rs`, `ai_package.rs`, the Dim 5 movers, `faction_hostility` / `npc_dialogue` / `forcegreet` / `dialogue_voice` / `knockdown` / `flee` / `revive` / `head_look`, `magic_effects.rs` and `obscript_speech.rs` finds nothing. No new `#[allow(dead_code)]` in gameplay files; the two `#4464` allowances still stand. |

## Findings

### MEDIUM

### GAME-D3-2026-10-21-01: An ingestible with no restoration plan is consumed through the caster runtime, which skips the effect conditions and the poison/addiction header, and drops unsupported effects without notice
- **Severity**: MEDIUM
- **Dimension**: Consumables & Timed Effects (Dim 3; the fail-closed policy)
- **Location**: `byroredux/src/inventory.rs:517-527` (`install_catalog` builds `script_effects`), `:1484-1491`, `:1570-1573` (`consume_item` starts them); `crates/scripting/src/magic_effects.rs:80-105` (`cast_effect`, `consumable_cast_effects`)
- **Status**: NEW (from `d1b298921`, 2026-10-18)
- **Trigger**: Any game whose `Aid` items carry `authored_effects` (Skyrim, FO3, FNV; Oblivion takes this path for every potion because `restoration_plan` returns `None` for it). The player uses an ALCH that `restoration_plan` refuses for any reason. The reasons include:
  - an effect-local CTDA other than `GetHasPerk` / NV `IsHardcore`;
  - one effect on an actor value outside the `restoration()` map;
  - a touch or target delivery;
  - a cleared `simple_consumption_header` (attached script, poison, addiction, withdrawal).
- **Description**: `restoration_plan` is all-or-nothing and fail-closed by design: an unsupported condition, effect, delivery, header or AV means `None`, "never a partial potion". `install_catalog` now adds a second table. For every item without a plan, it takes all of `consumable_cast_effects(index, id)` that `is_modelled()` (scripted, or a value modifier). `consume_item` treats a non-empty table as consumable and calls `magic_effects::start_effects` on it. `cast_effect` reads only `entry.effect`. It never looks at `ConsumableEffect.conditions`, `delivery` or `area`, and `consumable_cast_effects` never checks `simple_consumption_header`. As a result:
  - effect conditions that the plan path refused fail **open** here;
  - the sibling effects the runtime does not model are dropped (a partial potion);
  - poisons, whose detrimental value effects the header excluded, are "drunk" by the player.
- **Evidence**: `inventory.rs:522-524`:
  ```rust
  let effects: Vec<_> = byroredux_scripting::magic_effects::consumable_cast_effects(index, id)
      .into_iter()
      .filter(|effect| effect.is_modelled() && (!planned || effect.script != 0))
  ```
  `magic_effects.rs:102-105` maps `entry.effect` only. `restoration_plan` (`crates/plugin/src/consumables.rs:51-127`) is the only place that evaluates `conditions` / `condition_flags`, `delivery` and the header. The menu flag at `inventory.rs:1431-1433` now reports these items as consumable, while their catalog detail line still reads "Consumable (effects unavailable)" (`:553`).
- **Impact**: An item's conditional effects all apply at once, perk-gated or mode-gated variants included. Skyrim / Oblivion / Fallout poisons used from the inventory damage the player. That damage has no death edge (GAME-D4-03), so a strong enough poison leaves the player alive at Health ≤ 0. I did not count how many vanilla ALCH records each game routes here (no data read).
- **Related**: GAME-D4-03; the `restoration_plan` guards `unavailable_consumption_never_changes_health_or_count` and `poison_conditions_scripts_duration_and_malformed_payloads_are_unavailable` stay green because they test the plan path, not this one.
- **Suggested Fix**: Build `script_effects` only for items whose header is simple-consumption (or whose only header bit is the attached script). Refuse the whole item when any effect carries a CTDA the plan path cannot evaluate, or a non-self delivery. Keep the all-or-nothing rule: if any authored effect is not modelled, the item stays "effects unavailable". Fix the detail string to match. Add a guard test: a poison and a CTDA-gated effect are both `Unavailable`.

### GAME-D4-2026-10-21-01: `PushActorAway` tears an actor's animation, package and fight down before ragdoll activation can fail; on failure it is left frozen with no package, and the player loses its body idle
- **Severity**: MEDIUM
- **Dimension**: Combat & Death Pipeline (Dim 4, M47.3 script-driven actor states)
- **Location**: `byroredux/src/systems/knockdown.rs:90-125` (`knock_down`), `:130-142` (`release_actor_for_ragdoll`); unchanged-winner short-circuit `byroredux/src/npc_spawn/ai_package.rs:1129-1136`
- **Status**: NEW (from `8d32b3fe9`, 2026-10-18)
- **Trigger**: FO3 / FNV / Oblivion. ObScript `PushActorAway` (`obscript_more/actions.rs:94-101`), or `TrapUpdate` with a push force (`obscript_more/extras.rs:323-327`, which pushes every actor in contact). The target can be one of:
  - the player, whose visual body has no `RagdollTemplate` (`npc_spawn/resumable/mod.rs:52`);
  - an NPC whose seed pose `activate_ragdoll` rejects (#5161, `ragdoll.rs:327-...`);
  - any actor without a template.
- **Description**: `knock_down` runs `release_actor_for_ragdoll` first. That calls `clear_ambient_behavior`, removes `AiCombatState`, and removes `AnimationPlayer` / `AnimationStack` on the actor and its skeleton root. Only then does it call `activate_ragdoll(world, root)?`. On `Err`, the `?` returns before `KnockedDown` and `AiSuspended` are inserted. `get_up_system` therefore never sees the actor, and `stand_up` (which re-inserts the idle and calls `rearm_ai`) never runs. `clear_ambient_behavior` deliberately leaves `AmbientPackageRuntime.active_package_form_id` set. On the next ambient tick the winner is unchanged, and the `!changed => continue` branch never reinstalls the cleared behavior.
- **Evidence**: `knockdown.rs:101-104`:
  ```rust
  release_actor_for_ragdoll(world, actor, root);
  if world.get::<crate::ragdoll::RagdollActive>(root).is_none() {
      crate::ragdoll::activate_ragdoll(world, root)?;
  }
  ```
  The player capsule carries `AnimationTarget` → body skeleton plus the KF idle `AnimationPlayer` (`player_body.rs:353-399`). Both are removed, and nothing re-inserts them.
- **Impact**:
  - **NPC**: it stops animating and stands in its last sampled pose with no package behavior until the schedule's winner changes. A scripted fight it was in is lost.
  - **Player**: the third-person body loses its idle for the session; only the walk take animates it.
  - **Traps**: `TrapUpdate` re-raises the push every frame of contact while `continuous` is set, so the warning repeats.
  - The failure is logged only as `PushActorAway: actor … not pushed`. I did not count how often vanilla scripts target the player.
- **Related**: GAME-D4-02 (the success path's `stand_up`); #5161 (seed rejection).
- **Suggested Fix**: Check `RagdollTemplate` presence (or activate first) before releasing anything. On activation failure, restore nothing and return. Refuse the player explicitly, the same way `knock_down` refuses the dead. Pin it with a test: a template-less actor keeps its `AnimationPlayer` and its installed behavior.

### GAME-D4-2026-10-21-02: `stand_up` (revive and knockdown get-up) gives every actor the humanoid idle, so a revived or pushed creature stands up with a clip that binds none of its bones
- **Severity**: MEDIUM
- **Dimension**: Combat & Death Pipeline (Dim 4, the reverse death edge)
- **Location**: `byroredux/src/systems/revive.rs:102-111` (`stand_up`)
- **Status**: NEW (from `d1b298921`, 2026-10-18; #6078 `578d8432a` and #6100 `abd3d5001` fixed the spawn paths but not this one)
- **Trigger**: FO3 / FNV / Oblivion. ObScript `Resurrect` / `ResurrectActor` on a creature (`ReviveRequest`, `obscript_effects.rs:599-600`), or a `PushActorAway` / `TrapUpdate` knockdown of a creature that then gets up (`knockdown.rs:187`).
- **Description**: `stand_up` resolves the idle as `humanoid_default_idle_kf_path(game)`, i.e. `_male\locomotion\mtidle.kf` (FO3/FNV) or `_male\idle.kf` (Oblivion), and inserts it on the actor with `with_root(skeleton_root)`. A creature's own idle comes from `creature_idle_kf_paths(game, dir)` beside its skeleton (`npc_spawn.rs:610-626`). The function's own doc says creatures "do not share the humanoid `idle.kf` — a rat's skeleton has none of the humanoid bone names". The ragdoll is freed, but no clip moves the creature's bones out of the pose the ragdoll last wrote. The root `Transform` stands up while the skeleton stays in its collapsed pose, and its re-keyframed bone bodies follow that pose.
- **Evidence**: `revive.rs:102-110`:
  ```rust
  let idle = ... .and_then(|game| {
      crate::npc_spawn::humanoid_default_idle_kf_path(game).and_then(|path| { ...clips.get_by_path(path) })
  });
  ```
  Nothing branches on `ActorBodyClass` or the creature skeleton directory.
- **Impact**: A resurrected or knocked-down creature (FO3/FNV creatures, Oblivion creatures) gets up visibly crumpled and stays that way. The walk take can still animate it while it moves. The humanoid case is also slightly off: the restored idle loses the per-NPC `idle_desync` phase and speed. I did not count how many vanilla scripts resurrect or push creatures.
- **Related**: GAME-D4-01; #6078, #6100.
- **Suggested Fix**: Resolve the idle the way spawn does, through `creature_idle_kf_paths` for a creature skeleton and the humanoid path otherwise. Ideally share the spawn's idle-selection helper so the two cannot diverge again, and re-apply `idle_desync`.

### GAME-D4-2026-10-21-03: A lethal magic value effect has no death edge; an actor (the player included) is left alive at Health ≤ 0
- **Severity**: MEDIUM
- **Dimension**: Combat & Death Pipeline (Dim 4: every producer that can take Health to zero must reach the death path)
- **Location**: `crates/scripting/src/magic_effects.rs:313-353` (`magic_effect_system`: the instant and per-second `apply_damage` branches); `:254-260` (`change_value`)
- **Status**: NEW (from `d1b298921`). It is the class #5862 closed for script actor-value writes (`queue_health_death`, `fragment/effects/evaluate.rs:653-675`), which this new writer bypasses.
- **Trigger**: FO3 / FNV / Oblivion. A detrimental value-modifier effect on Health, instant or per second, reaches the actor through:
  - ObScript `Cast` / `CastImmediateOnSelf` (`obscript_effects.rs:525-535`);
  - an ability (`magic.rs:231`);
  - a challenge reward;
  - or, since GAME-D3-01, a drunk poison.
- **Description**: `magic_effect_system` writes `values.apply_damage(value.actor_value, -value.amount)` under its own `ActorValues` guard. It never calls `queue_health_death`, `spare_from_death` or `raise_kill_story_event`. The docs of #5862's fix state the rule: "Nothing else sweeps actors at Health <= 0". Only the next weapon hit (`combat_damage_system`) would kill such an actor.
- **Evidence**: `grep -n "queue_health_death\|PendingScriptKills\|Dead" crates/scripting/src/magic_effects.rs` finds only test lines.
- **Impact**: A poisoned or spell-damaged NPC stays standing, keeps its package, can be talked to and bartered with, and its OnDeath / KILL never fires. The player keeps acting at Health ≤ 0, because `player_can_act` tests `Dead`, not Health.
- **Related**: GAME-D3-01, GAME-D4-04, #5862.
- **Suggested Fix**: After each Health-reducing `change_value`, call the scripting crate's `queue_health_death(world, target)` (or a crate-visible wrapper) once the guard is dropped. The Late `scripted_kill_system` then spares essentials and raises KILL like every other producer. Add a test: an instant lethal Health effect queues one `PendingScriptKills` entry.

### LOW

### GAME-D4-2026-10-21-04: Death neither ends an actor's magic effects nor runs their `ScriptEffectFinish`, contrary to the module doc
- **Severity**: LOW
- **Dimension**: Combat & Death Pipeline (Dim 4, `reconcile_dead_actor` must grow with each per-actor runtime component)
- **Location**: `crates/scripting/src/magic_effects.rs:16` (doc), `:292-383` (no `Dead` skip); `byroredux/src/combat.rs:1019-1128` (`reconcile_dead_actor` has no dispel)
- **Status**: NEW (from `d1b298921`)
- **Description**: The module doc says a script effect "runs … `ScriptEffectFinish` once when it ends — the duration elapsed, or `Dispel` / death". Nothing dispels on death. The only `dispel` callers are `RemoveSpell` (`magic.rs:255`) and ObScript `Dispel` (`obscript_effects.rs:617`). `magic_effect_system` keeps running `ScriptEffectUpdate` blocks and per-second value changes on corpses until each duration lapses, and an `indefinite` effect never lapses. The list is saved and parked on corpses (#6085), so it outlives saves and evictions too.
- **Impact**: Effect scripts keep executing with a corpse as their calling reference, and an effect's Finish-time cleanup is deferred to its natural end or never happens.
- **Suggested Fix**: In `reconcile_dead_actor` (live producers only, not the corpse-restore sweep), mark the corpse's effects `ending` the way `dispel(world, actor, None)` does, so Finish runs once on the next pass. Correct the doc if a different rule is intended.

### GAME-D4-2026-10-21-05: `HeadLook` survives death: `head_look_system` keeps turning a corpse's head
- **Severity**: LOW
- **Dimension**: Combat & Death Pipeline (Dim 4, reconciler growth)
- **Location**: `byroredux/src/systems/head_look.rs:52-127` (no `Dead` / ragdoll skip); `byroredux/src/combat.rs:1019-1128`
- **Status**: NEW (from `d1b298921`)
- **Description**: ObScript `Look` stamps `HeadLook`. Only `StopLook`, or the target losing its `GlobalTransform`, removes it. `reconcile_dead_actor` does not remove it, and the system never checks `Dead` or `RagdollActive`. On a corpse it keeps writing the head bone's local `Transform`. Each frame it applies the yaw/pitch delta to whatever rotation the head currently has. The module doc assumes an animation re-poses the head every frame, and a corpse has no `AnimationPlayer`. On a corpse that still has a ragdoll, the ragdoll writes the head bone too, and which write lands last decides what is visible.
- **Impact**: Cosmetic. A dead `Look` caller's head can drift or rotate, most visibly when the ragdoll was refused and nothing else writes the bone.
- **Suggested Fix**: Remove `HeadLook` in `reconcile_dead_actor`, and skip `Dead` actors in `head_look_system` as a second guard.

### GAME-D2-2026-10-21-01: The scalar and bulk dialogue-refusal checks disagree for knocked-down, fleeing and AI-disabled actors
- **Severity**: LOW
- **Dimension**: Interaction & Dialogue (Dim 2, the #5043 / #5017 refusal contract)
- **Location**: `byroredux/src/systems/npc_dialogue.rs:647-659` (`npc_refuses_dialogue`, uses `is_unconscious`), `:661-698` (`collect_dialogue_refusals`, uses `collect_unconscious`); `crates/scripting/src/player_control.rs:222-256`
- **Status**: NEW (`8d32b3fe9` folded `AiSuspended` and `SetActorsAI 0` into `collect_unconscious` and not into `is_unconscious`)
- **Description**: `collect_dialogue_refusals`' doc says the bulk form and the scalar "must always check the same three conditions". The bulk form now also refuses every `AiSuspended` actor (knocked down, fleeing) and every `SetActorsAI 0` actor. The scalar refuses neither. The scalar form is the one the following paths use:
  - activation selection (`:754`);
  - the per-frame "partner can no longer hold a conversation" head (`:609`);
  - topic clicks (`:816`);
  - `forcegreet_open` (`forcegreet.rs:165`);
  - `begin_barter` (`barter.rs:228`).
- **Impact**: The HUD offers no "Talk" on a knocked-down or fleeing NPC. A scripted activation or `StartConversation` still opens a conversation with it, and a conversation already open stays open when the partner is knocked into a ragdoll or starts to flee. No test pins the two forms against each other.
- **Suggested Fix**: Decide the rule once (most likely: `AiSuspended` refuses, and `SetActorsAI 0` follows the GECK's behavior) and express it in a shared predicate both forms call. Add a parity test over the same state matrix.

### GAME-D1-2026-10-21-01: The quest-alias inventory grant appends a plain row per grant, bypassing the #5871 fold
- **Severity**: LOW
- **Dimension**: Inventory & Equipment Model (Dim 1, index stability and one row per plain form)
- **Location**: `crates/scripting/src/scene/quest_alias.rs:757-763`
- **Status**: NEW (residual of #5871; the code predates it, `64fed2c49`)
- **Description**: `Inventory::add` (#5871) folds a plain stack into the form's existing row. The skill names scripted `AddItem`, barter, loot and pickup as going through it. The alias-injected inventory (`ALCO`/`COCT` items and Create-In created objects) still calls `inventory.push(ItemStack::new(item, count))`. An alias that grants a form the actor already holds (the player alias included) adds a second plain row for it. The spawn-time builders (`npc_spawn.rs:1514`, `cell_loader/references/attach.rs:272`) also push, but they build fresh inventories, where a duplicate only arises from duplicate CNTO entries.
- **Impact**: Duplicate rows for one form. They are listed twice by the inventory menu and saved that way. Counts stay correct because readers sum rows.
- **Suggested Fix**: Use `inventory.add(...)` at both grant sites. Optionally build the spawn inventories through it too.

## Disproved / not filed

- **Flee leaves the package uninstalled.** It does not. `flee_system` stamps `AiSuspended`. `ambient_ai_package_system` folds it in through `collect_unconscious` and calls `suspend_ambient_behavior_for_combat` every frame, which resets `active_package_form_id`. When the flight ends, the next tick re-selects and reinstalls (`ai_package.rs:964-973`). The knockdown success path relies on the same reset and on `rearm_ai`.
- **`flee_system` is a mover outside `BYRO_NO_AI_LOCOMOTION`.** True (`update.rs:636`), but it is script-driven like the cinematic routes the skill excludes. Not filed.
- **ObScript `SetAV` / `ModAV` / `ForceAV` skip the death edge.** They do not: `SetAV` / `DamageAV` go through the effect applier, and `ModAV` / `ForceAV` through `mod_actor_value_permanent`, both of which call `queue_health_death` (`evaluate.rs:626-637`).
- **`ResetInventory` breaks equipment indices.** It replaces the whole `Inventory`, but only for `CONT` bases (`authored_container_inventory` returns `None` otherwise, `attach.rs:243`), which carry no equipment. The pool leak is SCR-2026-10-21-12.
- **10-19 GAME-SD-01…04.** All fixed by `611769b16` (#6071–#6074). I verified the row fields, the `scripts` map, `install_package_now`, and that the markers are dropped when the snapshot's package is not installable.

## Known-Open Register

| Issue | State today |
|---|---|
| #5495 creature seating | Still dormant: no production code inserts `CreatureActor` (only tests do). It now has two more consumers that read it as "humanoid": `obscript_state::note_death` counts every creature the player kills under "People Killed" instead of "Creatures Killed" (`obscript_state.rs:510-513`), which also feeds challenge kill counts, and ObScript `GetIsCreature` (`obscript_more.rs:347`) answers 0. The fix for #5495 fixes these too; not re-filed. |
| #5721 | Open. `loot_appearance.rs` was touched only by #6128 / #6132 and the exclusion chain is unchanged. |
| #6087 | Open (post-load re-seat runs before `ScriptPackage` is overlaid). Separate from the eviction fix #6072. |
| #4232, #6061, #5750, #6150, #6169 | Open, unchanged by this delta. |
| ESM-2026-10-21-01 | The Skyrim+ PSDT duration unit makes every package match every hour, upstream of `active_package`. Owned by `/audit-esm`. |
| SCR-2026-10-21-12 | The `ResetInventory` pool leak. Owned by `/audit-scripting`, gameplay-classified there. |
| Skill known-open (re-verified) | The save-load revival of a ragdolled player still never calls `stand_up` (`7aa1d7741` path). There is still no pickpocket / theft-alarm hook for #5017. Seven PACK procedures still have no runtime. |

Suggested publish: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-10-21.md` (labels `gameplay` + `combat` / `inventory` / `dialogue` / `ai`; `game:fnv` / `game:fo3` / `game:oblivion` only on the ObScript-only findings).
