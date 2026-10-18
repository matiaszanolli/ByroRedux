# SCR-D2-2026-10-17-05: `GetVMScriptVariable` (CTDA 630) cannot see a VM-run reference script's variables

Issue: #5985
Labels: medium, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-05: `GetVMScriptVariable` (CTDA 630) cannot see a VM-run reference script's variables

- **Severity**: MEDIUM
- **Dimension**: Fragment Dispatch & Locks (owner-keyed variable stores) / Event Runtime
- **Location**: reader `crates/scripting/src/condition/eval/global.rs:124-139`; VM writes `fragment/vm/interp.rs:560-611` (`write_quest_variable` under the reference FormID) and `docs/engine/scripting.md:975-978`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: A reference script that runs on the VM keeps its variables in the saved `QuestScriptVariables`/`QuestScriptObjects` stores under the *reference FormID* (correctly owner-keyed, outliving the cell). `get_vm_script_variable` however reads only the entity component `vm_state::ScriptVariables` (`world.get::<ScriptVariables>(target)`), which is written by the lowered `SetReferenceVariable` effect and the two native activator systems - never by the VM. For a `CTDA GetVMScriptVariable(ref, "::x_var")` that targets a VM-run script the condition therefore always evaluates `0.0` ("the engine ran the script, the condition silently reads nothing"). The quest-side twin `get_vm_quest_variable` goes through `quest_variables::quest_variable` and does see VM writes, so the asymmetry is specific to reference scripts. `docs/engine/scripting.md:411` documents the entity component only for the *lowered* write path; the VM wave (`852c61078`, `5f1671e59`) did not extend it.
- **Evidence**: `get_vm_script_variable`: `resolve_entity_by_global_form_id(...)` -> `world.get::<ScriptVariables>(target).and_then(|v| v.get(variable)).unwrap_or(0.0)`; no reference to `QuestScriptVariables` in that function or in `condition/`.
- **Impact**: Dialogue/package/perk conditions gated on a VM-script reference variable (CTDA 630 is the reference-script analogue of the 3 319 Skyrim `GetVMQuestVariable` rows; census for 630 not taken - unverified) evaluate to a fixed 0.0 - wrongly open or closed gates with no diagnostic. Not a regression (no condition read of VM state existed before the wave), but the VM wave made the gap reachable.
- **Related**: #5752 / #5750 / #5778 (catalog gaps of the same fixed-0.0 flavour; this is a different cause). Not filed.
- **Suggested Fix**: Make `get_vm_script_variable` fall back to `QuestScriptVariables` under the target's FormID (same `property_key` spelling), or mirror numeric VM writes into `ScriptVariables`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
