# SCR-2026-10-31-03: `Effect::SetRelationshipRank` (and the VM friend-rank native) raise `CHRR` under the paired quest guards, outside `apply_effect`'s lock inventory

Labels: low,scripting,bug,concurrency

Source: `docs/audits/AUDIT_SCRIPTING_2026-10-31.md` (SCR-2026-10-31-03). Severity LOW (doc and inventory drift; no cycle found). Dimension: Fragment Dispatch & Locks. Introduced by CHRR producer commit `6298eda29`.

**Location**: `crates/scripting/src/fragment/effects/reference_state.rs:62-80`, `crates/scripting/src/fragment/vm/natives/favor.rs:126-129`; inventory in `crates/scripting/src/fragment/effects.rs` (doc block, ~lines 55-125)

## Description
`apply_reference_state_effect` runs inside the `QuestStageState` + `QuestObjectiveState` pair. The M43 change added a call to `relationships::raise_relationship_change`, which reaches `story_manager::raise_story_event`: a `StoryEvent` storage write and, on a same-entity collision, a `PendingScriptStoryEvents` write. It also reads `ScriptedRelationships` / `AuthoredRelationships` via `relationship_rank` for the old rank. None of these is in the doc block inventorying what the effect arms acquire, and the `nested_lock_residual_list` scan does not follow the cross-file hop. `Effect::SendStoryEvent` queues after the guards drop specifically to avoid this nesting.

## Evidence
The dispatcher drops its `StoryEvent` / `PendingScriptStoryEvents` guards before taking any quest lock, and every quest-resource system is `add_exclusive`, so no ABBA cycle was found; the lock-order lane is not red on it.

## Impact
None today. The next change that holds a `StoryEvent` guard across a condition read would form the cycle the doc block's invariant is meant to flag first.

## Suggested Fix
Collect `(actor, other, old, new)` on `DeferredFragmentEffects` and raise after the guards drop, as `jail_sends` and `word_gains` do; or add the four types to the inventory. The deferred form also removes the ordering dependence on the rank flush.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a lock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
