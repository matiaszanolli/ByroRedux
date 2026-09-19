---
description: "Deep audit of the gameplay layer in byroredux/src — inventory/equipment, interaction/locks/loot persistence, consumables, combat + death, NPC spawn → AI packages → locomotion, player feedback, gameplay-state save coverage and stage order"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Gameplay Audit (P2/P3 slice, M41–M42)

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

Audits what the player and NPCs *do* once content loads. Elsewhere: CHARAL formulas → `/audit-character`; save schema/atomicity → `/audit-save` (this skill owns *what should be restored*); lock order / declared access → `/audit-ecs` + `/audit-concurrency`; Scaleform/MenuXml rendering → `/audit-ui`; KCC/ragdoll → `/audit-physics`; ALCH/MGEF/LVLI/PACK decoders → `/audit-esm`; Papyrus/ObScript → `/audit-scripting`.

**Architecture**: one Task agent per dimension (max 3). Delta-first: skip a dimension whose Paths are unchanged since the last `docs/audits/AUDIT_GAMEPLAY_*.md` and whose Guard is green (unless `--depth deep`).

## Ground truth (docs lag; re-verify)
`docs/engine/npc-spawn-ai-packages.md`, `docs/engine/packal.md`, `docs/engine/playable-vertical-slice.md`, `docs/feature-matrix.md`. Smoke gates (Vulkan + data): `docs/smoke-tests/{p0-door-interaction,p2-melee-core,p5-save-restart}.sh`. Never launch a second engine beside the user's.

