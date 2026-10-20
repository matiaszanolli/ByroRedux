######## 5959 [OPEN] bug,renderer,medium,performance,legacy-compat,game:skyrim
# SKY-D4-2026-10-17-02: Whiterun BanneredMare is 7-13% slower than the bench-of-record's own commit (same machine, same harness)
**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (report ID SKY-4-02) · **Severity**: MEDIUM · **Dimension**: 4 (control bench) · **Game**: Skyrim SE

## Description
Whiterun BanneredMare is 7–13% slower at HEAD than the bench-of-record's own commit, measured on the same machine in the same session. Per the standing methodology the outgoing record's commit `a3a7a43b0` was rebuilt in a worktree (`--release`) and run through `scripts/fsr-bench-matrix.sh` (harness `a4ede5fa0`, `renderer-stepped/orbit`, 3 runs x 300 frames, `FSR_BENCH_SCENES=whiterun`), then HEAD `eec7e00cf` was run the same way immediately after. Camera pose is identical to the record and to each other (`177.223,188.000,1048.424`).

## Evidence (medians)
| Config | Control `a3a7a43b0` | HEAD `eec7e00cf` | Delta |
|---|---:|---:|---:|
| TAA native | 83.8 FPS / 11.94 ms | 74.3 FPS / 13.46 ms | +1.52 ms (+12.7%) |
| FSR native-AA | 79.9 / 12.52 | 72.6 / 13.77 | +1.25 ms |
| FSR Quality | 131.2 / 7.62 | 120.4 / 8.30 | +0.68 ms (+8.9%) |
| FSR Balanced | 157.7 / 6.34 | 143.8 / 6.95 | +0.61 ms |
| FSR Performance | 189.7 / 5.27 | 178.2 / 5.61 | +0.34 ms (+6.5%) |

Per pass (TAA): `gpu_main` 11.161 -> 12.546 ms (+1.385), `gpu_svgf` 0.506 -> 0.856 (+0.350, +69%), `gpu_volumetrics` 0.818 -> 0.902, `gpu_volumetrics_inject` 0.790 -> 0.877. At Quality: `gpu_main` +0.664, `gpu_svgf` 0.265 -> 0.443. Entities 5,825 -> 5,837; draws (`1289/392b/24c`), TLAS (1,226) and lights (18) are equal. Run spread is 0.01–0.35 ms, well below the delta. An earlier HEAD-only pass the same hour agreed (TAA 76.3 FPS / 13.10 ms, Quality 122.0 / 8.20). Run conditions: RTX 4070 Ti, desktop apps resident, 1-min load average 3.5 at the start of the control bench and 2.2 at the end of the HEAD bench (HEAD was not disadvantaged by ordering).

## Impact
About 1.5 ms of a 12 ms frame at TAA native on the project's Skyrim control scene. The ROADMAP record (92.4 FPS TAA) is no longer reproducible; today's control build itself reads 83.8, so the machine is also ~9% slower than at record time. Only the same-session A/B is meaningful. No correctness impact observed.

## Candidate commits (NOT bisected)
Renderer range `a3a7a43b0..eec7e00cf`: `3f8194117` (SVGF progressive accumulation no longer waits for every NPC), `1092b5b9e` / `24eb7dba8` (SVGF history age / invalidation), `c2647d468`, `17e0b7464`, `6110b6078`, `2e7d5bead` (ReSTIR reservoir ownership / temporal reuse), `triangle.frag` +253 lines (`triangle.frag.spv` 432,576 -> 444,880 B), and a new `shadow_filter.comp` pass whose GPU time is not among the harness columns (unmeasured).

## Suggested fix
Bisect `a3a7a43b0..eec7e00cf` on this exact harness (the control-worktree build takes ~2 minutes); decide per commit whether the cost is the intended price of the feature; refresh the bench-of-record once the number is understood.

Related: ROADMAP bench-of-record; `/audit-performance`, `/audit-renderer`. Closed #2367 / #5117 are earlier, unrelated regressions.

######## 5985 [OPEN] bug,medium,scripting
# SCR-D2-2026-10-17-05: `GetVMScriptVariable` (CTDA 630) cannot see a VM-run reference script's variables
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-05: `GetVMScriptVariable` (CTDA 630) cannot see a VM-run reference script's variables

