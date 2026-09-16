# #4426: NIFAL-D8-2026-09-16-01: Flipbook frames bypass the per-role texture resolver — authored CLAMP is dropped on the vanilla Oblivion gates, and Normal/Height/SmoothSpec flips would upload as sRGB

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4426
- **Labels**: medium,nifal,renderer,animation,game:oblivion,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: MEDIUM. Rendering is wrong on 9 vanilla flip controllers, but the only visible effect is at texture edges. The colour-space and fallback halves are latent on vanilla content.
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: single-boundary
- **Game Affected**: Oblivion (live, clamp mode). Any game with a mod-authored non-base flipbook (colour space, fallback).
- **Location**: `byroredux/src/anim_convert.rs:246-253` (frame resolve). Compare with `byroredux/src/asset_provider/texture.rs:712-781` (`resolve_material_texture_handles_with_clamp` / `map_secondary_texture_handles`). Consumer: `byroredux/src/render/static_meshes.rs:158-195` (`apply_texture_flip_roles`) and `:447-452` (base-color flip replaces `TextureHandle`).
- **Status**: NEW. The resolve line dates from #2221 (`7fbc5bafb`). #3901 (`4520f8d53`) made every non-base role live through it.
- **Description**: Static material textures reach the GPU through one per-role resolver. `resolve_material_texture_handles_with_clamp` decides three things for each role:
  1. **Sampler address mode**: the material's authored `texture_clamp_mode`. Its doc says the walk exists so that "structures, clutter, actors, and exterior statics cannot drift".
  2. **Colour space**: `map_secondary_texture_handles` uploads `normal`, `smooth_spec` and `height` as Linear, and `emissive`, `dark`, `detail` and the decals as sRGB.
  3. **Missing texture**: a secondary role that is authored but missing collapses to `0`, so the shader treats it as absent instead of sampling the magenta fallback.

  The flipbook resolves the frames that replace those same roles with `crate::asset_provider::resolve_texture(ctx, tex_provider, Some(path))`. That call fixes all three to REPEAT (3), sRGB and the checker fallback, whatever the role. After #3901, `apply_texture_flip_roles` writes those handles straight into `MaterialTextureSet<u32>` slots that the static resolver had filled under different rules.
- **Evidence**:
  - `byroredux/src/anim_convert.rs:252`: `.map(|path| crate::asset_provider::resolve_texture(ctx, tex_provider, Some(path)))`. `resolve_texture` is `resolve_texture_with_clamp(.., 3)` → `TextureColorSpace::Srgb` (`byroredux/src/asset_provider/texture.rs:509-519`, `:547-560`).
  - The static paths resolve the base with `resolve_texture_with_clamp(.., material.texture_clamp_mode)` (`byroredux/src/cell_loader/spawn/mesh_instance.rs:933`, `byroredux/src/scene/nif_loader.rs:1141`) and the secondary roles with the per-role table.
  - **Census** (all 9 Oblivion mesh BSAs, every `NiFlipController` joined to its host `NiTexturingProperty` through the controller chain), as a histogram of `(TexType, authored clamp of that slot's TexDesc)`: `(0 BASE, 3 WRAP) × 49`, **`(0 BASE, 0 CLAMP_S_CLAMP_T) × 9`**, `(4 GLOW, 3) × 1`, and 1 host not resolved (`magiceffects\shockshield.nif`).
  - The 9 CLAMP controllers are the 16-frame portal flipbooks in `meshes\oblivion\gate\obliviongate_forming.nif`, `oblivionarchgate01.nif` and `obliviongate_simple.nif` (3 each). Those are the Oblivion Gate meshes placed throughout the Tamriel worldspace.
  - The flipped handle replaces `TextureHandle` on every frame (`byroredux/src/render/static_meshes.rs:451-452`), so the WRAP-sampled frame is what is always drawn. The material's CLAMP applies only to the unused spawn-time base handle.
- **Impact**:
  - **Oblivion gates**: the portal surface samples with REPEAT where the art authors CLAMP. Bilinear filtering blends in opposite-edge texels along UV borders, the edge bleed #610 exists to prevent. Each frame path is also cached twice, under `(path, 3)` for the flip and `(path, 0)` for the static base (a small duplicate VRAM cost).
  - **Latent**: a flipbook on `Normal`, `Height` or `SmoothSpec` (reachable since #3901, which measured 0 vanilla instances) would upload through the sRGB curve. A flat 125/255 normal texel would decode to about 0.21 instead of 0.49, which is exactly the failure `resolve_linear_texture`'s new doc (`60cef1c64`) describes.
  - **Latent**: a missing non-base frame binds the magenta checker into a secondary role, where the static path binds `0`.
- **Related**: #3901, #4301, #2221, #610, #3516, NIFAL-D8-2026-09-16-02
- **Suggested Fix**:
  1. Expose a single-role entry point from `map_secondary_texture_handles`'s table, a `(role → cubemap, colour space)` lookup keyed on `FlipTextureRole`.
  2. Resolve flip frames through it, using the target entity's `Material.texture_clamp_mode` (already inserted when `attach_animation_sinks` runs) and the same fallback-to-0 rule for non-base roles.
  3. Pin it with a test that drives a `Normal` flip and a CLAMP base flip through the attach path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
