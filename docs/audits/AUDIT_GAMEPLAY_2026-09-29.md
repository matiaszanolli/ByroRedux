# Gameplay Audit — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: `AUDIT_GAMEPLAY_2026-09-24.md` (`aabd99a05`) · **Audited**: Dim 1, 2, 4, 5, 6, 7 · **Unchanged since baseline (skimmed)**: Dim 3 (Consumables & Timed Effects)

**Scope**: the default `/audit-gameplay` scope, delta-first, run as part of `/audit-suite --preset comprehensive`. The baseline is 149 commits old. Three new gameplay features landed since then:
- the P3 player body, with third-person walk/idle (`a070baaad`, `db8351587`);
- mid-life gear import (`0182fc5e8`);
- P4 NPC dialogue: selection plus the native response surface (`ab31cfefe`, `766e1746e`).

All 15 findings from the baseline were published as #4812–#4826, and all 15 are now closed. Every dimension was analysed synchronously in this pass, with no sub-agents. Each finding below was re-read against the code and then checked for a way to disprove it before it was kept.

**Games and cells exercised**: no engine was launched and no GPU process was run. The evidence comes from:
- the binary, plugin and scripting test suites;
- a raw-ESM Python census of `Skyrim.esm` (SE): every DIAL's `QNAM`, `DATA` and `SNAM`, plus the full MS01 (`0x00018B4B`) topic list.

## Executive Summary

