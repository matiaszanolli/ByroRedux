# Issue #3458: NIFAL-2026-08-27-01: Skyrim's SLSF2_Soft_Lighting gate crosses the boundary without its slot-2 mask on the tint family — 50.3% of every soft-lighting property in the vanilla game, and the shader substitutes an unauthored vec3(1.0)

**Labels**: high, nifal, nif-parser, nif, shaders, game:skyrim, bug
**Filed**: 2026-08-27 via /audit-publish

**Severity**: HIGH
**Dimension**: Shader-flags / texture sets (Dim 8)
**Tier Violated**: `no-leak` (the authored slot-2 texture reaches only one of the two roles the wire format multiplexes onto it) + `no-fabrication` (the consumer invents the missing half)
**Game Affected**: Skyrim SE / LE (the `TextureSlotLayout::Skyrim` arm is the only one that sets these gates)
**Location**: `crates/nif/src/import/material/slot_role.rs` (`slot_to_role`, the slot-2 Skyrim/Starfield arm), `crates/nif/src/import/material/dedicated_shader.rs` (`apply_bs_lighting_shader` gate extraction), `byroredux/src/material_translate.rs:523-525` + `byroredux/src/cell_loader.rs:267-275` (`pack_imported_material_flags`), `crates/renderer/shaders/triangle.frag:2659-2663` (the substituted mask), `crates/renderer/shaders/include/lighting.glsl:92-108` (`bethesdaDiffuseLightFactor`)
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-27.md` — NIFAL-2026-08-27-01

## Description

`apply_bs_lighting_shader` reads Skyrim's SLSF2 bits 25/26/27 into
`MaterialInfo.{soft,rim,back}_lighting` **unconditionally for the Skyrim slot
layout**, and `pack_imported_material_flags` turns them into
`MAT_FLAG_SOFT_LIGHTING` / `_RIM_` / `_BACK_` on the canonical `Material`.
Separately, `slot_to_role` decides where the property's slot-2 texture lands,
and its very first test is the tint family:

```rust
// crates/nif/src/import/material/slot_role.rs (slot-2 Skyrim/Starfield arm)
(TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield, 2) => {
    if tint_family {
        Some(TextureRole::Tint)
    } else if context.glow_map {
        Some(TextureRole::Emissive)
    } else if context.soft_lighting || context.rim_lighting {
        Some(TextureRole::LightingMask)
    } else {
        None
    }
}
```

So on a `FaceTint` (4) / `SkinTint` (5) / `HairTint` (6) property the authored
`*_sk.dds` becomes `Tint` and `MaterialTextureSet::lighting_mask` stays
`None` — while `MAT_FLAG_SOFT_LIGHTING` crosses regardless. The renderer then
fills the hole itself:

```glsl
// crates/renderer/shaders/triangle.frag:2659-2663
vec3 lightingMask = vec3(1.0);
if (mat.lightingMaskMapIndex != 0u) {
    lightingMask = texture(
        textures[nonuniformEXT(mat.lightingMaskMapIndex)], sampleUV).rgb;
}
```

and `bethesdaDiffuseLightFactor` mixes at that weight:

```glsl
// crates/renderer/shaders/include/lighting.glsl:96-107
if ((mat.materialFlags & MAT_FLAG_SOFT_LIGHTING) == 0u) return vec3(front);
float width = mat.subsurfaceRolloff > 0.0
    ? mat.subsurfaceRolloff : mat.lightingEffect1;
width = clamp(width, 0.0, 4.0);
float wrapped = max((rawNdotL + width) / (1.0 + width), 0.0);
return mix(vec3(front), vec3(wrapped), clamp(lightingMask, 0.0, 1.0));
```

There is no `material_kind` gate on that call — it runs from the main lit loop
(`triangle.frag:2856`) and from every `shadowableLightRadiance` call site — so
the whole tint-family surface takes the wrapped lobe at full weight.

## Evidence

Census over `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa` (22,047 NIFs,
`100 <= bsver < 130`), classifying each `BSLightingShaderProperty` that sets
SLSF2 bit 25 by whether its slot-2 texture actually reaches
`TextureRole::LightingMask`:

```
BSLightingShaderProperty props = 73125
SLSF2_Soft_Lighting            = 8058
  routed to LightingMask       = 3975
  tint-family (slot 2 -> Tint) = 4054      <- gate crosses, mask does not
  slot 2 empty                 =   24
  Glow_Map wins slot 2         =    5
  UNMASKED with lighting_effect_1 > 0 = 4083   (all of them)
