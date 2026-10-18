# SCR-D1-2026-10-17-01: Set-stage trigger recognizers default absent `doOnce` / `disableWhenDone` / `onlyOnce` to false; the shipped scripts default them to true

Issue: #5969
Labels: high, scripting, bug, quests

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-01: Set-stage trigger recognizers default absent `doOnce` / `disableWhenDone` / `onlyOnce` to false; the shipped scripts default them to true

- **Severity**: HIGH
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:266-281` (and the pinning tests at `:1028-1048`, fixture at `:736-746`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `recognize_set_stage_trigger` reads the optional Bool properties as `opt_bool_property("disableWhenDone")?.unwrap_or(false)` and `opt_bool_property(once_only)?.unwrap_or(false)` (`:269,279`). When the placed reference's VMAD does not list the property, the Papyrus semantics are "the script's declared default", not `false`. The same function already does this correctly for `prereqStageOPT` (`:236-249`, falls back to the decompiled script's initializer) and `two_state_activator` does it for all three of its bools - the two Bool options are the outliers.
- **Evidence**: Decompiled vanilla scripts (Skyrim SE `Skyrim - Misc.bsa`, `dump_pex_script`): `defaultSetStageTrigScript` declares `Property doOnce Bool = True`, `Property disableWhenDone Bool = True`, `prereqStageOPT = -1`, and its `OnTriggerEnter` does `If doOnce GotoState("hasBeenTriggered")` and `If disableWhenDone Self.Disable(false)`. `defaultSetStageTRIGSpecificActor` declares `disableWhenDone = True` and `onlyOnce = True`. Raw VMAD scan of `Skyrim.esm` (uncompressed records): 330 `defaultSetStageTrigScript` instances - `doOnce` present in only 10, `disableWhenDone` in 69, so ~97% / ~79% of placements omit them; 49 `defaultSetStageTRIGSpecificActor` instances - `onlyOnce` present in 10, `disableWhenDone` in 15. The test at `quest_stage_gate.rs:1043-1046` asserts "absent `disableWhenDone`/`onlyOnce` default to false" and the new player-trigger fixture declares `doOnce = True` yet asserts `!component.disable_after_advance, "doOnce comes from the VMAD, not the source"` (`:~800`), so the wrong default is pinned by tests. The fixture also declares `disableWhenDone = False` where the real script says `True`.
- **Impact**: For the large majority of vanilla placements the recognized `QuestAdvanceOnActivate` has `disable_after_advance = false` and `disable_reference_after_advance = false`: instead of firing once and disabling itself, the trigger re-issues `SetStage` on every re-entry (`papyrus_demo/quest_advance.rs:389-437` has no other once-guard), re-running the stage's fragment effects and able to move the quest's current stage backwards (`QuestStageState::set_stage` sets unconditionally). This is an approximate lowering of 600+ vanilla Skyrim placements - worse than leaving them inert.
- **Related**: #3940 (mistyped-vs-absent split, fixed the other half), #4326 (`onlyOnce` vs `disableWhenDone` semantics), #4334 (once-only persistence).
- **Suggested Fix**: Fall back to the script's own `Bool Property` initializer (same helper shape as `script_int`) before `false`, for `disableWhenDone` and the variant's once option; correct the fixtures (`disableWhenDone = True`) and invert the pinning assertions; add a real-data check on `defaultSetStageTrigScript.pex`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
