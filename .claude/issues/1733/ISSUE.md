# FO4-2026-06-23-L01: #1592 comment claims Glow_Map is OR'd into MaterialInfo, but only MSN + Alpha_Test are

**Issue**: #1733
**Severity**: LOW
**Labels**: low, nif-parser, nif, documentation
**Dimension**: 5 — FO4 shader flags
**Location**: `crates/nif/src/import/material/walker.rs:307-340`
**Source audit**: AUDIT_FO4_2026-06-23 (FO4-2026-06-23-L01)

## Description
The #1592 block comment (lines 310-311) states the walker ORs "F4SF2 bit 25 (Alpha_Test) and bit 6 (Glow_Map)" into `MaterialInfo`, but the code only consumes `MODEL_SPACE_NORMALS` (F4SF1 bit 12) and `ALPHA_TEST` (F4SF2 bit 25). There is no `GLOW_MAP` (F4SF2 bit 6) gate in this band. The const exists (`shader_flags.rs:177`) but is unused in the walker.

## Impact
Effectively none for content correctness — the glow texture is sourced unconditionally from the `BSShaderTextureSet` glow slot whenever present. Comment-vs-code drift, not a content-loss bug.

## Related
#1592 (closed, `f7fbbed5`).

## Suggested Fix
Drop "and bit 6 (Glow_Map)" from the comment (honest fix), or add the one-line flag gate (low value).