- **Severity**: MEDIUM
- **Dimension**: Fragment Dispatch & Locks (owner-keyed variable stores) / Event Runtime
- **Location**: reader `crates/scripting/src/condition/eval/global.rs:124-139`; VM writes `fragment/vm/interp.rs:560-611` (`write_quest_variable` under the reference FormID) and `docs/engine/scripting.md:975-978`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: A reference script that runs on the VM keeps its variables in the saved `QuestScriptVariables`/`QuestScriptObjects` stores under the *reference FormID* (correctly owner-keyed, outliving the cell). `get_vm_script_variable` however reads only the entity component `vm_state::ScriptVariables` (`world.get::<ScriptVariables>(target)`), which is written by the lowered `SetReferenceVariable` effect and the two native activator systems - never by the VM. For a `CTDA GetVMScriptVariable(ref, "::x_var")` that targets a VM-run script the condition therefore always evaluates `0.0` ("the engine ran the script, the condition silently reads nothing"). The quest-side twin `get_vm_quest_variable` goes through `quest_variables::quest_variable` and does see VM writes, so the asymmetry is specific to reference scripts. `docs/engine/scripting.md:411` documents the entity component only for the *lowered* write path; the VM wave (`852c61078`, `5f1671e59`) did not extend it.
- **Evidence**: `get_vm_script_variable`: `resolve_entity_by_global_form_id(...)` -> `world.get::<ScriptVariables>(target).and_then(|v| v.get(variable)).unwrap_or(0.0)`; no reference to `QuestScriptVariables` in that function or in `condition/`.
- **Impact**: Dialogue/package/perk conditions gated on a VM-script reference variable (CTDA 630 is the reference-script analogue of the 3 319 Skyrim `GetVMQuestVariable` rows; census for 630 not taken - unverified) evaluate to a fixed 0.0 - wrongly open or closed gates with no diagnostic. Not a regression (no condition read of VM state existed before the wave), but the VM wave made the gap reachable.
- **Related**: #5752 / #5750 / #5778 (catalog gaps of the same fixed-0.0 flavour; this is a different cause). Not filed.
- **Suggested Fix**: Make `get_vm_script_variable` fall back to `QuestScriptVariables` under the target's FormID (same `property_key` spelling), or mirror numeric VM writes into `ScriptVariables`.

