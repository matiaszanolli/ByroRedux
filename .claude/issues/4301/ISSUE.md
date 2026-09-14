# #4301: REN-2026-09-14-D6-02: #3901 lets a flipbook replace the `Normal` / `Height` texture index, but the draw's `normal_has_alpha` gate still describes the spawn-time normal map

- **Labels**: low,nifal,renderer,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4301
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/render/static_meshes.rs` (`apply_texture_flip_roles`, `collect_static_mesh_draws`)
- **Status**: NEW
- **Description**: `apply_texture_flip_roles` writes active `FlipTextureRole::Normal` / `Height` frames into `texture_indices`. `normal_has_alpha` is still read from the spawn-time `MaterialTextureHandles`. Two gates use it against the flipped index:
  - the #3562 `PARALLAX_ALPHA_HEIGHT_BIT` gate;
  - `normal_alpha_spec_binding_applies`, the gloss-slot rebind.
  A Normal flip whose frames differ in alpha presence would reintroduce the BC1/BC5 alpha misread #3562 fixed.
- **Evidence**: `apply_texture_flip_roles(flip, &mut texture_indices); let normal_map_index = texture_indices.normal; let normal_has_alpha = material_texture_handles.map(|handles| handles.normal_has_alpha)…`
- **Impact**: None on shipping content. Per the #3901 commit, the only vanilla non-base flipbook is a `GLOW_MAP` flip on Oblivion's `battle.nif`. Latent.
- **Related**: #3901, #3562, #4260, #1480.
- **Suggested Fix**: Carry per-frame alpha presence on `TextureFlipEntry`, resolved at clip-attach, for the Normal role; or restrict the Normal/Height arms until real content exists, and pin the choice with a test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`) or `Material::resolve_pbr`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer. See `/audit-nifal`.
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
