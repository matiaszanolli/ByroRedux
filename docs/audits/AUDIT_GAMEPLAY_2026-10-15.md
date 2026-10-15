# Gameplay Audit — 2026-10-15

**HEAD**: 9e11eb1fb · **Baseline**: `AUDIT_GAMEPLAY_2026-10-12.md` (`5d31fd637`) · **Audited**: Dim 4 (combat and death, in depth), Dim 5 (NPC spawn → AI package → locomotion, PF_ fragments), Dim 1 (inventory, barter and script item natives), Dim 2 (interaction, activation, loot persistence, dialogue), Dim 6 (player feedback), Dim 7 (save coverage, stage order, determinism) · **Unchanged since baseline (skimmed)**: Dim 3 (zero commits on its Paths; spot-checked against the new M47.2 actor-value writers)

## Scope

`/audit-gameplay` was run delta-first. 382 commits have landed since the baseline, and nearly all of the gameplay-relevant ones are M47.2 Papyrus-VM work that opened new paths into gameplay state:
- **Death and damage**: essential and protected bleedout (`dd843f9e6`), script `Kill`/`KillSilent` (`48b52b198`, `cd61e7b60`), `Weapon.Fire` projectiles (`9e6c44cea`), trap hits and `ProcessTrapHit` (`6dc948ac4`, `f6a64fe20`), `SetGhost` (`ec058e5e5`), `DamageValue`/`RestoreValue` (`02f22d793`), Fallout Damage Threshold and kill XP (`dbe562c1f`, `52b59e43d`).
- **Inventory and ownership**: the barter menu (`61ad95cf3`), script `RemoveItem`/`DropObject`/`GetCaps`, ownership (`b2868b848`, `8b3b75d4d`).
- **Activation**: `BlockActivation` (`9b5a3e29b`), activate-parents (`745d64830`), `Activate(…, true)` (`e2a234c6e`).
- **AI packages and locomotion**: PF_ package fragments (`fb0fff11c`, `7d34b99f2`), the console walk and graph locomotion pair (`746ba8dfd`), FO4 behavior-graph actors (`3bb6806b5`, `df6ad0410`, `e8026e737`), encumbrance (`ed59c81f7`).
- **Refactors**: `save_io` and `esm/cell` module splits (`d7c47d741`, `ae41a55ca`), #5794 (shared spawn finalize tail), #5813 (one player-entity read).

**Evidence base**: code reading by three dimension agents, every MEDIUM-or-higher claim re-checked at HEAD by the orchestrator, plus tests. No engine was launched.
- **Phase 1 baseline filter set** (`cargo test -p byroredux --bin byroredux -- combat:: inventory:: interaction:: npc_spawn:: reference_state ambient_locomotion walk_anim eat_sleep forcegreet dialogue_voice save_io::`): **384 passed, 0 failed, 20 ignored**.
- **Dim 3/4/6/7 guard set** (consum, restoration, timed_, notifications, loading_screen, spawn_tests, registry_completeness, round_trip, scheduler_access, ambient_locomotion_default, locomotion_kill_switch, story_dispatch, bleedout, scripted_kill, hit_event, encumbrance, projectile, trap_hit, live_reload): **182 passed, 0 failed, 4 ignored**. `byroredux-plugin consumables`: 9 passed.
- **The green guards do not cover these findings.** Every finding below lives in a path that no current test exercises. The new findings need new tests, named under each Suggested Fix.

**Dedup**: checked against 119 open issues, and against closed issues by keyword (bleedout, remove_items, HitEvent overwrite, DamageActorValue, package fragment, carried-key unlock, ActivePackages).
- Three pairs of agent findings were merged:
  - Dim 4 and Dim 2 both found the Bleedout loss on cell eviction.
  - The Dim 1/5 and Dim 2 agents each found a carried-key unlock that leaks past a refused activation; one root cause.
- The baseline's GAME-D4-2026-10-12-01 (console `setav`/`modav` Health without a death edge) is **fixed** (#5728, `76cc2dcf7`; `commands/actor_value.rs:124` calls `commit_actor_value_deaths`). The script-native sibling is GAME-D4-2026-10-15-05 below.

## Executive Summary

