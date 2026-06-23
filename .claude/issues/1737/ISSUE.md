# SCR-D7-01: per-REFR (Skyrim+) VMAD override scripts are never resolved

Filed as: matiaszanolli/ByroRedux#1737
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring · Untrusted-Input: Yes
- **Location**: `byroredux/src/cell_loader/references.rs:386,1556`
- **Labels**: medium, import-pipeline, legacy-compat, bug

## Description
Both the trigger-volume path and `attach_vmad_scripts` resolve scripts ONLY via `base_record_script_instance(base_form_id)` — the base record's VMAD. Skyrim+ supports a per-REFR VMAD on the placed reference itself (uniquely-scripted placed objects/levers/quest items). That override VMAD is never read → those scripts attach nothing, a silent miss.

## Suggested Fix
Consult the REFR's own decoded VMAD first, falling back to the base-record VMAD (override-then-base).
