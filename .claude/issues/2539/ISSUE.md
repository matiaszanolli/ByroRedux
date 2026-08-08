# SCR-D6-NEW6-02: a844c26b's six new quest-lifecycle Effect arms add two more nested resource acquisitions inside the exact hold-scope #2269 already flagged as fragile

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2539
**Finding ID**: SCR-D6-NEW6-02

**Severity**: MEDIUM (rated to match the established sibling finding, `#2269`, which the concurrency audit rated MEDIUM for the identical mechanism against a different resource pair)
**Dimension**: Scripting Runtime Systems
**Untrusted-Input**: No
**Location**: `crates/scripting/src/fragment.rs:1218-1220` (`quest_fragment_dispatch_system`'s `resource_2_mut::<QuestStageState, QuestObjectiveState>()` hold scope, spanning the whole cascade loop), nested acquisitions at `:459` (`QuestDefinitionRegistry`, `StartScene`/`StopScene`), `:750-751`/`:769-771`/`:780-782`/`:849-851`/`:862-863` (`QuestDefinitionRegistry`, the new `SetStage`/`StartQuest`/`StopQuest`/`CompleteAllObjectives`/`FailAllObjectives` arms), and every `crate::scene::mark_scene_actor_bindings_dirty(world)` call inside those same arms (`:473`, `:774`, `:797`, `:804`, `:812` — nested-acquires `SceneActorBindings` write)
**Status**: NEW (all six call sites are new in `a844c26b`; the surrounding hold-scope and nesting *pattern* is pre-existing and already tracked as `#2269`)

## Description
`#2269` (open, concurrency-audit-owned, not re-derived here) documents that `quest_fragment_dispatch_system` holds `(QuestStageState, QuestObjectiveState)` write guards across its entire cascade loop, and that two *pre-existing* `apply_effect` arms nested-acquire `CinematicPresentationState` from inside that scope — a lock order a *different* `add_exclusive` system acquires in reverse. `#2269`'s own "Completeness Checks" section lists an explicitly **unchecked** action item: "SIBLING: Other `apply_effect` arms checked for the same nested-resource-acquisition pattern." `a844c26b`'s six new lifecycle-effect arms are exactly that unchecked sibling check, materialized as new code: `StartScene`/`StopScene`'s ambiguous-property resolution path and `SetStage`/`StartQuest`/`StopQuest`/`CompleteAllObjectives`/`FailAllObjectives` all nested-acquire `QuestDefinitionRegistry` (read) from inside the same `stages`+`objectives`-held scope, and several additionally nested-acquire `SceneActorBindings` (write, via `mark_scene_actor_bindings_dirty`). Traced every other acquisition site of both resources looking for a live reverse-order caller: **none found** — both of `QuestDefinitionRegistry`'s write sites take `&mut World` (load-time-only), and `SceneActorBindings`'s consumer (`refresh_scene_actor_bindings`) drops its own `QuestStageState` borrow before touching `SceneActorBindings`. Consistent with `#2269`'s own stated risk profile ("no live deadlock today... becomes a real cross-thread ABBA risk the moment either system is promoted to the parallel lane"), not an escalation beyond it.

## Evidence
`apply_effect`'s and `apply_quest_scoped_effect`'s signatures changed this exact commit specifically to add a `world: &World` parameter enabling these nested lookups. Confirmed directly: `fragment.rs:1218-1220` shows `let (mut stages, mut objectives) = world.resource_2_mut::<QuestStageState, QuestObjectiveState>();` at the top of the cascade-loop scope.

## Impact
None live today (same "exclusive-scheduling-only" caveat `#2269` already states) — but the surface `#2269`'s eventual fix needs to sweep is now larger: two more resources (`QuestDefinitionRegistry`, `SceneActorBindings`) join `CinematicPresentationState` as things nested inside `quest_fragment_dispatch_system`'s hold scope, landing in the same fast-growing function on the same day as the original finding.

## Related
`#2269` (open, concurrency-audit-owned) — this finding directly answers that issue's own open "SIBLING" completeness checkbox with concrete new instances. Recommend appending this evidence to `#2269` rather than tracking as a separate issue — filed here per this session's convention of filing every NEW-status finding, but flagged for potential merge into #2269 by whoever picks it up.

## Suggested Fix
Same fix shape `#2269` already proposes — resolve `QuestDefinitionRegistry`-derived values and the `SceneActorBindings`-dirty signal without a nested acquisition while `QuestStageState`/`QuestObjectiveState` are held (clone/snapshot `QuestDefinitionRegistry` the same way `QuestStageFragments` is already cloned before the hold scope begins), and queue the dirty-bindings signal as a post-loop batch flush.

## Completeness Checks
- [ ] **LOCK_ORDER**: Fix sweeps `QuestDefinitionRegistry` and `SceneActorBindings` together with `CinematicPresentationState`, not just this cycle's new arms
- [ ] **SIBLING**: Consider merging into #2269 rather than tracking separately, since both describe the same underlying hold-scope pattern
