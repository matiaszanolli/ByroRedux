# Issue #446

FO3-3-04: PACK AI package records skipped — 30-procedure system has no producer

---

## Severity: Medium

**Location**: `crates/plugin/src/esm/records/actor.rs:43`, `crates/plugin/src/esm/records/mod.rs` (no PACK arm)

## Problem

`NpcRecord` collects `ai_packages: Vec<u32>` from PKID sub-records but the referenced PACK records are never parsed — they hit the catch-all skip.

## Impact

The AI Packages & Procedures memo catalogs 30 composable procedures; none of them have a data producer today. NPC scheduling, guard patrols, merchant behavior, dialogue triggers all depend on PACK data.

## Fix

Add `b"PACK"` dispatch arm. Read: PKDT (flags + procedure type), PSDT / PLDT (package schedule / location), PKTG (package target), PKCU (package custom), PKPA (procedure tree).

## Completeness Checks

- [ ] **TESTS**: Parse Fallout3.esm, verify PACK count > 0 and one NPC's PKID resolves to a valid PACK
- [ ] **SIBLING**: DIAL (FO3-3-05) has similar SCRI/PKID refs that need cross-linking
- [ ] **DOCS**: Update `ai_packages_procedures.md` memory with current parser state

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-3-04)
