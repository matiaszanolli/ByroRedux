# SKY-D2-02: shader_flags.rs module doc asserts Skyrim has an Alpha_Test SLSF1 bit -- nif.xml has none, and the file contradicts itself 37 lines later

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2580
**Finding ID**: SKY-D2-02

**Severity**: LOW
**Dimension**: BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/shader_flags.rs:203` (vs `:240-241`)
**Status**: NEW

## Description
The `fo4_slsf2` module doc's parenthetical ("Skyrim has Alpha_Test on SLSF1!") is unsupported by nif.xml — no `Alpha_Test` option exists anywhere in `SkyrimShaderPropertyFlags1`/`2` (bit 25 is `Remappable_Textures`). Skyrim routes alpha-test exclusively via `NiAlphaProperty`, which the same file's own doc states correctly 37 lines below.

## Evidence
Confirmed directly: `shader_flags.rs:203` reads "Bit 25 is `Alpha_Test` on FO4 (Skyrim has Alpha_Test on SLSF1!)" while `:240-241` correctly reads "Bit 25 — `Alpha_Test` on FO4. Skyrim routes alpha-test via `NiAlphaProperty` on a sibling block, not a shader flag bit." nif.xml:6396 confirms bit 25 is `Remappable_Textures`.

## Impact
No runtime effect (no code reads Skyrim SLSF1 bit 25), but this file's stated purpose is documenting per-game bit semantics for future contributors — exactly the error class behind #414/#1879.

## Related
#414, #1879

## Suggested Fix
Fix the parenthetical to match `fo4_slsf2::ALPHA_TEST`'s own correct doc.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