| Severity | NEW | Regression | Existing (open) re-confirmed |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 1 | 0 | 0 |
| MEDIUM | 5 | 1 (#4324) | 2 (#4232, #5721) |
| LOW | 12 | 0 | 6 (#5495-dormant-gate, #5722, #5724, #5726, #5678, #5647) |

The recurring theme: **M47.2 added new gameplay producers that do not go through the invariants the earlier producers were brought into line with.**
- The projectile and trap producers repeat #4324's `HitEvent` overwrite.
- The script `DamageActorValue` and `SetActorValue` Health writes repeat the #4702/#5728 class: Health ≤ 0 with no death edge.
- `Bleedout` was added to the save registry but not to the per-reference eviction row, unlike its sibling `ActorControlState`.
- `remove_items` (barter, `RemoveItem`, `DropObject`, `GetCaps`) zeroes rows correctly, but skips the `EquippedWeapon` re-derivation and the unequip event that `transfer_loot` performs.
- `reconcile_dead_actor` was not extended for the FO4 behavior-graph runtime.

Games and cells reasoned about (statically): Skyrim `RiverwoodSleepingGiantInn` (bleedout), FO4 humanoids sharing one behavior graph, Starfield/FO76 (no `BleedoutRules`), FNV/FO3 (Travel, barter). No cell was loaded.

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path | **DRIFTED** | Projectiles (`systems/projectiles.rs:284-299`) and `flush_trap_hits` (`crates/scripting/src/fragment/effects/deferred.rs:654-673`) emit `HitEvent` only, and `combat_damage_system` is the sole hit writer, registered after every Update producer (ee6468fd1). But the new producers overwrite a same-frame hit (GAME-D4-2026-10-15-02), a Late `ProcessTrapHit` is dropped (-06), and script Health writes bypass the death edge (-05). |
| Death reconciled once | **DRIFTED** | The `Dead` producers checked: `combat.rs:380`, `scripted_kill_system` (`combat.rs:1195`, Late 257 → reconcile 279), `extensions/commands.rs:564` (SDK and console via #5728), `water.rs:77`, `character.rs:1594` (the player, never spared), and `reference_state.rs:312/381` (restore). Each asks `spare_from_death` where the victim is live, reconciles once, and raises KILL (`KillSilent` raises `OnDeath` only). The teardown misses the FO4 graph runtime (-03), the dialogue surface (GAME-D2-2026-10-15-03), and `ActivePackages` (GAME-D5-2026-10-15-03). |
| Index stability | **VERIFIED / DRIFTED** | No production row removal was added: barter, `RemoveItem` and `DropObject` zero in place, and `ItemInstance` moves atomically. `remove_items` leaves `EquippedWeapon` pointing at a zeroed row (GAME-D1-2026-10-15-01). |
| Fail-closed consumables vs fail-open packages | VERIFIED | `restoration_plan` is unchanged and all-or-nothing, and its conditions stay fail-closed. `package_conditions_pass` is unchanged. #5722 (fully-modeled Say-To winner) is still open. |
| Stage order | VERIFIED | Update: restoration → bleedout → encumbrance → interaction → flush → container_loot → combat_input → faction_hostility → npc_combat_ai → ambient AI → force-greet → scene package → activate-parent relay → trap/projectile producers → hit_event → combat_damage. PostUpdate: movers → walk_anim → console_walk → graph_locomotion. Late: water → scripted XP/skill → scripted_kill → reconcile_pending → equipment_appearance → dialogue selection → event_cleanup. The guard for the HitEvent order is weaker than it looks (GAME-D7-2026-10-15-01). |
| State saved or re-derived | **DRIFTED** | `Bleedout` is a saved replacing column but absent from `ReferenceState` and the park path (-01). An infinite `Bleedout` makes a save that cannot be loaded (-04, HIGH). The other new per-placement facts are covered: ownership overrides, BlockActivation, destruction stages, map markers, crime gold, scripted factions, ObScript locals, FormList additions, skill XP and cleared locations are all registered, and `SetGhost` rides `ActorControlState`. The allowlist rebuild claims for `GraphLocomotion`, `ObjectAnimationGraph`, `PlayerEncumbrance`, `ActiveBarter` and `ActorProtection` were checked against production. The `CreatureActor` row is still false (#5495). |
| Determinism | VERIFIED (one LOW) | `rand::|Instant::now|SystemTime|thread_rng` is empty over the skill's list plus the new systems. `OnTrapHitStop` order follows `HashSet` iteration (GAME-D4-2026-10-15-07). |

## Findings

### HIGH

### GAME-D4-2026-10-15-04: An infinite `Bleedout` timer serializes as JSON `null`, so every save taken afterwards fails to load
- **Severity**: HIGH (save severity table: a load that refuses gameplay state; every later save in the session is affected)
- **Dimension**: Combat & Death (Dim 4), with Dim 7 (save coverage)
- **Location**: `crates/scripting/src/bleedout.rs:211-213`; save registration `byroredux/src/save_io/registry.rs:372`
- **Status**: NEW (from `dd843f9e6`)
- **Trigger**: a game with no `BleedoutRules` installed (Starfield, FO76): script `Kill()` on an Essential actor, or on a Protected actor whose killer is not the player. This includes Essential/Protected quest aliases. Then save, then load.
- **Description**: When no `BleedoutRules` resource exists, `spare_from_death` stores `Bleedout { remaining_secs: f32::INFINITY }`. `bleedout_system` returns early without `BleedoutRules`, so the actor never recovers and the component stays for the rest of the session. The save registry serializes components through `serde_json::to_value`, which writes non-finite floats as `null`. On load, `validate_snapshot_types` rejects the row ("null, expected f32"; `crates/save/src/driver.rs:122`, `byroredux/src/save_io/live_load.rs:433`).
- **Evidence**:
  ```rust
  let remaining_secs = world
      .try_resource::<BleedoutRules>()
      .map_or(f32::INFINITY, |rules| rules.death_time_secs);
  …
  bleedouts.insert(victim, Bleedout { remaining_secs });
  ```
- **Impact**: Every save from that point on cannot be loaded. On Starfield/FO76 NPCs have no `ActorVitals`, so the reach is limited to `scripted_kill_system`, which does not need them; the impact is total loss of save progress.
- **Suggested Fix**: Use a finite sentinel, or `Option<f32>` with "no rules" meaning "never recovers". Alternatively, decline to spare when no `BleedoutRules` exist. Add a round-trip test for `Bleedout` with the no-rules value.

### MEDIUM

### GAME-D4-2026-10-15-01: `Bleedout` is not in the per-reference eviction row; an essential NPC evicted mid-bleedout comes back standing at Health ≤ 0 and never recovers
- **Severity**: MEDIUM
- **Dimension**: Dim 2 (loot/placement persistence) × Dim 4
- **Location**: `byroredux/src/cell_loader/reference_state.rs:27-58` (`ReferenceState`), `:111-254` (`capture`), `:257-310` (`restore`), `:668-760` (`park_unresolved_snapshot_rows`)
- **Status**: NEW (from `dd843f9e6`). Found independently by the Dim 4 and Dim 2 agents.
- **Trigger**: Skyrim, `RiverwoodSleepingGiantInn`. Put Delphine into bleedout, leave through the inn door within `fEssentialDeathTime` (20 s), and return. The save path hits the same gap: a save taken while she sits in the hysteresis band is parked, then reloaded.
- **Description**:
  - A spared actor keeps Health ≤ 0 (`combat.rs:362-371`), and only `bleedout_system` brings it back, by visiting `Bleedout` holders.
  - `capture` and the park path record `actor_values`, `control` and `dead`, but not `Bleedout`, so `restore` re-inserts the ≤ 0 Health with no `Bleedout`.
  - The actor is then neither dead nor unconscious: AI, dialogue and combat resume at ≤ 0 HP, and no NPC health regeneration exists to fix it.
  - The #5017 sibling `ActorControlState` is in the row; `Bleedout` was added to the save registry (`save_io/registry.rs:372`) but not here.
- **Impact**: The essential NPC is stuck in a broken state, and later saves keep it.
- **Suggested Fix**: Add a required `bleedout: Option<f32>` field to `ReferenceState`, carried through `capture`, park and `restore`; no serde default (#4465) and a `FORMAT_MAJOR` bump (`/audit-save`). Test: capture then restore an actor holding `Bleedout` with ≤ 0 Health, and assert it still bleeds out and then recovers.

### GAME-D4-2026-10-15-02: Projectile and `ProcessTrapHit` producers overwrite a same-frame `HitEvent`
- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4)
- **Location**: `byroredux/src/systems/projectiles.rs:284-299`; `crates/scripting/src/fragment/effects/deferred.rs:654-673` (`flush_trap_hits`)
- **Status**: Regression of #4324 (the single-slot `HitEvent` overwrite, fixed for `npc_combat_ai_system` at `combat_ai.rs:262-280`)
- **Trigger**: any game. A dart-trap or `Weapon.Fire` hit lands on an actor in the same frame as a player or NPC melee hit, or two trap hits land together.
- **Description**: `HitEvent` is one SparseSet slot per target. #4324 made the NPC producer hold its strike when `events.get(target).is_some()`. Both new producers, registered after `combat_input_system` and `npc_combat_ai_system`, call `events.insert(target, …)` unconditionally, replacing the earlier hit. `hit_event_consumers_run_after_every_producer` pins only the order, not the overwrite.
- **Evidence**: `projectiles.rs:287-298`: `for (target, shooter, damage) in hits { … events.insert(target, HitEvent { … }) }`, with no existing-row check.
- **Impact**: Melee damage is silently lost when a projectile or trap hit lands the same frame, and several trap hits collapse into one.
- **Suggested Fix**: Hold or merge as `combat_ai.rs` does, or make `HitEvent` a per-target queue. Add a test with a melee hit and a projectile hit landing on one target in one frame, asserting both damages apply.

### GAME-D4-2026-10-15-03: Fallout 4 corpses are re-animated by their behavior graph
- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4), the `reconcile_dead_actor` teardown
- **Location**: `byroredux/src/combat.rs:963-1056` (`reconcile_dead_actor`); `byroredux/src/asset_provider/actor_graph.rs:307-318` (`resolve_wanted`); the graph step (`object_graph_step_system`) and `byroredux/src/systems/graph_locomotion.rs:124-166`
- **Status**: NEW (from `3bb6806b5` / `df6ad0410` / `e8026e737`)
- **Trigger**: FO4, any cell where a humanoid NPC dies while another FO4 actor's graph loads a clip on demand. Also any later graph step or locomotion tick.
- **Description**: The teardown removes `AnimationPlayer` and `AnimationStack` on the actor and its skeleton root, but leaves `ObjectAnimationGraph` and `GraphLocomotion` in place. Three paths then put a clip back on the corpse through `apply_playback_for`:
  1. `resolve_wanted` calls `graph.start()` on **every** graph sharing the loaded graph. FO4 humanoids all share one, and no `Dead` filter exists. This was verified at HEAD: the filter is `std::ptr::eq(graph.graph(), &*job.loaded)` only.
  2. The graph step has no `Dead` filter.
  3. `graph_locomotion` reads `Dead` but treats the corpse as parked and sends `moveStop`.

  FO4 ragdolls do not activate yet (`docs/engine/physal.md`), so nothing overrides the restored pose.
- **Impact**: An FO4 corpse stands, idles or walks in place.
- **Suggested Fix**: Remove `ObjectAnimationGraph` and `GraphLocomotion` (and `ConsoleWalk`) in `reconcile_dead_actor`, or make every graph consumer skip `Dead`. Restrict `resolve_wanted`'s restarts to the graphs actually waiting on the provided clip; restarting all of them also makes live actors hitch, which belongs to `/audit-scripting`.

### GAME-D4-2026-10-15-05: Script `DamageActorValue` / `SetActorValue` on Health has no death edge
- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4), with Dim 3 (the actor-value writers)
- **Location**: `crates/scripting/src/fragment/effects.rs:515-549` (`DamageActorValue`, `02f22d793`); `:477-513` with `crates/scripting/src/fragment/effects/evaluate.rs:565` (`SetActorValue` / `SetActorValueNamed`)
- **Status**: NEW (the script-native sibling of #4702 and of the baseline's console finding, fixed by #5728)
- **Trigger**: a quest or reference script calling `DamageActorValue("Health", n)` / `DamageValue` / `SetActorValue("Health", 0)` on an actor or on the player.
- **Description**: These natives write the Health damage or base without `spare_from_death`, `Dead`, KILL or reconciliation. The SDK batch and console paths now call `commit_actor_value_deaths`; the script path does not. Nothing else sweeps actors at Health ≤ 0.
- **Impact**: A script-killed actor or player stays alive at ≤ 0 HP, and an essential actor never enters bleedout. Quests that wait on `OnDeath` stall.
- **Suggested Fix**: Route the script AV writes through the same `commit_actor_value_deaths` the console and SDK use, so that one helper is the only Health-to-death edge outside `combat_damage_system`. Test: a script `DamageActorValue` past zero marks the actor `Dead` and raises KILL; on an Essential actor it enters bleedout.

### GAME-D1-2026-10-15-01: `remove_items` (barter, `RemoveItem`, `DropObject`, `GetCaps`) leaves `EquippedWeapon` stale and emits no unequip event
- **Severity**: MEDIUM
- **Dimension**: Inventory & Equipment Model (Dim 1)
- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:17-114` (slot release `:84-92`). Callers: `byroredux/src/barter.rs:438-482`, the `DropObject` native (`crates/scripting/src/fragment/vm.rs:3302-3325`), `GetCaps` (`crates/scripting/src/fragment/effects.rs:366-376`)
- **Status**: NEW (from `61ad95cf3` and the M47.2 item natives)
- **Trigger**: FNV/FO3/Skyrim. Sell the player's equipped weapon at a vendor; `grouped()` does not exclude equipped items. Or a script `RemoveItem` of an NPC's equipped weapon or armor.
- **Description**: Rows are zeroed in place and `EquipmentSlots::release` frees the slots, which is correct. But `EquippedWeapon` is not re-derived: only `byroredux/src/inventory.rs` calls `reconcile_equipped_weapon`. No `emit_equipment_changes` is sent either; the only emit in this file is in the equip path at `:234`. `transfer_loot` does both.
- **Impact**:
  - `attack_damage` (`combat.rs:818-820`) keeps the sold weapon's damage until a reload. The load-time reconcile covers the player only.
  - An NPC's removed armor stays visible for the session, because no `EquipmentEventBatch` is sent.
  - `OnObjectUnequipped` never fires.
  - The `loot_appearance.rs:282-293` doc calling the `added == false` release path "forward-latent" is now stale: barter sell, `GetCaps` and player `RemoveItem` reach it.
- **Suggested Fix**: Have `remove_items` collect the emptied equipped indices, emit the unequip changes and re-derive `EquippedWeapon`. That means moving `reconcile_equipped_weapon` (or its core) to where both crates can call it, rather than duplicating it. Test: barter-sell the equipped weapon, and assert `EquippedWeapon` is gone and an unequip event was emitted.

### GAME-D5-2026-10-15-01: The spawn-time package winner is reported before alias overlays or the restored clock are applied, so PF_ fragments run Begin/Change for a package the actor never ran
- **Severity**: MEDIUM (the real impact depends on what the fragments do; no census was run)
- **Dimension**: NPC Spawn → AI Package Selection (Dim 5)
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:800-862` (report at `:847`); alias overlay merge `:980`, `:1042`; the save-load reseat at `byroredux/src/save_io/live_load.rs:610`
- **Status**: NEW (from `fb0fff11c`)
- **Trigger**: Skyrim/FO4, the only games with PACK VMAD:
  - an actor whose quest alias injects a package that wins (every spawn of that actor);
  - a save load whose restored hour or quest state picks a different winner than spawn did.
- **Description**: `apply_ai_package_behavior` selects over the base PKID list only, with the clock and quest state live at spawn, and immediately reports `None → A`. The first tick merges `QuestAliasInjectedOverlays`, which are stamped after spawn, and the save-load reseat uses the restored clock. Either can pick `B ≠ A` and report `A → B`. Next frame `package_fragment_system` runs Begin(A), Change(A) and Begin(B). The load drain discards pending story events but not `PendingPackageTransitions`.
- **Impact**: Package Begin/Change fragments run for a package the actor never ran. If a fragment calls `SetStage` or moves the actor, quest state goes wrong.
- **Suggested Fix**: Report the transition only from the first-tick confirmation, so spawn stamps the behaviour without reporting. Also clear `PendingPackageTransitions` in the load drain beside the story events.

### LOW

### GAME-D2-2026-10-15-01: The carried-key unlock runs before the activation refusal, so a script-blocked or Parent-Activate-Only locked reference is unlocked with nothing opened
- **Location**: `byroredux/src/interaction.rs:1423-1453` (`unlock_with_carried_key`), called at `:1477-1500` before the `default_activation_blocked` / Parent-Activate-Only refusal (`:1535-1559`); target selection `:1229-1395`
- **Status**: NEW (from `9b5a3e29b` and `745d64830`). Merged from two agents.
- **Trigger**: a locked reference with `BlockActivation` set by script, or flagged Parent-Activate-Only, while the player carries its key.
- **Description**: `activate_target` unlocks first: it removes `Locked`, writes the saved `ReferenceLockState` and notifies "Unlocked with key". Only then is the activation refused. Parent-Activate-Only references are also not filtered from target selection or the HUD prompt, and each press logs at `log::error!`. Loot itself stays correctly blocked in `container_loot_system`.
- **Suggested Fix**: Evaluate the refusal before the key unlock, and filter Parent-Activate-Only references out of target selection.

### GAME-D2-2026-10-15-02: Script ownership changes never reach the theft rule
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:75-77` and `:863-872`; `crates/scripting/src/fragment/effects/deferred.rs:611-620`; `byroredux/src/inventory.rs:890-916`
- **Status**: NEW (from `b2868b848` and `8b3b75d4d`)
- **Description**: REFR `XOWN` is stamped twice: as core `Owned` (read by `transfer_is_theft` / `pickup_loot`) and as scripting `ExplicitOwner`. `SetActorOwner` / `SetFactionOwner` write only `OwnerOverrides`, which only the VM's `GetActorOwner` / `GetFactionOwner` read.
- **Impact**: The "Stolen"/"Added" wording and the `stolen` flag are wrong after a script ownership change. This becomes a real bug once crime has consequences; faction crime gold now exists.
- **Suggested Fix**: Make one ownership read (override, then `Owned`) that both the theft rule and the natives use.

### GAME-D2-2026-10-15-03: A conversation stays open when the partner dies, bleeds out or enters combat
- **Location**: `byroredux/src/combat.rs:963-1052` (no dialogue teardown); `crates/debug-ui/src/lib.rs:335-337` (dialogue doesn't pause); `byroredux/src/barter.rs:239-266` (`begin_barter` has no `Dead`/`player_can_act` gate)
- **Status**: NEW. Related: #5678 (the voice side for a despawned speaker).
- **Description**: `reconcile_dead_actor` cuts the voice line but leaves `DialogueSurfaceState` and `NpcDialogueTopic` in place, and entering bleedout or combat tears nothing down. While the page stays open:
  - it lists the corpse's topics (each click is refused);
  - `IsInDialogueWithPlayer` returns true for the corpse.

  When the player closes the page, the dead speaker's OnEnd fragments run. One that calls `ShowBarterMenu` opens barter with a corpse.
- **Suggested Fix**: Call `end_open_conversation` from `reconcile_dead_actor` and on bleedout or combat entry of the open partner. Gate `begin_barter` on `npc_refuses_dialogue`.

### GAME-D2-2026-10-15-04: Scripted, parent-relayed or default-processing activation of a load door does nothing
- **Location**: `byroredux/src/interaction.rs:1494-1520`; `docs/engine/scripting.md:1737`
- **Status**: NEW (from `745d64830` and `e2a234c6e`)
- **Description**: Apart from the `door.*` console command, the only production `queue_door_transition` call is inside the player's E-press path. No `ActivateEvent` consumer handles `DoorTeleport`, yet `scripting.md` lists the door as one. A script `Activate` of a load door (or a lever wired to one) is a silent no-op.
- **Suggested Fix**: Add a `DoorTeleport` arm on `ActivateEvent` for `activator == player`, or correct the doc and log the decline.

### GAME-D5-2026-10-15-02: The Travel-arrival End fragment (`7d34b99f2`) is unreachable on fragment-bearing games
- **Location**: `byroredux/src/systems/travel.rs:369-380`; tree fallback `byroredux/src/npc_spawn/ai_package.rs:349-389`
- **Status**: NEW
- **Description**: `TravelBehavior` installs only from the flat `procedure_type == 6` arm, which only FO3/FNV/Oblivion packages reach. Skyrim+ packages use the procedure tree, which resolves Sandbox and Patrol leaves only (`docs/engine/packal.md`), and PACK VMAD exists only on Skyrim+. So every End this path reports belongs to a package without fragments. Only the synthetic VM test drives it, yet `scripting.md` and the commit message describe it as live. Escort, Guard, Eat and Sleep report no arrival End at all.
- **Suggested Fix**: Correct the doc, or resolve the Travel leaf on tree packages first (`packal.md` scope).

### GAME-D5-2026-10-15-03: `ActivePackages` is never retired on death or unload
- **Location**: `byroredux/src/combat.rs:963-1003` (removes `AmbientPackageRuntime` with no transition report); reader at the VM's `GetCurrentPackage` (`crates/scripting/src/fragment/vm.rs:2939`)
- **Status**: NEW
- **Description**: `GetCurrentPackage` on a corpse returns its last package for the rest of the session, but None after a reload. The map grows for the session. Package End fragments also do not run on death.
- **Suggested Fix**: Report a `→ None` transition from `reconcile_dead_actor` and from unload.

### GAME-D1-2026-10-15-02: Barter and `AddItem` append a new row per transfer
- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:65-79` and `:243-268`; `byroredux/src/barter.rs:456-480` (two transfers per click)
- **Status**: NEW
- **Description**: Items without an instance are never merged into an existing row, so gold and goods fragment into one row per trade on both sides. The rows are saved and never compacted, and the inventory menu (`inventory.rs:1314-1345`) lists each one.
- **Suggested Fix**: Merge into the first row with the same form, no instance and `count > 0`, or into a zeroed row of that form. Never remove rows.

### GAME-D4-2026-10-15-06: A `ProcessTrapHit` raised from a spoken dialogue line is dropped
- **Location**: `byroredux/src/boot/schedule/late.rs:527` (the dialogue-fragment block declaring the `HitEvent` write at `:750`); `event_cleanup_system` at `:821`
- **Status**: NEW (no vanilla trigger confirmed)
- **Description**: The Late dialogue path can flush a trap hit into `HitEvent`, and `event_cleanup_system` clears it before any consumer runs.
- **Suggested Fix**: Queue it for the next frame's Update flush instead of inserting it in Late.

### GAME-D4-2026-10-15-07: `OnTrapHitStop` events fire in `HashSet` iteration order
- **Location**: `byroredux/src/systems/trap_hits.rs:182`
- **Status**: NEW
- **Description**: The stop events are raised in `HashSet::difference` order, which is seeded per process, so script run order differs between runs.
- **Suggested Fix**: Sort the difference by FormID or entity before raising.

### GAME-D6-2026-10-15-01: Level-ups, XP, caps and non-silent `AddItem` give the player no feedback
- **Location**: `byroredux/src/combat.rs:619-690` (`grant_experience` logs only); `byroredux/src/commands/gameplay.rs:352` (`levelup` is the only reader of banked `LevelUpPoints`); `crates/scripting/src/fragment/effects.rs:366-378`
- **Status**: NEW (from `38539eee8` and `722421899`)
- **Description**: The production `notifications::push` producers are interaction, inventory, the barter refusal and live-load. Progression events reach only the log, and banked level-up points can only be spent from the console.
- **Suggested Fix**: Push level-up, XP, caps and item-added notices through `PlayerNotifications`.

### GAME-D7-2026-10-15-01: The `HitEvent` ordering guard silently skips producers it cannot find
- **Location**: `byroredux/src/boot/schedule/mod.rs:138-140` (`hit_event_consumers_run_after_every_producer`)
- **Status**: NEW (test gap in `ee6468fd1`)
- **Description**: The test does `continue` when a producer's name is no longer in the source. A renamed producer therefore leaves the guard green while checking nothing, which is exactly the dart-trap regression the guard exists to catch. A missing consumer name panics; a missing producer name should too.
- **Suggested Fix**: Panic on a missing producer name.

### GAME-D2-2026-10-15-05: Doc and tracking drift: the `PapyrusPlayerEntity::get` sync claim, and Skyrim `.fuz` voice
- **Location**: `crates/scripting/src/papyrus_demo/mod.rs:75-84`; ROADMAP M43; `byroredux/src/systems/dialogue_voice.rs`
- **Status**: NEW (doc-rot)
- **Description**:
  - The `get` doc says `extension_player_entity_sync_system` keeps `PlayerEntity` in sync. That system only copies `PlayerEntity` into the WASM host. The two resources agree because `scene.rs:1195/1217` writes both to the same body; in FlyCam they differ, harmlessly.
  - Skyrim `.fuz` voice is still unbuilt: #5464 corrected only the container description, and `dialogue_voice.rs` resolves `.ogg` only. ROADMAP points the gap at closed #5367, and no open issue tracks it.
- **Suggested Fix**: Correct the doc comment. File, or link, an open tracker for the xWMA `.fuz` decoder.

## Not filed (observations)

- **Two `Transform` writers for a graph-driven actor.** `graph_root_motion_system` (Update, `update.rs:716`) and the PostUpdate `write_back_walk` movers can both move one actor, with no exclusion between them. It is latent while FO4 walks keep `bAnimationDriven = 0` and no FO4 NPC walks on its own. Re-check when FO4 ambient walking lands.
- **`ConsoleWalk`** (debug `actor.walk`) writes `Transform` directly, ignores `Dead`, and is not removed by `reconcile_dead_actor`. It is debug-only; folded into the GAME-D4-2026-10-15-03 fix.
- **`encumbrance_system` clones the player's `Inventory` every frame** (`encumbrance.rs:115`). The cost is small; for `/audit-performance`.
- **`ActorBaseProtectionOverrides` is a saved std `HashMap`**, so its JSON key order varies between runs. For `/audit-save`.
- **`resolve_wanted` restarts every live FO4 actor's current clip** whenever any clip loads on demand, so live actors hitch. For `/audit-scripting`; the corpse half is GAME-D4-2026-10-15-03.

## Known-Open Register

| Issue | State at HEAD |
|---|---|
| #4232 `effective_actor_level` returns 0 | open, unchanged |
| #5495 creature seating gate | open; `CreatureActor` is still stamped nowhere (`runtime.rs:437` `let _ = world;`), and the allowlist row `registry_completeness_tests.rs:621` still claims a stamp |
| #5721 gear-import failure ping-pong | open; `loot_appearance.rs:565/632/957/999` still exclude a single failed form |
| #5722 fully-modeled Say-To Dialogue winner | open; `select_active_package` gates only on fully-modeled (`ai_package.rs:757-785`) |
| #5724 Travel/Guard/Escort freeze an unspawned target | open; `travel.rs:119-127`, `guard.rs:89`, `escort.rs:113` |
| #5726 unbounded In-Cell radius | open; `eat_sleep.rs:348` still `f32::INFINITY` |
| #5678 despawned speaker's voice not cut | open |
| #5647 `StoryEvent` one slot per entity | open (scripting) |
| #5748 `m42-eat-sleep.sh` seat leg | open |
| #5526 Oblivion voice | open |
| Baseline GAME-D4-2026-10-12-01 (console setav death edge) | **fixed** (#5728, `76cc2dcf7`) |
| #5642 force-greet stale partner | **fixed** (`c0c9c9d1b`; verified it re-reads the open conversation per open) |
| #5460 double `reconcile_worn_gear` | **fixed** (`bda5a1167`) |

## Skill drift noted for `/audit-sync`

- Dim 1: "the `added == false` release path is forward-latent" is stale. Barter, `GetCaps` and player `RemoveItem` now reach it.
- Dim 4: the producer list should add the projectile/trap overwrite rule (#4324 class), and note that script AV natives are a Health writer outside `commit_actor_value_deaths`.
- Dim 2: "Skyrim+ `.fuz` voice is unbuilt … filed separately": no open tracker exists.
- Dim 7: the Update order line should include bleedout, encumbrance and the activate-parent relay. PostUpdate should end with console_walk → graph_locomotion. Late should include the scripted XP and skill systems.

## Suggested labels (for `/audit-publish`)

| Finding | Labels |
|---|---|
| GAME-D4-2026-10-15-04 | `bug` `high` `gameplay` `save-load` `game:starfield` |
| GAME-D4-2026-10-15-01 | `bug` `medium` `gameplay` `combat` `save-load` |
| GAME-D4-2026-10-15-02 | `bug` `medium` `gameplay` `combat` |
| GAME-D4-2026-10-15-03 | `bug` `medium` `gameplay` `combat` `animation` `game:fo4` |
| GAME-D4-2026-10-15-05 | `bug` `medium` `gameplay` `combat` `scripting` |
| GAME-D1-2026-10-15-01 | `bug` `medium` `gameplay` `inventory` |
| GAME-D5-2026-10-15-01 | `bug` `medium` `gameplay` `ai` `scripting` |
| GAME-D2-2026-10-15-01..04 | `bug` `low` `gameplay` (+ `dialogue` for -03) |
| GAME-D5-2026-10-15-02/03 | `bug` `low` `gameplay` `ai` |
| GAME-D1-2026-10-15-02 | `bug` `low` `gameplay` `inventory` |
| GAME-D4-2026-10-15-06/07 | `bug` `low` `gameplay` `combat` |
| GAME-D6-2026-10-15-01 | `bug` `low` `gameplay` `ui` |
| GAME-D7-2026-10-15-01 | `bug` `low` `test-gap` `combat` |
| GAME-D2-2026-10-15-05 | `documentation` `low` `doc-rot` `dialogue` |
