# Oblivion Compatibility Audit — Dimension 4 (Rendering Path) — 2026-05-06

## Executive Summary

- **Scope**: Dim 4 only — property → MaterialInfo → GpuMaterial → shader routing for Oblivion-era NIF blocks (`NiTexturingProperty`, `NiMaterialProperty`, `NiAlphaProperty`, `NiZBufferProperty`, `NiStencilProperty`, `NiVertexColorProperty`, plus the four `NiFlagProperty` subtypes: Specular, Wireframe, Shade, Dither).
- **Findings**: 1 total — 0 CRITICAL, 0 HIGH, 0 MEDIUM, 1 LOW.
- **Verdict**: **Materially clean.** The only finding is a parsed-and-dropped pair (`wireframe`, `flat_shading`) that the walker already documents as "future work." Every other Oblivion-era property block routes through the importer to a GPU representation and a shader consumer. The historical regression class (`NiTexturingProperty` reading `u32` count vs `bool`-gated count, see `afab3e7` / commit comment at `blocks/properties.rs:344-364`) is intact.

## Property Routing Matrix

| Property | Parser | Importer routing | GPU representation | Shader consumer | Status |
|---|---|---|---|---|---|
| `NiTexturingProperty` (base) | `blocks/properties.rs:240-273` | `walker.rs:474-478` → `info.texture_path` | `GpuMaterial.texture_index` | `triangle.frag:1058`-ish (`textures[mat.textureIndex]`) | OK |
| `NiTexturingProperty` (dark slot 1) | `properties.rs:243` | `walker.rs:515-518` → `info.dark_map` | `GpuMaterial.dark_map_index` (offset 56, `material.rs:98`) | `triangle.frag:1082-1083` (`albedo *= dark`) | OK |
| `NiTexturingProperty` (detail slot 2) | `properties.rs:248` | `walker.rs:505-508` → `info.detail_map` | `GpuMaterial.detail_map_index` | `triangle.frag` | OK |
| `NiTexturingProperty` (gloss slot 3) | `properties.rs:253` | `walker.rs:509-512` → `info.gloss_map` | `GpuMaterial.gloss_map_index` | `triangle.frag` | OK |
| `NiTexturingProperty` (glow slot 4) | `properties.rs:258` | `walker.rs:501-504` → `info.glow_map` | `GpuMaterial.glow_map_index` | `triangle.frag` (emissive) | OK |
| `NiTexturingProperty` (bump slot 5 → normal, #131) | `properties.rs:263` | `walker.rs:484-491` → `info.normal_map` (Oblivion uses bump slot, dedicated normal slot landed in FO3+) | `GpuMaterial.normal_map_index` | `triangle.frag` (TBN matrix) | OK |
| `NiTexturingProperty` (parallax slot 7, v20.2.0.5+) | `properties.rs` | `walker.rs:526-529` → `info.parallax_map` | `GpuMaterial.parallax_map_index` | `triangle.frag` POM branch | OK |
| `NiTexturingProperty` decal slots 0..=3 | `properties.rs` | `walker.rs:530-538` (NOT routed — #705 / O4-07 removed extraction) | — | — | **Intentionally dropped** (no descriptor binding consumes them; one-line re-add when shader path lands) |
| `NiTexturingProperty.base.flags` clamp_mode (lower 4 bits) | `properties.rs:464` | `walker.rs:563-567` → `info.texture_clamp_mode` | `GpuMaterial.flags` (clamp pair) | `vkSamplerAddressMode` pair at descriptor write | OK (#610) |
| `NiMaterialProperty.diffuse` | `blocks/properties.rs` | `walker.rs:460` → `info.diffuse_color` | `GpuMaterial.diffuse_color` | `triangle.frag` modulator | OK (raw monitor-space, `feedback_color_space.md` / `0e8efc6`) |
| `NiMaterialProperty.ambient` | `blocks/properties.rs` | `walker.rs:464` → `info.ambient_color` (#221) | `GpuMaterial.ambient_color` | `triangle.frag` cell-ambient mod | OK (was discarded pre-#221) |
| `NiMaterialProperty.specular` / `emissive` / `shininess` / `alpha` / `emissive_mult` | `blocks/properties.rs` | `walker.rs:465-469` | `GpuMaterial.specular_color` / `emissive_color` / `glossiness` / `alpha` / `emissive_mult` | `triangle.frag` BRDF | OK |
| `NiAlphaProperty` (flags byte) | `blocks/properties.rs` | `mod.rs:670-687` `apply_alpha_flags` | `GpuMaterial.flags` + draw-batch key | Pipeline pick (`pipeline.rs:53` `Blended { src, dst, two_sided }`) + dynamic state | OK (full 16×16 src×dst + alpha-test func) |
| `NiZBufferProperty.z_test` / `z_write` / `z_function` | `blocks/properties.rs` (#398) | `walker.rs:446-455` | Per-batch dynamic state | `cmd_set_depth_test_enable` / `_write_enable` / `_compare_op` at `draw.rs:1263-1266`, batch-level at `:955-957` | OK (8-value enum clamped, LESSEQUAL fallback) |
| `NiStencilProperty` (two_sided derivation) | `blocks/properties.rs` | `walker.rs:714-721` → `info.two_sided` | Pipeline variant `opaque_two_sided` / `Blended { two_sided }` (`pipeline.rs:49,53,117`) | `cmd_set_cull_mode(NONE)` for blended; baked `CULL_BACK` vs `CULL_NONE` for opaque | OK |
| `NiSpecularProperty` (flags=0 disables) | `NiFlagProperty` (#703) | `walker.rs:728-732` → `info.specular_enabled = false`; finalised at `:787-790` (zeroes color too, #696 covers IOR-glass re-promotion) | `GpuMaterial.specular_color = 0` collapses BRDF | `triangle.frag` BRDF (specStrength × specColor) | OK |
| `NiVertexColorProperty` (vertex_mode + lighting_mode) | `blocks/properties.rs` (#214/#694) | `walker.rs:767-770` → `info.vertex_color_mode` via `VertexColorMode::from_property` | `GpuInstance.vertex_color_emissive` (`render.rs:912`) | `triangle.frag` per-vertex emissive routing | OK |
| `NiWireframeProperty` (flags=1 enables) | `NiFlagProperty` | `walker.rs:733-738` → `info.wireframe = true` | Propagated to `ImportedMesh` (`mesh.rs:641,933,1139`) | **NOT consumed.** Pipeline always `vk::PolygonMode::FILL` (`pipeline.rs:212,222,410,573`) | **PARSED-DROPPED** — see Finding O4-D4-NEW-01 |
| `NiShadeProperty` (flags=0 → flat shading) | `NiFlagProperty` | `walker.rs:739-744` → `info.flat_shading = true` | Propagated to `ImportedMesh` (`mesh.rs:641,933,1139`) | **NOT consumed.** No `flat` qualifier in `triangle.vert/frag` | **PARSED-DROPPED** — see Finding O4-D4-NEW-01 |
| `NiDitherProperty` (flags=1 → 16-bit dithering) | `NiFlagProperty` | `walker.rs:746-748` (intentional drop, "no Vulkan analogue") | — | — | **Intentionally dropped** (documented) |

## Verified Invariants

Each item with file:line evidence — none of these regressed since prior audit.

- **`NiTexturingProperty` reads `u32` count UNCONDITIONALLY** (no leading `Has Shader Textures: bool` gate). `crates/nif/src/blocks/properties.rs:344-366` — the comment explicitly cites the regression class (#149 / `afab3e7`) where following nif.xml's bool gate consumed the first byte of the u32 count and 3-byte-misaligned every subsequent block. Read instruction at `:366` is `let num_shader_textures = stream.read_u32_le()?;`. **PASS.**
- **Gamebryo color space is raw monitor-space — no `srgb_to_linear`.** `walker.rs:460-466` copies `mat.diffuse.{r,g,b}` directly into `info.diffuse_color`; ditto ambient/specular/emissive. Per `feedback_color_space.md` / commit `0e8efc6` this is correct. **PASS.**
- **`NiAlphaProperty` flags fully unpacked.** `crates/nif/src/import/material/mod.rs:670-687`:
  - `blend = flags & 0x001` (bit 0)
  - `src_blend_mode = (flags >> 1) & 0xF` (bits 1-4)
  - `dst_blend_mode = (flags >> 5) & 0xF` (bits 5-8)
  - `test = flags & 0x200` (bit 9)
  - `alpha_test_func = (flags & 0x1C00) >> 10` (bits 10-12, 3-bit comparison func)
  - Test wins over blend when both set (#208 precedence)
  - Threshold normalised to f32 by `/ 255.0`
  - All 16 src factors × 16 dst factors round-trip (no truncation). **PASS.**
- **Normal map from bump slot (#131) for Oblivion content.** Oblivion stores tangent-space normal maps in `bump_texture` (slot 5); the dedicated `normal_texture` slot landed in FO3+. `walker.rs:484-491` checks `normal_texture.or_else(bump_texture)`. **PASS.**
- **Property-block mid-import re-evaluation order**: shape-direct properties first, then inherited, with `if !info.alpha_blend && !info.alpha_test` guard so a closer alpha property cannot get overwritten (`walker.rs:425-443`). **PASS.**

## Findings

### O4-D4-NEW-01: `NiWireframeProperty` and `NiShadeProperty` are parsed and propagated but never consumed

- **Severity**: LOW
- **Dimension**: 4 (Rendering Path)
- **Location**: `crates/nif/src/import/material/walker.rs:733-744` (set), `crates/nif/src/import/mesh.rs:641-642, 933-934, 1139-1140` (propagate), `crates/renderer/src/vulkan/pipeline.rs:212,222,410,573` (FILL forced), shaders `triangle.vert` / `triangle.frag` (no `flat` qualifier)
- **Status**: NEW (the walker comments document this as "Renderer consumption is future work" so it's tracked-but-unfiled)
- **Description**:
  - **Wireframe**: when `NiWireframeProperty.flags == 1`, the importer sets `info.wireframe = true` and the flag rides through `MaterialInfo` and `ImportedMesh` to the renderer. The renderer never consults it. Every blend-pipeline arm at `pipeline.rs` hard-codes `polygon_mode(vk::PolygonMode::FILL)` (`:212, :222, :410, :573`); there is no `WireframePipeline` variant.
  - **Flat shading**: when `NiShadeProperty.flags == 0`, the importer sets `info.flat_shading = true`. Same propagation chain. The shaders use smooth interpolation throughout — no GLSL `flat` qualifier on `frag_normal` or related varyings.
- **Evidence**:
  ```rust
  // walker.rs:733-744 — capture
  "NiWireframeProperty" if flag_prop.enabled() => {
      info.wireframe = true;  // captured
  }
  "NiShadeProperty" if !flag_prop.enabled() => {
      info.flat_shading = true;  // captured
  }
  ```
  The comments at `:734-735` and `:740-742` explicitly note "Renderer consumption is future work" for both. Grep confirms zero non-import-side consumers:
  ```
  $ grep -rn "info\.wireframe\|info\.flat_shading\|mat\.wireframe\|mat\.flat_shading" crates/renderer/ byroredux/
  (no matches)
  ```
- **Impact**:
  - **Wireframe**: zero impact on Oblivion vanilla. The walker comment at `:734-735` notes wireframe is "Not present in Oblivion vanilla but used by FO3/FNV mods." A debug-only feature set whose absence does not affect the gameplay-content render.
  - **Flat shading**: low impact on Oblivion vanilla. Walker comment at `:740-741` notes it appears on "a handful of Oblivion architectural pieces" — meshes that should look faceted will instead render with smooth normals. Subtle visual mismatch, not a black-content failure.
- **Related**: None known. No prior open issue covers either. `walker.rs` comments treat it as deferred work.
- **Suggested Fix**:
  - **Wireframe**: ship a third pipeline variant `WireframeOpaque { two_sided }` in `pipeline.rs` with `polygon_mode(vk::PolygonMode::LINE)`, and one matching `Blended` variant. Pipeline-cache key already includes blend mode and two-sided; one more boolean fits naturally. Estimated 30-40 lines including the `MaterialKind` arm.
  - **Flat shading**: less clean. Adding a `flat` qualifier to a fragment input forces vertex-shader `flat`-out on the same varying, which fights the per-vertex normal interpolation the rest of the pipeline depends on. Either (a) build a parallel `triangle_flat.vert/frag` pair and switch pipeline at draw time, or (b) compute face normals in fragment via `dFdx(worldPos) × dFdy(worldPos)`. Option (b) is simpler but adds 2 instructions to every fragment; gate behind a per-batch flag. Defer until a player-visible Oblivion mesh is identified that needs it.

## Out-of-Scope Notes

These were observed during the Dim 4 sweep but are tracked elsewhere or fall outside this dimension:

- **Decal slots 0..=3 on `NiTexturingProperty`** are intentionally dropped at the importer (`walker.rs:530-538`, #705 / O4-07 removed the extraction added by #400 / OBL-D4-H4). The block parser still exposes the raw slots; re-adding the routing is a one-line addition once a fragment-shader overlay path consumes them. Not a finding — it's a documented design choice.
- **`NiDitherProperty`** is intentionally dropped (`walker.rs:746-748`) because there is no Vulkan analogue for DX9-style 16-bit color dithering. Documented; not a finding.
- **`NiSpecularProperty` glass IOR re-promotion (#696)**: the BRDF glass-IOR branch at `triangle.frag:1004` does `specStrength = max(specStrength, 3.0)`, which used to silently re-promote spec on glass surfaces even when the NIF said `flags == 0`. The `walker.rs:787-790` finalizer zeroes both `specular_strength` AND `specular_color`, so the BRDF multiply at frag:1293, 1396 collapses regardless. PASS — no finding here, but worth re-noting if anyone touches the glass branch in `triangle.frag`.
- **Color-space transform** is *not* a finding. Per `feedback_color_space.md`, Gamebryo colors are raw monitor-space floats — applying `srgb_to_linear` would be wrong. Audits flagging it have stale premise.
- **Dim 2 (BSA v103)** lives in `AUDIT_OBLIVION_2026-05-06.md` (separate report from earlier today).
- **Dim 1 (NIF v20.0.0.5 parser)** is not re-audited here; the regression guard for `NiTexturingProperty` u32 count is verified in the "Verified Invariants" section of this report only because it's load-bearing for the rendering path.
