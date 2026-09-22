# SCR-D6-2026-09-22-02: obscript_quests.rs doc table links nonexistent ObScriptDiagnostics type

**Issue**: #4751
**Filed**: 2026-09-22 (audit-publish, AUDIT_SCRIPTING_2026-09-22.md)

**Severity**: LOW
**Dimension**: Legacy ObScript Execution
**Location**: `crates/scripting/src/obscript_quests.rs:23`

## Description
The module-doc dispatch table links `[`ObScriptDiagnostics`]` as the type that counts untranslated ObScript commands. That type does not exist anywhere in the workspace. The real counting type is `ObScriptQuestTimers` (`:78`, `:87`, written at `:285-292`).

## Evidence
Verified against HEAD `c3f298a24`: `obscript_quests.rs:23` links `[`ObScriptDiagnostics`]`; `grep -rn "ObScriptDiagnostics" --include="*.rs" .` returns zero matches anywhere in the workspace; `ObScriptQuestTimers` is the actual resource performing the counting.

## Impact
Cosmetic doc-link rot only.

## Related
None — isolated doc-link drift.

## Suggested Fix
Change the doc-table link target from `[`ObScriptDiagnostics`]` to `[`ObScriptQuestTimers`]` at `obscript_quests.rs:23`.
