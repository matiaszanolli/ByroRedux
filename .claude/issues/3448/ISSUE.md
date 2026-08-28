# Issue #3448: REN-2026-08-27-D17-01: bethesdaRimFactor turns a "no value authored" 0.0 into the clamp floor 0.25, producing a near-full-surface albedo add — the hazard #2589 fixed for the sibling fields

**Filed**: 2026-08-27 via /audit-publish from `docs/audits/AUDIT_RENDERER_2026-08-27.md`

**Severity**: MEDIUM
**Dimension**: Disney BSDF / PBR gating (Bethesda lighting response)
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-27.md` (REN-2026-08-27-D17-01)
**Status**: NEW — consumer landed 2026-08-25 (`b80313f6` / `ceb69d24`); the fields were inert before that.

## Location
- `crates/renderer/shaders/include/lighting.glsl:110-116` (`bethesdaRimFactor`)
- The `0.0` no-value sites it falls back onto:
  - `crates/nif/src/blocks/shader.rs:826` (`material_reference_stub`)
  - `crates/nif/src/blocks/shader.rs:1116` (`parse_fo4`)
  - `crates/nif/src/blocks/shader.rs:1313` (`parse_fo76_plus`)
  - `crates/nif/src/import/material/mod.rs:1152` (`MaterialInfo::default`)
  - `crates/nif/src/import/types.rs:653` (`ImportedMaterial` default)
  - `crates/core/src/ecs/components/material.rs:518` (`Material` default)

## Description
`bethesdaRimFactor` resolves its exponent as `rimlightPower > 0.0 ? rimlightPower : lightingEffect2`, then `clamp(exponent, 0.25, 16.0)`. When **both** lanes are zero — the state every no-value site above installs — the exponent becomes the clamp *floor*, 0.25, i.e. the broadest possible rim rather than a neutral or disabled one.

Its two siblings in the same file handle their zero case deliberately and correctly: `bethesdaDiffuseLightFactor` degenerates to plain `max(N·L, 0)` at `width == 0`, and `bethesdaBackFactor` explicitly documents *"zero there therefore means the Skyrim unit-strength convention rather than disabling a feature whose flag is already set"* and substitutes `1.0`. Rim is the one lobe with no such treatment.

## Evidence
```glsl
// lighting.glsl:110-116
float bethesdaRimFactor(GpuMaterial mat, float NdotV, float frontNdotL) {
    if ((mat.materialFlags & MAT_FLAG_RIM_LIGHTING) == 0u) return 0.0;
    float exponent = mat.rimlightPower > 0.0
        ? mat.rimlightPower : mat.lightingEffect2;
    exponent = clamp(exponent, 0.25, 16.0);
    return pow(clamp(1.0 - NdotV, 0.0, 1.0), exponent) * frontNdotL;
}
```
and the contribution it feeds (`lighting.glsl:244-248`):
`brdfResult += albedo * clamp(lightingMask,0,1) * rim * (1.0 - metalness)`.

At exponent 0.25 the rim weight is `0.56` even head-on (`NdotV = 0.9`) and `0.84` at `NdotV = 0.5` — more than half the diffuse lobe again, added across the whole surface rather than at its silhouette.

`nif.xml` (`/mnt/data/src/reference/nifxml/nif.xml:6605-6606`) gives `Lighting Effect 1` **default 0.3** and `Lighting Effect 2` **default 2.0**; every site listed above installs `0.0` for both.

In the *same* struct literals, two lines below, #2589 already applied precisely this correction to the neighbouring fields — `grayscale_to_palette_scale: 1.0`, `fresnel_power: 5.0` — with a comment stating *"`0.0` here silently survived … producing a full-strength (`pow(1-cosθ,0)==1`) Fresnel term at every view angle the moment a shading consumer reads it — latent only because no consumer exists yet"*. A consumer now exists.

## Impact
Visual only, no crash. Reachable on (a) Skyrim content, where `parse_skyrim` hard-sets `rimlight_power = 0.0` by design and the real rim power lives in `lighting_effect_2`, so any `SLSF2_Rim_Lighting` material with an unset/zero `lighting_effect_2` over-brightens; and (b) any FO4+ material reaching the shader through `material_reference_stub` or `MaterialInfo::default` with the rim flag set. Real-content prevalence is **unmeasured** — the audit environment has no GPU and no census was run — so the finding is the degenerate branch, not a claim about how many meshes hit it.

## Related
#2589 (SKY-D7-01, the identical fix on the sibling fields), #2284, `feedback_no_guessing.md`

## Suggested Fix
Apply #2589's own rule to these two fields — seed `lighting_effect_1: 0.3` / `lighting_effect_2: 2.0` at the no-value sites, matching `nif.xml`'s declared defaults — and/or give `bethesdaRimFactor` an explicit zero arm (`return 0.0`, or substitute the format default) the way `bethesdaBackFactor` already has one, so the clamp floor is never load-bearing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bethesda lighting lobes and every no-value default site for these fields)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **SPV**: If `lighting.glsl` changes, every dependent `.spv` is recompiled with plain `glslangValidator -V` and committed
