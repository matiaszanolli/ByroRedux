# Gameplay Audit — 2026-10-05

**HEAD**: `a2c24b16e` · **Baseline**: `AUDIT_GAMEPLAY_2026-09-29.md` (`9fcfdc3fc`) · **Audited**: Dim 1, 2, 4, 5, 6, 7 · **Unchanged since baseline (skimmed)**: Dim 3 (Consumables & Timed Effects; its only commit, `ce73f658b` / #5084, changes how the EFID is looked up, not the behaviour, and its guards are green)

**Scope**: the default `/audit-gameplay` scope, delta-first, run as part of `/audit-suite --preset comprehensive`. The baseline is 313 commits old. All eight 2026-09-29 findings were published (#5027, #5031, #5034, #5037, #5040, #5043, #5046, #5049). Seven are now closed and verified. #5031 is still open, and its code is unchanged. I analysed every dimension myself, synchronously, without sub-agents. I re-read each finding against the code and tried to disprove it before keeping it.

**Games and cells exercised**: no engine was launched and no GPU process was run. The evidence comes from:
- the binary, plugin and scripting test suites;
- a raw-ESM Python census of `FalloutNV.esm`: every DIAL's `QSTI` and `DATA`, every INFO's `QSTI`, and the `CTDA` function ids on INFOs of multi-quest topics (script left at `/tmp/audit/gameplay/fnv_dial_census.py` for this run).

## Executive Summary

| Severity | NEW | Regression | Existing (open) re-confirmed |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 2 | 0 | 2 (#5031, #4232) |
| LOW | 6 | 0 | 0 |

**The two MEDIUM findings:**

- **GAME-D1-2026-10-05-01**: the post-load worn-gear reconcile (#5034) always counts the equipped weapon as a missing worn mesh, because weapons never get a mesh root. It shares #5031's one-request-per-wearer queue, and the order it picks forms in comes from a `HashSet`. So, after a load, the player's root-less armor imports or not at random. Once the weapon takes the slot, the remaining armor never imports.
- **GAME-D4-2026-10-05-01**: `reference_state::restore` brings a corpse's parked `dead` row back by running `reconcile_dead_actor`, and with it `activate_ragdoll`, synchronously at actor-job completion. #4814 identified that point as one where bone `GlobalTransform`s are not yet in world space, and queued `apply_starts_dead` to avoid it. When the spawn finishes in the same frame its skeleton imports, a corpse returning to a cell gets its ragdoll seeded from identity bone transforms.

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path (HitEvent → `combat_damage_system`; water, drowning, SDK batch and Starts Dead are the documented extra producers) | VERIFIED | The production `Dead` inserts are `combat.rs:363`, `water.rs:60`, `character.rs:1567`, `extensions/commands.rs:516`, `reference_state.rs:298` and `:353`. #5223 adds a new input (`script_killed_corpse_forms`) to the existing `apply_starts_dead` site (`references/mod.rs:761-766`), not a new insert site. Every other hit is inside `#[cfg(test)]`. The player strike now resolves through `cast_ray_corridor` from the eye (#5160, #5124); NPC strikes are range-based (`combat_ai.rs`) |
| Death reconciled once | **DRIFTED** (timing) | Every producer reaches `reconcile_dead_actor`. `restore` is the one producer that runs it before the skeleton has propagated (GAME-D4-2026-10-05-01). `AmbientEngagement` drops itself at the next hostility evaluation. `PendingGearImport` is dropped by `step_imports`' `Dead` check |
| Inventory index stability (rows zeroed, never removed) | VERIFIED | `transfer_loot` (`All` takes whole stacks; `Stack` zeroes in place), `pickup_loot`, `restore` and `consume_item` are unchanged. The new #5028 "still held" filter tests row *presence* instead of `count > 0` (GAME-D1-2026-10-05-02) |
| Fail-closed consumables vs fail-open packages | VERIFIED | `restoration_plan` is still all-or-nothing. #5084 only reroutes the EFID lookup, and the game gate still makes Oblivion and FO4+ Aid items "unavailable". `package_conditions_pass` is still fail-open (`unknown_condition_function_remains_fail_open` passes) |
| Stage order | VERIFIED | Update registers in this order (`update.rs`): restoration `:20`, interaction `:111`, flush `:181`, container_loot `:185`, combat_input `:225`, faction_hostility `:257`, npc_combat_ai `:288`, combat_damage `:339`; ambient `:400` runs before scene `:413`. Late registers water `:187`, pending_dead `:202`, equipment_appearance `:417`, npc_dialogue `:439` and event_cleanup `:521`, in that order. #4709 moved combat feedback outside the kill-switch block, still after walk_anim |
| State saved-or-rederived | **DRIFTED** (visual) | `Dead` and `ActorControlState` are now replacing columns (#5027/#5052), so the prior HIGH is fixed. The player's faction reset (#5058), the worn-gear reconcile (#5034) and the #5054 park path all carry `control`. Drift: the worn-gear re-derivation is lossy when a weapon is equipped (GAME-D1-2026-10-05-01). The completeness guard cannot see 8 fully-qualified impls (GAME-D7-2026-10-05-01) |
| Determinism (no RNG or wall clock in gameplay logic) | VERIFIED, one caveat | The grep covered the 15 skill files plus `objectives`, `restoration` and `reference_state`. The only hit is `player_body.rs:193`, `Instant::now`, which times the attach for a log line. Caveat: the order of #5034's import queue comes from `HashSet` iteration (GAME-D1-2026-10-05-01) |

## Findings

### MEDIUM

### GAME-D1-2026-10-05-01: The post-load worn-gear reconcile counts the weapon slot as a missing mesh and shares #5031's one-request queue, so a loaded player's root-less armor imports at random (and stops importing once the weapon takes the slot)
- **Severity**: MEDIUM (visual and gameplay divergence from saved state; the canonical `EquipmentSlots` are correct)
- **Dimension**: 1 — Inventory & Equipment Model (P3 post-load reconcile, #5034)
- **Location**:
  - `byroredux/src/npc_spawn/loot_appearance.rs:515-534` (`equipped_forms` = occupants `.chain(equipment.weapon)`)
  - `:582-597` (the `missing` set is built from `HashSet` iteration, then passed to `queue_midlife_imports`)
  - `:407-426` (the first request per wearer wins)
  - `:441-463` (a form with no resolved paths is dropped with `continue`)
  - `:1000-1005` (the chained reconcile runs only on a successful import)
  - `crates/plugin/src/equip.rs:185-192` (`resolve_armor_meshes` returns empty for anything that is not `ItemKind::Armor`)
- **Status**: NEW. This is a new call site, added by #5034 (`20717d5b7`, closed), inheriting the #5031 queue shape (open). The event-path half stays #5031's.
- **Trigger**: the player has a weapon equipped plus at least one equipped armor piece that has no spawn-time root. Examples:
  - FNV: a pistol plus mid-life Leather Armor;
  - Skyrim: a sword plus a looted helmet.

  Save, then load (quickload, or `--load` from boot), and switch to third person.
- **Description**:
  - #5034's `reconcile_worn_gear` builds the set of equipped forms from every occupied biped slot **and the weapon slot**. Any equipped form with no live non-intrinsic `NpcEquipmentPart` root goes into the `missing` list.
  - Weapons never get a root. Both spawn arms build `NpcEquipmentPart` only from `armor_to_spawn` (`resumable/prebaked.rs:89-104`, `resumable/runtime.rs:341-356` and `:437-452`), and no mid-life weapon mesh is imported. So the weapon form is in `missing` on every call.
  - `queue_midlife_imports` keeps only the **first** request per wearer (`:422`). It resolves meshes only afterwards, and a WEAP resolves to no paths, so that request is discarded with `continue` (`:458`) and nothing is queued.
  - `missing` is collected from a `std::collections::HashSet` (`RandomState`), so whether the weapon or an armor comes first changes from run to run.
  - The chained reconcile at `:1005` re-runs only after a *successful* import. Once the weapon is picked first, nothing ever runs the reconcile again. The import-failure paths (`:884-893`, `:908-931`) do not chain either.
- **Evidence**:
  - The guard test `load_reconcile_diffs_roots_against_restored_slots` (`:1296-1371`) puts an **armor** form in the weapon slot (`slots.equip_weapon(InventoryIndex(1))` with `install_index(0xCCC, "...gauntlet.nif")`). It never exercises a real WEAP, and it has only one missing form.
  - `grep -rn 'NpcEquipmentPart {' byroredux/src` finds only the armor builders, `player_body.rs` and `loot_appearance.rs:949`.
- **Impact**: after any load with a weapon equipped, the third-person body may show none, some, or all of the saved mid-life armor, and which one it shows varies from run to run. NPCs are mostly unaffected: their restored slots usually match the record outfit, so the weapon is the only missing form and nothing needs importing. An NPC that equipped a new piece mid-life and was then evicted hits the same stall when it returns.
- **Related**: #5031 (the same queue, event path); #5034 (introduced the call site); ECS-2026-10-05-D7-01 (the gear-release `Children` leak).
- **Suggested Fix**: drop the weapon slot from `equipped_forms`, or skip forms that `resolve_armor_meshes` cannot serve *before* choosing the request. Also chain the next reconcile on the failure paths. The #5031 fix (a per-wearer list instead of a single slot) removes both halves; build `missing` in a deterministic order (slot order). Add a test with a real WEAP and two root-less armors.

### GAME-D4-2026-10-05-01: `reference_state::restore` runs the death teardown (ragdoll activation) synchronously at actor-job completion — the exact un-propagated-skeleton point #4814 queued `apply_starts_dead` to avoid
- **Severity**: MEDIUM (a corpse renders or simulates from a wrong ragdoll seed; loot and `Dead` state are correct)
- **Dimension**: 4 — Combat & Death Pipeline (death reconciled once / correctly); also Dim 2 (loot persistence)
- **Location**:
  - `byroredux/src/cell_loader/reference_state.rs:297-300` (`if state.dead { world.insert(entity, Dead); reconcile_dead_actor(world, entity); }`)
  - versus `:341-355` (`apply_starts_dead`: "Queued rather than run here because the ragdoll seeds from bone `GlobalTransform`s, which a freshly spawned skeleton does not have in world space until the next PostUpdate propagation")
  - caller: `cell_loader/references/synth_child.rs:80-98` (`stamp_quest_reference` → `restore`), at the actor-job completion `references/mod.rs:746-766`
  - `byroredux/src/combat.rs:566-640` (`reconcile_dead_actor` → `activate_ragdoll`)
- **Status**: NEW. It predates the baseline: the dead branch is unchanged, but #4814 (`f87490826`) documented the hazard after this branch existed. It may relate to #4772 (open; FO3 restore's first ragdoll solve blows up), which is unconfirmed.
- **Trigger**: any game with ragdoll templates (FO3/FNV/Skyrim). Kill an NPC; its ragdoll settles. Leave the cell (door transition, or exterior stream-out), so `capture` parks `dead: true`. Then return. The respawned actor job completes, and if its skeleton import and completion land in the same budgeted call (`NpcSpawnJob::step` loops units until `budget.should_yield()`), the corpse is affected.
- **Description**:
  - New entities are spawned with `GlobalTransform::IDENTITY` (`scene/nif_loader.rs:1343`, `:1794`). Only the placement root is seeded with a world `GlobalTransform` (`npc_spawn/resumable/mod.rs:358`).
  - `restore` inserts `Dead` and calls `reconcile_dead_actor` immediately. That function calls `activate_ragdoll`, which seeds every body from `bone GT ∘ local offset` (`ragdoll.rs:338-345`).
  - On a skeleton that has not yet propagated, every bone sits at the origin. #5161's sanity gate (`crates/physics/src/ragdoll.rs:51-52, 98-117`) rejects only a seed more than 1e5 BU from the root, so within 100k BU of the origin the ragdoll is **built at world origin**; further away, the activation is rejected.
  - Either way, `reconcile_dead_actor` has already stripped the corpse's `AnimationPlayer`/`AnimationStack`. A rejected corpse stands in its bind pose.
  - Nothing retries the activation. `reconcile_dead_actor_runtime_state` runs propagation first, but only on a save load, and it would then report "ragdoll already active" for the origin case.
  - `apply_starts_dead`, called at the same completion point, deliberately routes through `queue_dead_actor_reconciliation` → Late `reconcile_pending_dead_actors_system`, which runs after PostUpdate propagation.
- **Evidence**:
  - The #4814 rationale comment quoted above.
  - The two producers sit side by side in the completion arm: `restore` (via `stamp_quest_reference`) and then `apply_starts_dead`.
  - `restore_resident` (`save_io.rs:1796`) also calls `restore` before the propagation inside `reconcile_dead_actor_runtime_state` (`combat.rs:655-672`).
  - The tests `empty_container_and_dead_actor_survive_repeated_evictions` and `starts_dead_actor_is_a_queued_corpse_at_completion` use fixtures without a `RagdollTemplate`, so neither can observe the seed.
- **Impact**: a revisited corpse appears at world origin, or stands upright, instead of lying where it fell. This happens on every revisit where the respawn completes in a single budgeted call. Looting is unaffected because `Dead`, `Inventory` and the collider owner live on the root. Whether a given respawn completes in a single call depends on the budget. I did not prove the frequency, because no engine was run.
- **Related**: #4814 (the queueing precedent); #4772 (open, ragdoll blow-up on FO3 restore); #5161 (the seed gate); GAME-D1-2026-10-05-01 (the same restore call also drives `reconcile_worn_gear`).
- **Suggested Fix**: in `restore`, insert `Dead` and call `crate::combat::queue_dead_actor_reconciliation(world, entity)` in place of the synchronous reconcile, mirroring `apply_starts_dead`. Pin it with a test that gives the restored actor a `RagdollTemplate` and asserts that no `RagdollActive` exists until the Late drain.

### LOW

### GAME-D1-2026-10-05-02: #5028's gear-release path has no production producer, and its "still held" filter contradicts the zero-in-place row convention
- **Severity**: LOW (a tested-but-unwired path; latent)
- **Dimension**: 1 / 7 (written-never-read class)
- **Location**:
  - `byroredux/src/npc_spawn/loot_appearance.rs:293-392` (`queue_gear_releases`; the filter is at `:359-372`)
  - `byroredux/src/inventory.rs:921-934`, `:1011-1023` (the only producer of `added: false`)
- **Status**: NEW. #5028 is closed.
- **Description**:
  - `queue_gear_releases` acts only on an `ItemEventBatch` row with `added == false` for a wearer **without** `CellRoot`, which in practice means only the player.
  - The only production producer of `added: false` is `transfer_loot`, and it emits it on the *source*. `transfer_loot` refuses `player == source` (`:927`).
  - `pickup_loot` emits only `added: true`. The scripting `Effect`s add items but never remove them (`fragment/effects.rs:770-880`). No drop, sell or `RemoveItem` path exists.
  - So the player can never produce the event the release path waits for. Its doc comment claims "drop, sell, destroy". The six `release_*` tests insert the event batch by hand.
  - Separately, when a producer does land, the "still held" test `inventory.items.iter().all(|stack| stack.base_form_id != form_id)` (`:364-367`) checks whether a row is *present*. Rows are zeroed in place, never removed (`LootSelection::Stack`, `consume_item`), so a zero-count row of the same base would block the release. The filter should test `stack.count > 0`, the same way `reconcile_worn_gear` does at `:528`.
- **Impact**: none today. An unequipped mid-life piece stays resident and hidden, by design. The ECS finding ECS-2026-10-05-D7-01 (a despawned gear root left in the body root's `Children`) is latent for the same reason.
- **Suggested Fix**: correct the doc to say the path is forward-latent until a player-side removal exists. Change the filter to count only rows with `count > 0`. Add a test with a zeroed same-base row.

### GAME-D1-2026-10-05-03: #5058 inserted `reset_player_factions_to_record` between `reconcile_player_equipped_weapon`'s doc comment and its `fn` — rustdoc now attaches the #3488 weapon doc to the faction reset
- **Severity**: LOW (documentation; the same class #5227 fixed elsewhere)
- **Dimension**: 1
- **Location**: `byroredux/src/inventory.rs:1629-1697`
- **Status**: NEW
- **Description**:
  - The 25-line `///` block beginning "Rebuild the runtime [`EquippedWeapon`] consequence…" (`:1629-1653`) now runs straight into the #5058 block ("post-load faction reset…") and documents `reset_player_factions_to_record`.
  - `pub(crate) fn reconcile_player_equipped_weapon` (`:1697`) is left with no doc at all, and the doc it lost is the one that explains the additive-overlay contract that the reconciler exists for.
- **Suggested Fix**: move the #5058 block and its function above `:1629`, or below `reconcile_player_equipped_weapon`.

### GAME-D2-2026-10-05-01: FO3/FNV dialogue ignores each INFO's own owning quest — a multi-quest topic can speak an INFO whose quest is not running (and quest priority / quest dialogue conditions are not applied)
- **Severity**: LOW (mostly masked by `GetIsID`; misroutes the residue only)
- **Dimension**: 2 — NPC dialogue (P4 route, extended to FO3/FNV by #5224)
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:112-128` (`owned_topic_records`: any running bound quest in the DIAL's `quest_refs`)
  - `:226` (`select_first_info` walks every INFO of the DIAL)
  - `:240` (`owning_quest = quest_refs.first()`)
  - `crates/plugin/src/esm/records/misc/dialogue.rs:249-` (`InfoRecord` has no quest field)
- **Status**: NEW. The decode half belongs to `/audit-esm`, and the INFO/CTDA semantics to `/audit-scripting`.
- **Trigger**: FNV or FO3. Activate an NPC bound by a running quest that owns a shared Top-level topic, for example `DoctorMedical` `0x1DCEE` or `FollowersHired` `0x37084`.
- **Description**: in FO3/FNV, each INFO names its own quest (`QSTI`), and one DIAL can list several. In the GECK, an INFO counts only while its quest is running. That quest's priority orders the INFOs, and its dialogue conditions gate them all ("Quest conditions are checked first; only if those are true are the conditions on the infos evaluated", GECK *Quest Data Tab*). None of this is decoded or applied: once any one running bound quest owns the DIAL, INFOs from every listed quest compete in file order.
- **Evidence** (Python census of `FalloutNV.esm`):
  - All 23,247 INFOs carry `QSTI`.
  - 138 DIALs list more than one quest. In 28 Topic-type DIALs, the INFOs span more than one quest.
  - The Top-level (`DATA[1] & 0x02`) ones are `DoctorMedical`, `FollowersHired`, `FollowersFired`, `DoctorSupplies`, `DoctorRadiation` and four Gomorrah/Primm topics. Most of their INFOs have `GetIsID` (fn 72) conditions, which mask the problem: `DoctorMedical` has 34 INFOs, 5 without `GetIsID`; `FollowersHired` 26 and 2; `FollowersFired` 11 and 1.
  - `GREETING` (`0xC8`, 209 quests, 5,300 INFOs) has flags `0x00`, so #5224 makes it link-only, and it never opens a list.
- **Impact**: on these topics, an NPC can speak a line that belongs to a stopped quest, or skip a higher-priority quest's line. The NPC-side log also names the wrong owning quest. Skyrim (one `QNAM` per DIAL) is unaffected by the per-INFO half, but Skyrim quest dialogue conditions are not applied either.
- **Suggested Fix**:
  - Decode the INFO `QSTI` (FO3/FNV/Oblivion) into `InfoRecord.quest`.
  - In `select_on_topic` / `top_level_menu`, filter INFOs to running quests that bind or own the speaker, and order them by quest priority.
  - Have `/audit-scripting` decide on applying quest-level dialogue conditions.

### GAME-D6-2026-10-05-01: `drain_transition_notifications` documents that it tolerates an absent `QuestDefinitionRegistry`, then `.expect`s it
- **Severity**: LOW (the panic is unreachable today; the contract and the code disagree)
- **Dimension**: 6 — Player feedback (#5153 journal announcements)
- **Location**: `byroredux/src/objectives.rs:175-214`, specifically `:187-190` and `:198`. The function is called every frame from `app_frame.rs:254`.
- **Status**: NEW
- **Description**:
  - The function's comment says: "The definitions guard is optional: a notification must compose even when the registry is absent (the objective state outlived it across a reload boundary) — the objective then announces by index".
  - The objective *text* honours that: `definitions.as_ref().and_then(...)`. The quest *name* does not: `quest_display_name(definitions.as_ref().expect("guard above"), event.quest)`.
  - There is no guard above. With pending objective events and no registry, the per-frame drain panics in the render loop.
  - Production installs the registry at scripting registration (`crates/scripting/src/lib.rs:209` → `quest_stages::register`) and never removes it, so this is latent.
- **Suggested Fix**: fall back to `format!("Quest 0x{:08X}", quest.0)` when `definitions` is `None` (for example, make `quest_display_name` take an `Option`). Add a test that drains an event with no registry.

### GAME-D7-2026-10-05-01: The save registry-completeness guard only recognises a literal `impl Component for X` / `impl Resource for X`, so 8 fully-qualified impls — including #5028's `PendingGearRelease` — are neither registered nor allowlisted
- **Severity**: LOW (a guard gap; every affected type appears correctly unsaved)
- **Dimension**: 7 — Gameplay-state coverage (the guard that is supposed to prove each name is classified)
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:121-137` (`impl_target_type`)
- **Status**: NEW. It was noted during the 2026-10-05 audit-skill sync but not filed. The guard's owner is `/audit-save`.
- **Description**: `impl_target_type` strips the prefix `"impl Component for "` / `"impl Resource for "`. A line written `impl byroredux_core::ecs::Component for X` or `impl byroredux_core::ecs::resource::Resource for X` does not match, so the type is never discovered. Eight types are invisible this way, and none of them is classified anywhere:
  - `PendingGearRelease` (`npc_spawn.rs:1166`, #5028), the newest gameplay handoff component;
  - `TriggerOccupancyState` (`crates/scripting/src/trigger.rs:143`), keyed by session `EntityId`;
  - `HudControl` (`hud.rs:276`);
  - `GracefulExitRequested` (`app_events.rs:100`);
  - `ScaleformHudDiag` (`scaleform_hud.rs:199`);
  - `CellLoadPhaseTimings` (`cell_loader/load.rs:64`);
  - `SceneEffectSoftCache` (`render/mod.rs:57`);
  - `ScriptProvider` (`asset_provider/script.rs:80`).

  Five more fully-qualified impls (`NpcEquipmentPart`, `NpcSkeletonBones`, `ActorBodyClass`, `PendingGearImport`, `DialogueSurfaceState`) are classified only because their rows were added by hand.
- **Impact**: the next fully-qualified gameplay component that really does need saving would pass the guard silently. Today's eight are all runtime scratch, diagnostics or providers.
- **Suggested Fix**: match any path that ends in `Component for` / `Resource for`, for example a regex `impl\s+(?:[\w:]+::)?(Component|Resource)\s+for\s+(\w+)`. Then allowlist the eight types with rationales. Add a fixture line in the guard's self-test.

### GAME-D7-2026-10-05-02: `ModelStage.key` sits under a bare `#[allow(dead_code)]` and is never read
- **Severity**: LOW (dead code; the allow has no justification)
- **Dimension**: 7 (justify every `#[allow(dead_code)]`) and 6 (the LSCR model cover, `e60911864`)
- **Location**: `byroredux/src/loading_screen.rs:84-85`; the only writer is `:425-427`
- **Status**: NEW
- **Description**:
  - The model-stage key is built (`:209`) and stored, but nothing compares it. Every cover spawns a fresh stage, and only the `Artwork::Image` key is compared, for reuse (`:249-252`).
  - Every other `allow(dead_code)` in the gameplay files carries a reason (`inventory.rs:894`, `:907` for #4464; `interaction.rs:181` is `cfg_attr(not(test))`).
- **Suggested Fix**: drop the field and the attribute. If the field is meant for a future stage cache, add a comment saying so.

## Fix verification (since the 2026-09-29 baseline)

| Finding / issue | Verdict | Notes |
|---|---|---|
| GAME-D7-01 / #5027 (+#5052), dead player survives a load | VERIFIED | `Dead` and `ActorControlState` are now `register_replacing_component` (`save_io.rs:434`, `:471`). The skill's known-open entry for this case (ragdoll re-activation on revive) still applies |
| GAME-D1-01 / #5031, one-request gear queue | **Open, unchanged** | `loot_appearance.rs:422`, `:438` and `:458` are as reported. Its load-path twin is GAME-D1-2026-10-05-01 |
| GAME-D1-02 / #5034, worn meshes vs loaded slots | VERIFIED, with a gap | `reconcile_worn_gear` runs from `restore` (NPCs) and after the overlay (player, which first drops its stale `PendingGearImport`). Gap: GAME-D1-2026-10-05-01 |
| GAME-D2-01 / #5037 (+#5045), topic ownership | VERIFIED | Category filter, Blocking / Top-Level / LinkOnly branch model, per-NPC INFO pass, and #5224's FO3/FNV top-level bit. Residual: GAME-D2-2026-10-05-01 |
| GAME-D1-03 / #5040, module docs | VERIFIED | Both `player_body.rs:28-35` and `loot_appearance.rs:211-214` now describe the queue → loader path |
| GAME-D2-02 / #5043, dialogue gates | VERIFIED | `player_can_act` gates both entry points. `npc_refuses_dialogue` (dead / combat / unconscious) is shared by the selection, the topic click and the Talk arm's bulk twin `collect_dialogue_refusals` (#5109) |
| GAME-D4-01 / #5046, StartCombat over an ambient fight | VERIFIED | `Effect::StartCombat` removes `AmbientEngagement` (`fragment/effects.rs:1547`) |
| GAME-D7-02 / #5049, `NpcEquipmentPart.inventory_index` | VERIFIED | Both the field and `inventory_index_for` are gone; hide/reveal/release match on `form_id` |
| #4709 kill switch drops combat feedback | VERIFIED, closed | `70d9896fb`: registered outside the `locomotion_enabled` block (`post_update.rs:132-148`) |
| #4710 walk_anim docs | VERIFIED, closed | `f002763b4` |
| #5028 (player gear release), #5061 (shared appearance providers), #5047 (TPLT player template), #5058 (faction reset), #5017 (unconscious), #5054 (hysteresis park), #5056 (cinematic purge) | VERIFIED | #5028 has no reachable producer yet (GAME-D1-2026-10-05-02). The #5058 doc placement is GAME-D1-2026-10-05-03 |

## Cross-audit pointers (not re-reported here)

- **`/audit-ecs` (today)**:
  - D1-01 HIGH, the `footstep_system` lock-order cycle (#5146, `systems/audio.rs`). It is in this audit's Dim 7 paths but is not gameplay-state.
  - D5-02 LOW, the `npc_dialogue_selection` Access row after #5152.
  - D7-01 LOW, `release_entities` for player gear leaves the despawned root in the body's `Children`. That is latent because of GAME-D1-2026-10-05-02.
  - The ECS report also found that the LSCR turntable never turns while the cover is up, because of `dt == 0`.
- **`/audit-performance` (today)**: PERF-D1-01 LOW, #5109's Talk check rebuilds the alias tables every frame.
- **`/audit-concurrency`**: #5069 (open; the `equipment_appearance_system` Access row) and #5066 (open; the `npc_dialogue` guard shadow) are unchanged.
- **`/audit-physics` (today)**: no overlap with GAME-D4-2026-10-05-01. Their seed gate verdict ("sound") concerns insane seeds; the identity-transform seed described here is sane under the gate. #4772 is a candidate relation.
- **`/audit-esm`**: the decode half of GAME-D2-2026-10-05-01 (INFO `QSTI`). ESM-2026-10-05-D2-01 (the `InfoRecord.data_flags` docs) is adjacent.
- **`/audit-save`**: GAME-D7-2026-10-05-01 is a defect in that audit's own guard.

## Known-Open Register (verified today; cite, don't re-file)

| Issue | State at HEAD |
|---|---|
| #4232 `effective_actor_level` returns 0 | Open and unchanged. #5047 now feeds it the Use-Stats terminal (`inventory.rs:378`, `:581`) through the same function |
| #4415 magic runtime (partial) | Open |
| #5031 one-request gear queue | Open and unchanged (above) |
| #4739 / #4743 / #4744 combat audio | Open (owned by audio). #4742 is closed |
| #5095 Skyrim player body has no head | Open (FaceGen; the one slice item still open in `player_body.rs`'s doc) |
| #5065 scripted Enable/Disable on a resident reference | Open (scripting) |
| Reviving an already-ragdolled player has no ragdoll-deactivate path | Unchanged (7aa1d7741) |
| No pickpocket / theft-alarm system | Unchanged. `LootOutcome.stolen` is still forward-latent (`inventory.rs:907`) |
| 10 of ~17 PACK procedures have no runtime; FO4+ walk clips are absent; cross-tile pathing is blocked | Unchanged. The `from_package` / `insert_at_*` / `clear_ambient_behavior` arms have no diff since the baseline |

## Test verification

| Command | Result |
|---|---|
| `cargo test -j8 -p byroredux --bin byroredux -- combat:: inventory:: interaction:: npc_spawn:: reference_state ambient_locomotion walk_anim save_io:: loot_appearance player_body gear notifications loading_screen loading_model spawn_tests registry_completeness scheduler_access npc_dialogue consum restoration timed_ faction_hostility synth_child scripted_lock_gate objectives` (toolchain 1.96.0) | **436 passed, 0 failed, 19 ignored** (installed data) |
| Python census of `FalloutNV.esm`: DIAL `QSTI`/`DATA`, INFO `QSTI`/`CTDA` | 18,215 DIALs; 138 with more than one quest; 23,247 INFOs, all carrying `QSTI`; 28 Topic DIALs whose INFOs span quests (per-topic counts in GAME-D2-2026-10-05-01) |

No `--ignored` tests were run. The Dim 3 data-gated stimpak test guards code that is unchanged. No probe test was built: this run was not allowed to edit source. GAME-D1-2026-10-05-01 rests on three things: the spawn sites that never create a weapon root, the order of `queue_midlife_imports`'s two loops, and `resolve_armor_meshes`' non-armor return. GAME-D4-2026-10-05-01 rests on #4814's own stated hazard and the identical call point.

## Deduplication

- Open issues came from `/tmp/audit/issues.json` (97). Every issue and audit cited above was checked for state with `gh issue view`.
- Searches across all issue states: "corpse ragdoll restore eviction" (nearest match #4817, closed, a different defect), "reconcile_worn_gear weapon" (#5034, closed; this is its gap) and "INFO quest QSTI running" (none).
- Today's sibling reports (ECS, PERFORMANCE, CONCURRENCY, ESM, PHYSICS, CHARACTER, SAFETY, RENDERER, NIF, NIFAL, PARSERS) were grepped for every new finding's symbols. The only overlaps are the cross-references above.

Next step: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-10-05.md`. Suggested labels are `gameplay` plus:
- `inventory` + `save-load` for GAME-D1-2026-10-05-01;
- `combat` + `physics` + `save-load` for GAME-D4-2026-10-05-01;
- `inventory` + `tech-debt` for GAME-D1-2026-10-05-02;
- `doc-rot` for GAME-D1-2026-10-05-03;
- `dialogue` + `quests` + `game:fnv` for GAME-D2-2026-10-05-01 (also FO3; title-specific to the Fallout games);
- `quests` + `ui` for GAME-D6-2026-10-05-01;
- `save-load` + `test-gap` for GAME-D7-2026-10-05-01;
- `tech-debt` for GAME-D7-2026-10-05-02.
