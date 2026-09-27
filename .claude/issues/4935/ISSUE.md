# EXT-D6-2026-09-27-06: exal.md still states the #3321-falsified premise

**Issue**: #4935
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,documentation,doc-rot

**Severity**: LOW (doc rot)
**Dimension**: Distant LOD and trees
**Tier Violated**: no-fabrication
**Game Affected**: FO3/FNV (doc)
**Status**: NEW
**Location**:
`docs/engine/exal.md:259`, `:156-166`, `:801-803`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- §4's table says FO3/FNV have "neither scheme", which §5.2 itself names as the wrong pre-#3321 guess.
- §2 calls legacy LOD textures unconsumed, contrary to `b50a8e6a9`.

## Suggested Fix
Change the §4 row to the `FalloutLegacyBlocks` scheme, retitle §2, and qualify `:801`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **TESTS**: Doc text matches the code it describes (re-grep the cited symbols)
