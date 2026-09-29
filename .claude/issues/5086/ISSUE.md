# #5086: OBL-2026-09-29-D2-03: EsmIndex::clothing doc says "~150 vanilla records"; Oblivion.esm has 604 CLOT

**Labels**: documentation, low, legacy-compat, game:oblivion, esm-plugin, doc-rot

**Source report**: `docs/audits/AUDIT_OBLIVION_2026-09-29.md`
**Severity**: LOW (doc-rot)
**Dimension**: BSA v103 & ESM Data Slice

## Location
`crates/plugin/src/esm/records/index.rs` (`EsmIndex::clothing` field doc).

## Description
The doc says "~150 vanilla records (robes, hoods, shirts, pants, shoes)". `Oblivion.esm` holds 604 `CLOT` records. The doc also does not say how CLOT reaches inventory.

## Evidence
Raw census of `Oblivion.esm`: CLOT = 604 (also recorded in the `/audit-oblivion` SKILL authoring counts).

Validated at HEAD 9fcfdc3fc: the field doc still reads "~150 vanilla records".

## Impact
Cosmetic; misleads anyone sizing CLOT handling.

## Suggested Fix
State 604 (`Oblivion.esm`), and state that CLOT enters inventory as `ItemKind::Armor` via `parse_clot`.

## Completeness Checks
- [ ] **SIBLING**: Other Oblivion-only `EsmIndex` field docs (BSGN, APPA, SGST, SLGM) checked for the same stale counts

