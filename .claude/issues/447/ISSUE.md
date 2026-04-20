# Issue #447

FO3-3-05: DIAL/INFO/QUST/MESG/PERK/SPEL/MGEF record groups skipped

---

## Severity: Medium

**Location**: `crates/plugin/src/esm/records/mod.rs:200-203` (catch-all)

## Problem

Seven top-level record groups present in Fallout3.esm hit the catch-all skip:
- DIAL (dialogue topic) + INFO (dialogue line, nested under DIAL GRUP)
- QUST (quest)
- MESG (message / popup)
- PERK (perk definition)
- SPEL (spell / ability)
- MGEF (magic effect — bridge for AV changes)

## Impact

- No dialogue system can wire up.
- No quest lifecycle / objectives / alias fill (blocks `quest_alias_system.md` + `quest_story_manager.md` memos).
- No perk entry point application (blocks `perk_system.md` + `perk_entry_points.md`).
- No popup / tutorial messages.
- No spell/magic effect catalog.

## Fix

Recommended order: QUST first (unblocks Story Manager), then DIAL+INFO (recurses into nested GRUPs via existing `extract_records`), then PERK/SPEL/MGEF, MESG last.

## Completeness Checks

- [ ] **TESTS**: Per-record-type count assertion against Fallout3.esm vanilla numbers
- [ ] **SIBLING**: Ensure FNV counts regression — should pick up FNV data for free
- [ ] **DOCS**: Update record type catalog memory

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-3-05)