######## 5986 [OPEN] bug,low,scripting,concurrency,test-gap
# SCR-D2-2026-10-17-06: `apply_effect`'s nested-lock inventory is wrong and its scan has blind spots
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-06: `apply_effect`'s nested-lock inventory is wrong and its scan has blind spots

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Location**: doc block `crates/scripting/src/fragment/effects.rs:45-134`; scan `fragment/tests.rs:4252-4415` (`apply_effect_family_bodies`, `the_nested_lock_residual_list_names_every_type_apply_effect_acquires`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The guards are green but the inventory they pin is incomplete, and one sentence is false:
  1. The doc says "The `FragmentExecutionQueue` write is *not* in this list: it belongs to the latent-continuation path in the dispatch system, outside these guards" (`effects.rs:115-117`). `apply_effects` itself takes `try_resource_mut::<FragmentExecutionQueue>()` at `effects.rs:792` (VM `Utility.Wait`), `:937` (`WaitWhile`) and `:983` (`Wait`) - all inside the paired `QuestStageState`+`QuestObjectiveState` guard (`deferred.rs:1222-1233`). Also `budget::defer` (`effects.rs:764`).
  2. Types acquired under the guards but absent from the doc: `EquipmentSlots` (`inventory.rs:97`, `query_2_mut_mut` at :208), `EquippedWeapon` (:109), `ItemInstancePool` (:120) and the `EquipmentEventBatch`/`ItemEventBatch` batches (`equipment.rs:51,75`) via `remove_items`/`add_items` (called inline from `apply_inventory_effect` :172/:189 and `effects.rs:371-373`); `MiscStats`, `Dead`, `ActorVitals`, `PendingScriptKills` via `evaluate.rs:185,283,594-604`.
  3. The Papyrus VM interpreter runs inside the same guards (`effects.rs:769` -> `run_fragment`) and its natives take well over twenty more component/resource locks (`Inventory`, `SpellList`, `Seated`, `PapyrusReferenceScripts`, `InputLayers`, `EvaluatePackageRequest`, `ConditionForms`, `HavokImpulseRules`, `VmFrameBudget` write at `interp.rs:145`, ...). The doc block says "this list still inventories every lock any of them takes" and has no mention of the VM.
  Why the scan passes: it only reads functions named `apply_*_effect`, only matches the `query/query_mut/resource/resource_mut/try_resource(_mut)/get/get_mut` turbofish forms (not `has`, `query_2_mut_mut`, `resource_2_mut`), and does not follow helper functions (`remove_items`, `add_items`, `evaluate_guard`, `evaluate_value`, `resolve_actor`, `update_actor_cinematic_state`, `emit_*`, any VM native).
- **Evidence**: see line refs above; `grep -n 'FragmentExecutionQueue' effects.rs` shows the three acquisitions below the doc block.
- **Impact**: No deadlock today - the safety argument (every quest-resource system exclusive; no non-exclusive system takes quest resources) holds and was re-verified - but the artefact meant to let an auditor spot a reverse-order acquirer is false for the highest-traffic path (the VM), so the next reverse-order acquirer would not be seen by the guard.
- **Related**: #3949 / #5071 (earlier rounds of the same inventory rot), #3493.
- **Suggested Fix**: Rewrite the block to describe the real invariant ("everything under the paired guards, including the VM, relies on exclusive scheduling; no component/resource in this crate may take QuestStageState/QuestObjectiveState while holding another lock") and extend the scan to follow `fn`s reachable from `apply_effects` (or scan all of `SOURCES` for `QuestStageState`/`QuestObjectiveState` acquisitions inside non-system functions).

######## 5987 [OPEN] bug,low,scripting
# SCR-D2-2026-10-17-07: `hit_event_system` drops a hit the budget cannot start instead of deferring it
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-07: `hit_event_system` drops a hit the budget cannot start instead of deferring it

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/hits.rs:205-214`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: With `starts == 0` the matching registration is skipped (`continue` at :210) while the `HitEvent` marker is drained at the end of the frame by `event_cleanup_system`. Every other budgeted path either defers the run (`run_papyrus_call`, `events::enqueue`) or leaves a *state-based* registration due (`OnUpdate`, distance polling). A hit is a one-shot marker, so this loses it; `RegisterForHitEvent` is itself one-shot, so the script stays armed for a hit that was already consumed by the engine. `docs/engine/scripting.md:909-918` describes deferral and OnUpdate coalescing only and does not list hits.
- **Evidence**: `if starts == 0 { continue; } starts -= 1; fired.push(...)` with the loop above reading `hits` (the marker snapshot).
- **Impact**: Under budget saturation (already a failing-script condition) FO4/Starfield hit handlers miss events silently.
- **Related**: #5764.
- **Suggested Fix**: When `starts == 0` build the `Effect::Interpret` and `budget::defer` it (removing the registration as for a fired one), as `reference_script_event_system` does for `OnHit`.

######## 5988 [OPEN] bug,low,scripting
# SCR-D2-2026-10-17-08: `ALIAS_OWNER_BASE` (0xFC00_0000) overlaps regular plugin slot 0xFC
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-08: `ALIAS_OWNER_BASE` (0xFC00_0000) overlaps regular plugin slot 0xFC

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/alias_scripts.rs:26-28,262-264`; slot map `crates/scripting/src/load_order.rs:84-97`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The doc says the alias-owner range "is no plugin's" and `is_alias_owner` tests only `owner >> 24 == 0xFC`. `global_slot_of` treats `0xFE` as light and `0xFD` as Starfield medium, so `Regular(0xFC)` (the 253rd full plugin) is a valid slot. A reference or quest FormID in that slot would be classified as an alias owner by `owners.rs`/`reference.rs` (`standing`, `vmad`, `alias_owner_*`) and resolve to `None`/stopped.
- **Evidence**: `owner >> 24 == ALIAS_OWNER_BASE >> 24` with no check against the loaded slot table.
- **Impact**: Only load orders with >= 253 full (non-ESL) plugins; scripts of that plugin lose events or run as aliases. Narrow.
- **Related**: none.
- **Suggested Fix**: Move the synthetic range above the real slot space (e.g. keep a separate `OwnerId` enum/flag bit) or assert at boot that no `Regular(0xFC)` slot is allocated.

######## 5989 [OPEN] bug,high,scripting,game:fnv,game:fo3,game:skyrim,game:oblivion,character
# SCR-D3-2026-10-17-01: Authored `GetActorValue` (fn 14) reads an actor-value *enum code* as an AVIF FormID on every pre-FO4 game
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-01: Authored `GetActorValue` (fn 14) reads an actor-value *enum code* as an AVIF FormID on every pre-FO4 game

- **Severity**: HIGH
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/condition/catalog.rs:282` (row), `crates/scripting/src/condition/eval.rs:367` (`actor_value_view` keyed lookup),
  `crates/plugin/src/esm/records/condition.rs:532` (fn 14 in `param1_is_form_id`)
- **Status**: NEW (the 2026-10-12 report noted the type mismatch in passing - `AUDIT_SCRIPTING_2026-10-12.md:148` - "not verified"; verified here)
- **Untrusted-Input**: No
- **Description**: xEdit types fn 14's `param_1` as `ptActorValue` = `wbActorValue` *enum* for TES4, FO3, FNV and TES5 (`wbDefinitionsTES4.pas:776`,
  `FO3.pas:2149`, `FNV.pas:2240`, TES5 `GetActorValue ptActorValue`); only FO4+ stores an AVIF FormID (`GetValue`, `wbActorValue := wbFormIDCkNoReach`).
  The catalog documents/handles it as "the global-space AVIF FormID" for all games, the plugin remaps it as a FormID, and `ActorValues` is keyed by
  AVIF FormID (FNV `Strength` 0x3E8, Skyrim `AVHealth` 0x3E8).
- **Evidence**: CTDA census of the vanilla masters: FalloutNV.esm fn 14 = 1 576 uses (INFO 1 264, IDLE 162, PACK 37, MESG 41, TERM 36, SPEL 35) with
  `param_1` in {0x2b (540), 0x20 (209), 0x28, 0x25, 0x1d, 0x1e, 0x19, 0x09 ...} - SPECIAL codes 5-11, skill/stat codes - while the FNV AVIF FormIDs
  are 0x3E8+ (Strength 0x3E8, Intelligence 0x3EC, Health 0x450); Fallout3.esm 958 uses; Oblivion.esm 120; Skyrim.esm 543 (INFO 318, PACK 169) with
  `param_1` in {68, 95, 17 (Speechcraft), 73, 74, ...} vs Skyrim AVIF FormIDs from 0x3E8. No translation exists anywhere between `parse_ctda`
  and `actor_value_view` (grep: no code->AVIF table; the only construction sites with real AVIF FormIDs are engine-built conditions
  `barter.rs:361`, `combat.rs:1138`, `encumbrance.rs:143`, `commands/gameplay.rs:400`). The unit test `condition/tests.rs:747` uses FormID stand-ins,
  so it cannot see this. Not verified by running the engine.
- **Impact**: every authored skill/SPECIAL/karma/stat gate on FO3/FNV/Skyrim dialogue, packages, quests and scenes evaluates the *absent-AV default
  0.0* (`actor_value_view` returns 0.0 for a missing key) instead of the stat, and `is_modeled` reports it as modeled so no fail-closed consumer
  declines it: `GetActorValue Speech >= 25` options never unlock, `== 0` / `< N` gates always pass. The plugin remap additionally rewrites the code
  into `masters[0]`'s global slot (identity today only because masters sit at slot 0).
- **Related**: `/audit-character` Dim 4 (ActorValues key space); #1663 (closed, implemented the FormID reading for FO4-style authoring); #2987.
- **Suggested Fix**: translate the enum code to the game's AVIF FormID once at the parser boundary (per-game `wbActorValue` table -> `index.actor_values`
  by EditorID) and drop fn 14 from `param1_is_form_id` for the enum games; until then make `is_modeled` return `false` for fn 14 when the loaded game
  is not Fallout 4 / Starfield so the fail-closed gates decline instead of reading 0.0.

######## 5991 [OPEN] bug,medium,scripting
# SCR-D3-2026-10-17-03: Reference/alias-script `OnHit` is delivered before the projectile and `ProcessTrapHit` hits exist
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-03: Reference/alias-script `OnHit` is delivered before the projectile and `ProcessTrapHit` hits exist

- **Severity**: MEDIUM
- **Dimension**: Event Runtime
- **Location**: `byroredux/src/boot/schedule/update.rs:519-523` (`reference_script_event_system`) vs `:569,577` (`projectile_flight_system`,
  `pending_trap_hit_flush_system`); consumer `crates/scripting/src/fragment/reference.rs:605` (`collect_deliveries`) and `:858` (`relay_engine_events`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `HitEvent` is a one-frame marker drained in Late. #5872/ee6468fd1 moved the queued hits (projectile flight, `ProcessTrapHit`) to land
  at `update.rs:577`, "after the script systems ... before every consumer", and `hit_event_consumers_run_after_every_producer` pins
  `combat_damage_system` and `hit_event_system` after every producer. But `reference_script_event_system` is itself a `HitEvent` *consumer* (it turns
  the marker into the Papyrus `OnHit(akAggressor, ..)` delivery, and `relay_engine_events` into remote `OnHit` relays) and sits at `:521`, before
  both producers; the same guard lists it only as a *producer*. A projectile or trap hit lands at `:577`, is consumed by damage/RegisterForHitEvent,
  and is drained by `event_cleanup_system` before the next frame's `:521` ever runs, so `OnHit` never reaches a reference/alias script for those
  hits. Melee/NPC strikes (`:283/:346`, earlier) are delivered.
- **Evidence**: stage order above; `HitEvent` landed only via `trap_hit_requests.rs:62-78` for the queued sources; no other reader of the queue.
- **Impact**: scripts whose `OnHit` reacts to dart-trap / `Weapon.Fire` / `ProcessTrapHit` damage on their reference never run (the damage itself
  does land). One event class silently lost; no state corruption.
- **Related**: #5872, #5860, ee6468fd1.
- **Suggested Fix**: either register a second `HitEvent`-reading delivery after `pending_trap_hit_flush_system` (or move `reference_script_event_system`'s
  OnHit collection there), and add `reference_script_event_system` to the consumer list of `hit_event_consumers_run_after_every_producer`.

######## 5992 [OPEN] bug,medium,scripting
# SCR-D3-2026-10-17-04: `GetTriggerObjectCount` returns a wrong count (excludes the player) instead of declining
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-04: `GetTriggerObjectCount` returns a wrong count (excludes the player) instead of declining

- **Severity**: MEDIUM
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/trigger.rs:145-149` (`trigger_object_count`), `:229` (candidate set excludes the player);
  VM native `fragment/vm/natives/reference.rs:126-129`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The count is `|{(trigger, actor)}|` from `TriggerOccupancyState.inside`, whose actor set is `SceneAliasCandidate` entities
  **minus the player** (`*entity != player`, `:229`); the player's occupancy lives only in `TriggerVolume.occupant_inside`. The CK says "the number of
  objects currently inside this trigger volume" (any object). The native returns the number as a normal `Int` rather than stopping the run (the VM
  contract is "a native that cannot honour its values stops the run"). Secondary: when the candidate list is empty the occupancy pass is skipped
  entirely (`if !actors.is_empty()`, `:254`), so `inside` keeps stale pairs for actors that despawned and the count stays > 0 until another candidate
  exists.
