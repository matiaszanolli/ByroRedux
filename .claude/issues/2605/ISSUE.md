# FO4-D5-05: BSVER 140-154 dead shader_type band undocumented in code

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2605
**Finding ID**: FO4-D5-05

**Severity**: LOW
**Dimension**: 5 (Materials/Shading)
**Location**: BSVER 140-154 shader_type gate
**Status**: NEW

## Description
BSVER 140-154 forces `shader_type=0` (`ShaderTypeData::None`), meaning no
SkinTint/HairTint/etc. payload is possible in that version band. This is a
documented dead band — no shipping game uses BSVER 140-154 — so it's
informational rather than a live defect.

## Evidence
The `shader_type` decode forces `0`/`None` for any BSVER in the 140-154
range.

## Impact
None — no shipping game content falls in this BSVER band.

## Suggested Fix
No action required; documented as informational. Consider a comment noting
the band is intentionally dead so a future reader doesn't mistake it for an
oversight.

## Completeness Checks
- [ ] **TESTS**: N/A — no live content exercises this band
