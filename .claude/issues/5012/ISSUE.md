# PAR-D5-2026-09-29-01: Spec-disabled BGSM roughness has two contradictory contracts

**Labels**: low,bug,import-pipeline,nifal,game:fo4,game:fo76

**Source report**: `docs/audits/AUDIT_PARSERS_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**: `byroredux/src/asset_provider/material/merge.rs:647`, `byroredux/src/asset_provider/material/merge.rs:658-659`, `byroredux/src/asset_provider/material/merge.rs:1102-1134`; `byroredux/src/asset_provider/tests/bgsm_merge.rs:1409-1462`
- **Status**: NEW (owner overlap `/audit-nifal`)
- **Trigger Input**: vanilla BGSMs with `specular_enabled = false`, `smoothness >= 1.0` and no gloss (`smooth_spec`) map.
- **Description**: the merge states two rules for the same case.
  - **What the code says.** The #4836/#4941 comment on the disabled arm says "Roughness and the tint are left to whatever the NIF side classified" (`:647`). That arm derives no roughness.
  - **What actually happens.** The #3639 near-mirror fallback, `if leaf.smoothness >= 1.0 && material.textures.smooth_spec.is_none() { roughness_override = Some(NEAR_MIRROR_NEUTRAL_ROUGHNESS) }`, runs after the chain walk with no `specular_enabled` check. Its stated premise is "smoothness 1.0 lowered roughness to the 0.04 floor above", which holds only on the enabled arm.
  - **What the test pins.** `bgsm_specular_disabled_zeroes_specular_and_keeps_matte_roughness` pins `Some(0.5)` and calls it "the matte neutral default". So the spec-off outcome depends on an unrelated fallback firing.
- **Evidence** (probe `bgsm-specoff`, vanilla material archives):
  ```
  Fallout4 - Materials.ba2   : 6,616 BGSM; spec off 467; off & smoothness>=1 304; + no smooth_spec map 247 (219 without a template)
  SeventySix - Materials.ba2 : 25,888 BGSM; spec off 653; off & smoothness>=1 623; + no smooth_spec map 619 (603 without a template)
  e.g. props\comicsandmagazineshighres\backside\comicbackblue.bgsm, setdressing\paintingsgeneric\paintinggeneric13.bgsm, decals\stain11b.bgsm
  ```
  One authoring intent (specular off) produces two outcomes:
  - 247 FO4 and 619 FO76 files get roughness 0.5.
  - The other spec-off files keep the NIF-side value.
- **Impact**: the visible effect is small today, for two reasons:
  - `metalness_override = 0` keeps these materials out of the `metalness > 0.3` environment-reflection gate (`triangle.frag:2963-2966`).
  - A zeroed `specStrength` removes direct highlights.

  Any roughness consumer that specStrength does not scale still sees 0.5. The contract is also self-contradictory, which the renderer audit's #4836 already tripped on: its evidence reads `roughness_override=Some(0.5)` as "the same treatment as roughness".
- **Related**: #4654, #4836, #4941, #3639, #3905
- **Suggested Fix**:
  - Choose one rule for the disabled arm (an explicit matte roughness, or NIF-side) and set it inside that branch.
  - Gate the #3639 fallback on `leaf.specular_enabled`.
  - Align the comment and the test with the chosen rule.

**Validated at HEAD 9fcfdc3fc**: `byroredux/src/asset_provider/material/merge.rs` still has the "Roughness and the tint are left to whatever the NIF side classified" comment on the `!leaf.specular_enabled` arm, and the post-walk `if leaf.smoothness >= 1.0 && material.textures.smooth_spec.is_none()` near-mirror fallback has no `specular_enabled` gate; `bgsm_specular_disabled_zeroes_specular_and_keeps_matte_roughness` still pins the 0.5 "matte neutral default".

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
