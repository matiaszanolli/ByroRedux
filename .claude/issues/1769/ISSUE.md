# #1769: D7-NEW-01: VMAD attach dedup is case-sensitive; Papyrus names are case-insensitive

Filed from `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` on 2026-06-27. Snapshot as-filed (GitHub is authoritative for live state).

**Severity**: LOW · **Dimension**: Engine Attach & Trigger Wiring · **Untrusted-Input**: Yes
**Location**: `byroredux/src/cell_loader/references.rs:1594-1604` (`attach_vmad_scripts`)
**Status**: NEW
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` (D7-NEW-01)

## Description
The collision-dedup added by the #1737 per-REFR VMAD fix keys on the raw byte string — `seen: HashSet<&str>; … seen.insert(script.name.as_str())`. Papyrus identifiers are case-insensitive, and the codebase honors that everywhere else (`ScriptInstance::property` / `ScriptInstanceData::script` use `eq_ignore_ascii_case`; the `.pex` path normaliser lowercases; `translate/tables.rs` lowercases). If a REFR's own VMAD names `"MyScript"` and the base record names `"myscript"` (the same script under Papyrus rules), the case-sensitive `seen` set does not treat them as equal, so the base copy is attached a second time.

## Evidence
`script.name.as_str()` inserted verbatim into `HashSet<&str>`; contrast `script_instance.rs` name comparisons via `eq_ignore_ascii_case`.

## Impact
A redundant second `extract_pex` + `translate_pex` + `(recognized.spawn)` for the same logical script. Because the recognizer spawn closures insert into `SparseSetStorage` (overwrite, not append), the second insert overwrites the first with identical data — ECS outcome is idempotent. Wasted-work / contract-inconsistency, not a double-advance or corruption bug.

## Related
#1737 (the per-REFR VMAD fix that introduced the dedup set).

## Suggested Fix
Lowercase the key before insertion (`seen.insert(script.name.to_ascii_lowercase())`, set becomes `HashSet<String>`) to match the case-insensitive script-name contract used by the rest of the VMAD/recognizer code.

## Completeness Checks
- [ ] **SIBLING**: confirm the base-vs-REFR script-name comparison is the only case-sensitive name match left on the attach path
- [ ] **TESTS**: a regression where REFR VMAD names `MyScript` and base names `myscript`, asserting the base copy is skipped (one attach, not two)
