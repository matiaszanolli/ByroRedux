# #4334 SCR-2026-09-14-FOLLOWUP: onlyOnce actor-base stage triggers re-arm after a cell reload (no persistent Papyrus script state)

**Labels**: medium,scripting,quests,bug,game:skyrim
**Source**: split from #4326 (commit 358999c40)

**Severity**: MEDIUM
**Dimension**: Scripting Runtime Systems
**Split out of**: #4326

## Description

Skyrim SE's reusable actor-base stage trigger, `defaultSetStageTRIGSpecificActor`, has two "fire once" options, and decompiling the shipped `scripts\defaultsetstagetrigspecificactor.pex` (Skyrim - Misc.bsa) shows they do different things:

- `disableWhenDone` → `self.Disable(false)`
- `onlyOnce` → `self.GotoState("hasBeenTriggered")`, where `hasBeenTriggered` is an empty state; the reference stays enabled.

#4326 now persists the `disableWhenDone` half through `ReferenceEnableState` (`QuestAdvanceOnActivate::disable_reference_after_advance`), so the cell loader's spawn gate keeps such a trigger inert after a reload.

The `onlyOnce` half is still enforced only in-session: `quest_advance_system` removes the live `QuestAdvanceOnActivate`, but on the next cell load (or save load) the attach chain decompiles the script again and inserts a freshly armed component. The trigger can then fire `SetStage` a second time — `QuestStageState::set_stage` overwrites `current_stage`, so the stage can move backward, and the stage's fragment is re-queued.

Recording `onlyOnce` in `ReferenceEnableState` would be wrong: vanilla never disables the reference, so condition checks and a later `Enable()` would diverge.

## Evidence

- `crates/scripting/src/translate/recognizers/quest_stage_gate.rs` `recognize_specific_actor_trigger`: `disable_after_advance = disableWhenDone || onlyOnce`, `disable_reference_after_advance = disableWhenDone`.
- `crates/scripting/src/papyrus_demo/quest_advance.rs` `quest_advance_system`: component removal for both flags; ledger write for `disable_reference_after_advance` only.
- No persistent store of per-reference Papyrus script state exists (VM state lives in `crates/scripting/src/vm_state.rs` for the two-state activator only).

## Impact

After any cell revisit or save load, an `onlyOnce` actor-base trigger whose actor re-enters fires its `SetStage` again, re-running non-idempotent stage fragments (`AddItem`, `MoveTo`). How many vanilla placements set `onlyOnce` without `disableWhenDone` is unmeasured.

## Suggested Fix

Persist a FormID-keyed "script state" for references whose attached script leaves its auto state (starting with `GotoState("hasBeenTriggered")` here), consulted by the attach path so the recognizer's component is not re-inserted for a reference already in its terminal state. The same ledger would serve other `GotoState`-terminal once-only scripts.

## Completeness Checks
- [ ] **SIBLING**: Other recognizers whose script parks itself via `GotoState` (e.g. `two_state_activator`) use the same persistence
- [ ] **TESTS**: A regression test fires an `onlyOnce` trigger, reloads the attach path, and asserts it does not advance again
