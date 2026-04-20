# Issue #448

FO3-3-06: LVLC (leveled creature) records skipped — FO3 enemy spawn system unreachable

---

## Severity: Medium

**Location**: `crates/plugin/src/esm/records/mod.rs:163-172` (LVLI + LVLN only)

## Problem

LVLC is byte-identical to LVLI/LVLN but has no dispatch arm. FO3 uses LVLC for the majority of enemy encounters; FNV migrated most to LVLN which masked this gap during FNV development.

## Evidence

FO3 parses to 972 LVLI (loot) + 89 LVLN (actor, DLC-only style) records. LVLC-referencing encounter zones fail silently — the leveled list has no entries.

## Fix

Add `b"LVLC"` dispatch. Reuse `parse_leveled_list` (already works for LVLI/LVLN). Output to `EsmIndex::leveled_creatures`.

## Completeness Checks

- [ ] **TESTS**: Parse Fallout3.esm, assert LVLC count matches expected (~300-500 vanilla)
- [ ] **SIBLING**: Oblivion also uses LVLC — verify that parse benefits as well
- [ ] **DOCS**: Record type catalog memory updated

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-3-06)
