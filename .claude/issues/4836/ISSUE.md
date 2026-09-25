# #4836 — REN-D6-2026-09-24-01: `specular_enabled = false` is honoured for specular colour/strength/roughness/glossiness but not for the derived `metalness_override` — for `pbr = true` BGSMs the merge emits a metal with zero specular

**Labels**: bug,import-pipeline,medium,game:fo76,nifal
**Filed from**: docs/audits/AUDIT_RENDERER_2026-09-24.md (audited `main` @ `6c5555c70`)

- **Severity**: MEDIUM (nominally HIGH — "wrong/divergent `Material` out of NIFAL translation" — held one step below because the affected population is `pbr = true` BGSMs only)
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/asset_provider/material/merge.rs` — `merge_bgsm_arm` (`let metalness = if leaf.pbr {…}` / `material.metalness_override = Some(metalness)` / `if leaf.specular_enabled { roughness_override }`; chain loop `if bgsm.specular_enabled {…} else { zero }`).
- **Status**: NEW. Adjacent to closed #4654, whose body and regression test cover roughness, specular and glossiness only.
- **Description**: #4654 gated three consumers of the BGSM specular block on `specular_enabled`. `metalness_override` is derived from that same block (`bgsm_metalness(spec*mult, leaf.pbr)`) and is not gated. For `pbr = true` the derivation is F0 luminance; the authoring defaults (white, mult 1) give `metalness = 1.0`: a full conductor whose specular colour and strength were just zeroed.
- **Evidence**: scratch test (`pbr: true, specular_enabled: false, specular_color [1,1,1], specular_mult 1, smoothness 1`) through `merge_external_material` then `translate_material`: `metalness_override=Some(1.0) roughness_override=Some(0.5) specular_strength=0 specular_color=[0,0,0]`; `Material metalness=1 … specular_strength=0`. Shader consequence (`lighting.glsl` `shadowableLightRadiance`): `diffuseBrdf … * (1 - metalness)` with metalness 1 and strength/colour 0 leaves only the `metallicAmbient` fill.
  - Census: `Fallout4 - Materials.ba2` 6,616 BGSMs, 467 with `specular_enabled = false`, 0 of those `pbr=true`, 0 with metalness > 0.5. `SeventySix - Materials.ba2` 25,888 BGSMs, 653 spec-off, 624 `pbr=true`, **405** with metalness > 0.5 (all white spec × mult 1: posters, magazines, decals, beards, signage, lampshades).
- **Impact**: FO76 material files rendered through the loose-mesh path, and modded or CC FO4 `pbr = true` materials with specular disabled, render as ambient-only conductors with no direct diffuse or specular. Before #4654 the same inputs gave near-mirror chrome. Vanilla FO4/FO3/FNV/Skyrim/Oblivion/Starfield are unaffected. Needs a device to confirm visually.
- **Related**: #4654, #1476 (`bgsm_metalness`), #1591.
- **Suggested Fix**: When the leaf's `specular_enabled` is false, do not derive metalness from the disabled block: leave the NIF-side classification in `metalness_override` (the same treatment as `roughness_override`) or set 0.0. Add a `pbr: true` case next to `bgsm_specular_disabled_zeroes_specular_and_keeps_matte_roughness`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
- [ ] **CANONICAL-BOUNDARY**: The fix stays at the NIFAL parser→`Material` boundary (`translate_material` / merge) — never pushed into shaders or re-derived at render time. See `/audit-nifal`.
