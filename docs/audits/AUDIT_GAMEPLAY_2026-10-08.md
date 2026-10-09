# Gameplay Audit — 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: `AUDIT_GAMEPLAY_2026-10-05.md` (`a2c24b16e`) · **Audited**: Dim 2, 4, 5, 7 (in depth); Dim 1, 6 (only fixes of earlier findings landed; guards re-run) · **Unchanged since baseline (skimmed)**: Dim 3 (Consumables & Timed Effects: no commits on its paths since `a2c24b16e`)

**Scope**: the default `/audit-gameplay` scope, delta-first, run solo as part of `/audit-suite --preset comprehensive`. The baseline is 104 commits old. Since then, the 2026-10-05 findings #5266, #5267, #5268, #5269, #5255 and #5271 have been fixed, and I confirmed each fix is in the code. #5031, #5286 and #5287 are still open.

The new code this run focused on:
- Eat/Sleep procedures plus the ambient Dialogue-procedure wiring (`00f580e09`)
- #5367 dialogue layers G/L/F/V (`14cff35ae`, `f8950e7cc`) and Skyrim force-greet (`287214103`)
- the #5366 Story Manager engine-side producers and save coverage (`3ba9f1d5c`, `17fed2565`, `26b6a779c`, `006905d74`)

**Dedup applied** (already filed this suite, cross-referenced rather than re-reported): CONC-D5-2026-10-08-01, CONC-D3-2026-10-08-01, ECS-2026-10-08-D6-01, ECS D7-01/D7-03, PHYS-D4-2026-10-08-01, PERF D1-01/02/03.

**Evidence base**: no engine was launched. The evidence is:
- the full bin test suite (`cargo test -p byroredux --bin byroredux` on 1.96.0): 2705 passed, 0 failed, 55 ignored;
- raw-ESM Python censuses, with scripts left under `/tmp/audit/gameplay/`:
  - `fnv_pack_census.py`: FNV PACK/NPC_ procedure, PTDT, PKDD and CTDA classification, and a winner simulation under the engine's fail-open rule;
  - `ctda_fn0_census.py`: CTDA function-index-0 usage in Skyrim.esm, Fallout3.esm and FalloutNV.esm;
  - `sky_alfe.py`: Skyrim QUST `ALFE`/`ALFD` event-fill pairs;
- the local GECK wiki pages for the Dialogue, Eat and Sleep packages.

## Executive Summary