- **Evidence**: `trigger.rs:229` filter, `:254-345` pass gated on `!actors.is_empty()`, `:651`/test `:687` only exercises actor pairs.
- **Impact**: a script gating on `GetTriggerObjectCount() == 0` / `> 0` (trap reset, "area is empty") reads 0 while the player stands in the volume
  and a stale non-zero after the last occupant unloads.
- **Related**: `3cb6056e0` (introduced), docs/engine/scripting.md:1272.
- **Suggested Fix**: add `occupant_inside == Some(true)` of the volume to the count, clear `inside`/`observed_*` when the candidate list is empty, or
  stop the VM run when the occupancy pass has not covered the player.

######## 5993 [OPEN] bug,medium,scripting,quests
# SCR-D3-2026-10-17-05: Scene / phase conditions trust the fail-open Unknown-function pass
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-05: Scene / phase conditions trust the fail-open Unknown-function pass

- **Severity**: MEDIUM
- **Dimension**: Event Runtime (consumer lives in Dim 5)
- **Location**: `crates/scripting/src/scene/playback.rs:385,402` (`evaluate(&scene.conditions ..)`, `evaluate(&phase.start_conditions ..)`);
  same pattern at `quest_stages.rs:1011,1373`, `trigger.rs:541`, `byroredux/src/inventory.rs:1432`
