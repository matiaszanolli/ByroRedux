# FO4-D5-02: HairTint defaults to black albedo instead of a neutral fallback

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2602
**Finding ID**: FO4-D5-02

**Severity**: LOW
**Dimension**: 5 (Materials/Shading)
**Location**: `byroredux/src/material_translate.rs` (HairTint / `material_kind==6` consumer)
**Status**: NEW

## Description
The HairTint (`material_kind==6`) consumer defaults to black albedo
`[0.0, 0.0, 0.0]` when `hair_tint_color` is missing, unlike SkinTint's
default which is a safe identity value. This is a hardening gap, not a live
defect observed on any FO4 vanilla content in this audit pass.

## Evidence
HairTint's missing-color default resolves to `[0.0;3]` (black) rather than a
neutral/identity color the way the SkinTint path does.

## Impact
Low — no vanilla content triggers this (all FO4 hair materials carry a valid
`hair_tint_color`), but any future or modded content missing the field would
render pitch-black hair instead of a neutral fallback.

## Suggested Fix
Default `hair_tint_color` to the same kind of identity/neutral value
SkinTint uses, for consistency and to avoid a jarring pitch-black fallback.

## Completeness Checks
- [ ] **TESTS**: A regression test for HairTint with missing `hair_tint_color` pins the new default
