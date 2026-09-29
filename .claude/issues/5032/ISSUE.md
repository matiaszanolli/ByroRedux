# #5032 — ECS-2026-09-29-D1-02: Two test bodies hold a guard across a second acquisition and supply the closing back edge of both lock-order cycles

**Labels**: low, bug, ecs, concurrency, dialogue

**Source**: `docs/audits/AUDIT_ECS_2026-09-29.md` — finding `ECS-2026-09-29-D1-02`

**Severity**: LOW (test hygiene). It must still be fixed for the lane to go green if the production nests stay.

**Dimension**: 1 — Lock Ordering

**Location**:
- `byroredux/src/save_io/round_trip_tests.rs:1386-1401` (`cinematic_trio_survives_save_load_round_trip`): `restored_actor`, a `ComponentRef<ActorCinematicState>`, is alive across `dst.get::<HorseTetherState>(cart)`. The test dates from 2026-08.
- `crates/scripting/src/scene/quest_alias_tests.rs:249-257` (`quest_alias_collection_fill_type_still_declines_not_binds`): `bindings`, a `ResourceRead<SceneActorBindings>`, is alive across `quest_alias_diagnostics(&world, …)`, which takes `SceneQuestAliasRegistry`.

**Status in report**: NEW. The same class as #3304, #3312 and #4984.

## Description

Each test records the reverse of a production order. Neither red is a production bug on its own, but each supplies the closing edge of a D1-01 cycle.

## Evidence

Running just the two scripting tests, `--test-threads=1 quest_alias_collection_fill_type_still_declines_not_binds running_quests_binding_entity_reports_only_running_bound_quests`, gives 1 FAILED. Either test alone passes.

## Impact

1 panic outright (scripting), and a co-cause of the 5 binary panics.

## Related

D1-01, #4984.

Dialogue-landing cluster (`ab31cfefe` / `766e1746e`), filed as distinct issues: ECS-2026-09-29-D1-01 (#5025), ECS-2026-09-29-D5-01 (#5035), ECS-2026-09-29-D7-01 (#5038), SCR-D3-2026-09-29-01 (#5041), LC-D3-01 (#5045), ESM-2026-09-29-D2-03 (#5048). The per-frame Talk-candidate scan on the same code (ECS-2026-09-29-D6-01 = PERF-D1-2026-09-29-01) is tracked under open #3475.

## Suggested Fix

`drop(restored_actor)` / `drop(bindings)` before the second acquisition, or copy the asserted fields out first.

Validated at HEAD 9fcfdc3fc: `cinematic_trio_survives_save_load_round_trip` still holds `restored_actor` (`ComponentRef<ActorCinematicState>`) across `dst.get::<HorseTetherState>(cart)`; `quest_alias_collection_fill_type_still_declines_not_binds` still holds `bindings` (`ResourceRead<SceneActorBindings>`) across `quest_alias_diagnostics(&world, …)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other tests that keep a `ComponentRef`/`ResourceRead` alive across a second acquisition)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: the scripting pair passes together under `BYRO_LOCK_ORDER_CHECK=1 --test-threads=1`
