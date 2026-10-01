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


---

## Solution

**Fixed**: clothing doc states 604 (raw census re-run this session: CLOT 604)
and that CLOT enters inventory as `ItemKind::Armor` (armour fields zeroed)
via `parse_clot` (verified at items.rs:819). SIBLING check found one more
stale count: sigil_stones "~30" → 150 (census + AUDIT_ESM_2026-08-13 both
say 150); BSGN ~13 correct; APPA/SLGM state no count. Commit: `Fix #5086`
(e7e8829b9).

## Verification

- Census re-measured this session with a corrected Oblivion walker
  (20-byte headers, label at group offset 8): BSGN 13, CLOT 604, APPA 23,
  SGST 150, SLGM 29.
