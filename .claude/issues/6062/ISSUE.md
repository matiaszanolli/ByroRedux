# #6062 — SAFE-2026-10-19-01 (snapshot as filed)

Labels: high,safety,concurrency,ecs,bug
Report: docs/audits/AUDIT_SAFETY_2026-10-19.md

**Severity**: HIGH — `_audit-severity` Special Rules: "ECS deadlock potential → HIGH minimum". **Latent at runtime today**: both systems are registered `add_exclusive`, so no production schedule runs either beside another system.
**Dimension**: Dim 3 adjacent (owner for the fix: `/audit-concurrency`, lock ordering)
**Source**: `docs/audits/AUDIT_SAFETY_2026-10-19.md` (SAFE-2026-10-19-01)

## Location
- `byroredux/src/systems/head_look.rs:36-49` (`find_head`): `world.query::<Name>()` is held for the whole traversal, then `world.get::<Children>(entity)` is taken inside the loop.
- `byroredux/src/systems/magic_shader.rs:23-35` (`mesh_entities`), called from `magic_shader_tint_system` (`:62-66`) **inside** `let Some(materials) = world.query::<Material>()`: `mesh_entities` takes `Children` and a second same-thread read on `Material` (`world.has::<Material>`) while the outer `Material` read guard is live.

Both from `d1b298921` (2026-10-18, "Implement new ObScript features and state management"). #6003 is a different pair (`StoryManagerNodeState <-> QuestStageState`).

## Description
The tracker (`crates/core/src/ecs/lock_tracker.rs`, `global_order::record_and_check`) records process-wide acquisition orders and aborts when a new edge closes a cycle. The established orders in this binary are `Children → Name` and `Children → Material` (e.g. `byroredux/src/anim_convert.rs:40-41` takes `children_q` before `name_q`). The two new systems take the pairs the other way round (`Name → Children`, `Material → Children`), so the graph holds both directions and the detector panics the owning test.

## Evidence
CI job 118240323405 (run 39305200056) and a local reproduction:
```
BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux
test systems::head_look::tests::the_head_turns_toward_the_target_within_the_necks_range ... FAILED
test systems::magic_shader::tests::a_playing_shader_lights_the_meshes_and_gives_them_back ... FAILED
ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of `…hierarchy::Children` while holding `…name::Name` … `Children → Name → Children`
test result: FAILED. 3033 passed; 2 failed; 63 ignored
```
Registration: `boot/schedule/update.rs:620` (`magic_shader_tint_system`) and `:841` (`head_look_system`), both `add_exclusive(Stage::Update, …)`.

## Impact
1. The ABBA lane is red on a known cycle — exactly what masked the FormIdPool and #4546 cycles for 18 days (#4603); a *real* new cycle in a parallel system would be hidden.
2. Moving either system into the parallel batch later reintroduces a true ABBA (structural writer on `Children`/`Name` vs these readers).
3. `magic_shader_tint_system` re-acquires `Material` read while its own `materials` guard is live — a same-thread recursive read that `RwLock` documents as able to deadlock when a writer queues between the two reads.

## Related
#313 / #2675 (tracker design), #4603 (lane-blinding precedent), #5535 (sixth red, fixed), #6003 (separate open cycle).

## Suggested Fix
In `find_head`, read each node's `Name` through a transient `world.get::<Name>(entity)` (or collect the subtree first with `byroredux_core::ecs::collect_subtree`, then take the `Name` query). In `magic_shader_tint_system`, call `mesh_entities(world, owner)` **before** taking `world.query::<Material>()`, then look materials up in a second pass; `mesh_entities` must hold no guard beyond its own loop iteration.

## Completeness Checks
- [ ] **SIBLING**: Other systems added in `d1b298921` checked for the same inverted `Children` order
- [ ] **LOCK_ORDER**: TypeId-sorted / established `Children → Name/Material` order preserved
- [ ] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` is green (the two failing tests pass)
