# #6261: SKY-D2-02: The EyeEnvmap branch samples the eye cubemap at LOD `roughness * 8`; vanilla eyes carry the classifier's no-signal roughness 0.85, so every eye reads the 1×1 mip of the 32² cube

**Labels**: medium,bug,shaders,renderer,game:skyrim,legacy-compat

**Source**: docs/audits/AUDIT_SKYRIM_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 2 (render-complete column of the shader matrix) · **Location**: `crates/renderer/shaders/triangle.frag`, the `if (isEyeEnvmap)` block: `textureLod(cubemaps[nonuniformEXT(mat.envMapIndex)], cubeDirection, roughness * 8.0)`.

## Location
- `crates/renderer/shaders/triangle.frag`, the `if (isEyeEnvmap)` block: `textureLod(cubemaps[nonuniformEXT(mat.envMapIndex)], cubeDirection, roughness * 8.0)`.
- Roughness source: `crates/core/src/ecs/components/material.rs` `classify_pbr_keyword` (terminal `PbrMaterial::NO_SIGNAL_NEUTRAL`), reached from `crates/nif/src/import/material/mod.rs` `classify_legacy_pbr`.

## Status at filing
NEW (from #5961, `b4d073454`).

## Description
- The branch's own comment says "No roughness attenuation: the cube IS the eye's reflection, and eye smoothness values in the wild straddle the gate". The LOD term contradicts it.
- Vanilla eyes have no roughness signal, so `roughness` is the classifier's no-signal neutral of 0.85. The texture path `eyebrown.dds` matches no keyword, there is no gloss map, and type 16 authors no `env_map_scale`. The authored `glossiness` of 479 is ignored without a gloss map.
- 0.85 × 8 gives LOD 6.8.
- The vanilla cube `textures\cubemaps\eyecubemap.dds` (`Skyrim - Textures3.bsa`) is DXT1, 32×32, with 6 mips. LOD 6.8 clamps to mip 5, which is one texel per face.

## Evidence
- `material_dump eyesmale.nif` prints `kind 16, rghO 0.85, env 0.00, gloss 479`.
- The DDS header reads `h 32 w 32 mips 6 fourcc DXT1 caps2 0xfe00` (all six cube faces).
- `triangle.frag` changes `roughness` only through the gloss-map mix and the weather terms (lines 495, 1539, 1631, 1636). None of them applies to an eye.

## Impact
every vanilla kind-16 eye reflects one averaged colour per cube face, a uniform tint over the iris. There is no glint, and the specular "alive eye" look is lost even after SKY-D2-01 and REN-H-01 are fixed.

## Related
SKY-D2-01, REN-H-01, #5961. The no-signal neutral itself is deliberate (#2707). The defect is only that this branch reads it as an eye smoothness.

## Suggested Fix
sample the eye cube at LOD 0, as the comment already describes, or at an LOD derived from an authored eye value. Do not use the classifier fallback. Add a source-shape pin so the eye `textureLod` does not take `roughness`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