| Severity | NEW | Regression | Existing (open) re-confirmed |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 1 | 0 | 0 |
| MEDIUM | 3 | 0 | 1 (#4232) |
| LOW | 4 | 0 | 2 (#4709, #4710) |

**The HIGH finding:**

- **GAME-D7-2026-09-29-01: loading a save cannot bring a dead player back.** `Dead` is registered as an additive save column. The player entity survives the reload, so the live overlay never removes the marker. After any player death (drowning or an NPC kill), every load produces a player with restored health who still carries `Dead`. That player cannot move, attack, activate, loot or equip until the process restarts.

**The three MEDIUM findings** all sit in today's P3/P4 features:

- **GAME-D1-2026-09-29-01**: the mid-life gear queue keeps only one equip per wearer and silently drops the rest. This is the handoff routed from ECS, confirmed. It is wider than first reported: a weapon equip, or a same-frame displacement, also takes the one slot.
- **GAME-D1-2026-09-29-02**: worn meshes are never reconciled against the `EquipmentSlots` a load restores. The player's third-person body keeps showing the pre-load session's gear.
- **GAME-D2-2026-09-29-01**: an NPC "owns" every DIAL of every running quest that binds it, with no filter on speaker, category or branch. On MS01, Eltrys lists 117 topics that belong to about 14 speakers, including scene lines and barks.

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path (HitEvent → `combat_damage_system`; water, drowning, SDK batch and Starts Dead are the documented extra producers) | VERIFIED | Production `Dead` inserts are `combat.rs:347`, `water.rs:59`, `character.rs:1470`, `extensions/commands.rs:516`, `reference_state.rs:282`, and `:330` (`apply_starts_dead`, #4814). All reach `reconcile_dead_actor`. Every other hit is inside `#[cfg(test)]` |
| Death reconciled once | VERIFIED | #4814: Starts Dead is applied once, at the single actor-job completion site (`references/mod.rs:752`). #4817: `CombatDisposition` is armed at finalize (`resumable.rs:1338`, `:2134`). New runtime components clean up on death: `AmbientEngagement` and `PendingGearImport` are dropped. `NpcDialogueTopic` persists on a corpse (see the ECS-D7-01 cross-reference) |
| Inventory index stability (rows zeroed, never removed) | VERIFIED | `inv.add` → `add_stack` merges into a same-base row or appends. `pickup_loot` (#4695/#4818) and restore are unchanged. `inventory_index_for` picks the first same-base row, but nothing reads the result (GAME-D7-2026-09-29-02) |
| Fail-closed consumables vs fail-open packages | VERIFIED | Dim 3 is unchanged. `restoration_plan` is untouched, and `package_conditions_pass` is still fail-open (`unknown_condition_function_remains_fail_open` passes) |
| Stage order | VERIFIED | #4712 is fixed. The Update order is: restoration → interaction → fragment_activation_flush → container_loot → combat_input → faction_hostility → npc_combat_ai → combat_damage. Ambient packages run before scene packages. In Late, `equipment_appearance_system` → `npc_dialogue_selection` → … → `event_cleanup_system` (last). `apply_action` runs between frames (`app_events.rs:984`) |
| State saved-or-rederived | **DRIFTED** | **GAME-D7-2026-09-29-01**: a player `Dead` is never cleared by a load. **GAME-D1-2026-09-29-02**: worn meshes are not re-derived from the loaded slots. The allowlist rows for today's new types have accurate rationales |
| Determinism (no RNG or wall clock in gameplay logic) | VERIFIED | The grep covered `combat`, `inventory`, `interaction`, `ai_package`, `faction_hostility`, `npc_dialogue`, `combat_ai`, `loot_appearance`, `player_body` and the 9 Dim-5 systems. The only hit is `player_body.rs:191` `Instant::now`, which times the attach for a log line |

## Findings

### HIGH

### GAME-D7-2026-09-29-01: A dead player stays `Dead` after loading a save — the additive overlay never clears the marker from the process-lifetime player
- **Severity**: HIGH (a load silently fails to restore gameplay state; no in-session recovery)
- **Dimension**: 7 — Gameplay-state coverage (saved-or-rederived)
- **Location**:
  - `byroredux/src/save_io.rs:418` (`.register_component::<Dead>("Dead")`)
  - `byroredux/src/save_io.rs:108-110` (`Dead` in `MUTABLE_DELTA_COLUMNS`)
  - `byroredux/src/save_io.rs:1798-1813` (post-overlay reconcilers)
  - `crates/save/src/registry.rs:99-103`
- **Status**: NEW. This is the same class as #3488 (closed; that fix covered `EquippedWeapon` only) and sits downstream of #4701 (closed).
- **Trigger**: any game with a live death producer. Examples:
  - FNV Lake Mead: drown (`character.rs:1470`);
  - Skyrim MQ101, or any hostile NPC: killed through `combat_damage_system` (`combat.rs:347`).

  Then quickload, or load any save in which the player was alive.
- **Description**:
  - `Dead` has a plain, additive registration. Only `register_replacing_component` makes a saved *absence* authoritative for FormID-matched entities (`registry.rs:99-103`). `Perks` and `TimedRestorations` use it precisely so that the overlay "clears a saved absence on the live player" (`save_io.rs:101`, `:395-396`).
  - The player entity is process-lifetime and "survives the cell reload untouched" (`save_io.rs:1800-1806`).
  - After the overlay, only two reconcilers run:
    - `reconcile_dead_actor_runtime_state`, which only acts on entities that *have* `Dead`;
    - `reconcile_player_equipped_weapon`.

    Nothing removes the pre-load session's `Dead` from the player.
- **Evidence**:
  - `grep -rn 'remove::<Dead>'` finds no production site. All five hits are in `#[cfg(test)]`: `inventory.rs:1814` and `:2997`, `interaction.rs:2360`, `loot_appearance.rs:765`, `npc_dialogue.rs:619`.
  - This is also why `round_trip_tests::delta_columns_removed_at_runtime_have_a_load_reconciler` stays green. It scans runtime *removal* sites, but this gap is a runtime *insert* on an entity that outlives the reload.
  - #4701's own issue body states "There is no game-over or reload flow". Loading a save is the only recovery path.
- **Impact**: the overlay restores the saved `ActorValues`, so health is back above 0, but the player keeps `Dead`:
  - `player_controller` refuses movement (`character.rs:172`);
  - `player_can_act` (`character.rs:100-105`) blocks attack, activate, loot and equip;
  - `faction_hostility` treats the player as a non-target.

  The session cannot be recovered without restarting the engine. A secondary effect: the death teardown (`reconcile_dead_actor(player)`) stripped the capsule's idle `AnimationPlayer`, which `db8351587` now attaches, along with the body skeleton root's player. Nothing re-attaches them after a "revival".
- **Related**: #3488, #4701, #3022; GAME-D1-2026-09-29-02 is the same process-lifetime-player class.
- **Suggested Fix**: register `Dead` with `register_replacing_component`. The saved absence of `Dead` on a FormID-matched actor is authoritative, because a resident NPC is respawned alive anyway. Or add a post-overlay player reconciler that clears `Dead` and re-runs the body's locomotion attach. Pin the fix with a dead-player → load-alive-save test.

### MEDIUM

### GAME-D1-2026-09-29-01: The mid-life gear queue keeps one root-less equip per wearer per batch and drops the rest — a second never-worn item (or one equipped while an import is pending) never gets its worn mesh
- **Severity**: MEDIUM
- **Dimension**: 1 — Inventory & Equipment Model (P3 mid-life gear import)
- **Location**:
  - `byroredux/src/npc_spawn/loot_appearance.rs:285-309` (`queue_midlife_imports` request collapse)
  - `:321-323` (pending skip)
  - `:338-343` (empty-path `continue`)
  - `:673-681` (multi-path drain)
- **Status**: NEW. The handoff was routed from `/audit-ecs` (noted in `AUDIT_ECS_2026-09-29.md`, Dim 7) and is confirmed here.
- **Trigger**: several equips for the same wearer land in one frame. Change batches merge per wearer per frame (`crates/scripting/src/equipment.rs:51-68`), so this happens when:
  - a quest fragment runs `EquipItem` twice (`fragment/effects.rs:840-880`);
  - several queued `inv.equip` or menu toggles drain in one `drain_pending_inventory_actions`;
  - one equip displaces another.

  It also happens when a second equip arrives while a multi-ARMA Skyrim import is still draining at one NIF per frame.
- **Description**:
  - `if !requests.iter().any(|(w, _)| w == wearer)` keeps only the **first** root-less `equipped: true` change per wearer. Later changes in the same batch are discarded.
  - `if world.get::<PendingGearImport>(wearer).is_some() { continue; }` discards a new request while an earlier import is still draining.
  - There is no retry. Late `event_cleanup_system` clears `EquipmentEventBatch` the same frame, and nothing rescans `EquipmentSlots`.
- **Wider than reported**:
  - **Non-armor equips take the slot.** `resolve_armor_meshes` returns empty for non-`Armor` items (`crates/plugin/src/equip.rs:185-192`). If a weapon equip comes first in the batch, the request is dropped at `:338-343`, and an armor equipped after it in the same batch is never queued.
  - **Unequips are ignored by the queue** (`:291-293`), and the hide pass only sees roots that already exist. Take the batch `[equip A, equip B that displaces A]`: A (now unequipped) is imported and drawn, and B is dropped. The wrong piece shows.
  - **A multi-path import is not re-checked mid-drain.** If a multi-path (ARMA) import is unequipped mid-drain, its remaining paths still attach visibly, because the loader never re-reads `EquipmentSlots` (`:673-681`).
- **Impact**: the item is equipped in `EquipmentSlots` but is invisible in third person, or a displaced item is drawn instead. The state persists until the player unequips and re-equips by hand. The feature is actor-generic, so NPC scripted equips are affected too.
- **Related**: REN-D5-2026-09-29-02, PERF-D7-2026-09-29-02 and CONC-D4-2026-09-29-01 cover the same feature and are cross-referenced, not re-filed. See also GAME-D1-2026-09-29-02.
- **Suggested Fix**:
  - Queue a list per wearer (`Vec<(form_id, paths)>`) and append instead of skipping.
  - Resolve each change in batch order, so an unequip cancels a queued equip of the same form.
  - Have the loader re-check `EquipmentSlots` before each path.
  - Add tests for a two-item batch, weapon-then-armor, and equip-while-pending.

### GAME-D1-2026-09-29-02: Worn meshes are never reconciled against loaded `EquipmentSlots` — after a load the player's body shows the pre-load session's gear (NPCs re-attach wearing the record outfit)
- **Severity**: MEDIUM (visual/gameplay divergence from saved state; the canonical `EquipmentSlots` are correct)
- **Dimension**: 1 — Player body + mid-life gear (the skill's "verify post-load worn meshes follow saved `EquipmentSlots`" check fails)
- **Location**:
  - `byroredux/src/player_body.rs:120-236` (a single, idempotent attach from `NPC_ 0x7`; `:128-133`)
  - `byroredux/src/save_io.rs:1798-1813`
  - `byroredux/src/cell_loader/reference_state.rs:265-267` (restore inserts `EquipmentSlots` only)
- **Status**: NEW
- **Trigger**:
  - **Player**: equip a never-worn item and/or unequip an outfit piece, then save. Change gear again, then load. Or start with `--load` from boot, which builds the body from the `NPC_ 0x7` defaults. Switch to third person.
  - **NPC**: script-unequip a spawn-time piece, or mid-life equip a new one, then leave the cell and return (the parked row restores the slots).
- **Description**:
  - The player body is assembled once, from `NPC_ 0x7`'s outfit, and survives the reload untouched.
  - The load overlays `EquipmentSlots` and re-derives only `EquippedWeapon`. It emits no `EquipmentEventBatch`.
  - None of the visual state is saved, by design: `NpcAppearanceHidden`, `HiddenFirstPerson`, the mid-life `NpcEquipmentPart` roots, and `PendingGearImport`.
  - The result is that the body shows whatever the pre-load session last showed:
    - saved mid-life equips draw no mesh;
    - saved unequips of default outfit pieces still draw;
    - after a boot `--load`, the `NPC_ 0x7` default outfit draws regardless of the saved slots.
  - NPCs have the same problem. `NpcSpawnJob` assembles `armor_to_spawn` from the record, and `reference_state::restore` then overwrites `EquipmentSlots` with the parked row. Nothing re-derives the meshes from the restored slots.
- **Impact**: third-person gear disagrees with inventory and equipment state after any load or cell return that follows a gear change. Combat, `GetEquipped` and the save are all correct; only the visuals diverge.
- **Related**: GAME-D1-2026-09-29-01; GAME-D7-2026-09-29-01 (same process-lifetime-player overlay class); #3488.
- **Suggested Fix**: after the overlay (player) and after `restore` (NPC), diff the live `NpcEquipmentPart` roots against `EquipmentSlots`:
  - hide roots whose form is no longer equipped;
  - reveal roots whose form is equipped;
  - queue a `PendingGearImport` for equipped forms that have no root.

  This reuses `equipment_appearance_system`'s paths through a synthesized change list.

### GAME-D2-2026-09-29-01: NPC topic ownership ignores speaker, DIAL category and branch structure — Eltrys "owns" all 117 MS01 topics, and the opening line is picked by lowest form id
- **Severity**: MEDIUM
- **Dimension**: 2 — NPC dialogue (P4)
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:85-100` (`owned_topic_records`)
  - `:219-229` (list + first-passing selection)
  - `:329-336` → `crates/debug-ui/src/panels.rs:524` (renders every entry)
  - `crates/plugin/src/esm/records/misc/dialogue.rs:199` (`dial_type = DATA[0]`)
- **Status**: NEW
- **Trigger**: Skyrim, the P4 fixture. Start MS01, activate Eltrys in `MarkarthWarrens`. The same happens for any NPC bound to any running quest.
- **Description**:
  - An NPC owns every DIAL whose `quest_refs` contains any running quest that binds it. Nothing checks:
    - whether the topic's INFOs are spoken by this NPC;
    - its category (Topic vs Scene / Combat / Misc);
    - whether it starts a branch or is a child reached only through a parent INFO's link.
  - The topic list is built from **all** of them.
  - The opening line is the INFO on the lowest-form-id DIAL with a passing INFO.
- **Evidence** (a raw census of `Skyrim.esm` SE, counting DIALs whose `QNAM` is `0x00018B4B`):
  - **117 DIALs in total.** By `DATA` byte 1 (category): 108 are Topic (0), 5 are Scene (2: `0xD6627`, `0xD6628`, `0xD6641`, `0xD6642`, `0xD6643`), and 4 are Misc (7: `HELO` `0x228A5`, `GBYE` `0x228A4`, `IDAT` `0x82546`, `NOTI` `0x9C88D`).
  - **Their editor IDs name about 14 speakers**: Eltrys, Hogni, Kerah, Guard, Margret, Innkeeper, Thonar, Rhiada, Weylin, Nepos, Uaile, Garvey, Mulush and Omluag.
  - **The opening-line rule picks a mid-branch topic.** Lowest-form-id-first puts `0x18A30` `MS01EltrysBlockingShrineBranch01Topic01`, a mid-branch child, ahead of that branch's entry, `0x806B8` `MS01EltrysBlockingShrineBranch01EntryTopic`.
  - **`dial_type` cannot be used as a filter today.** `parse_dial` stores `DATA[0]`, which on Skyrim is the Topic *Flags* byte (0 on all 117 MS01 DIALs); the category is byte 1.
- **Impact**:
  - The response surface lists every MS01 prompt of every speaker, plus scene lines and barks.
  - Clicking another NPC's topic fails `select_on_topic` ("no INFO passes"), and only a `log::warn!` records it (`main.rs:1512`).
  - Whenever a mid-branch child's INFO passes, the NPC opens by answering a prompt the player never chose.

  This is a silent misroute in the one supported P4 route, not an absent feature. The deliberate "no generic greeting scan" scope is unaffected.
- **Related**: ECS-2026-09-29-D7-01 (the snapshot reads the first `NpcDialogueTopic`), ESM-2026-09-29-D2-03 (QNAM docs). `/audit-esm` owns the `DATA` byte layout.
- **Suggested Fix**:
  - Keep only topics with at least one INFO whose speaker condition can match this NPC.
  - Exclude Scene, Combat and Misc categories once `DATA[1]` is decoded (a Skyrim/FO4 layout fix for `/audit-esm`).
  - Start from branch-entry topics (DLBR `SNAM`, or the top-level flag) rather than from the lowest form id.
  - Add a fixture test with two speakers and a branch child.

### LOW

### GAME-D1-2026-09-29-03: Module docs still say mid-life gear import is open
- **Severity**: LOW (doc rot)
- **Dimension**: 1
- **Location**:
  - `byroredux/src/player_body.rs:28-33` ("Two halves stay open … no new-gear import for mid-life equips")
  - `byroredux/src/npc_spawn/loot_appearance.rs:199-205` ("Newly acquired gear … applied to mid-life equips later")
- **Status**: NEW. The skill already notes this ("Its module doc still calls … mid-life import open"). The walk/idle half of the `player_body.rs` doc was updated; the mid-life half was not.
- **Description**: both docs contradict code in the same file. `equipment_appearance_system` ends by calling `queue_midlife_imports`.
- **Suggested Fix**: rewrite both paragraphs to describe the queue → `GearImportLoader` path, with player FaceGen as the one remaining open item.

### GAME-D2-2026-09-29-02: Dialogue selection skips #4701's `player_can_act` gate and does not refuse a combatant NPC
- **Severity**: LOW
- **Dimension**: 2 — NPC dialogue
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:203-208`
  - `:251-265` (`select_topic_by_form_id`)
  - `byroredux/src/interaction.rs:1256-1292` (the Talk candidate arm)
- **Status**: NEW
- **Description**:
  - `player_can_act` (`character.rs:100`) fronts `combat.rs:143`, `interaction.rs:1451` and `inventory.rs:1109`/`:1525`. The two dialogue entry points never call it. A scripted player activation (`script.activate`) or a topic click on an already-open surface still selects and advances dialogue for a Dead player.
  - Neither the Talk arm nor the selection excludes an NPC that carries `AiCombatState`. An alias-bound NPC fighting the player therefore still shows a "Talk" prompt, and activating it opens the surface mid-fight. The simulation is deliberately not paused (`p4-quest-fixture.md`).
- **Suggested Fix**: gate both entry points on `player_can_act`. Skip NPCs with `AiCombatState` in the Talk arm and in the selection.

### GAME-D4-2026-09-29-01: A scripted `StartCombat` on an actor already ambient-engaged with the same target keeps the ambient marker, so the scripted fight auto-drops
- **Severity**: LOW (it needs an ambient engagement against the same target to already exist)
- **Dimension**: 4 — Combat
- **Location**:
  - `crates/scripting/src/fragment/effects.rs:1534-1547`
  - `byroredux/src/systems/faction_hostility.rs:474-496`
- **Status**: NEW (an edge case of #4816, now closed)
- **Description**:
  - `Effect::StartCombat` overwrites `AiCombatState` only. It cannot see `AmbientEngagement`, which is a binary-crate type.
  - `disengage_lost_contact` treats any combat whose current target equals `engagement.target` as ambient, and drops it after `DISENGAGE_GRACE_SECS` out of contact. The out-of-contact timer is not reset either.
  - The #4816 contract says "scripted combat is never dropped" (module doc `:50`, and the save allowlist row). `scripted_combat_never_disengages_on_lost_contact` only covers a *re-targeted* scripted combat (`:780-815`).
- **Trigger**: a quest fragment runs `StartCombat(player)` on an NPC that `faction_hostility` already armed against the player (a hostile-faction ambush actor). The player then breaks line of sight or range for 10 s.
- **Suggested Fix**: have `StartCombat` clear the marker, for example through a scripting-side `ScriptedCombat` tag that `faction_hostility` honours. Add a same-target test.

### GAME-D7-2026-09-29-02: `NpcEquipmentPart.inventory_index` is written but never read; `0182fc5e8` adds a helper and a test to populate it
- **Severity**: LOW (written-never-read)
- **Dimension**: 7
- **Location**:
  - `byroredux/src/npc_spawn.rs:1098`
  - writers: `npc_spawn/resumable.rs:680`, `:786`, `:1841`, and `npc_spawn/loot_appearance.rs:646`
  - `loot_appearance.rs:508-524` (`inventory_index_for` + its doc)
- **Status**: NEW
- **Description**:
  - No production code reads the field. Hide/reveal and ownership both match on `form_id`, and `grep '\.inventory_index'` outside `EquippedWeapon` and the tests finds nothing.
  - The new `inventory_index_for` claims to point "at the same row the equip events name". `EquipmentChange` carries only `item_form_id`, and the helper takes the first same-base row, which may be a zeroed row or an instance row.
- **Suggested Fix**: drop the field and the helper, or give the field the consumer its doc implies. If the field is kept, resolve the row from `EquipmentSlots`, not from the first same-base row.

## Fix verification (since the 2026-09-24 baseline)

| Fix | Verdict | Notes |
|---|---|---|
| #4812 (`e52d4a8a1`), templated outfit and level | VERIFIED | Race comes from Use Traits, level from the Use Stats terminal, and DOFT/CNTO from the Use Inventory terminal (`npc_spawn.rs` ~1215-1300). No shell reads remain |
| #4696 (`e52d4a8a1`), LVLO counts | VERIFIED | Carry lists use the counted walk; the outfit keeps the flat walk |
| #4813 / #4820 (`f87490826`), Initially Disabled | VERIFIED | The gate honours a scripted enable over the default. `reference_enable_gate` tests pass (12, together with the activation-order and `ambient_ai` filters) |
| #4814 (`f87490826`), Starts Dead | VERIFIED | `apply_starts_dead` runs once, at the single actor-job completion (`references/mod.rs:752`), after `stamp_quest_reference`, and is a no-op if `restore` already made the actor dead |
| #4815 (`424aad268`), package reseat | VERIFIED | `reseat_ambient_packages_after_restore` runs before the procedure-state overlay, and the probe is kept as a regression test |
| #4816 (`424aad268`), disengage | VERIFIED; one edge case | GAME-D4-2026-09-29-01 |
| #4817 / #4819 / #4822 / #4823 (`017c5e8a5`) | VERIFIED | Disposition is armed at finalize (both twins, `!player_body`). `SpellList` is parked and restored after `stamp_spell_list`. An empty `SpellList` is stamped. The `NPC_SPAWN_STAMPED` rows are fixed |
| #4818 / #4695 (`6c5555c70`) | VERIFIED | `restore_resident` applies the tombstone in the reload window, and colliders are despawned |
| #4712 (`373f6a4cc`) | VERIFIED | The flush runs before `container_loot_system` |
| #4713 / #4714 / #4711 / #4821 / #4824 / #4825 / #4826 | VERIFIED | Pins and guards pass |

## Cross-audit pointers (not re-reported here)

- **Already filed today** (cross-referenced, not re-filed):
  - `/audit-ecs`: ECS-2026-09-29-D7-01 (the dialogue snapshot reads the first `NpcDialogueTopic`); D6-01 (the per-frame Talk candidate scan); D1-01 (lock-order cycles in `populate_candidates` / `running_quests_binding_entity`); D5-01 (Access rows).
  - `/audit-performance`: PERF-D1-2026-09-29-01 (same scan); PERF-D7-2026-09-29-02 (`GearImportLoader` opens a third archive set on the main thread).
  - `/audit-renderer`: REN-D5-2026-09-29-02 (gear imports are never released).
  - `/audit-concurrency`: CONC-D4-2026-09-29-01 (`equipment_appearance_system` does not declare gear-import access); CONC-D3-2026-09-29-01 (the dialogue guard is shadowed).
  - `/audit-esm`: ESM-2026-09-29-D2-03 (QNAM docs).
- **`/audit-esm`**: on Skyrim, `DialRecord::dial_type` reads `DATA[0]`, which is the Topic Flags byte; the category is `DATA[1]` (census above). This is the decode half of GAME-D2-2026-09-29-01.
- **`/audit-save`**: GAME-D7-2026-09-29-01 is a registration-kind decision (additive vs replacing) for `Dead`. The fix probably touches the serde and format guards only if the registration kind is fingerprinted.
- **`NpcDialogueTopic` on a corpse**: it persists after death. Fold this into ECS-D7-01's "clear on close / new selection" fix instead of filing it separately.
- **#4739** (the combat sound cache, owned by the audio audit) is still open, but `546366364` moved `combat_anim.rs` onto `SoundCache` (`:384`). It is probably ready to close.

## Known-Open Register (verified today; cite, don't re-file)

| Issue | State at HEAD |
|---|---|
| #4232 `effective_actor_level` returns 0 | Open and unchanged. #4812 now reads the Use Stats terminal's level, but through the same function |
| #4415 magic runtime (partial) | Open. The #4819/#4822 slices landed |
| #4710 `walk_anim` module doc | Open and unchanged |
| #4709 the kill switch also drops combat feedback | Open and unchanged |
| #4742 / #4743 / #4744 combat audio | Open (audio-owned) |
| 10 of ~17 PACK procedures have no runtime; FO4+ walk clips are absent; cross-tile pathing is blocked | Unchanged. The findings above are misroutes of supported cases only |
| Every 2026-09-24 finding (#4812–#4826) | Closed and verified (see the table above) |

## Test verification

| Command | Result |
|---|---|
| `cargo test -j8 -p byroredux --bin byroredux -- combat:: inventory:: interaction:: npc_spawn:: reference_state ambient_locomotion walk_anim save_io:: loot_appearance player_body gear notifications loading_screen spawn_tests registry_completeness scheduler_access npc_dialogue consum restoration timed_ faction_hostility synth_child scripted_lock_gate` (skill baseline plus the Dim 1–7 guards) | 392 passed, 0 failed, 18 ignored (installed data) |
| `… --bin byroredux -- reference_enable_gate fragment_activation_order ambient_ai` | 12 passed |
| `cargo test -j8 -p byroredux-plugin --lib consumables` | 9 passed |
| Python census of `Skyrim.esm` (SE): the DIAL top group (15,037 DIALs), `QNAM` / `DATA` / `SNAM` / `EDID`, and the MS01 owner list | 117 MS01 DIALs; the category distribution is given in GAME-D2-2026-09-29-01 |

The `#[ignore]`d installed-data tests were not re-run, because the code they guard is unchanged (Dim 3). No probe test was built; GAME-D7-2026-09-29-01 rests on the registration kind, the missing production removal, and the survives-reload contract, each cited above.

## Deduplication

- The open issues come from `/tmp/audit/issues.json` (163). Closed issues were searched for "dialogue topic", "DLBR", "dialogue branch", "gear import", "mid-life", "player body equipment", "dial_type", "Dead player load", "Dead additive overlay", "revive player" and "NpcEquipmentPart". The nearest matches were #3488, #3022, #4701 and #1846. All four are closed, and each covers a different case.
- Today's sibling reports (ECS, RENDERER, PERFORMANCE, CONCURRENCY, SAFETY, NIF, NIFAL, PARSERS, ESM) were read for overlap. Their overlaps are the cross-references above. GAME-D1-2026-09-29-01 is the ECS handoff and is filed only here.

Next step: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-09-29.md`. Suggested labels are `gameplay` plus:
- `save-load` for GAME-D7-2026-09-29-01;
- `inventory` for GAME-D1-2026-09-29-01, -02 and -03;
- `dialogue` + `quests` for GAME-D2-2026-09-29-01 and -02 (add `game:skyrim` to -01, which is title-specific);
- `combat` + `ai` for GAME-D4-2026-09-29-01;
- `tech-debt` for GAME-D7-2026-09-29-02.
