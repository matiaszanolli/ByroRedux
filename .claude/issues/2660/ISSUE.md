# #2660: SCR-D6-NEW11-03: #2539's lock isolation is partial -- the hold scope still nests 6 resource acquisitions (3 writes) and 12 component acquisitions

**Severity**: MEDIUM
**Dimension**: Scripting Runtime Systems (Dimension 6)
**Untrusted-Input**: No
**Location**: `crates/scripting/src/fragment.rs` -- the `(QuestStageState, QuestObjectiveState)` `resource_2_mut` hold scope spanning the cascade loop; residual `SceneActorBindings` read via `resolve_object` (`:246-248`); three `PlayerControlState` writes; 12 component acquisitions incl. `Inventory` (`AddItem`) and `GlobalTransform`+`Transform` (`MoveTo`)
**Status**: NEW (the two resources #2539 named are correctly fixed; these are the residual)

## Description

`6ad64ef6` did exactly what it scoped: `QuestDefinitionRegistry` is snapshot-cloned before the guards (`:337-341`) and every former in-scope `try_resource::<QuestDefinitionRegistry>()` now reads `deferred.quest_definitions` (`:544`, `:837`, `:849`, `:864`, `:929`, `:943`); every in-scope `mark_scene_actor_bindings_dirty(world)` became `deferred.scene_actor_bindings_dirty = true` (`:549`, `:564`, `:837`, `:852`, `:877`, `:884`, `:892`).

But `SceneActorBindings` is still **read**-acquired inside the scope via `resolve_object` (`:246-248`), so the `QuestStageState -> SceneActorBindings` nesting is only half-eliminated -- and five other resources plus twelve component locks remain nested inside the same hold scope.

The issue was closed as though the isolation were complete; it is not.

## Evidence

Full enumeration performed this pass. Residual nested acquisitions inside the `(QuestStageState, QuestObjectiveState)` scope: `SceneActorBindings` (read, via `resolve_object`), `PlayerControlState` (3 writes), and 3 further resources; plus 12 component acquisitions, including `Inventory` for the `AddItem` arm and `GlobalTransform` + `Transform` for the `MoveTo` arm.

No live reverse-order acquirer exists for any residual resource -- all scripting systems are registered with `add_exclusive` (`byroredux/src/boot.rs:747-846`), so nothing runs concurrently today. This matches #2269's own stated risk profile: "no live deadlock today ... becomes a real cross-thread ABBA risk the moment either system is promoted to the parallel lane".

## Impact

None live today. The cost is that the surface any future parallelization must sweep is materially larger than #2539's closure implies -- a maintainer reading that issue would reasonably conclude the hold scope is clean.

This is the same fast-growing function that absorbed #2269, #2539, and SCR-D6-NEW11-01/-02 in a single window, so the nesting is still actively accumulating.

## Related

#2539 (closed), #2269 (closed), #2270 (open -- the undocumented "snapshot before iterate" house rule this should be recorded under)

## Suggested Fix

Resolve alias lookups before the guards are taken -- the `resolve_object` results the loop needs are knowable from the queue up front -- and record the residual nesting in the house-rule documentation #2270 asks for, so the next arm added to `apply_effect` does not silently extend it again.

A cheap structural guard: assert in a test that `apply_effect` takes no `&World` resource handle that is not already on the deferred struct.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
