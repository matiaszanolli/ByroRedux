# ESM-2026-09-21-D4-02: two doc comments on the creature-routing path still say placed creatures are "Oblivion ACRE, ACHR→CREA from FO3 on"

**Issue**: #4646
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: LOW
**Dimension**: Record Schema Dispatch & Coverage
**Location**: `crates/plugin/src/esm/records/index.rs:520-525` (`EsmIndex::actor` doc); `byroredux/src/cell_loader/references/mod.rs:639-643`

## Description
Both comments state the premise that closed #3755 showed false (that #3755 corrected only in `cell/walkers.rs`) — that `ACRE` is Oblivion-exclusive and `ACHR`→`CREA` is FO3+-exclusive.

## Evidence
`Fallout3.esm` has 3,349 ACRE and 2,154 ACHR; `FalloutNV.esm` has 2,999 ACRE and 3,386 ACHR — both coexist on FO3/FNV.

## Impact
None at runtime — routing goes by base record through `EsmIndex::actor`, which already handles both. Documentation hazard for future edits only.

## Suggested Fix
Reword both to "placed `ACRE` (Oblivion/FO3/FNV) or `ACHR`→`CREA`".

## Related
#3755, #2567 (closed — #3755 fixed the `cell/walkers.rs` sibling comment)

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D4-02)