| Severity | NEW | Regression | Existing (open) re-confirmed |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 1 | 0 | 0 |
| MEDIUM | 5 | 0 | 2 (#5031, #4232) |
| LOW | 6 | 0 | 2 (#5286, #5287) |

**The HIGH finding:**

- **GAME-D5-2026-10-08-01**: `00f580e09` makes every winning Dialogue-procedure package (PKDT 15) install the force-greet bridge. Package conditions fail open, so 60 FalloutNV NPC_ records get a Dialogue package that wins at every hour of the day:
  - 56 win because a condition the engine cannot evaluate passes them (mostly `GetQuestVariable`, fn 79);
  - 4 win because they have no conditions at all.

  These NPCs force-greet the player on every spawn, every save load and every combat end. One of them is Sunny Smiles in Goodsprings, the reference title's opening area. The commit's own fix for Sunny (mapping CTDA fn 0 to `GetButtonPressed`) never fires: her gate is fn 79, and no FO3 or FNV record authors fn 0. When no INFO line passes, the bridge is never consumed, so the NPC keeps retrying for as long as it carries the directive.

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path | VERIFIED | The production `Dead` inserts are the same set as on 2026-10-05: `combat.rs:363`, `water.rs:60`, `character.rs:1567`, `extensions/commands.rs:537` and the `reference_state.rs` restore/starts-dead sites. Every other hit sits after a `#[cfg(test)]`. #5366 adds a `StoryEvent` stamp next to `combat.rs:363`, but no new damage writer. |
| Death reconciled once | VERIFIED | #5267 is fixed: `restore` queues the teardown (`reference_state.rs:298-305`) and `apply_starts_dead` no-ops on an already-dead actor. `reconcile_dead_actor` → `clear_ambient_behavior` now also removes `EatBehavior`, `SleepBehavior`, `EatSleepState` and `ForceGreetDirective` (`ai_package.rs:604-610`). |
| Index stability (zero rows in place) | VERIFIED | #5266 and #5268 are fixed, and the `inventory::`/`loot_appearance` guards are green. |
| Fail-closed consumables vs fail-open packages | VERIFIED (policy) / **DRIFTED (consequence)** | The asymmetry itself is unchanged (`ai_package.rs:36-56`). What is new is that an intrusive procedure (the force-greet takes over player control) now sits behind the fail-open gate: GAME-D5-2026-10-08-01. |
| Stage order | VERIFIED | `KILL` is raised in Update by `combat_damage_system` and dispatched later in the same Update by `story_manager_dispatch`. `AHEL` is raised by Late `npc_dialogue_selection_system` and by Update `forcegreet_system`, then drained by the dispatcher the next frame (Pattern B, `cleanup.rs:43`). `eat_sleep_system` runs after transform propagation and before walk_anim (`post_update.rs:129`). |
| State saved or re-derived | **DRIFTED** | Three allowlist claims are false: `StoryEventAliasFill` (GAME-D7-2026-10-08-01), `EatSleepState` "idempotent" (GAME-D5-2026-10-08-03) and `ForceGreetDirective` "re-greets mid-approach" (it now re-greets after every load, GAME-D5-2026-10-08-01). |
| Determinism | VERIFIED | `grep -nE 'rand::\|Instant::now\|SystemTime'` over the skill's file list plus `eat_sleep.rs`, `forcegreet.rs`, `story_events.rs` and `dialogue_voice.rs` returns nothing. |

## Findings

### HIGH

### GAME-D5-2026-10-08-01: Ambient Dialogue-procedure wiring + fail-open package conditions = 60 FNV NPCs force-greet the player unconditionally on every spawn and load; a failed open is never consumed
- **Severity**: HIGH
- **Dimension**: NPC Spawn → AI Package Selection → Locomotion (Dim 5) / Interaction (Dim 2)
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:209-219` (the `from_package` Dialogue arm), `:410-418` / `:529-536` (installed at spawn and at runtime), `:36-56` (`package_conditions_pass`); `byroredux/src/systems/forcegreet.rs:69-124`; `crates/scripting/src/condition.rs:197-206`
- **Status**: NEW
- **Trigger**: FNV, any cell holding one of the affected NPCs, e.g. Goodsprings (Sunny Smiles, `GSSunnySmiles` 0x104E84), at any hour, with fresh quest state; repeats on every save load and after every combat.
- **Description**: Before `00f580e09` the only installer of `ForceGreetDirective` was the `dialogue.forcegreet` console door (the `forcegreet.rs` module doc still says vanilla lists these packages on 0 NPC_ defaults). Now any winning PKDT-15 package installs the directive, both at spawn and at re-selection.

  The selector is `active_package`, which takes the first package whose schedule matches and whose conditions pass, and package conditions fail open: any `ConditionFunction::Unknown` makes the whole list pass. The PACK census over FalloutNV.esm, simulated with the engine's own known-function set and schedule rule, gives the same result at hours 3, 12 and 20: 60 NPC_ base records whose first eligible package is a Dialogue package.
  - 56 of them pass only because a condition fails open. The unknown functions are fn 79 `GetQuestVariable` (59 occurrences), then fns 50, 53, 310, 136, 289, 612 and 421.
  - 4 carry no conditions at all.
  - Examples: `VFSGrannyGangerMugPackage`, `VRRCRepConsequenceBark`, `ElDoradoTrooperBarkPackage`, `VHDLegionOliverFightLegateWalkOut`, `VRRCMessengerDialogue`, Sunny's `SunnyMeetPlayerDialoguePackage`.

  The commit adds a CTDA fn 0 → `GetButtonPressed` mapping to stop exactly this for Sunny, but it cannot help:
  - Sunny's package condition is `CTDA 00000000 00000000 4f000000 …`. That is function 79 (`GetQuestVariable` on quest 0x104C66), still `Unknown`, so it still fails open.
  - FalloutNV.esm and Fallout3.esm author zero fn-0 CTDAs anywhere.

  Three further problems:
  - **No consumption on failure.** `forcegreet_open` returns `false` when the topic is missing or no INFO passes. `forcegreet_system` consumes the directive only on success or refusal, so the NPC keeps stepping toward the player (or standing within 128 units) and re-runs `select_on_topic` every frame for as long as it carries the directive.
  - **Re-greets on every load.** Saved state cannot suppress a re-greet: the directive is re-derived by `reseat_ambient_packages_after_restore` on every load and on every combat end (`suspend_ambient_behavior_for_combat` clears the winner).
  - **Wrong target and dialogue type.** The procedure's PTDT target and PKDD Dialogue Type are ignored. Of the 141 NPC-default references, 4 target a non-player reference (e.g. `FollowersVeronicaForceGreetIntercom` → 0xE27FC) and 5 are "Say To". The GECK defines Say To as one spoken line with no dialogue menu. Both still force a player conversation.
- **Evidence**:
  ```rust
  } else if package.procedure_type == PROCEDURE_DIALOGUE {
      Some(Self::Dialogue { topic: package.dialogue_topic })   // no PTDT / dialogue-type / condition-confidence check
  ```
  ```rust
  if forcegreet_open(world, npc, directive.topic) { opened.push(npc); }  // false ⇒ directive kept, retried next frame
  ```
- **Impact**: On FNV, the reference title, NPCs whose force-greets vanilla gates on quest state open a conversation with the player at first contact, regardless of quest progress. They do it again after every load and every fight. With ECS-2026-10-08-D6-01 the walk step is currently discarded, so today the conversation opens when the player comes within 128 BU. Once that bug is fixed, the NPC walks to the player. A greet whose INFO pool is empty leaves the NPC glued to the player.
- **Related**: ECS-2026-10-08-D6-01, PHYS-D4-2026-10-08-01 (walk mechanics); #5367; GAME-D5-2026-10-08-05 (the fn-0 mapping); GAME-D2-2026-10-08-01.
- **Suggested Fix**: Install the force-greet only from a Dialogue package whose conditions were *all* evaluable (do not let the fail-open pass arm a player-control takeover), whose PTDT targets the player, and whose PKDD type is Conversation. Consume or cool down the directive when the open fails. Drop or justify the fn-0 mapping.

### MEDIUM

### GAME-D5-2026-10-08-02: Sleep can never reach a sleep marker on FO3/FNV — every sleeper sits upright in the nearest sit marker (chair or bed), and Eat can pick bed markers
- **Severity**: MEDIUM
- **Dimension**: Dim 5
- **Location**: `byroredux/src/systems/eat_sleep.rs:173-180, 225-227`; `byroredux/src/cell_loader/references/attach.rs:57-71`
- **Status**: NEW
- **Trigger**: FNV/FO3, any NPC whose Sleep package wins (617 NPC-default references in FalloutNV.esm), in a cell with furniture, while `SandboxSitClip` resolves.
- **Description**: `seat_at_marker` prefers markers whose `kind == FurnitureMarkerKind::Sleep` and falls back to `is_sit_marker`. The translate boundary resolves `Sleep` only from `animation_type == 2`, a Skyrim+ field. Legacy Oblivion/FO3/FNV markers carry `animation_type = 0` and always resolve to `Sit`, a documented over-match that includes bed and lean markers.

  PKDT procedures 3 and 4 exist only in the FO3/FNV enum, and Skyrim tree packages resolve only Sandbox and Patrol leaves. So in production the sleep-marker preference never matches. Every sleeper takes the nearest `Sit` marker, chairs included, and an Eat actor can equally take a bed's marker. The unit test `sleep_actor_prefers_the_sleep_marker` passes only because it hand-builds an `animation_type: 2` marker that FO3/FNV data cannot produce. The module doc's "falling back to sit markers when a cell's beds author none" therefore describes every FO3/FNV bed.
- **Evidence**: `kind: match m.animation_type { 2 => Sleep, 3 => Lean, _ => Sit }` (attach.rs). The FO3/FNV comment says: "0 = legacy … no AnimationType authored at all".
- **Impact**: NPCs "sleep" sitting in chairs all night, and diners sit on beds. The Sleep/Eat distinction is silently absent on the only games that run these procedures.
- **Related**: `FurnitureMarkerKind` Phase C (legacy `furnituremarkerNN.nif` decode deferred).
- **Suggested Fix**: Until legacy marker kinds are decoded, make the Sleep arm fall back explicitly and document it. Better, resolve legacy bed furniture from the FURN record or the marker's referenced `furnituremarkerNN.nif` at the translate boundary, so `Sleep` is reachable.

### GAME-D5-2026-10-08-03: Eat/Sleep with a non-NearReference PLDT walk to a hash-random point around wherever the actor currently stands — re-picked on every reinstall and load (the save allowlist's "idempotent" claim is false)
- **Severity**: MEDIUM
- **Dimension**: Dim 5 / Dim 7
- **Location**: `byroredux/src/systems/eat_sleep.rs:97-116`; `byroredux/src/systems/travel.rs:117-126`; `byroredux/src/npc_spawn/ai_package.rs:133-136`; `byroredux/src/save_io/registry_completeness_tests.rs:570`
- **Status**: NEW
- **Trigger**: FNV, an Eat package whose PLDT is In-Cell, Near Current Location or Near Editor Location (418 of 706 Eat NPC-default references), or the equivalent Sleep package (324 of 617, including 142 In-Cell). Especially after an earlier package (Travel/Sandbox) moved the actor away from home.
- **Description**: `from_package` passes a PLDT target only for `NearReference`. For every other location type, `resolve_destination` falls back to `pick_wander_target(home, radius, form_id, 0)`, with `home` set to the actor's current `GlobalTransform` at first sight.
  - **"Near editor location" eats near the current spot.** An actor that a Travel package took to a bar eats near the bar, not at home.
  - **"In cell X" is ignored.** The actor sleeps in the nearest sit marker around its current position.
  - **Random walk before seating.** The actor first walks a random offset (up to the radius, or 512 BU when unauthored) and then searches seats within the radius of that random point, rather than taking "any chair within the location radius" (GECK Eat/Sleep Package).

  `EatSleepState` is unsaved, and its allowlist row justifies that with "resolve_destination is idempotent". For this path it is not: after a load, combat or handover the new pick is centred on the actor's new position.
- **Evidence**: `let home = world.get::<GlobalTransform>(npc)…; resolve_destination(world, target_form_id, radius…, form_id, home)` with `target_form_id = None` for non-NearReference PLDTs.
- **Impact**: Diners and sleepers drift wherever their previous package left them, walk random offsets, and can end up seated outside the authored location. The destination after a save load differs from the one before it.
- **Related**: PHYS-D4-2026-10-08-01 (no waypoints, `blocked` dropped); PERF D1-03.
- **Suggested Fix**: For the location-type cases, use the location's centre (editor location = the spawn placement, current location = no walk, cell = skip when not resident) as the destination and the seat-search centre. Correct the allowlist row.

### GAME-D2-2026-10-08-01: `forcegreet_open` has no `player_can_act` gate and no open-conversation guard — a dead player is force-greeted, and a force-greet hijacks a live conversation with another NPC
- **Severity**: MEDIUM
- **Dimension**: Interaction & dialogue (Dim 2)
- **Location**: `byroredux/src/systems/npc_dialogue.rs:477-507`; `byroredux/src/systems/forcegreet.rs:49-125`
- **Status**: NEW
- **Trigger**: Any game. A Dialogue-package NPC (GAME-D5-2026-10-08-01) or a console-installed directive while (a) the player is dead, or (b) the player is mid-conversation with another NPC.
- **Description**: The activation selection and the topic click both gate on `player_can_act` (#5043/#4701). The skill names a third dialogue path without that gate as the regression class. `forcegreet_open` checks only `npc_refuses_dialogue`, and `forcegreet_system` does not check the player at all, so the NPC walks to the player's corpse and opens a conversation. It also never looks at `DialogueSurfaceState.npc`. `apply_selection` then strips the current partner's `NpcDialogueTopic`, runs that partner's OnEnd fragment and re-points the surface, cutting off a conversation the player is in.
- **Evidence**: `if npc_refuses_dialogue(world, npc).is_some() { return false; }` is the only actor gate before `apply_selection(world, npc, selected, record)`.
- **Impact**: The dialogue surface opens over a death screen and the spoken line's OnBegin fragment runs. A live conversation is pre-empted mid-line, and the outgoing line's OnEnd fragment fires early. With the ambient wiring, any of the 60 always-winning NPCs triggers this.
- **Related**: #5043, #5367, GAME-D5-2026-10-08-01.
- **Suggested Fix**: Gate `forcegreet_open` (or `forcegreet_system`) on `player_can_act` and on no open surface (`DialogueSurfaceState.npc.is_none()`). Keep the directive pending rather than consuming it.

### GAME-D2-2026-10-08-02: Dialogue voice maps a FormID's load-order slot to the wrong plugin in every multi-plugin session (slot 0 = `--esm`, but masters own slot 0) — and re-implements an existing correct helper
- **Severity**: MEDIUM
- **Dimension**: Dim 2
- **Location**: `byroredux/src/systems/dialogue_voice.rs:66-85`; correct helper at `byroredux/src/cell_loader/load_order.rs:315`; slot assignment at `load_order.rs:591-` and `cell_loader/load.rs:426-430`
- **Status**: NEW
- **Trigger**: FO3/FNV with any `--master` (e.g. `--master FalloutNV.esm --esm DeadMoney.esm`, the documented DLC route), any voiced line.
- **Description**: `plugin_file_for` treats byte 0 as `LoadedPluginSet.esm_path` and byte *k* as `masters[k-1]`. The loader builds the load order as `masters…, esm`, and `#1554` global-slot assignment hands regular slots out in that order. So masters occupy 0..n-1 and the `--esm` takes slot n.
  - A vanilla FalloutNV INFO (slot 0) composes `sound\voice\deadmoney.esm\…`.
  - A DLC INFO composes `falloutnv.esm\…`.
  - ESL slots (0xFE) and medium slots (0xFD) are not handled.

  Every line misses and falls back to the subtitle estimate without any warning, by design. `load_order::plugin_for_form_id` already resolves slot → basename correctly, including ESL sub-indices, and has three tests. The single-plugin smoke (`dt1`) cannot see this, because there slot 0 really is the `--esm`.
- **Evidence**: `let raw = if byte == 0 { esm_path.as_str() } else { masters.get(byte - 1)… }` versus `plugin_paths = masters.iter().chain(once(esm_path))`.
- **Impact**: Phase V voice is silent for every line in any DLC or multi-master FO3/FNV session.
- **Related**: #5367 (V).
- **Suggested Fix**: Resolve through the session's `LoadOrder` with `plugin_for_form_id` (widen its visibility and keep the `LoadOrder` reachable as a resource) instead of the duplicate mapping.

### GAME-D7-2026-10-08-01: Story-Manager-started quests lose their event-filled aliases across a save/load — the `StoryEventAliasFill` allowlist's "quests restart through fresh events" is false
- **Severity**: MEDIUM
- **Dimension**: Gameplay-state coverage (Dim 7)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:508`; `crates/scripting/src/scene/quest_alias.rs:733-800`
- **Status**: NEW (the save-coverage half; the in-process stale-EntityId half is ECS D7-03)
- **Trigger**: Skyrim, an SM-started quest with `ALFE`/`ALFD` event-fill aliases running at save time. The census found 19 such aliases: `WIKill03/04/05/06`, `MGSuspensionQuest`, `DA02KillFriend` (KILL R1/R2/L1) and `DialogueGenericDogHellos` (AHEL R1/R2). Load the save in a fresh process.
- **Description**: The allowlist says the resource needs no save because "after a load the quests restart through fresh events". They do not. The quest is restored as running by the saved `QuestStageState`, and the event that started it (an old kill, an old hello) is never raised again. The alias refresh then runs as follows:
  - it drops every prior binding for registered quests (`resolved.retain(|(quest, _), _| !registered_quests.contains(quest))`);
  - it fills `FromEvent` aliases only from `StoryEventAliasFill`, bypassing the candidate scan by design;
  - with the map empty after a fresh-process load, `story_fills.get(quest)` is `None` and the alias stays unbound.
- **Evidence**: `if let Some(AliasFillType::FromEvent { data, .. }) = alias.fill_type { if let Some(slots) = story_fills.get(quest) { … } continue; }`
- **Impact**: After a load, a running radiant or SM quest's Victim, Killer or speaker aliases resolve to nothing, so stage logic and conditions that read them see `None`.
- **Related**: ECS D7-03 (in-process stale rebind); #5366.
- **Suggested Fix**: Save the per-quest event slots in a FormId-keyed form (e.g. the slots' persistent reference ids), or bind the restored `SceneActorBindings` entries for FromEvent aliases instead of dropping them. Correct the allowlist row.

### LOW

### GAME-D5-2026-10-08-04: `forcegreet_system` moves NPCs outside the `BYRO_NO_AI_LOCOMOTION` kill switch, and the kill-switch guard cannot see `eat_sleep_system`
- **Severity**: LOW
- **Dimension**: Dim 5
- **Location**: `byroredux/src/boot/schedule/update.rs:442`; `byroredux/src/boot/schedule/post_update.rs:90-136`; `byroredux/src/boot/schedule/mod.rs:304-345`
- **Status**: NEW
- **Description**: The skill's contract is that `BYRO_NO_AI_LOCOMOTION=1` gates exactly the motion systems. `forcegreet_system` steps NPCs toward the player but is registered unconditionally in Update. Since `00f580e09`, ambient selection installs it on its own, so a locomotion-isolated run still has NPCs walking. `eat_sleep_system` is correctly inside the block. However, `locomotion_kill_switch_gates_exactly_the_motion_systems` only collects `crate::systems::make_*` names, so `crate::systems::eat_sleep::eat_sleep_system` is invisible to it: moving it out of the block would fail no test. `walk_animation_registers_after_all_six_movers` likewise lists neither new mover.
- **Suggested Fix**: Gate the force-greet walk step on the switch (keep the open path ungated) and extend both guards to the new movers.

### GAME-D5-2026-10-08-05: The new `GetButtonPressed` (CTDA fn 0) mapping rests on a false premise, and its doc comment swallowed `GetDistance`'s
- **Severity**: LOW
- **Dimension**: Dim 5 (package conditions; evaluator owned by `/audit-scripting`)
- **Location**: `crates/scripting/src/condition.rs:83-89, 197-206, 711-717`
- **Status**: NEW
- **Description**: `from_index`'s comment says the FNV corpus authors fn 0 on Sunny Smiles' packages. The census says otherwise:
  - FalloutNV.esm and Fallout3.esm contain zero fn-0 CTDAs.
  - Sunny's gate is fn 79.
  - Skyrim.esm's 19 fn-0 CTDAs all sit on IDLE records (`PowerAttack`, `BashFail`, `WispLeftAttack`, …), combat predicates rather than a MessageBox query. They now evaluate to a constant -1 if IDLE conditions are ever evaluated.

  `GetButtonPressed` is a script-only function (the CS raw function list does not type it "Condition"). Separately, the variant was inserted between `GetDistance`'s doc comment and `GetDistance`. Rustdoc now attaches "GetDistance(target_form_id) → f32 … index 1" to `GetButtonPressed`, and `GetDistance` is left undocumented (the same defect class as #5269).
- **Suggested Fix**: Remove the fn-0 arm, or re-derive what fn 0 is from xEdit's table, and restore the `GetDistance` doc placement.

### GAME-D2-2026-10-08-03: Dialogue voice segments are fire-and-forget — changing topic, closing or the speaker dying does not stop the line
- **Severity**: LOW
- **Dimension**: Dim 2
- **Location**: `byroredux/src/systems/dialogue_voice.rs:183-196`; `byroredux/src/systems/npc_dialogue.rs:319-396, 514-534`
- **Status**: NEW
- **Description**: `play_line_voice` schedules every response segment as `play_oneshot` with a delayed start and keeps no handle. When `apply_selection` replaces the line (topic click) or `end_open_conversation` closes it, the old segments, including ones not yet started, keep playing over the new line. This is inconsistent with Phase L's line-lifetime model.
- **Suggested Fix**: Keep the line's sound handles on the dialogue state and stop them in `apply_selection`'s outgoing-line path and in `end_open_conversation`.

### GAME-D2-2026-10-08-04: The FO3 profile ships no sound archives, so the FO3 half of #5367 Phase V never resolves through the profile path
- **Severity**: LOW
- **Dimension**: Dim 2
- **Location**: `assets/debug_profiles.toml:134-150` (`[profiles.fo3]`)
- **Status**: NEW
- **Description**: Phase V is documented as "FO3/FNV", but only `[profiles.fnv]` gained a voice archive (`Fallout - Voices1.bsa`). `[profiles.fo3]` has no `default_sounds_bsas` at all, although `Fallout - Voices.bsa` and `Fallout - Sound.bsa` ship in the FO3 Data dir. `SoundArchiveProvider` is therefore empty, and every FO3 line quietly falls back to the estimate, along with FO3 footsteps, splash and REGN audio (#3788's FNV fix was never mirrored).
- **Suggested Fix**: Add `default_sounds_bsas = ["Fallout - Sound.bsa", "Fallout - Voices.bsa"]` to the FO3 profile, after verifying the voice path convention against the FO3 archive.

### GAME-D4-2026-10-08-01: The `KILL` story event is raised by one of the four live death producers and never carries its location
- **Severity**: LOW
- **Dimension**: Combat & death (Dim 4)
- **Location**: `byroredux/src/combat.rs:365-386`; `byroredux/src/systems/water.rs:59-60`; `byroredux/src/systems/character.rs:1567`; `byroredux/src/extensions/commands.rs:537`
- **Status**: NEW
- **Description**: Only `combat_damage_system` stamps `KILL`. Deaths from NPC drowning, player drowning and the SDK AV batch insert `Dead` without raising it. On Skyrim (the SM title) a drowned NPC therefore never reaches KILL-rooted nodes. The KILL stamp also sets `location_1: None`, while the AHEL producer resolves the session LCTN (`resolve_current_lctn`). `WIKill06` authors a KILL→L1 alias fill (census). The location-alias runtime is Phase-3+ scope, so this half is latent today.
- **Suggested Fix**: Raise KILL from a shared helper at every live death producer (not the corpse restores), with `location_1: resolve_current_lctn(world)`.

### GAME-D7-2026-10-08-02: `StoryLocationCursor` survives an in-process load, so loading a save from another cell raises a CLOC whose "old location" is the pre-load session's
- **Severity**: LOW
- **Dimension**: Dim 7
- **Location**: `crates/scripting/src/story_manager.rs:351-353, 382-419`; `byroredux/src/save_io/registry_completeness_tests.rs:507`
- **Status**: NEW
- **Description**: The cursor is unsaved and is installed only when absent. An in-process load therefore keeps the outgoing session's key and LCTN:
  - loading a save in a different cell fires `CLOC` with `L1` = a location the loaded game was never in;
  - loading a save in the same cell fires nothing;
  - a fresh-process load fires with `L1 = None`.

  The allowlist's "a fresh boot/load legitimately re-fires the event" covers only the last case. CLOC-rooted SM nodes see load-path-dependent event data.
- **Suggested Fix**: Reset the cursor (or its `location`) as part of the save-load apply so every load behaves the same, and document whether a load should raise CLOC at all.

## Dimension notes

- **Dim 1** (fixes only: #5266, #5268, #5269, #5079, #5095, and the racial spell pair 9813af435): guards are green in the full bin run. No new finding.
- **Dim 3**: unchanged since 2026-10-05.
- **Dim 6** (only #5306 landed): guards are green. #5286 and #5287 are still open.
- **Dim 5, four-list diff**: Eat, Sleep and Dialogue are present in `from_package`, `insert_at_spawn`, `insert_at_runtime` and `clear_ambient_behavior`. Eat and Sleep are registered in the debug server (the binary-crate `ForceGreetDirective` cannot be). `suspend_ambient_behavior_for_combat` and the unconscious suspension both go through `clear_ambient_behavior`, so they cover the new components.

## Known-Open Register

| Issue | State |
|---|---|
| #4232 `effective_actor_level` returns 0 | open, unchanged |
| #5031 mid-life gear queue keeps one root-less equip per wearer | open, unchanged |
| #5286 `drain_transition_notifications` `.expect`s | open |
| #5287 `ModelStage.key` bare `#[allow(dead_code)]` | open |
| #5367 dialogue layers | open: Skyrim+ `.fuz` voice remains (Skyrim force-greet landed in 287214103) |
| Cross-audit, this suite | ECS-2026-10-08-D6-01 (movers write `GlobalTransform`), PHYS-D4-2026-10-08-01, CONC-D5/D3-2026-10-08-01, ECS D7-01/D7-03, PERF D1-01/02/03 |

## Summary

| Severity | NEW | Existing (tracked) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 5 | 2 |
| LOW | 6 | 2 |

Suggested next step: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` (labels `gameplay` + `ai` / `dialogue` / `quests`; `game:fnv` on GAME-D5-2026-10-08-01/02/03, `game:fo3` on GAME-D2-2026-10-08-04, `game:skyrim` on GAME-D7-2026-10-08-01).