SLSF2_Rim_Lighting  = 256
SLSF2_Back_Lighting = 2063   (36 with an empty slot 7)
```

Every one of the 4,054 tint-family cases has a **non-empty** slot 2 (the bucket
is `else if tint_family` after the empty-slot test) and
`lighting_effect_1 == 0.4`, so `width = 0.4` and the substituted mask is fully
load-bearing: `wrapped = (N·L + 0.4)/1.4` replaces `max(N·L, 0)` across the
entire surface. Representative paths, transcribed from the census:
`meshes\actors\character\facegendata\facegeom\skyrim.esm\0006765a.nif`
(`ty=4`, slot 2 `Actors\Character\Male\MaleHead_sk.dds`),
`meshes\armor\hide\f\cuirassheavychieftain_1.nif` (`ty=5`, slot 2
`textures\actors\character\female\FemaleBody_1_sk.dds`),
`meshes\clothes\archmage\m\archmagerobesm_1.nif` (`ty=5`).
`slot_role.rs`'s own #2694 comment records that **3158/3158** vanilla FaceTint
properties populate slot 2, so the population is total, not incidental.

For contrast, the FO4/BGSM lane genuinely has no mask to lose —
`forward_bgsm_rim_subsurface` (`byroredux/src/asset_provider/material.rs:77-97`)
sets `soft_lighting` from `bgsm.subsurface_lighting` and BGSM authors no
companion texture — so the unit default is defensible *there* and only there.

## Impact

Visual, no crash, but the blast radius is every NPC face and every skin-tinted
body/armour surface in Skyrim, plus the ~3,975 correctly masked non-tint
materials are shaded by a different rule than their tint-family neighbours. It
is a behaviour change introduced in the 08-24→08-27 window (before the
soft/rim/back work the lobe did not exist at all), and nothing in the test suite
can see it: `slot_to_role`'s own new test
(`skyrim_feature_flags_route_soft_rim_and_back_lighting_maps`) builds its
context with `skyrim(0, …)`, i.e. shader type 0, which is exactly the arm that
*does* work.

The remediation is a ground-truth question the audit deliberately does not
answer by guessing. nif.xml documents slot 2 as
`Glow(SLSF2_Glow_Map)/Skin/Hair/Rim light(SLSF2_Rim_Lighting)`
(`/mnt/data/src/reference/nifxml/nif.xml:6313`) — it attributes the slot to
Skin/Hair **and** to rim light, and does not mention soft lighting at all. So one
texture legitimately serves two simultaneous roles on the tint family, and the
canonical `MaterialTextureSet` model (one slot → at most one role) cannot
currently express that. What is not defensible either way is the present state:
the gate crosses the boundary while its mask does not, and the shader silently
picks the *maximally active* substitute.

## Related

- `#2694` (the fix that gave the tint family slot 2)
- `#3068` (the fix that stopped slot 2 becoming self-illumination without a flag)
- REN-2026-08-27 rim-lobe clamp floor — the sibling defect in the same three new
  lobes; do not merge the two: that one is the exponent, this one is the mask

## Suggested Fix

Decide the coupling explicitly at the boundary rather than at the shader default.
Either (a) let slot 2 fill **both** `tint` and `lighting_mask` when the tint
family also sets `Soft_Lighting`/`Rim_Lighting` — the wire format multiplexes it,
so `slot_to_role` returning a single role is the model gap, not the data — or
(b) clear `MaterialInfo.{soft,rim}_lighting` when no slot-2 texture reached
`LightingMask` on a Skyrim property, so the gate and its mask cross together and
the shader's `vec3(1.0)` stays reachable only for the BGSM lane that genuinely
has no mask. Whichever is chosen, extend
`skyrim_feature_flags_route_soft_rim_and_back_lighting_maps` to cover
`shader_type` 4/5/6 — the arm that carries 50% of the content.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other slot arms — FO4/FO76 slot-2 tint arms, slot-7 back-lighting)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
