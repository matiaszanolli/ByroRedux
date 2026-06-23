# SCR-D6-02: OnCellLoadEvent emitted but not drained — latent every-frame re-fire + one-frame-contract violation

Filed as: matiaszanolli/ByroRedux#1736
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems
- **Location**: `crates/scripting/src/cleanup.rs:27-38` vs `byroredux/src/cell_loader/references.rs:1461`
- **Labels**: medium, legacy-compat, bug

## Description
`attach_script_for_refr` emits `OnCellLoadEvent` on every scripted REFR; both its comment and events.rs:117-118 claim it is drained "so each script sees exactly one." cleanup.rs does NOT drain it. No consumer exists yet, so today it is an undrained accumulating marker (per-entity leak + broken one-frame contract). When the OnCellLoad first-tick consumer lands it re-fires every frame.

## Suggested Fix
Drain `OnCellLoadEvent` (and `OnEquipEvent`) in `event_cleanup_system`.
