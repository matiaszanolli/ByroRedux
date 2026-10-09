# #5402: REN-D5-2026-10-08-02: `parse_dds` rejects `DDPF_LUMINANCE` (L8) DDS, so the Oblivion glow-map texture role loses its map for Mehrunes Dagon and the Oblivion Gate set

**Labels**: medium,renderer,bug,game:oblivion
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5402

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-08.md` — `REN-D5-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: MEDIUM
- **Dimension**: Memory/Lifecycle (texture decode)
- **Location**: `crates/renderer/src/vulkan/dds.rs`, `parse_dds`: the final `else { bail!("Unsupported DDS pixel format (flags={:#x})", pf_flags) }` after the `DDPF_FOURCC` and `DDPF_RGB` branches. The consumer chain is in `byroredux/src/asset_provider/texture.rs` (`resolve_texture_view_with_clamp`, then `resolve_material_texture_handles_with_clamp`).
- **Status**: NEW (no open or closed issue matches "DDS luminance", "Oblivion glow map" or "Unsupported DDS pixel format" beyond #1542, which fixed the 16-/24-bpp RGB arm only).
- **Description**:
  - `DDPF_LUMINANCE` (`0x20000`) is neither `DDPF_FOURCC` nor `DDPF_RGB`, so the header falls to the `bail!` arm.
  - The queue path then logs `Failed to enqueue DDS '…': Unsupported DDS pixel format (flags=0x20000)`, hands back the checkerboard fallback handle, and `map_secondary_texture_handles` collapses an authored-but-failed secondary role to handle 0. The glow role is silently absent.
  - The existing `RgbExpand` machinery could decode the format: an 8-bit pixel with the luminance mask copied to R, G and B yields `(L, L, L, 255)`, which is what D3D9 L8 samples as.
- **Evidence**:
  - Census (`flags & 0x20000`, `bpp = 8`, `RBitMask = 0xFF`): Oblivion 415 files in `Oblivion - Textures - Compressed.bsa` (every name printed by the scan ends `_g.dds`, the glow-map convention), Shivering Isles 55, Skyrim LE and SE 1 each (`effects/noisevolume.dds`). FO3, FNV and every other archive scanned: 0.
  - These textures land in the glow role. `glowprobe` (a throwaway program calling `byroredux_nif::import_nif_scene`) shows, for `meshes\creatures\endgame\battle.nif`, `…\entry.nif`, `…\mehrunesdagon\mehrunesdagon.nif` and `…\weapon.nif`, `base` and `emissive` both set, with `emissive` pointing at the `_g` texture.
  - Exposure by exact path match over `Oblivion - Meshes.bsa` (8,032 NIFs): 8 NIFs reference 34 distinct L8 textures — `creatures/mehrunesdagon/dagon_g.dds`, `obliviongate/akportal/akgate001_g.dds`…`akgate008_g.dds`, `creatures/rat/rateye_g.dds`, `oblivion/citadel interior/citadelcolumn02b_g.dds`, a scorched-stone landscape glow and others. The matching is exact, so this is a lower bound; the remaining `_g` files may be referenced by DLC meshes or by flipbook sequences not matched.
  - `triangle.frag` consumes the role as the emissive mask (`mat.glowMapIndex != 0u`), so with handle 0 the authored glow shape is gone.
- **Impact**: Oblivion's iconic glow-mapped content — the Oblivion Gate portal frames, Mehrunes Dagon, rat eyes, the Citadel columns — renders without its authored self-illumination. Bounded to Oblivion and Shivering Isles; no crash and no checkerboard (the role collapses to "absent").
- **Related**: #399 (glow slot reaches the shader), #1542, REN-D5-2026-10-08-01.
- **Suggested Fix**: Accept `DDPF_LUMINANCE` at 8 bpp (and `DDPF_LUMINANCE | DDPF_ALPHAPIXELS` at 16 bpp) by building an `RgbExpand { src_bpp, r_mask = g_mask = b_mask = luminance mask, a_mask }`. Add header fixtures for both.

## Completeness Checks
- [ ] **SIBLING**: `DDPF_LUMINANCE | DDPF_ALPHAPIXELS` (16-bpp) and the 8-bpp `DDPF_ALPHA` case considered alongside L8
- [ ] **TESTS**: A regression test pins this specific fix
