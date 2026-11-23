# #7362 — CONC-2026-11-23-03

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-11-23.md` (finding CON-03).

- **Severity**: LOW
- **Dimension**: 4 (exclusive declarations, `sys.accesses` accuracy)
- **Location**: `byroredux/src/boot/schedule/update/combat/faction_hostility.rs:15-37`; `byroredux/src/boot/schedule/late/scripted_combat.rs:11-34` (`scripted_experience_system`, `scripted_skill_system`) and `:130-160` (`reconcile_pending_dead_actors_system`); scans live in `byroredux/src/boot/schedule/mod.rs:1151-1380`
- **Status**: NEW (closed #4574 / #3951 / #5069 were the same shape on other rows)
- **Description**: 16 exclusives have a completeness scan. Counting `add_exclusive_with_access` rows gives 57 rows, so 41 have a row and no scan: pool_regen_tick, bleedout, reconcile_pending_dead_actors, scripted_experience / scripted_skill / scripted_kill, evidence_deposit, jail_bed, guard_rush, encumbrance, the 13 `extension_*` sync / dispatch systems, player_body_facing, cinematic_animation_event, faction_hostility, water_interaction, npc_dialogue_selection, player_derived_stats, player_identity_sync, restoration, submersion, vats_mode, water_audio, water_damage, footstep, impact_audio, billboard, world_bound, particle, weather. The skill names only `bed_use_system` (a bare `add_exclusive`, no row at all) and `jail_bed_system`. Running an equivalent scan on both trees (`scan.py`, same-file calls followed, turbofish forms only) shows these rows under-declare same-file acquisitions:
  - `scripted_experience_system` and `scripted_skill_system`: `UiLabels` read, **new since the baseline** (f452ec0e4, #6825: `grant_experience` resolves `sStatsXP`, `combat.rs:1062`). `scripted_experience_system` also misses `ActorValueNames` (older).
  - `reconcile_pending_dead_actors_system`: `AiCombatState`, `AmbientPackageRuntime`, `EvaluatePackageRequest` removed by `reconcile_dead_actor` (`combat.rs:1605-1616`), `RagdollActive` read (older).
  - `make_faction_hostility_system`: `AmbushBehavior`, `LoadedCellIndex` (`ambush_gates`, `faction_hostility.rs:375-380`) and `IgnoreCrime` (`:538`) (older).
  The cross-file tails (`notifications::push`, `music::raise_sting`, `raise_level_story_event` behind `grant_experience`) are undeclared on the two XP rows as well; the scanner does not follow them.
- **Evidence**: `scripted_experience_system` row: `PendingPlayerXp`, `XpModifierRules`, `PlayerEntity`, `CharacterRuleset`, `CharacterLevel`, `LevelUpPoints`, `ActorValues` only; body reaches `UiLabels` through `grant_experience`.
- **Trigger Conditions**: none (exclusives never pair; the project rule is that an undeclared exclusive is by design). The rows are the operator view (`sys.accesses`) and the comparison basis for promoting a system to a parallel lane; a wrong row makes that promotion look safe.
- **Impact**: report accuracy and promotion safety only. No deadlock.
- **Verification Path**: re-run `scan.py` (or a `*_declares_everything_it_acquires` test over `grant_experience` for the two XP rows, which needs `COMBAT_SRC` entries `scripted_experience_system`, `scripted_skill_system`); today it fails on `UiLabels=read`.
- **Related**: #7319 (added six scans, left these), #7345.
- **Suggested Fix**: either add the scans for the rows that gained acquisitions this window (the two XP rows, `reconcile_pending_dead_actors_system`, `faction_hostility`), or drop the rows that cannot be kept honest; and correct the skill sentence ("`bed_use_system` and `jail_bed_system` stay bare") to the real count.
- **Dimension agent ID**: CON-D4-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other record-time latches, other temp stems, other exclusive rows)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
