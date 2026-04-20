# Issue #442

FO3-3-02: CREA record group unparsed — FO3 bestiary metadata unreachable

---

## Severity: High

**Location**: `crates/plugin/src/esm/records/mod.rs:128-204` (catch-all skip)

## Problem

`CREA` top-level GRUPs are dropped at the catch-all skip in `records/mod.rs`. In FO3 the CREA record holds the core bestiary: super mutants, deathclaws, radroaches, mirelurks, robots, brahmin, etc. FNV migrated most combat to NPC_ (3,816 NPCs vs FO3's 1,647), which masked this gap during FNV development.

## Impact

- No race/faction/inventory/AI metadata attached to any creature.
- `cell.rs:389` can spawn the visual from REFR + base CREA form, but the base form has no backing record — combat is effectively impossible.
- The `records/common.rs:97` `SCRI` ref on creatures dangles.

## Fix

Add `b"CREA"` dispatch arm reusing `parse_npc` — overlapping sub-records include EDID, FULL, MODL, RNAM, CNAM, SNAM, CNTO, PKID, ACBS. Register output in a new `index.creatures: HashMap<FormId, CreatureRecord>`.

## Completeness Checks

- [ ] **TESTS**: Parse Fallout3.esm, assert CREA count matches expected (~700-800 in vanilla)
- [ ] **SIBLING**: Verify FNV CREA count non-zero — FNV still has some legacy CREA entries for regression coverage
- [ ] **DOCS**: Record type catalog memory entry updated

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-3-02)
