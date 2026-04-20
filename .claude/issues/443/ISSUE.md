# Issue #443

FO3-3-03: SCPT (pre-Papyrus bytecode) record group unparsed

---

## Severity: High

**Location**: `crates/plugin/src/esm/records/mod.rs` (no SCPT arm), `crates/plugin/src/esm/records/common.rs:97` (SCRI ref dangles)

## Problem

FO3 and FNV predate Papyrus — scripts are stored as SCHR header + SCDA bytecode. The current parser skips SCPT at the records catch-all, so every NPC/item `SCRI` form reference has no target record.

M30 Phase 1's Papyrus `.psc` parser does not cover this bytecode format.

## Impact

- Terminal scripts unreachable.
- Trap triggers, quest hooks, activator callbacks lose their linkage.
- Dialogue result scripts disconnected.

## Fix

Add `b"SCPT"` dispatch arm. Extract: EDID + SCHR (5× u32: numRefs, compiled size, var count, type, flags) + SCDA (bytecode blob) + optional SCTX (source text). Extraction only for now — runtime execution is a separate track.

## Completeness Checks

- [ ] **TESTS**: Parse Fallout3.esm, assert SCPT count matches expected, one record's SCRI refs resolve
- [ ] **SIBLING**: Check DIAL/INFO/QUST for SCRI-style references (resolved in FO3-3-05)
- [ ] **DOCS**: Note bytecode runtime is out of scope — extraction only

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-3-03)