- **Status**: NEW (the fail-closed `is_modeled` gate exists for the five consumers listed in Verified-clean; these were not converted)
- **Untrusted-Input**: No
- **Description**: A scene starts (Begin fragments, actor movement/dialogue) and each phase starts on the bare `evaluate` result; an uncatalogued term
  evaluates 0.0 and satisfies any `== 0`/`<=` comparison. The skill's rule is "a consumer that *acts* on a pass must decline on any `Unknown` term
  first". Skyrim.esm SCEN carries 301 unmodeled CTDA terms out of 2 791 (top: fn 289 x130, fn 18 x78, fn 249 x19, fn 214, 32, 45 ...); QUST 2 184 of
  11 427.
- **Evidence**: census above (python walk over `Skyrim.esm`, SCEN/QUST CTDA function indices minus `CATALOG`); call sites above carry no `is_modeled`.
- **Impact**: scenes whose authored gate cannot be read can start (or phases can advance) on the vacuous 0.0; quest log entries with
  Complete/Fail flags and objective-target highlighting have the same exposure. Bounded to terms outside the catalog.
- **Related**: #5380 (same class, Story Manager), bd85de5ea (dialogue). Scene ownership: Dim 5.
- **Suggested Fix**: route these through one `evaluate_modeled` helper that returns `false` when any term fails `is_modeled` (the same wrapper the SM gate uses).

