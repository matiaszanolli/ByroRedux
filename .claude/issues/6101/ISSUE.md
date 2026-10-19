# SCR-SD-01: `evaluate_guard_guard_free` nests `QuestStageState` then `QuestObjectiveState` in code order; every other nested site is TypeId-sorted, so the ABBA lane flips red or green with the build configuration

Labels: high,scripting,concurrency,bug

**Severity**: HIGH (ECS deadlock potential floor; latent in production, see Impact) · **Dimension**: Fragment Dispatch & Locks (lane handed over by `AUDIT_CONCURRENCY_2026-10-19.md`, which could not attribute it)
**Location**: `crates/scripting/src/fragment/effects/evaluate.rs:614-617` (`let stages = world.try_resource::<QuestStageState>()?; let objectives = world.try_resource::<QuestObjectiveState>()?;`), against `fragment/effects/deferred.rs:496` and `:1226` and `byroredux/src/commands/quest.rs:221`, all `world.resource_2_mut::<QuestStageState, QuestObjectiveState>()`
**Introduced by**: `70832c690` (2026-10-18, "Run ObScript item, combat, faction, lock and actor-value commands on the effect runtime"). Same pair as closed #3819 family 4, but that was a test-hygiene fix and does not cover this site. Not #6003 (`StoryManagerNodeState`) and not #5986.

## Description
`resource_2_mut` acquires in TypeId order (`world.rs:810`). `evaluate_guard_guard_free` acquires in a fixed code order, Stage then Objective. When `TypeId(QuestObjectiveState) < TypeId(QuestStageState)` in a given binary, the sorted sites record Objective -> Stage and the guard-free site records Stage -> Objective. The process-wide graph (`lock_tracker::record_and_check`) then holds both edges and panics in whichever test closes the cycle. TypeIds are hashes of crate identity, and a crate's identity changes with the unified feature set, so the sorted order, and with it the verdict, differs between `cargo test -p byroredux-scripting` and `cargo test -p byroredux-core -p byroredux-physics -p byroredux-scripting`. That is why it looked transient.

## Evidence (run at HEAD cb5368348)
```
BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux-scripting --lib                       -> 1168 passed
BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux-core -p byroredux-physics -p byroredux-scripting --lib
  obscript_effects::tests::get_is_reference_compares_form_ids --- FAILED
  obscript_effects::tests::stop_combat_with_a_target_stops_only_that_fight --- FAILED
  attempted acquisition of QuestObjectiveState while holding QuestStageState ... cycle:
  QuestObjectiveState -> QuestStageState -> QuestObjectiveState
```
Four runs, same two tests each time. Those two tests are the ones that route a getter through `obscript_effects.rs:1299` (`evaluate_guard_guard_free`). Every other file that names both resources (`obscript_quests.rs:855-880`, `obscript_more/actions.rs:21-40`, `byroredux/src/objectives.rs`, `commands/quest.rs:436-460`) takes one guard at a time, so this is the single unsorted nesting. The failure the concurrency sibling saw "once, not reproduced in 45 reruns" (`AUDIT_CONCURRENCY_2026-10-19.md`, out-of-area observations) is this: deterministic per build configuration.

## Impact
Production deadlock is latent. The ObScript getter runs from exclusive systems (`update.rs:491`, `:523`, `:611`, `:622`) and the sorted sites from exclusive systems and the debug drain, so no second thread holds the opposite order today. The cost is that the CI lock-order lane turns red or green on unrelated dependency or feature changes. That is the lane-blinding precedent of #4603 and the safety sibling's SAFE-2026-10-19-01.

## Related
#3819, #4603, #6003, #6062 (same lane, different pair), #6008.

## Suggested Fix
Take the pair in one call that sorts by TypeId (a read counterpart of `resource_2_mut`), or copy what `evaluate_guard` needs from `QuestStageState` into a snapshot and drop that guard before taking `QuestObjectiveState`. Add a guard test that runs the guard-free getter and a `resource_2_mut` apply in one process under the lane.

Source: docs/audits/AUDIT_SCRIPTING_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
