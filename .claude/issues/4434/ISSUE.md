# #4434: FO4-D2-2026-09-16-03: `RefrTextureOverlay::fill_from_bgsm` routes BGSM's named roles back through `BSShaderTextureSet` slot numbers, so on FO4 a BGSM displacement map becomes the palette LUT and a BGSM glow map becomes the tint mask

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4434
- **Labels**: low,nifal,import-pipeline,game:fo4,legacy-compat,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_FO4_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW (latent: 0 vanilla references to the affected BGSMs)
- **Dimension**: 2 (BGSM texture roles), with 8 (REFR overlay spawn)
- **Location**:
  - `byroredux/src/cell_loader/refr.rs:251-374` (`fill_from_bgsm`): `displacement_texture` and `greyscale_texture` → `self.height`; `glow_texture` → `self.glow`; `inner_layer_texture` → `self.inner`
  - Consumer: `byroredux/src/cell_loader/spawn/mesh_instance.rs:333-346` and `:369-378` (`pick(2, o.glow, …)`, `pick(3, o.height, …)`), `:399` and `:408` (`pick(6, o.inner, …)`)
  - Table: `crates/nif/src/import/material/slot_role.rs:361-367` (FO4 slot 2 is `Tint` on the tint family), `:388-395` (FO4/FO76 slot 3 is always `GreyscaleLut`), `:450-452` (FO76 slot 6 is `Specular`)
- **Status**: NEW
- **Description**: `merge_external_material` maps each BGSM field straight to a named role: displacement → `height`, greyscale → `greyscale_lut`, glow → `emissive`, inner layer → `inner_layer`. The overlay resolver instead writes the same fields into wire-slot-named fields, and `slot_to_role` decides at spawn time what the slot means for the host's layout and shader type. Consequences:
  - On FO4/FO76, a BGSM `displacement_texture` resolves to **GreyscaleLut**, never `Height`. It also wins slot 3 over the BGSM's real `greyscale_texture`, because first-wins fills displacement first.
  - On an FO4 tint-family host, a BGSM `glow_texture` resolves to **Tint** and overrides the NIF's `_sk` mask.
  - On an FO76-layout host, `inner_layer_texture` would resolve to **Specular**.

  The overlay result wins in `resolve_effective` over the correctly merged #4290 material. `docs/engine/nifal.md` says "The source-specific slot vocabulary is gone before runtime spawning", and this path is the exception.
- **Evidence**:
  - BGSM corpus: 58 BGSMs author `displacement_texture` (all tessellation `_nvtess` materials). **1** authors both displacement and greyscale (`materials\actors\mirelurk\fishingnet_nvtess.bgsm`). That contradicts the test doc at `byroredux/src/cell_loader/refr_texture_overlay_tests.rs:643-644` ("real content doesn't do" this). 4 hair BGSMs author `glow_texture`.
  - Reachability: a raw scan of all 226,009 NIFs in the 8 mesh archives finds 0 references to `_nvtess.bgsm`, and `Fallout4.esm` plus the three story DLC ESMs contain no `_nvtess` string, so no vanilla overlay reaches a displacement BGSM. The test `fill_from_bgsm_forwards_every_bgsm_texture_role` asserts only overlay field contents, never the resolved role.
  - A related latent issue on the merge side: `displacement → textures.height` feeds `parallaxMapIndex`, and the POM branch (`triangle.frag:273`) has no material-kind gate. A tessellation displacement map would be ray-marched as a parallax height field with the default 0.04 scale. The same 0-reference measurement applies.
- **Impact**: None on vanilla content. A mod or modded MSWP/TXST that swaps in a tessellation or hair BGSM would bind the displacement map as a palette LUT, or a glow map as a skin-tint mask. That is a semantic change, not just a texture swap: the exact class #2695 set out to remove.
- **Related**: #2695, #2594, #3234, #3187, #4287, #4290, #4400 (open), #2708
- **Suggested Fix**: Give `RefrTextureOverlay` named-role fields for BGSM/BGEM output (or carry a `MaterialTextureSet` for them) and resolve those without `slot_to_role`. Keep wire-slot fields only for TXST/XTXR. #2708 already asks for one shared "resolve external material → roles" helper; this is the concrete reason to build it. Separately, decide whether BGSM `displacement_texture` (a tessellation input) belongs in the POM `height` role at all.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