**Known-open (verified 2026-09-19; cite, don't re-file)**: #4414 `FactionRelations` written, never read — combat starts only from script `StartCombat`. #4232 `effective_actor_level` returns 0 verbatim, emptying leveled lists. #4248 is stale-open (`attach_container_inventory` now calls `expand_leveled_loot`). Ten of ~17 PACK procedures have no runtime, FO4+ walk clips are absent, cross-tile pathing is blocked (`docs/engine/npc-spawn-ai-packages.md`, `docs/engine/navmesh-pathfinding.md` §9): a finding is a *silent misroute* of such a case, not the absence.

**Extra field**: **Trigger** (game, cell, save state).

## Phase 1: Setup
1. `mkdir -p /tmp/audit/gameplay`; save `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels` to `/tmp/audit/gameplay/issues.json` and dedup every finding against it (`_audit-common.md`); read the newest `docs/audits/AUDIT_GAMEPLAY_*.md`.
2. Baseline `cargo test -p byroredux -- combat:: inventory:: interaction:: npc_spawn:: reference_state ambient_locomotion walk_anim save_io::`.

## Phase 2: Dimensions

### Dimension 1: Inventory & Equipment Model
**Paths**: `byroredux/src/inventory.rs`, `crates/core/src/ecs/components/inventory.rs`, `crates/core/src/ecs/resources/mod.rs` (`ItemInstancePool`), `crates/plugin/src/equip.rs`
**First step**: `git log --since=<last> --format='%h %cs %s' -- byroredux/src/inventory.rs crates/core/src/ecs/components/inventory.rs`
**Guard**: `inventory::tests::{armor_toggles_never_touch_the_equipped_weapon, load_reconciler_*}`, core `no_armor_mask_can_reach_the_weapon_slot`.
- Canonical state is `Inventory` + `EquipmentSlots` + `EquippedWeapon` (core). `InventoryCatalog` and the two player templates are read-only, rebuilt by `install_catalog` per content load; a mutable inventory copy in the binary is a boundary violation.
- Rows are zeroed in place, never removed (equipment and `EquippedWeapon.inventory_index` point into `items`): check consume, `transfer_loot`, `pickup_loot`, `reference_state::restore`. Slot mutations re-derive `EquippedWeapon` (`reconcile_equipped_weapon`); armor toggles must not.
- `ItemInstance` has no fields (`_reserved`); production allocates only in `restore`. Reject claims that per-instance condition/mods/charge exist; `Some(instance)` paths must still be atomic.

### Dimension 2: Interaction, Locks, Containers & Loot Persistence
**Paths**: `byroredux/src/interaction.rs` (action half), `byroredux/src/cell_loader/{reference_state,unload}.rs`, `cell_loader/references/{attach,synth_child}.rs`, `byroredux/src/npc_spawn/loot_appearance.rs`, `crates/core/src/ecs/components/lock.rs`, `crates/plugin/src/equip.rs`
**First step**: `cargo test -p byroredux -- reference_state scripted_lock_gate synth_child interaction::`
**Guard**: `reference_state::tests` (4), `synth_child_tests::actor_placement_stamp_restores_only_the_looted_instance_of_a_shared_base`, `scripted_lock_gate_tests`, `interaction::tests::key_gate_rejects_missing_zero_count_wrong_and_keyless_locks`, `loot_appearance::tests`.
- Pipeline: `interaction_system` (ahead of every OnActivate consumer) → `ActivateEvent` (read, not drained) → `container_loot_system` → `transfer_loot` (validates source/lock/both inventories *before* mutating; whole stacks; source equipment cleared, one unequip event each) or `pickup_loot`.
- Locks: key unlock happens on activation, not prompt, and is recorded in `ReferenceLockState` (saved). `carried_unlock_key` reads `PapyrusPlayerEntity`; loot/combat read `PlayerEntity` (both set in `scene.rs`; the fly-cam placeholder has no `Inventory`) — can they diverge? `lock_level` has no consumer.
- Persistence: `capture` runs before unload frees instances; `restore` at attach consumes the row; `without_parked_state` shields a save reload; parked rows own their payloads; identity is the REFR `FormIdPair`, not the base. Every new placement fact (lock override, `Dead`, `picked_up`, inventory, equipment, AVs) needs a row field or ledger, else eviction loses it. No serde defaults on `ReferenceState` (#4465): schema change ⇒ `FORMAT_MAJOR` bump (`/audit-save`).
- Leveled loot: containers use `expand_leveled_loot` (LVLO counts multiplied, Use-All `0x04`, non-item leaves kept); NPC inventories (`npc_spawn.rs`) use equip-oriented `expand_leveled_form_id`, which ignores LVLO counts and drops non-item leaves (caravan cards/money). Verify one LVLI yields the same loot on a corpse and a container (candidate divergence).
- `NpcLootAppearance` hides originals only once the replacement is fully staged; never strips a living actor.

### Dimension 3: Consumables & Timed Effects
**Paths**: `byroredux/src/inventory.rs` (`consume_item`), `byroredux/src/systems/restoration.rs`, `crates/core/src/ecs/components/restoration.rs`, `crates/core/src/ecs/resources/hardcore.rs`, `crates/plugin/src/consumables.rs`
**First step**: `cargo test -p byroredux -- consum restoration timed_`; `cargo test -p byroredux-plugin consumables`
**Guard**: `unavailable_consumption_never_changes_health_or_count`, `consuming_invalid_instance_is_atomic`, `save_io/consumable_tests.rs`. The Hardcore/perk test `real_stimpaks_restore_scaled_health_and_limbs_but_hardcore_only_health` is `#[ignore]`d (FO3+NV data): it guards nothing unless run `--ignored`.
- Chain: ALCH EFID/EFIT/CTDA → `ConsumableEffect` (Skyrim, FO3/NV only) → `restoration_plan` → `InventoryCatalog.restorations` → `consume_item` → instant `restore` or `TimedRestorations` (constant rate, simulation seconds, no overheal banking) → `restoration_system` (clears on `Dead`/health ≤ 0).
- `restoration_plan` is all-or-nothing: any unsupported effect, condition, area, delivery, script or AV ⇒ `None`, never a partial potion. Conditions fail-CLOSED (`GetHasPerk` 448/449; `IsHardcore` 586 on the NV profile only); package CTDA is fail-OPEN (Dim 5). The asymmetry is deliberate; a flip is a finding.
- Hardcore is an effect CTDA, not a branch in `consume_item`; `HardcoreMode` is a saved flag only. Conditions and Medicine scaling are evaluated at use and the resolved rate stored, so later skill changes do not rewrite a dose.
- AV map in `restoration()` (Skyrim 24/25/26, FO3/NV 16/12) vs `docs/engine/charal-*-ruleset.md`; Oblivion/FO4+ Aid items are "effects unavailable" by design. `Unavailable` only reaches `log::warn!` (`main.rs`): #4458 (player lacked AVs) looked like a dead button.

### Dimension 4: Combat & Death Pipeline
**Paths**: `byroredux/src/combat.rs`, `byroredux/src/systems/combat_ai.rs`, `crates/scripting/src/combat.rs`, death producers in `byroredux/src/systems/{water,character}.rs`
**First step**: `grep -rn 'Dead)' byroredux/src --include='*.rs' | grep -v test` — every production `Dead` insert must reach `reconcile_dead_actor` (in an exclusive, or `queue_dead_actor_reconciliation` → Late `reconcile_pending_dead_actors_system`). Today: `combat_damage_system`, water/drowning, `reference_state::restore`.
**Guard**: `combat::tests::{consumer_applies_the_producers_damage_rather_than_recomputing_it, dead_state_reconciliation_removes_respawned_ai}`, `combat_ai::tests::{physics_world_is_taken_only_between_the_storage_passes, a_same_frame_hit_on_the_target_survives_and_the_strike_lands_next_frame}`.
- One damage path: `combat_input_system` and `npc_combat_ai_system` emit `HitEvent`; only `combat_damage_system` writes Health/`Dead`. `HitEvent` cleanup is Late `event_cleanup_system` — not a leak.
- `reconcile_dead_actor` teardown = `clear_ambient_behavior` set + `AmbientPackageRuntime` + `EvaluatePackageRequest` + `AnimationPlayer`/`AnimationStack` (actor and skeleton root) + ragdoll; it must grow with each new per-actor runtime component (Dim 5).
- `attack_damage`: weapon + CHARAL melee bonus; `CreatureAttack` only when unarmed; else `UNARMED_DAMAGE` (formulas → `/audit-character`). `MeleeState` is per combatant; `PhysicsWorld` is taken only between storage passes (#4325).

### Dimension 5: NPC Spawn → AI Package Selection → Locomotion
**Paths**: `byroredux/src/npc_spawn.rs`, `npc_spawn/{ai_package,resumable}.rs`, `byroredux/src/systems/{sandbox,wander,travel,follow,escort,guard,patrol,walk_anim,locomotion}.rs`, `crates/core/src/ecs/components/{sandbox,furniture,wander,travel,follow,escort,guard,patrol}.rs`, `boot/schedule/post_update.rs`, `crates/plugin/src/esm/records/misc/pack.rs` (`active_package`)
**First step**: `cargo test -p byroredux -- ambient_locomotion npc_spawn::ai_package walk_anim locomotion::`
**Guard**: `boot/schedule/mod.rs::ambient_locomotion_default_on_tests::{locomotion_registers_behind_the_single_kill_switch, walk_animation_registers_after_all_six_movers}` (live source-shape) pin registration and order, not procedure semantics. `ai_package::tests::{same_winner_preserves_runtime_state, unknown_condition_function_remains_fail_open}`, `walk_anim::tests`.
- Selection: `apply_ai_package_behavior` at spawn (TPLT `Use AI Packages` chain, #4093), then `ambient_ai_package_system` (Update) at most once per in-game minute or on `EvaluatePackageRequest`, swapping only when the winning PACK FormID changes; QUST alias packages override the base stack; an empty registry at spawn must not erase the winner. `active_package` = first by priority whose PSDT covers `GameTimeRes.hour` with passing CTDA (fail-OPEN in `package_conditions_pass`: flag a flip, not the policy).
- One behavior per actor by construction (`AmbientBehavior`), but nothing ties its `insert_at_spawn`/`insert_at_runtime` arms, `from_package`'s `is_*` chain, `clear_ambient_behavior`'s removals (incl. `WalkStuckTimer`/`NavPath`) and `crates/debug-server/src/registration.rs`: diff all four per procedure. Skyrim+ tree packages resolve only Sandbox + Patrol leaves (`.find()`, no per-leaf CTDA; `docs/engine/packal.md`).
- Follow / Escort-collect re-read the target per tick; Travel / Escort-lead / Guard freeze once; Patrol reuses Wander's `step_oscillating_wander`; `prune_seat_reservations` keeps furniture and claimant liveness.
- Locomotion: speed is `WalkSpeed` (clip stride, clamp [30,250]) else `LOCOMOTION_WALK_SPEED`, combat chase included; `advance_stuck_repick` (2.5 s) serves only Wander/Patrol — Travel/Follow/Escort/Guard drop `blocked`. `BYRO_NO_AI_LOCOMOTION=1` is the only switch; the old per-procedure vars must stay unread.
- `walk_anim`: take (0.1 s) / restore (0.4 s) / yield / abandon (seated, dead, cinematic); its module doc still says fixed 100 u/s (stale since M42.11). Both spawn paths in `resumable.rs` stamp `WalkAnimation` + `WalkSpeed`.

### Dimension 6: Player Feedback & Flow
**Paths**: `byroredux/src/{notifications,loading_screen,hud}.rs`, `byroredux/src/commands/{gameplay,hud}.rs`, `byroredux/src/scene.rs` (`plan_character_spawn`), `byroredux/src/scene/spawn_tests.rs`
**First step**: `cargo test -p byroredux -- notifications loading_screen spawn_tests`
**Guard**: `notifications::tests::feedback_is_bounded_ordered_and_consumed_once`, `loading_screen::tests::selector_never_widens_restricted_or_malformed_screens`, `spawn_tests::door_threshold_floor_does_not_certify_an_overlapping_capsule`.
- `PlayerNotifications`: 8-deep, `push` no-ops without the resource, drained once per frame in `app_frame.rs` into the egui overlay only — check that a `--hud` run without debug-ui still drains and every producer (`interaction`, `inventory`, `save_io`) reaches it.
- `hud::fraction` takes the first `ActorValues` holder in iteration order, not `PlayerEntity`: verify it cannot show an NPC's bars. Rendering → `/audit-ui`.
- Loading screen: `Phase` × `Owner` never strands a cover; artwork is Oblivion/FO3NV-only.
- Spawn: explicit camera column > nearest door > fallback, with `capsule_overlaps_solid` clearance; interiors spawn at the first door's own placement (*interior_spawn_point_fix*). `SetInChargen` save refusal → `/audit-save`; `settings_io.rs` is a shim over `crates/settings-io`.

### Dimension 7: Gameplay-State Coverage, Stage Order & Determinism
**Paths**: `byroredux/src/save_io.rs` (registrations), `byroredux/src/save_io/{registry_completeness,round_trip}_tests.rs`, `byroredux/src/boot/schedule/{update,post_update,late}.rs`
**First step**: `grep -nE 'Combat|Melee|Walk|Ambient|Seat|Loot|Picked|Notif|Interaction|Faction' byroredux/src/save_io/registry_completeness_tests.rs`
**Guard**: `registry_completeness_tests::every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` and `round_trip_tests::npc_spawn_stamped_components_are_saved_or_intentionally_rederived` prove a *name* is classified, not that its stated rebuild path exists. `scheduler_access_tests::p2_gameplay_exclusives_declare_non_empty_access` covers three of the gameplay exclusives.
- Prove the claimed re-derivation for each gameplay allowlist row (`AmbientPackageRuntime` first tick, `WalkAnimation` from the spawn lookup, `PickedUp` re-stamped by `restore`). Genuinely unsaved: `AiCombatState` (session `EntityId` target), `FactionRelations` — a mid-fight save loses the fight. Saved: `TimedRestorations`, `HardcoreMode`, `PersistentReferenceStates`, `ReferenceLockState`.
- The load overlay is additive-only: a runtime *removal* that follows from a saved fact must be re-derived after load (models: `reconcile_dead_actor_runtime_state`, `reconcile_player_equipped_weapon`).
- Update order is comment-enforced, unpinned: `restoration_system` → `interaction_system` → `container_loot_system` → `combat_input_system` → `npc_combat_ai_system` → `combat_damage_system`; `ambient_ai_package_system` before `scene_package_system`; Late `water_damage_system` → `reconcile_pending_dead_actors_system` → `event_cleanup_system`. Verify each consumer sees its producer the same frame; `inventory::apply_action` runs between frames with `&mut World`.
- Determinism: `grep -nE 'rand::|Instant::now|SystemTime'` over `combat.rs`, `inventory.rs`, `interaction.rs`, `ai_package.rs` and the Dim 5 systems must be empty (`Instant` in `resumable.rs`/`unload.rs` is budget/telemetry timing).
- Written-never-read / tested-but-unwired is the repeated defect class: each gameplay resource needs a production reader (`FactionRelations`), each `pub(crate)` entry a non-test caller (#4464 closed: `transfer_loot` had none while six tests passed); justify each `#[allow(dead_code)]`.

## Phase 3: Merge
Write `docs/audits/AUDIT_GAMEPLAY_<TODAY>.md`: Executive Summary (severity counts; games/cells exercised); **Invariant Matrix** (single damage path · death reconciled once · index stability · fail-closed consumables vs fail-open packages · stage order · state saved-or-rederived — verified/drifted); Findings (deduplicated; cross-audit dedup per the split above); Known-Open Register.

## Phase 4: Cleanup
`rm -rf /tmp/audit/gameplay`; tell the user the report is ready and suggest `/audit-publish docs/audits/AUDIT_GAMEPLAY_<TODAY>.md` (labels `gameplay` + `ai`/`combat`/`inventory`/`quests`; `game:*` only when title-specific).
