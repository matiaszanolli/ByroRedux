# #2700: FO4-D2-01: `is_pbr` for BGSM was narrowed from "any resolved BGSM" to "an explicit `pbr` bit", measured at 0 of 6,616 vanilla BGSMs - reversing #1352 across 100% of vanilla content, with stale tests and comments still asserting the old c...

- **Severity**: MEDIUM
- **Dimension**: 2 — BGSM consumption / NIFAL PBR routing
- **Location**: `byroredux/src/asset_provider/material.rs:19-25` (`bgsm_uses_pbr_bsdf`), `:808`
- **Status**: NEW (introduced `a0f75fc5`, 2026-08-11)
- **Description**: `a0f75fc5` replaced `material.is_pbr = true;` (the #1352 fix: any successful BGSM resolve routes through the Disney diffuse lobe) with `material.is_pbr |= bgsm_uses_pbr_bsdf(&resolved)` — set only when some file in the resolved chain carries the `pbr` bit. The design argument in the new comment ("BGSM is a container, not a BRDF declaration") is coherent, and the blast radius is narrower than it first looks: `MAT_FLAG_PBR_BSDF` gates only the *diffuse lobe* choice, while the GGX specular and `F0 = mix(0.04, albedo, metalness)` path is not gated on it (`crates/renderer/shaders/include/lighting.glsl:183-195`, `crates/renderer/shaders/triangle.frag:2426-2435`). What makes this a finding is the verification state around it, not the shading choice.
- **Evidence** (measured, not cited):
  ```
  cargo run --release --example dump_bgsm -p byroredux-bgsm -- \
      ".../Fallout 4/Data/Fallout4 - Materials.ba2" ".bgsm"
  → 6,616 BGSMs scanned, 0 with pbr=true
  ```
  `MAT_FLAG_PBR_BSDF` is therefore unreachable for every vanilla FO4 material. Three artefacts still assert the pre-`a0f75fc5` contract: (1) `byroredux/src/asset_provider/tests.rs:820-856` asserts *"#1352: from_bgsm now implies is_pbr regardless of bgsm.pbr"* and passes only because it is a mirror test (FO4-D2-03); (2) the `/audit-fo4` Dimension 5 pin names *"a BGSM material falling back to Lambert"* as **the** FO4-specific regression; (3) #2607's premise (BGSM subsurface/rim never reaching the Disney lobe) changes, since that lobe is now dead for all vanilla FO4.
- **Impact**: Every vanilla FO4 surface's diffuse lobe changed shading model in an untested direction, and no test in the tree distinguishes the old contract from the new one. If intended, the pins are stale and will send the next sweep to "fix" it back; if it was collateral from the colour-space refactor `a0f75fc5` (whose commit message never mentions PBR routing), it is an unnoticed whole-game rendering regression.
- **Related**: #1352, #2607, #2609, FO4-D2-03.
- **Suggested Fix**: Decide the contract explicitly. Then replace the mirror test with one that calls `merge_external_material` against a fixture provider and asserts the chosen `is_pbr` value, fix its doc comment, and correct the `/audit-fo4` Dimension 5 pin.

---
**Source**: `docs/audits/AUDIT_FO4_2026-08-12.md` (finding `FO4-D2-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

