# #1592 — FO4-D5-MEDIUM-01: FO4 NIF shader-flag bits (model-space-normals/alpha-test/glow-map) parsed but never consumed at import

**Severity**: MEDIUM · **Dimension**: FO4 Shader Flags & BGSM PBR Routing
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D5-MEDIUM-01)
**Location**: `crates/nif/src/import/material/walker.rs:126-339` (BSLightingShaderProperty arm); constants `crates/nif/src/shader_flags.rs:114-209` (`fo4_slsf1::MODEL_SPACE_NORMALS`=bit12, `fo4_slsf2::ALPHA_TEST`=bit25, `fo4_slsf2::GLOW_MAP`=bit6)

## Description
For an FO4 `BSLightingShaderProperty` (bsver 130) the parser reads the full F4SF1/F4SF2 u32 pair into `shader_flags_1/2`, but the walker only consumes decal, two-sided, and effect-shader bits. Three render-affecting FO4 bits are never tested: `MODEL_SPACE_NORMALS` (F4SF1 bit12), `ALPHA_TEST` (F4SF2 bit25), `GLOW_MAP` (F4SF2 bit6). These attributes are sourced exclusively from the companion BGSM/BGEM file.

## Evidence
grep for `model_space`/`0x1000`/`GLOW_MAP`/`fo4_slsf2::ALPHA_TEST` in `walker.rs` returns no consumption site (only a comment at :505). `model_space_normals` setters live only in `asset_provider.rs` (BGSM merge), never in the NIF property path.

## Impact
Vanilla FO4 is always BGSM/BGEM-backed (BGSM is authoritative), so vanilla content is unaffected. The gap bites inline-authored / loose / modded FO4 NIFs that set these bits in the NIF and ship no matching BGSM: an object-space normal map decoded as tangent-space (wrong shading), an inline alpha-tested cutout rendering opaque, a flag-only glow signal dropped. Visual-only, bounded to non-vanilla content.

## Related
#1352 (CLOSED — BGSM authoritative for MSN/PBR); #1384 (MODEL_SPACE_NORMALS bit value, benign); #1241 (PBR-scalar consumption pattern).

## Suggested Fix
In the BSLightingShaderProperty walker arm, OR the NIF flag-derived signals into MaterialInfo as a LOWER-priority source than the BGSM merge (`if shader_flags_1 & fo4_slsf1::MODEL_SPACE_NORMALS != 0 { info.model_space_normals = true; }`, likewise alpha-test/glow), keeping BGSM as the override. Handle the CRC32 path for bsver≥132 DLC content and gate the bit vocabulary on the FO4 variant so a Skyrim property isn't read with FO4 semantics.

## Completeness Checks
- [ ] **SIBLING**: CRC32 flag path for bsver≥132 DLC content covered; bit vocabulary gated on the FO4 variant (Skyrim property not read with FO4 semantics)
- [ ] **CANONICAL-BOUNDARY**: NIF-derived flags merged into MaterialInfo at the parser→Material boundary as a lower-priority source than BGSM; never re-derived at render time
- [ ] **TESTS**: A regression test pins MSN/alpha-test/glow being OR'd from the NIF flags when no BGSM overrides
