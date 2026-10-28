**HEAD**: fa6be4a6b · **Baseline**: `AUDIT_GAMEPLAY_2026-10-21.md` (`4a3e89d56`) · **Audited**: Dim 1 (inventory, delta), Dim 2 (interaction / dialogue / jail delta), Dim 4 (combat, death, VATS 3a–3c), Dim 5 (the 2026-10-24 package runtimes), Dim 7 (save coverage, stage order, determinism) · **Unchanged since baseline (skimmed)**: Dim 3 (its paths had no commits; I read the #6229 fix in `inventory.rs`), Dim 6 (3 small commits)

# Gameplay Audit, 2026-10-27

This audit ran inside the comprehensive suite, solo, with no sub-agents. I made no source edits and did not launch the engine. 395 commits landed since the baseline. Per dimension, path-scoped: Dim 1 had 25, Dim 2 29, Dim 3 0, Dim 4 26, Dim 5 40, Dim 6 3, Dim 7 39.

## Scope and evidence

- **Baseline tests**: I ran `cargo test -p byroredux --bin byroredux -- combat:: inventory:: interaction:: npc_spawn:: reference_state ambient_locomotion walk_anim eat_sleep find_package flee_package use_weapon_package crime:: forcegreet dialogue_voice vats save_io:: consum restoration timed_ notifications loading_screen spawn_tests revive knockdown flee` in the shared repo `target/`. Result: **598 passed, 0 failed, 25 ignored**. No test exercises any finding below.
- **Prior findings**: all eight 10-21 findings are closed and their fixes are in the tree: #6229, #6230, #6231, #6232, #6286, #6287, #6288, #6289. I re-read the code for #6229, #6287, #6288 and #6289.
- **Dedup**: I checked the findings against `/tmp/audit/open_issues_snapshot.txt` (84 open issues), against suite § 6 (safety, renderer, regression, ecs, concurrency, performance, nifal, nif, esm, physics, parsers, character), and against closed-issue searches (`UseWeapon`, `GAME-D`, quest-alias). Several VATS items are already filed and are cited here, not re-filed: ECS-2026-10-27-02 (lock order), ESM-2026-10-27-01/02/03 (data decode), PERF-D1-01, CHAR-2026-10-27-M-01 (Damage Threshold applied once to folded shots), CHAR-2026-10-27-L-01 (skill fallback), #6429 (access declaration), and PHYS-D3-2026-10-27-01 (PushActorAway never pushes).
- **Not checked**: smoke scripts, anything that needs Vulkan, and `--ignored` real-data tests. All findings are static. Vanilla counts I could not measure are marked as unmeasured.

## Executive Summary

| Severity | NEW | Regression | Existing |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 2 | 0 | 0 |
| LOW | 3 | 0 | 0 |

The 2026-10-24 package runtimes and the M47.3 script-actor states are mostly sound. The two medium defects share one pattern: **a new runtime that skips a shared rule the older gameplay paths enforce.**

- The Find package raises its activation outside the next-frame activation queue (GAME-D5-01).
- A scripted `UseWeapon` volley ignores the shared incapacitation gate (GAME-D4-01).

Games reasoned about (statically): FNV and FO3 (Find, UseWeapon, FleeNotCombat, VATS) and Oblivion (jail bed, StolenGoods chest).

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path | **VERIFIED** | `vats_confirm` is a `HitEvent` producer. Its console callers run in `DebugDrainSystem`, which `debug_server::start` (`main.rs:1193`) registers after every boot system, so after `event_cleanup_system`. The event therefore reaches the next frame's Update consumers. Scripted volleys go through `PendingTrapHits` / `PendingProjectileLaunches`. |
| Death reconciled once | **VERIFIED** | Every production `Dead` insert reaches the reconciler: `combat.rs:380`, `scripted_kill_system` `:1278`, `extensions/commands.rs:562`, `water.rs:78`, `character.rs:1659`, and restore / starts-dead. The live sites call `reconcile_new_death` (#6288). `reconcile_dead_actor` drops `HeadLook` (#6289), and `clear_ambient_behavior`'s remover table covers the Flee / Ambush / Find / UseWeapon rows. |
| Index stability | **VERIFIED**, one residual | Production `Inventory::push` is used only by the spawn-time CNTO builds. Every other add path folds through `add`, including `take_placed_item` (Find's NPC taker) and `deposit_into_reference` (jail chest). Residual: one held-count sum was missed by #6363 (GAME-D1-01). |
| Fail-closed consumables vs fail-open packages | **VERIFIED**, one gap | `caster_effects` (#6229) refuses the whole item. Package CTDAs stay fail-open by policy. Find, UseWeapon and Dialogue apply the #5376 fully-modelled gate; Flee and Ambush do not (GAME-D5-02). |
| Stage order | **DRIFTED** | Find's PostUpdate `ActivateEvent` insert misses every Update consumer (GAME-D5-01). The pinned order tests are green. |
| State saved or re-derived | **VERIFIED** | Every 10-24 package component and M47.3 transient has an allowlist row whose re-derivation path exists: spawn install plus ambient re-selection, tombstones for taken items, and `ObScriptLedger` evidence for undelivered goods. |
| Determinism | **VERIFIED** | Grepping `rand::|Instant::now|SystemTime|thread_rng` over combat, inventory, interaction, crime, `ai_package` and every Dim 4–5 system (VATS, the package movers, `script_requests`) finds nothing. The `#[allow(dead_code)]` sites are the two #4464 rows and `InputAction::Pause` (reasoned). |

## Findings

### MEDIUM

### GAME-D5-2026-10-27-01: The Find package's "activate" arrival inserts `ActivateEvent` directly from PostUpdate, so every Update OnActivate consumer misses it
- **Severity**: MEDIUM
- **Dimension**: NPC Spawn → AI Package (Dim 5); stage order (Dim 7)
- **Location**: `byroredux/src/systems/find_package.rs:375-380` (registered in `Stage::PostUpdate`, `byroredux/src/boot/schedule/post_update.rs:127`)
- **Status**: NEW (from `fcd1cce4a`, 2026-10-24)
- **Trigger**: FO3/FNV. A Find package whose target classifies as `Arrival::Activate`, that is, a non-actor, non-pickup reference such as an activator, door or container (`classify_arrival`, `:396-410`). The actor reaches it.
- **Description**: `PendingFragmentActivations::push` (`crates/scripting/src/fragment/state.rs:891-901`) documents itself as "the supported way for any producer that runs *after* an `ActivateEvent` consumer to reach every consumer exactly once". It warns that a direct insert "is delivered to whichever consumers happen to be scheduled later and drained by `event_cleanup_system` the same frame". This hazard is closed issue #2654 for quest fragments, #3936 for scene-package Activate leaves, and #4712 for loot. `find_package_system` runs in PostUpdate and calls `events.insert(target, ActivateEvent { activator: actor })` directly. By then every Update consumer has already run: `fragment_activation_flush_system`, `door_activation_system`, `container_loot_system`, `two_state_activator_system`, the ObScript object-script and activate-parent passes, and the Papyrus OnActivate dispatch. Only the Late readers (`late.rs:437-551`) see the event before cleanup removes it.
- **Evidence**:
  ```rust
  Arrival::Activate => {
      if let Some(mut events) = world.query_mut::<byroredux_scripting::ActivateEvent>() {
          events.insert(target, byroredux_scripting::ActivateEvent { activator: actor });
      }
      progress.done = true;
  }
  ```
- **Impact**: An authored "find X and activate it" never runs the target's `OnActivate` script, never flips a two-state activator, and never reaches the Update script blocks. Late consumers see it, so the delivery is inconsistent. `progress.done = true` means the package never retries, so the miss is silent and permanent until the package is re-installed. The number of the 81 FNV / 78 FO3 Find packages whose target is an activator was not measured.
- **Related**: #2654, #3936, #4712 (the same class, closed); `activation_flush_is_scheduled_before_every_activate_event_consumer` pins consumers, not producers.
- **Suggested Fix**: Push through `PendingFragmentActivations` (`push`, or `push_native` if default processing only is meant) so the event is delivered at the head of the next frame. Add a PostUpdate-producer case to the activation-order guard.

### GAME-D4-2026-10-27-01: A script-started `UseWeapon` volley keeps firing from a knocked-down, fleeing, bleeding-out, unconscious or AI-disabled actor
- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4); M47.3 script-driven actor states
- **Location**: `byroredux/src/systems/script_requests.rs:40-145` (`use_weapon_system`; the `alive` closure at `:62`); `byroredux/src/npc_spawn/ai_package.rs:845-850`; `byroredux/src/systems/knockdown.rs:128-141`
- **Status**: NEW (`use_weapon_system` from `07bd22a6b`, 2026-10-18)
- **Trigger**: FO3/FNV. An ObScript `UseWeapon` volley is in progress, and the shooter then receives `PushActorAway`, `ForceFlee`, an essential-actor bleedout, `SetUnconscious` or `SetActorsAI 0`.
- **Description**: The skill's rule is that every AI consumer honours `AiSuspended`, unconscious, `SetActorsAI 0` and bleedout through `byroredux_scripting::collect_unconscious` (scalar form `is_incapacitated`). `use_weapon_system` checks only `Dead` (`alive = ActorVitals absent || !Dead`).
  - `release_actor_for_ragdoll` and `flee_system` tear the actor down through `clear_ambient_behavior`.
  - `clear_ambient_behavior` removes `UseWeaponRun` only when `UseWeaponPackageStarted` is present. That is deliberate, so a scripted volley survives a package handover, but it also means a scripted volley survives incapacitation.
  - Each frame the run then rewrites the actor root's `Transform.rotation` toward the target. It is registered at `update.rs:723`, after `flee_system` at `:720`, so it overrides the flee facing.
  - It still pushes `PendingProjectileLaunches` or `PendingTrapHits` shots.
- **Evidence**: `if !alive(actor) || !alive(target) || run.request.shots == 0 { end_run(..) }` is the only state gate. There is no `is_incapacitated` call anywhere in `script_requests.rs`.
- **Impact**: A ragdolled or bleeding-out actor keeps shooting. A `ForceFlee`d actor runs away facing its target and firing. For Repeat Fire volleys this lasts until `PATIENCE_SECS` or the shot count runs out. The number of vanilla scripts that pair `UseWeapon` with these states was not measured.
- **Related**: GAME-D4-2026-10-21-01 / #6230 (the knockdown teardown), #6287 (`is_incapacitated`), PHYS-D3-2026-10-27-01 (PushActorAway never pushes; independent).
- **Suggested Fix**: Skip, or end, the run while `byroredux_scripting::is_incapacitated(world, actor)` holds. Ending it matches vanilla's "the package is interrupted"; skipping would let it resume after a get-up. Add a knockdown/flee case to the `use_weapon_system` tests.

### LOW

### GAME-D5-2026-10-27-02: FleeNotCombat (and Ambush) install from a fail-open condition pass; the #5376 fully-modelled gate covers only Find, UseWeapon and Dialogue
- **Severity**: LOW (latent until a vanilla Flee package with an uncatalogued CTDA is shown)
- **Dimension**: NPC Spawn → AI Package (Dim 5)
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:383-401` (the `is_flee_not_combat` / `is_ambush` arms), `:180-191` (`selectable_by_installer`)
- **Status**: NEW (`b540b104a`, `9aaf5e1cf`, 2026-10-24)
- **Description**: `use_weapon_behavior` and `find_behavior` return `None` unless `package_conditions_fully_modeled` holds, because a fail-open pass would start an intrusive takeover on content the quest has not reached. The Flee arm builds `Self::Flee { … }` unconditionally, and so does Ambush. FleeNotCombat takes the actor over: it runs from its `PTDT` target or holds at a refuge. A quest-gated flee whose gate uses an uncatalogued function (FNV's fn 79 `GetQuestVariable` was the #5376 example) would fire on first contact. The skill's Dim 5 text says every 10-24 runtime applies the rule. The doc (`npc-spawn-ai-packages.md`) states it only for UseWeapon and Find.
- **Impact**: An NPC flees, or waits at an ambush post, out of quest sequence. How many of the 23 FNV/FO3 Flee packages carry an unmodelled CTDA was not measured. If none do, this stays latent until mod content triggers it.
- **Suggested Fix**: Gate Flee (and Ambush, if Ambush is judged intrusive) on `package_conditions_fully_modeled` in `from_package` and `selectable_by_installer`, or record in the doc why Flee and Ambush are exempt.

### GAME-D4-2026-10-27-02: `vats.enter` claims "same gates as the key" but skips the fly-cam and `Control::Vats` gates; `vats.scan`/`queue`/`confirm` have no mode gate
- **Severity**: LOW
- **Dimension**: Combat & Death (Dim 4, VATS plumbing)
- **Location**: `byroredux/src/commands/vats.rs:13-52` vs `byroredux/src/systems/vats.rs:48-66`
- **Status**: NEW (`818eba7f4`, `7eb4de261`)
- **Description**: The command doc says "same gates as the key: model, living player, enabled control domain". The command checks the model and `player_can_act`, but not `PlayerMode != FlyCam` and not `control_vats_enabled`. `player_can_act` returns true when there is no player at all (`is_none_or`). So in fly-cam, `vats.enter` sets `TargetSelect`: `dt` pins to 0, the world freezes, and the scan is empty. A script's `DisablePlayerControls`-style VATS disable is also ignored on the smoke gate-of-record surface. `vats.scan`, `vats.queue` and `vats.confirm` work with `mode == None`, so a dead player can land VATS hits from the console.
- **Impact**: Debug and console surface only. The gate list is spelled twice and has already diverged.
- **Suggested Fix**: Extract the entry gates into one `vats_entry_refusal(world) -> Option<&str>`, shared by `vats_mode_system` and `vats.enter`. Have `vats_queue_part` and `vats_confirm` refuse when `mode == None`.

### GAME-D1-2026-10-27-01: `item_count` (GetItemCount CTDA 47 and its ObScript getter) is a twelfth hand-spelled held-count sum, and it does not saturate
- **Severity**: LOW
- **Dimension**: Inventory (Dim 1)
- **Location**: `crates/scripting/src/condition/eval/actor.rs:223-236`
- **Status**: NEW (#6363 / 5b43fe347 swept eleven sites and missed this one, added in f57c75616 #6251)
- **Description**: `inventory.items.iter().filter(|s| items.contains(&s.base_form_id)).map(|s| s.count).sum()` is the form-list form of the pattern `Inventory::count_of` replaced. The plain `u32` `.sum()` panics on overflow in debug builds and wraps in release, while `count_of` saturates (pinned by the `u32::MAX` test in `inventory.rs`).
- **Impact**: Only for contrived counts above u32::MAX, summed across rows. The main cost is maintainability: the one-accessor rule is broken at its most-used reader.
- **Suggested Fix**: Sum `inventory.count_of(form)` over the form-list members with `saturating_add`, or add an `Inventory::count_of_any(&[u32])`.

## Known-Open Register (cited, not re-filed)

- #4232 (`effective_actor_level` returns 0), #6350 (VATS tracker), #6422 (AP / fold, partly fixed by 1fb7e6f56), #6429 (`vats_mode_system` declared access), #6313 (encumbrance / head-look per-frame cost), #6356 / #6357 (`inventory.rs` / `npc_spawn.rs` size), #6375 (status floors for the 10-24 procedures), #6428 (`RadiationRads` has no writer), #6435 (player-body gender vs race source), #6259 (Starfield bleedout recovery).
- Suite duplicates: ECS-2026-10-27-02 / CONC-2026-10-27-01 (`vats_scan` lock order), ECS-2026-10-27-04 (`GearImportLoader.failed_forms` not pruned), ECS-2026-10-27-05 (`player_controller_system` story-event write), ESM-2026-10-27-01/02/03 (BPTD / WEAP VATS decode), PERF-D1-01, CHAR-2026-10-27-M-01 / L-01, PHYS-D3-2026-10-27-01, NIFAL-D4-2026-10-27-01 (legacy-bed Sleep doc-rot), NIFAL-D8-2026-10-27-01.
- Skill drift (no code finding): Dim 5's "Sleep … reachable only on Skyrim+; on FO3/FNV the sit fallback IS the sleep path" is stale, because 0c6cd46dc marks legacy furniture markers 1–10 as Sleep. The Dim 5 guard name `walk_animation_registers_after_all_seven_movers` is noted as stale in the skill itself.

Next step: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-10-27.md` (labels `gameplay` + `ai` / `combat` / `inventory`; `game:fnv` / `game:fo3` on GAME-D5-01, GAME-D4-01 and GAME-D5-02).
