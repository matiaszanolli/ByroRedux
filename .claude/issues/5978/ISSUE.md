# SCR-D1-2026-10-17-10: Non-numeric script variables/properties are classified as numeric quest-variable reads that default to 0.0

Issue: #5978
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-10: Non-numeric script variables/properties are classified as numeric quest-variable reads that default to 0.0

- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/guards.rs:18-33`, `fragment/effects/evaluate.rs:71-83`, `inline.rs:108-120`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `classify_value` turns any identifier starting `::` (quest VMAD) or listed in `script_variables` (which `inline_quest_script_call` fills with every `Variable` *and* `Property` of the callee script, any type) into `ValueExpr::QuestVariable(SelfRef, name)`; `evaluate_value` reads an unknown variable as `0.0` (`.unwrap_or(0.0)`) rather than failing. In `classify_condition`'s final `Compare` fallback (`guards.rs:225-231`) a `==`/`!=` between two bare object-typed properties (which `reference_side` rejects because it only takes locals/calls) therefore compares `0.0 == 0.0` and is true.
- **Evidence**: code above. Reachability needs both operands to be bare non-numeric script properties; not seen in the checked samples.
- **Impact**: Wrong branch for such a comparison instead of a decline.
- **Related**: none.
- **Suggested Fix**: Resolve the variable's declared type in the inline scope (the library script is available) and only classify Int/Float/Bool variables; treat a non-numeric ident as a decline.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
