# #4425: FO4-D2-2026-09-16-01: The BGEM merge replaces the NIF's `BsEffectShaderData` wholesale, erasing the authored palette-remap bits; the BGEM's own bits are dropped whenever the NIF filled the LUT — 362 vanilla effect shapes lose their palette remap

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4425
- **Labels**: high,nifal,import-pipeline,game:fo4,legacy-compat,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_FO4_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: HIGH (wrong colour/alpha on vanilla effect content; wrong effect flags in `Material.effect_shader_flags` out of `translate_material`)
- **Dimension**: 2 (BGSM/BGEM consumption)
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:1201-1211`: `material.effect_shader = Some(BsEffectShaderData { falloff…, soft…, lit…, lighting_influence, ..Default::default() })`
  - `byroredux/src/asset_provider/material/merge.rs:1026-1048`: BGEM palette capture gated on `material.textures.greyscale_lut.is_none()`, with no else branch
  - Consumers: `byroredux/src/cell_loader.rs:207-235` (`pack_effect_shader_flags` reads `effect_palette_color`/`_alpha` from `effect_shader`) and `:338-345` (`pack_imported_material_flags` reads `bgsm_greyscale_lut_*`). Shader gate: `crates/renderer/shaders/triangle.frag:1045-1060`.
- **Status**: NEW. This is the BGEM half of #3898. #3898's body noted "The BGEM sibling … has the same `greyscale_lut.is_none()` shape", but only the BGSM arm was fixed. The wholesale overwrite has been in place since the BGEM soft-falloff wiring and survived the #3857 split.
- **Description**: On an inline FO4 `BSEffectShaderProperty` (BSVER 130), the importer fills `ImportedMaterial.effect_shader` from the block (`shader_data.rs::capture_effect_shader_data`). That includes `effect_palette_color`/`effect_palette_alpha` from SLSF1, and the importer also fills the `greyscale_lut` role from the inline greyscale texture. For effect shaders, the palette bits exist **only** on `effect_shader`; `ImportedMaterial.bgsm_greyscale_lut_*` stays false.

  The BGEM arm then does two things:
  1. It replaces `effect_shader` with a new struct built from BGEM falloff/soft/lit/lighting-influence plus `..Default::default()`. Both palette bits drop to `false`, along with every other field the BGEM arm does not re-author.
  2. It skips its own palette capture because `greyscale_lut` is already `Some`. Unlike the BGSM arm after #3898, it has no `nif_supplied_greyscale_lut` branch.

  After the merge, neither `pack_effect_shader_flags` nor `pack_imported_material_flags` sets `EFFECT_PALETTE_COLOR`/`_ALPHA`. The shader's palette branch never runs, even though the LUT index is bound.
- **Evidence**: census over `Fallout4 - Meshes.ba2` + `Fallout4 - MeshesExtra.ba2`, shapes whose effect shader names a `.bgem` (1,292; 1 unresolved). The last column simulates the merge exactly as the code reads today.

  | NIF LUT | NIF bits | BGEM LUT | BGEM bits | post-merge palette | shapes |
  |---|---|---|---|---|---|
  | yes | **yes** | yes | **yes** | **off** | **360** (e.g. `bldglasschunk07.nif` → `glasstile01.bgem`, `fancylighton01.nif` → `glasstile01on.bgem`) |
  | yes | **yes** | yes | no | **off** | 2 (`mistlargerounddusty01.nif`, `blackglowfill01.nif`) |
  | no | yes | no | yes | off (no LUT anywhere) | 1 |
  | yes | no | yes/no | no | off | 45 |
  | no | no | yes/no | no | off | 883 |

  **362** shapes have a bound LUT and an authored remap but render with no remap, and the NIF alone would have remapped every one of them. None of the 362 can take the glass path: `bgem_uses_glass_behavior` requires both palette bits to be off (`asset_provider/material/mod.rs:274-280`), so they stay on the effect-shader branch that consumes the bits.
- **Impact**: Palette-driven FO4 effect surfaces render the raw greyscale source instead of the authored gradient: glass tiles, lit fixture glow cards, mist volumes. Colour and alpha (for `_ALPHA`) are wrong. Unlike #3897/#3898, the NIF-only path gets this right, so this is a regression introduced by the merge itself.
- **Related**: #3898, #3897, #2643, #1580, #2108, #4286, #4402 (open, precedence of the BGSM-arm OR)
- **Suggested Fix**: Merge into the existing `effect_shader` instead of replacing it. Update only the BGEM-authored falloff/soft/lit/influence fields, and start from `Default` only when the NIF had none. Mirror #3898 in the BGEM arm: when the NIF already won the `greyscale_lut` slot, OR the BGEM's `grayscale_to_palette_color`/`_alpha` into the palette state rather than skipping it. Add a merge test with an inline effect LUT, NIF palette bits and a BGEM that authors the same bits, asserting that the packed flags keep `EFFECT_PALETTE_COLOR`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
