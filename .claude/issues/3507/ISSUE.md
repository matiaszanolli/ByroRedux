# Issue #3507: BSLightingShaderProperty.texture_clamp_mode is parsed then dropped — 7.9% of vanilla FO4 lit materials lose their authored texture-address mode

**Filed**: 2026-08-27 · **Source**: `docs/audits/AUDIT_FO4_2026-08-27.md`

- **Severity**: MEDIUM
- **Dimension**: 5 (FO4 shader flags & BGSM PBR routing) ∩ NIFAL translatable-field drop
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs` (`apply_bs_lighting_shader`); field parsed in `crates/nif/src/blocks/shader.rs` (`BSLightingShaderProperty::texture_clamp_mode`); sibling consumer that *does* work at `dedicated_shader.rs:512-518` (`BSEffectShaderProperty`)
- **Source**: `docs/audits/AUDIT_FO4_2026-08-27.md` — finding `FO4-2026-08-27-D5-01`

## Description

nif.xml declares `Texture Clamp Mode` as an unconditional `BSLightingShaderProperty` field for every Skyrim-and-later BSVER (`nif.xml:6597`, no `vercond`), and the parser reads it into `BSLightingShaderProperty::texture_clamp_mode: u32`. `apply_bs_lighting_shader` — the importer arm that owns this property class for FO4, Skyrim, FO76 and Starfield — copies `uv_offset`, `uv_scale`, `alpha`, `specular_color`, `specular_strength`, `glossiness` and the whole FO4 PBR/DLC tail out of the block, but never touches `texture_clamp_mode`:

```rust
info.specular_color = shader.specular_color;
info.specular_authored = true;
info.specular_strength = shader.specular_strength;
info.glossiness = shader.glossiness;
info.uv_offset = shader.uv_offset;
info.uv_scale = shader.uv_scale;
info.has_uv_transform = true;
info.alpha = shader.alpha;
```

The three sites that *do* write `MaterialInfo::texture_clamp_mode` are all legacy: `apply_texturing_property` (`NiTexturingProperty`, Oblivion/FO3/FNV, `legacy_properties.rs:272-276`), `apply_pp_lighting_property` / `apply_no_lighting_property` / `apply_misc_shader_properties` (`legacy_properties.rs:405-407`, `:525-527`, `:596-598`, `:609-611`), and the `BSEffectShaderProperty` arm (`dedicated_shader.rs:518`). FO4 authors none of those on ordinary lit architecture, so `MaterialInfo::texture_clamp_mode` keeps its `3` (`WRAP_S_WRAP_T`) default (`import/material/mod.rs:1075`, `:1202`) and is carried verbatim through `into_imported_material` (`mod.rs:1455`) → `translate_material` (`material_translate.rs:561`) → `resolve_texture_with_clamp` (`cell_loader/spawn/mesh_instance.rs:648-653`, `scene/nif_loader.rs:930`), which is the *only* input to the renderer's 4-sampler `TexClampMode` table (`crates/renderer/src/texture_registry.rs:171-183`).

The **BGSM half is dropped too**: `BaseMaterial` parses `tile_u`/`tile_v` from the tile-flags word (`crates/bgsm/src/base.rs:173-175`), and `merge_external_material` forwards `u_offset`/`v_offset`/`u_scale`/`v_scale` into `uv_offset`/`uv_scale` (`byroredux/src/asset_provider/material.rs:1527-1528`, `:1727-1728`) but never maps the tile pair onto `texture_clamp_mode`. So neither of FO4's two authoring channels reaches the sampler.

## Evidence

Measured against both vanilla mesh archives (probe walked every `.nif`, downcast every `BSLightingShaderProperty`, skipped `material_reference` stubs, histogrammed `texture_clamp_mode`):

```
files parsed 159 866, non-stub BSLightingShaderProperty 810 489
  texture_clamp_mode 0 (CLAMP_S_CLAMP_T):  57 365
  texture_clamp_mode 1 (CLAMP_S_WRAP_T):        41
  texture_clamp_mode 2 (WRAP_S_CLAMP_T):     6 459
  texture_clamp_mode 3 (WRAP_S_WRAP_T):    746 624
```

63 865 / 810 489 = **7.88%** author a non-default mode; zero values fell outside the `0..=3` enum, which independently confirms the field is being read at the right stream position. Sample authors are exactly the asset classes `#610` was filed about — architecture wall-kit trim and their LOD siblings: `meshes\interiors\building\brick\med_wallkit\bldbrickmdwalltophole05.nif`, `meshes\architecture\buildings\hightech\lobby\hitextlobbycornerccapbottom01.nif`, `meshes\architecture\buildings\neoclassical\nca1x1trime01.nif`, `meshes\lod\buildings\decokit\decomaina1x2wall01_lod_0.nif`.

BGSM side, over `Fallout4 - Materials.ba2`:

```
materials parsed 6 899 (0 errors)
  tile_u && tile_v (WRAP/WRAP): 6 862
  tile_u == false:                  31
  tile_v == false:                  28
  both false (CLAMP/CLAMP):         22
    e.g. materials\landscape\plants\vinedecals.bgsm, materials\props\lab\beakers.bgem,
         materials\landscape\trees\elmprewaratlas.bgsm, materials\landscape\grass\meadowgrassobj01.bgsm
```

## Impact

Every FO4 surface whose UVs leave `[0,1]` — atlas-packed foliage cards, decals, wall-kit trim pieces, LOD atlases — samples with REPEAT instead of the authored CLAMP, so the atlas neighbour bleeds across the card edge instead of the border texel holding. The failure is invisible where UVs stay inside the unit square (most walls), which is why it survived: it shows up as a thin wrong-colour fringe on foliage/decal edges, not as a broken scene. Blast radius is FO4 **and Skyrim/FO76/Starfield** — the same import arm owns all four — but this report scopes only the FO4 measurement. Not a regression: this path has never consumed the field.

## Related

- `#610` — the identical fix for `NiTexturingProperty` + `BSEffectShaderProperty`
- `#2328` / FO3-D1-06 — the `_consumed` precedence gate the new writer should reuse
- `#2571` / OBL-D5-01 — put `texture_clamp_mode` on canonical `Material`, which is what makes the drop observable end-to-end
- `FO4-2026-08-27-D5-02` (filed separately) — the divergent default across tiers
- **Distinct from** `OBL-2026-08-27-01` (`docs/audits/AUDIT_OBLIVION_2026-08-27.md`, HIGH), which is the *legacy* `NiTexturingProperty` nibble mis-decode: that report correctly notes FO4 is unaffected by it "because [it carries] no `NiTexturingProperty`" — which is exactly why FO4's clamp authoring has to come from the lit shader property, and exactly the arm that never reads it. The two findings are complementary halves of the same field's coverage and should be fixed together.

## Suggested Fix

In `apply_bs_lighting_shader`, beside the existing `info.uv_offset`/`info.uv_scale` copy, add:

```rust
if !info.texture_clamp_mode_consumed {
    info.texture_clamp_mode = shader.texture_clamp_mode as u8;
    info.texture_clamp_mode_consumed = true;
}
```

Map the BGSM `tile_u`/`tile_v` pair onto the same field in `merge_external_material` so both FO4 authoring channels reach the sampler.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers) — Skyrim/FO76/Starfield ride the same import arm
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
