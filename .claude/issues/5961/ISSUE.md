# SKY-D2-2026-10-17-02: EyeEnvmap (shader type 16) has no shading branch and no open tracker; MultiLayerParallax refraction_scale unread

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5961 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (report ID SKY-2-02) · **Severity**: LOW · **Dimension**: 2 · **Game**: Skyrim SE + LE

## Description
`BSLightingShaderProperty` shader type 16 (EyeEnvmap) is parsed (28 B, `crates/nif/src/blocks/shader/lighting.rs:895-913`), imported, and packed into the GPU material and instance (`byroredux/src/render/static_meshes.rs:996+`, `crates/renderer/shaders/include/bindings.glsl:155-156`), but nothing in GLSL reads `eyeLeftCenter` / `eyeRightCenter` / `eyeCubemapScale`. The stub at `crates/renderer/shaders/triangle.frag:1602-1606` says it "requires a per-instance cubemap descriptor binding that doesn't exist yet". Type 11 (MultiLayerParallax) `refraction_scale` is likewise packed but unread.

## Status / tracking gap
Closed #619 (SK-D3-05) was closed by `19e7115`, which only gated the CPU pack by `material_kind`; its own comment says "shader stub today". The shading branch was never implemented and no open issue mentions EyeEnvmap or the eye cubemap (searched all states). The 2026-10-12 audit's matrix listed type 16 as "Consumed", which overstated it.

## Impact
About 3,251 SE / 2,628 LE eye blocks (2026-10-12 census) lack the authored iris-centred reflection; they fall back to the generic environment / RT reflection path. Cosmetic fidelity only.

## Suggested fix
Track as an enhancement. A per-eye reflection could reuse the existing RT reflection path rather than a new cubemap binding. Until implemented, label the audit matrix "packed, not shaded".

Related: #619, #623, #562 (all closed).

## Completeness Checks
- [ ] **SIBLING**: FO4 and FO76 eye shader types checked for the same gap
- [ ] **CANONICAL-BOUNDARY**: Any eye-reflection input stays resolved at the NIFAL parser->`Material` boundary (`translate_material`), never re-derived per game in the shader
- [ ] **TESTS**: A render or packing test pins that kind 16 reaches the shader
