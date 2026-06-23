# SCR-D6-03: stale deferred-to-Rapier comments for OnTriggerEnterEvent

Filed as: matiaszanolli/ByroRedux#1741
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems
- **Location**: `crates/scripting/src/lib.rs:62`, `crates/scripting/src/events.rs:96-99`
- **Labels**: low, legacy-compat, documentation

## Description
Both claim OnTriggerEnterEvent has no engine emit site ("deferred to Rapier"); `trigger_detection_system` is the live M47.2 emit site. This stale comment is the root cause of SCR-D6-01 (the drain was never added).

## Suggested Fix
Update both comments to point at `trigger_detection_system`.
