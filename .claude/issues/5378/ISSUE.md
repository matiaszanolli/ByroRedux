# #5378: REN-D5-2026-10-08-01: `parse_dds` ignores the channel masks of 32-bpp `DDPF_RGB` headers, so A8R8G8B8 / X8R8G8B8 files upload as R8G8B8A8 with red and blue swapped — including every Skyrim SE distant-terrain diffuse atlas

**Labels**: high,renderer,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5378

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-08.md` — `REN-D5-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: the `bpp == 32` arm of `parse_dds` (`crates/renderer/src/vulkan/dds.rs` ~:500) still returns `R8G8B8A8_SRGB, expand: None` without reading the masks. Also affects Oblivion / FO3 / FNV assets (see table); labelled by the headline Skyrim SE impact.

- **Severity**: HIGH
- **Dimension**: Memory/Lifecycle (texture decode)
- **Location**:
  - `crates/renderer/src/vulkan/dds.rs`, `parse_dds`, the `bpp == 32` arm of the `DDPF_RGB` branch ("32-bpp R8G8B8A8 uploads directly — zero-copy").
  - Consumers: `TextureRegistry::load_dds_with_clamp_and_color_space` / `enqueue_dds_with_clamp_and_color_space` (`crates/renderer/src/texture_registry/upload.rs`).
  - The affected Skyrim consumer: `spawn_btr_block` (`byroredux/src/cell_loader/terrain_lod_btr.rs`), which resolves `btr_diffuse_path`.
- **Status**: NEW. No open or closed issue matches the keyword searches listed under Process notes (#1542, #1074 and #4830 are adjacent but cover other arms). `AUDIT_FO3_2026-07-02.md` Dim 6 records the parser as handling "32-bpp uncompressed RGBA/BGRA", which was never checked against the masks.
- **Description**:
  - The 16-/24-bpp arm reads `dwRBitMask`…`dwABitMask` (file offsets 92..108) and decodes through `RgbExpand`.
  - The 32-bpp arm never reads them. It sets `format: R8G8B8A8_SRGB, expand: None` and borrows the file bytes.
  - A DDS with `R=0x00FF0000, G=0x0000FF00, B=0x000000FF` stores bytes in B,G,R,(A|X) order. Uploaded as R8G8B8A8, the sampler's `.r` is the file's blue.
  - For X8R8G8B8 (`A=0`) the fourth byte is the unused X byte, so alpha is sampled from it. It averages 0 across the 2,916 Tamriel atlases measured. `format_has_alpha(R8G8B8A8_SRGB)` is true, so `INSTANCE_FLAG_DIFFUSE_ALPHA` is set and the shader does not force `a = 1`. The LOD terrain draw is opaque with no alpha test, so the zero alpha is harmless today; the channel swap is the visible part.
  - The sibling DX10 path handles the same layouts correctly: DXGI 87/88/91 map to `B8G8R8A8_*`.
- **Evidence**:
  - Code: the `bpp == 32` arm in `parse_dds` (above).
  - Test gap: the only 32-bpp fixture, `make_uncompressed_header`, never sets the masks.
  - No compensation downstream: no `.bgr`/`.zyx`/`ComponentMapping` swizzle exists in the renderer shaders or image-view creation (grep).
  - Measured on the vanilla archives (header scan of every `.dds`; masks other than the RGBA order `R=0xFF, G=0xFF00, B=0xFF0000`):

    | Archive set | Layout | Count | What it is |
    |---|---|---|---|
    | Skyrim SE `Textures5/6/7.bsa` | X8R8G8B8 (`A=0`) | 9,326 | every terrain-LOD diffuse atlas under `textures/terrain/<world>/` (Tamriel 3,040; Solstheim 3,060; Soul Cairn 910; Apocrypha 990; Sovngarde 495; Skuldafn 340; the rest smaller) |
    | Skyrim SE | A8R8G8B8 | 6 + 11 + 2 + 3 + 1 (+ 641 flow maps, see below) | 6 tree-LOD atlases (`…/trees/<world>treelod.dds`), 11 DLC1 cubemaps, `sky/sun.dds` and `sky/sunglare.dds`, 3 lens-flare maps, `effects/highfrequencynormals.dds`; the 641 are `textures/water/skyrim.esm/flow.*.dds` |
    | Oblivion / Shivering Isles | A8R8G8B8 | 72 / 3 | lock-picking meshes, gate effects, 1×1 placeholders |
    | FO3 / FNV | A8R8G8B8 | 13 / 13 | including the WATR noise/foam maps `testwaternoisegrant.dds`, `wastelandmuckpoolnoise01.dds`, `toxicdumpwater01.dds`, `waterfoam01.dds` |
    | Skyrim LE | A8R8G8B8 | 1 | `effects/highfrequencynormals.dds` |

  - The masks, not the bytes, are what is right. On 2,916 Tamriel diffuse atlases the per-file mean of byte 2 minus byte 0 has p50 = +14.1 and p90 = +14.2 (of 255), and 2,554 files have byte 2 > byte 0. Decoded per the masks (R = byte 2), that is the warm, earthy tint Skyrim terrain has. Decoded the way the engine does (R = byte 0), it is a uniform 5 % shift toward blue.
  - Cross-check: the 21 level-32 `tamriel.32.*` atlases are authored with RGBA-order masks (so they decode correctly today), and their R is also above their B (73 vs 64 on the sample).
  - The 641 water flow maps are **not** consumed by the engine today: only the NIF-water `flow_map_index` is loaded, from the mesh, not these per-cell files (grep of `byroredux/src`). They are listed for completeness, not as impact.
- **Impact**:
  - Every Skyrim SE exterior that draws distant terrain from vanilla `.btr` LOD renders it with red and blue swapped, plus an alpha channel read from an unused byte. The magnitude per tile is modest (a few percent), but it is systematic, silent and affects a headline feature.
  - The tree-LOD atlases (6) and DLC1 environment cubemaps (11) are swapped the same way.
  - The WATR noise maps (FO3/FNV) and the Oblivion lock-picking/effect meshes are swapped too, with lower visibility.
  - Rated HIGH under the decision tree's "rendering correctness → at least HIGH" line. The per-tile hue shift is small, so a reviewer who reads this as "visual artifacts only" can down-rate it to MEDIUM.
- **Related**: #1542 (16-/24-bpp arm), #1074 (DXGI BGRA mappings), #4830 (mask hardening), REN-D5-2026-10-08-02.
- **Suggested Fix**:
  - Read the four masks in the 32-bpp arm.
  - Keep the zero-copy path only for the exact RGBA order with `A = 0xFF000000`.
  - Send every other combination (A8R8G8B8, X8R8G8B8, X8B8G8R8) through the existing `RgbExpand` machinery, which already accepts arbitrary masks and forces `A = 255` when the alpha mask is zero. `validate_expand_masks` already admits `bpp == 32`.
  - Alternatively map the exact BGRA masks to `B8G8R8A8_*` and force opaque alpha when `a_mask == 0`.
  - Pin it with fixtures that carry real masks (an X8R8G8B8 case asserting the channels land in the right slots and `A == 255`).

## Completeness Checks
- [ ] **SIBLING**: `average_rgb`'s uncompressed arm (`swap_rb = false` for `R8G8B8A8_*`) is fixed by the same change; DX10 BGRA mappings stay consistent
- [ ] **TESTS**: A regression test pins this specific fix
