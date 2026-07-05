# #1873 — REN-2026-07-04-M01: PBR classifier can't distinguish authored-white specular from struct default, chroming legacy flyers

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1873
**Labels**: bug, nif-parser, renderer, medium
**Filed via**: /audit-publish docs/audits/AUDIT_RENDERER_2026-07-04.md

---

- **Severity**: MEDIUM
- **Dimension**: NIFAL material translation / PBR classifier
- **Location**: `classify_pbr_keyword` (`crates/core/src/ecs/components/material.rs:548`), fed by `MaterialInfo::classify_legacy_pbr` (`crates/nif/src/import/material/mod.rs:1040`), authored via the `BSShaderPPLightingProperty` walker arm (`crates/nif/src/import/material/walker.rs:767`, `env_map_scale` set at line 847)
- **Status**: NEW

## Description

Confirmed root cause of a live user-reported symptom: decorative FO3/FNV wall
props ("flyers"/posters) render abnormally reflective/chrome instead of matte
paper.

`classify_pbr_keyword`'s env-map-scale arm infers metalness from
`specular_color` luminance for surfaces that hit no keyword match. It has no
signal for "was specular actually authored on this material", so it can't
distinguish a genuine authored-white specular (real metal/chrome intent) from
`MaterialInfo`'s own unset struct default of `[1.0, 1.0, 1.0]`
(`crates/nif/src/import/material/mod.rs:961`).

## Evidence

Mechanism, traced end-to-end (full detail in `docs/audits/AUDIT_RENDERER_2026-07-04.md`, Symptom 1):

1. A FO3/FNV decorative flyer/poster carries `BSShaderPPLightingProperty`. Its
   walker arm authors `env_map_scale ≈ 1.0` on "nearly every FNV surface" per
   the classifier's own source comment, but never touches `specular_color` —
   only the `BSLightingShaderProperty` arm (walker.rs:349, Skyrim+/FO4) or the
   `NiMaterialProperty` arm (walker.rs:635, Oblivion-era) do.
2. A PPLighting mesh with no co-bound `NiMaterialProperty` (common for
   decorative planes) leaves `specular_color` at the `[1,1,1]` struct default.
3. No classifier keyword matches a flyer/poster diffuse path, so it falls to
   the `env_map_scale > 0.3` arm.
4. With `specular_color = [1,1,1]`: `spec_lum = 1.0` →
   `metalness = ((1.0-0.5)*0.8).clamp(0,0.4) = 0.4`,
   `roughness = min(0.8, 0.55) = 0.55`.
5. `metalness=0.4, roughness=0.55` crosses the RT-reflection gate
   (`roughness < 0.6`, `triangle.frag:1795`) → RT reflections engage on a flat
   paper surface.

Every existing env-map-arm regression test uses a low specular `[0.2;3]`
fixture — none exercises the true `[1,1,1]` default, which is the coverage
hole this bug lives in.

Cross-checked clean by 3 other audit dimensions: no shader-side amplification
(plain Fresnel-only reflection weighting, no metalness-squared double gate),
no tangent/normal-map contribution (texture-less flyer has no normal map).
This is a pure CPU-side classification bug, not a rendering pipeline bug.

## Impact

Any legacy FO3/FNV (and likely Oblivion-era) mesh with a PPLighting/legacy
shader property, `env_map_scale > 0.3`, and no co-bound `NiMaterialProperty`
renders with an unintended chrome/mirror sheen. Purely visual, no crash/data
corruption, but affects arbitrary vanilla decorative content across the
FO3/FNV corpus.

## Related

Cross-referenced against the pre-existing "chrome means missing textures"
mechanism (checker-placeholder × normal map) — this is a **different**
mechanism (wrong PBR classification, not a missing texture), don't conflate
when triaging.

## Suggested Fix

Thread a `specular_authored: bool` into `PbrClassifierInputs` (or reuse
`MaterialInfo::has_material_data`); require it before the env-map arm lifts
metalness from `spec_lum`; when unauthored, force metalness=0 (matte
ceiling). Add the missing `[1,1,1]`-default regression test — self-contained
CPU change, fully verifiable via `cargo test -p byroredux-core`, no RenderDoc
needed for the fix itself. A live `tex.missing`/visual spot-check on real
content is the natural final confirmation that a specific vanilla flyer NIF
matches this exact path (content assertion, not yet verified).

## Completeness Checks
- [ ] **SIBLING**: Check the same PPLighting-without-NiMaterialProperty gap doesn't also affect other env-map-scale-driven classifications (e.g. windows, other decorative planes)
- [ ] **TESTS**: A regression test pins the `[1,1,1]` struct-default input to metalness=0, distinct from an authored-white-specular fixture which should still classify as metallic
