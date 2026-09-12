# OB-D4-03: gamebryo_to_vk_blend_factor's out-of-range fallback applies SRC_ALPHA to the destination factor too, where the engine default is INV_SRC_ALPHA

**Issue**: #4262 — https://github.com/matiaszanolli/ByroRedux/issues/4262
**Labels**: low,renderer,game:oblivion,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 4 — Rendering Path for Oblivion Shaders
**Location**: `crates/renderer/src/vulkan/pipeline.rs:214-227 (gamebryo_to_vk_blend_factor), call sites at lines 795-796`
**Status**: NEW

## Description
`gamebryo_to_vk_blend_factor` is a single shared function used for both the source and destination blend-factor lookups (`src_factor = gamebryo_to_vk_blend_factor(src)`, `dst_factor = gamebryo_to_vk_blend_factor(dst)` at lines 795-796). Its documented out-of-range fallback is `SRC_ALPHA`, which is correct for the source factor ("the Gamebryo default") but not for the destination factor, whose engine default is `INV_SRC_ALPHA`.

## Evidence
`gamebryo_to_vk_blend_factor(v: u8)` has a single `_ => vk::BlendFactor::SRC_ALPHA` fallback arm shared by both call sites; `NiAlphaProperty` storage is a nibble (max parsed value 15), so an out-of-range value (11-15) is reachable only on corrupted/fuzzed input.

## Impact
Defensive-path only — no vanilla content exercises an out-of-range blend-factor nibble. If it ever were hit on the destination side, the resulting blend would use `SRC_ALPHA` instead of the engine's actual default `INV_SRC_ALPHA`, producing an incorrect blend result rather than falling back to the documented default behavior.

## Related
None filed.

## Suggested Fix
Split the fallback per call site (or add a `is_dest: bool` parameter) so the destination-factor call falls back to `INV_SRC_ALPHA` instead of reusing the source-factor default.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
