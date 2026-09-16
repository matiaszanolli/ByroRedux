# #4422: REN-2026-09-16-D7-01: The detail role multiplies albedo by 2 × an sRGB-decoded sample, so vanilla Skyrim's own "blank" detail map darkens every NPC face to ≈11%

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4422
- **Labels**: high,renderer,shaders,game:skyrim,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: HIGH. The surface colour is wrong on every Skyrim NPC head, the
  secondary-ray albedo is wrong with it, and nothing in the draw path compensates.
- **Dimension**: Material Table (texture-role consumption)
- **Game Affected**: Skyrim (live on 3,149 of 3,149 vanilla FaceGeom head shapes).
  Oblivion, FO3 and FNV have no live occurrence: the census below finds 0 `detail`
  roles in their vanilla mesh archives.
- **Location**:
  - `crates/renderer/shaders/triangle.frag`: the `mat.detailMapIndex` block,
    `albedo *= detailSample * 2.0`.
  - `crates/renderer/shaders/include/ray_hit.glsl`: `rayHitAlbedo`,
    `rgb *= detailSample * 2.0`.
  - The colour-space choice is in `map_secondary_texture_handles`
    (`byroredux/src/asset_provider/texture.rs`): `detail: slot(&textures.detail, srgb)`.
  - The producer is the `(TextureSlotLayout::Skyrim, 3)` arm of `slot_to_role`
    (`crates/nif/src/import/material/slot_role.rs`): FaceTint slot 3 →
    `TextureRole::Detail` (#2694).
- **Status**: NEW. The ×2 dates from #399 (`c2cfcaa35`, 2026-04-19) and was
  written for Oblivion's `NiTexturingProperty` slot 2. The first live producer is
  #2694's FaceTint routing. The sRGB transfer for the role was made explicit in
  `a0f75fc56`; before that, every DDS was uploaded as `*_SRGB` anyway.
- **Description**:
  - **The shader's stated invariant.** The comment says the modulation is
    centred on 1.0 "so a 0.5 grey detail sample is a no-op".
  - **The upload breaks it.** The texture is uploaded with an sRGB view, so a
    texel is linearised before the ×2. The identity point therefore sits at a
    *linear* 0.5, which is encoded ≈188/255, not 128/255.
  - **The vanilla content breaks it further.** Skyrim's neutral detail texture is
    `blankdetailmap.dds`. By name it is the texture an NPC with no complexion
    detail gets, and it is a uniform (65, 64, 65)/255.
  - **Result.** Its linear value is ≈0.053, so the shader multiplies the face
    albedo by ≈0.106. The authored complexion maps (`maleheaddetail_*`,
    `femaleheaddetail_*`) average the same ≈0.25 encoded, so no vanilla face
    escapes the darkening.
  - **The ×2 was never right for this role, even ignoring the decode.** Applied
    in encoded space, it would still halve the face (2 × 0.255 = 0.51). Skyrim's
    FaceTint detail combine therefore does not have its neutral point at 0.5.
- **Evidence** (census, `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa`, every
  imported shape; textures from `Skyrim - Textures0..8.bsa`, fully BC1-decoded):
  - 78,146 imported shapes, of which 3,149 carry a `detail` role across 18 distinct
    paths, and 0 carry `dark`.
  - Every one of the 3,149 is `material_kind == 4`, inside
    `meshes\actors\character\facegendata\facegeom\`.
  - `textures\actors\character\male\blankdetailmap.dds` (256², DXT1):
    **65,536 of 65,536 texels = (65, 64, 65)**. It is referenced by 1,616 shapes,
    plus the Argonian and Khajiit copies (77 more), which are byte-identical.
  - `maleheaddetail_rough01.dds` (512², DXT1): mean (63.9, 61.9, 62.6)/255, and
    145,548 of 262,144 texels are exactly (65, 64, 65).
  - Shape-weighted encoded-luma histogram of all 3,149 detail textures: 3,140 in
    [0.2, 0.3) and 9 in [0.3, 0.4).
  - **Resulting shader multiplier** (`2 × srgb_decode(mean)`): 0.080–0.157. It is
    0.106 for the blank map.
  - Oblivion (9,545 NIFs, 41,132 shapes), FO3 (10,989 / 32,118) and FNV
    (14,881 / 48,982): **0** `detail` roles.
- **Impact**:
  - Every vanilla Skyrim NPC face (FaceGeom head) renders at about a tenth of its
    diffuse before lighting, on both the raster path and RT reflections/GI.
  - With D7-02 applied on top, the result is near-black.
  - The `DBG_BYPASS_DETAIL` flag (`BYROREDUX_RENDER_DEBUG=0x2`) removes this
    factor on the primary path only.
- **Related**: #399, #2694, REN-2026-09-16-D7-02, REN-2026-09-16-D6-01,
  NIFAL-D8-2026-09-16-05 (the colour-space test pins `detail` as sRGB, which
  freezes this choice).
- **Suggested Fix**:
  1. Do not pick a new constant by eye. First find a source for Skyrim's
     FaceTint detail combine: its scale, its neutral value, and whether it
     operates in encoded space.
  2. Encode that combine at the NIFAL boundary, as a canonical detail-blend
     parameter or a distinct role, rather than as a Skyrim branch in the shader.
  3. Whatever is chosen, pin it with a CPU-side test asserting that the measured
     vanilla neutral (65/255) maps to an albedo multiplier of 1.0 under the
     chosen transfer function and scale.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SHADER-SYNC**: `triangle.frag` and `include/ray_hit.glsl` (`rayHitAlbedo`) change in lockstep; `.spv` recompiled with plain `-V`
- [ ] **TESTS**: A regression test pins this specific fix
