# SCR-D7-NEW4-02: audit-scripting SKILL.md cites stale mod.rs path for three attach-path functions

**Issue**: #2190
**Labels**: low, documentation
**Dimension**: Engine Attach Path & Trigger-Volume Wiring
**Untrusted-Input**: No
**Location**: `.claude/commands/audit-scripting/SKILL.md` (Dimension 7 "Entry points" line, citing `byroredux/src/cell_loader/references/mod.rs` for `attach_vmad_scripts`, `attach_script_for_refr`, `trigger_volume_from_primitive`)
**Status**: NEW

## Description

These three functions were split out of `references/mod.rs` into a new sibling file `byroredux/src/cell_loader/references/attach.rs` (its own header states "Split out of the original `cell_loader/references.rs` (#1877)"). `mod.rs` now only re-exports them (`use attach::{attach_container_inventory, attach_script_for_refr, trigger_volume_from_primitive};`) and retains their test modules. The skill's entry-point line (`.claude/commands/audit-scripting/SKILL.md:55-57,757-758`) still names only `mod.rs`.

## Evidence

Confirmed: `byroredux/src/cell_loader/references/attach.rs` exists and defines the functions; `SKILL.md` lines 55-57 and 757-758 both cite `byroredux/src/cell_loader/references/mod.rs` exclusively.

## Impact

Cosmetic/navigational only — a future audit pass or contributor following the skill's entry-point list would look in the wrong file first. No functional impact; the functions themselves are correct and unchanged in behavior.

## Suggested Fix

Update the Dimension 7 "Entry points" line in `SKILL.md` to cite `byroredux/src/cell_loader/references/attach.rs` for these three functions, keeping `references/mod.rs` for the call sites / dispatch context.

## Completeness Checks
- [ ] **DOCS**: Update both cited locations in `SKILL.md` (the short "Entry points" summary line and the fuller Dimension 7 section)