######## 5994 [OPEN] bug,low,scripting
# SCR-D3-2026-10-17-06: `NativeActivation` is honoured by only some OnActivate consumers
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-06: `NativeActivation` is honoured by only some OnActivate consumers

- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/events.rs:30-39` (contract) vs consumers `papyrus_demo/quest_advance.rs:363`, `papyrus_demo/mod.rs:298`,
  `papyrus_demo/mg07_door.rs:325`, `papyrus_provider/execute.rs:109`, `byroredux/src/extensions/systems.rs:570`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: CK `Activate(ref, true)`: "no OnActivate event will be sent to any scripts". Only the VM delivery (`fragment/reference.rs:602-614,830`)
  filters `NativeActivation`. The lowered quest-advance, rumble, MG07, Papyrus-provider and WASM extension `OnActivate` paths read the raw
  `ActivateEvent`.
- **Impact**: a script's native-only activation of an object that carries one of those handlers runs the handler anyway. Low frequency; no loop
  today (provider IR has no Activate op).
- **Suggested Fix**: share one `papyrus_activations(world)` accessor (ActivateEvent minus NativeActivation) used by all OnActivate-modelling consumers.

######## 5995 [OPEN] bug,low,scripting,quests
# SCR-D3-2026-10-17-07: `quest_advance_system` double-fires for two triggerers on one volume in a frame
Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-07: `quest_advance_system` double-fires for two triggerers on one volume in a frame

- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/papyrus_demo/quest_advance.rs:370-376,389-437`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The per-entity `signalled` dedup stops ActivateEvent+OnTriggerEnter double delivery, but `triggerers` expands to one `(entity, triggerer)`
  pair each and every passing pair pushes its own `PendingAdvance`; each does `set_stage` (which journals a `QuestStageAdvanced`, `quest_stages.rs:349-362`).
  With an `Any` gate (or `BaseForm` plus a second matching actor) two actors entering in the same frame advance the stage twice - including past
  `onlyOnce`/`disableWhenDone`, whose disarm only applies after the loop.
- **Impact**: the stage fragment can run twice for a fire-once trigger (non-idempotent effects such as AddItem repeat). Needs two simultaneous
  entrants; PlayerOnly gates are unaffected.
- **Suggested Fix**: break after the first passing triggerer of an entity (or dedupe `PendingAdvance` by `(entity, quest, stage)`).

