# SF-2026-09-11-D3-04: Field::offset/Field::size are parsed and read by nothing in-tree, with no committed guard for the declaration-order-vs-offset-order divergence that found the XMCOLOR bug

**Issue**: #4275 — https://github.com/matiaszanolli/ByroRedux/issues/4275
**Labels**: low,import-pipeline,tech-debt,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 3 — CDB Material Database Correctness
**Location**: `crates/sfmaterial/src/types.rs (Field::offset, Field::size); crates/sfmaterial/src/reader.rs (read_user_class)`
**Status**: NEW

## Description
`Field::offset` and `Field::size` are parsed from the CDB wire format into the `Field` struct but have zero consumers anywhere in-tree — `read_user_class` reads fields in declaration order, never consulting `Field::offset`. This is exactly the class of divergence that produced the still-open XMCOLOR field-offset bug (#3398's own scope: 96 of 97 classes agree declaration order with offset order, XMCOLOR is the one exception) — but there is no committed regression guard that would catch a *second* such divergence if one exists or is introduced.

## Evidence
Grep-confirmed during this audit: `Field::offset`/`Field::size` are struct fields with no read site outside their own parse/construction code.

## Impact
No live defect beyond the already-known, already-tracked XMCOLOR case (#3398) — this finding is about the absence of a guard for a *future* or *undiscovered* instance of the same divergence class, not a new instance itself.

## Related
Adjacent to #3398 (CDB Phase 2, which names the XMCOLOR field-offset bug as a known exception).

## Suggested Fix
Add a debug-assertion or one-time validation pass comparing declaration order against `Field::offset`/`Field::size` ordering across the full class vocabulary, so a second XMCOLOR-class divergence is caught rather than silently reproduced.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
