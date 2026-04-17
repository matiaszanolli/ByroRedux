# Skyrim SE Compatibility Audit — 2026-04-16

Deep audit of ByroRedux readiness for **The Elder Scrolls V: Skyrim Special Edition** content, run as 6 parallel dimensions per `.claude/commands/audit-skyrim.md`.

## Executive Summary

| Pillar | State |
|---|---|
| BSA v105 (LZ4) extraction | ✅ End-to-end verified — 141,286 files / 9.5 GB / zero failures across all base + CC archives |
| NIF parse rate | ✅ 100.00% / 18,862 NIFs on `Skyrim - Meshes0.bsa` (but 55,348 stream-position recoveries hide systematic mis-parses — see S5-01/S5-02/S1-01) |
| Individual mesh rendering | ✅ Sweetroll, fxglow effect, tree LOD all render with full Vulkan + RT + BC1/BC3 textures |
| Skyrim ESM parsing | ⚠️ Better than ROADMAP claims — shared `esm/*` parser already auto-detects Tes5Plus and decompresses zlib `FLAG_COMPRESSED` records via `flate2`. CELL/REFR/STAT/LIGH/LAND/LTEX/TXST all parse. **Never run end-to-end against real `Skyrim.esm`.** |
| BSLightingShaderProperty 19 variants | ❌ All 19 parsed correctly, but only EnvironmentMap & GlowShader partially routed; SkinTint, HairTint, Eye, Snow, Multi-Layer Parallax, Dismemberment all render as default-lit. No `material_kind` channel exists in `GpuInstance`. |
| BSEffectShaderProperty visual params | ❌ Falloff cone, soft-falloff depth, greyscale palette, lighting influence all parsed and dropped on import. BsTriShape import path probes only BSLightingShader, ignoring BSEffectShader entirely (#128 was a strict subset). |
| NPC head / dragon body geometry | ❌ Every BSDynamicTriShape imports zero meshes (S5-01). Skyrim NPC faces invisible; dragon body invisible. |

**Top 3 must-fix to render a Skyrim interior cell with NPCs end-to-end:**
1. **S5-01** — One-line move of `particle_data_size` read out of the `data_size > 0` gate in `tri_shape.rs:423`. Restores all NPC heads + dragons.
2. **SK-D3-01 + SK-D3-02** — Add `material_kind` to `GpuInstance` (repurpose `_pad1`) + exhaustively match all 19 variants in `extract_material_info`. Restores skin/hair/eye/snow shading.
3. **S6-14** — REFR XESP enable-parent filter in `cell.rs`. Without it, every default-disabled REFR renders on cell load.

**Total findings: 31** (8 HIGH, 8 MEDIUM, 15 LOW). 5 positive verifications. 1 prior open issue recommended for closure (#176, premise wrong per nif.xml).

---

## Shader Variant Coverage Matrix (BSLightingShaderProperty)

Columns: **Parsed** = bytes consumed correctly per nif.xml. **MaterialInfo** = type-specific data routed into `MaterialInfo`. **Rendered** = renderer differentiates from "default lit".

| # | Variant | Parsed | MaterialInfo | Rendered |
|---|---|:-:|:-:|:-:|
| 0  | Default              | ✅ | ✅ | ✅ |
| 1  | EnvironmentMap       | ✅ | partial (env_map_scale only; texture slot 4/5 unread) | ❌ |
| 2  | GlowShader           | ✅ | partial | ✅ (default+emissive happens to look right) |
| 3  | Parallax             | ✅ | ❌ | ❌ |
| 4  | FaceTint             | ✅ | ❌ | ❌ |
| 5  | SkinTint             | ✅ | ❌ | ❌ |
| 6  | HairTint             | ✅ | ❌ | ❌ |
| 7  | ParallaxOcc          | ✅ | ❌ | ❌ |
| 8  | MultiLayerParallax   | ✅ | ❌ | ❌ |
| 9  | TreeAnim             | ✅ | ❌ | ❌ |
| 10 | LODLandscape         | ✅ | ❌ | ❌ |
| 11 | Snow / SparkleSnow   | ✅ | ❌ | ❌ |
| 12 | MultiPassLandscape   | ✅ | ❌ | ❌ |
| 13 | LODObjectsHD         | ✅ | ❌ | ❌ |
| 14 | EyeEnvmap            | ✅ | ❌ | ❌ |
| 15 | Cloud                | ✅ | ❌ | ❌ |
| 16 | LODLandscapeNoise    | ✅ | ❌ | ❌ |
| 17 | MultiTexLandLODBlend | ✅ | ❌ | ❌ |
| 18 | Dismemberment        | ✅ | ❌ | ❌ |

---

## Specialty Block Coverage

| Block | Parsed | Dispatched | Walker unwraps | Imported |
|---|:-:|:-:|:-:|:-:|
| BSFadeNode | ✅ | ✅ | ✅ | ✅ |
| BSBlastNode (alias BsRangeNode) | ✅ | ✅ | partial (discriminator lost) | partial |
| BSDamageStage (alias BsRangeNode) | ✅ | ✅ | partial | partial |
| BSDebrisNode (alias BsRangeNode) | ✅ | ✅ | partial | partial |
| BSRangeNode | ✅ | ✅ | ✅ | partial |
| BSMultiBoundNode (+culling_mode) | ✅ | ✅ | ✅ | partial (multi_bound_ref + culling_mode dropped) |
| BSTreeNode (+bones_1/bones_2) | ✅ | ✅ | ✅ | partial (bones dropped) |
| BSDynamicTriShape | ✅ | ✅ | ✅ | ❌ (S5-01 — zero meshes) |
| BSLODTriShape (+3×u32 LOD) | ✅ | ✅ | ✅ | partial (LOD ignored) |
| BSMeshLODTriShape (+3×u32 LOD) | ✅ | ✅ | ✅ | partial (LOD ignored) |
| BSSubIndexTriShape | ✅ | ✅ | ✅ | partial (segments ignored) |
| BSPackedCombinedGeomDataExtra | partial (header only) | ✅ | n/a | ❌ |
| BSPackedCombinedSharedGeomDataExtra | partial (header only) | ✅ | n/a | ❌ |
| BSEffectShaderProperty | ✅ | ✅ | n/a | partial (S4-01/S4-02 — falloff/greyscale/lighting dropped) |

---

## Forward Blocker Chain — "Skyrim Interior Cell Renders End-to-End"

1. ✅ **DONE** — BSA v105 LZ4 extraction (M18; 18,862/18,862 NIF parse).
2. ✅ **DONE** — `EsmReader::detect()` classifies Skyrim.esm as `Tes5Plus`.
3. ✅ **DONE** — zlib `FLAG_COMPRESSED` records decompressed via `flate2::ZlibDecoder` (verified by unit test only — never against real `Skyrim.esm`).
4. ✅ **DONE** — `parse_modl_group` extracts Skyrim STAT/LIGH/etc. identically to FNV.
5. ✅ **DONE** — `parse_cell_group` walks Skyrim CELL hierarchy (group-type IDs unchanged).
6. ✅ **PARTIAL** — XCLL fog/light fade extracted; directional-ambient cube still not consumed (S6-05).
7. ⚠️ **NEW WORK** — Honor REFR XESP enable-parent (S6-14) — required to suppress default-disabled refs.
8. ⚠️ **NEW WORK** — End-to-end smoke run: `cargo run -- --esm Skyrim.esm --cell <interior> --bsa Meshes0.bsa --textures-bsa Textures3.bsa`. Never executed.
9. ❌ **DEPENDS ON DIM 1/5** — S5-01 BSDynamicTriShape import (NPC heads invisible).
10. ❌ **DEPENDS ON DIM 3** — `material_kind` dispatch in `GpuInstance` + `triangle.frag` (SK-D3-01/02). Skin/hair/eye/parallax all render wrong.
11. 📋 **POST-MVP** — Localized strings (S6-03) for UI; ARMO/WEAP/AMMO sub-record fixes (S6-02); XCIM/XCLW/XCWT (S6-04); TXST slots 1-7 (S6-11); full ambient cube (S6-05); BSEffectShader falloff (S4-01/S4-02).

**Bottom line:** Renderer-side blockers (S5-01, SK-D3-01/02, S4-01/02) are larger than ESM-side blockers (just S6-14 + a smoke run). The shared `esm/*` parser is far closer to "works on Skyrim" than ROADMAP.md suggests.

---

## Findings by Severity

### HIGH (8)

#### S1-01 — BSTriShape FO76 `Bound Min Max` (24 bytes) never consumed
- **Severity**: HIGH | **Dim**: 1 (FO76 cross-impact) | **Status**: NEW
- **Location**: `crates/nif/src/blocks/tri_shape.rs:282-289`
- nif.xml line 8230-8232: `Bound Min Max` (6×float) is unconditionally present on `#BS_F76#` (BSVER==155) between `Bounding Sphere` and `Skin`. Parser jumps `radius` → `skin_ref` with no version branch. Every FO76 BSTriShape mis-parses skin/shader/alpha refs and `vertex_desc` u64. Per-block `block_size` recovery hides this from the 100% archive parse rate, but the block contents are wrong.
- **Fix**: `if stream.bsver() == 155 { stream.skip(24)?; }` between `radius` and `skin_ref`. Add regression test with `user_version_2 = 155`.

#### S5-01 — BSDynamicTriShape import drops every Skyrim NPC head/face mesh
- **Severity**: HIGH | **Dim**: 5 | **Status**: NEW
- **Location**: `crates/nif/src/blocks/tri_shape.rs:423-433`, `crates/nif/src/import/mesh.rs:222-224`
- `BsTriShape::parse()` gates the `Particle Data Size` u32 read inside `if data_size > 0`. Per nif.xml lines 8243-8246, `Particle Data Size` is unconditionally present on `#BS_SSE#` — only the trailing `Particle Vertices/Normals/Triangles` arrays are gated. With `data_size == 0` (every BSDynamicTriShape, since real vertex data lives in the dynamic Vector4 array), the parser is 4 bytes off, the dynamic vertex loop never runs, `extract_bs_tri_shape` returns `None`. Probe of `malehead.nif` block 3 (size 14,492): consumes only 124 bytes; 14,368 bytes of dynamic vertex data unread (898 verts × 16). Same root cause produces 5,599 "expected 120 consumed 116" warnings on plain BSTriShape and 21,140 on BSDynamicTriShape.
- **Impact**: Every Skyrim NPC face invisible. Dragon body invisible. The #157 closeout test passes against both broken and fixed parsers because it uses `data_size == 0` byte stream — needs a non-zero-data variant.
- **Fix**: One-line move of `particle_data_size` read out of `if data_size > 0` (keep inner guard for trailing arrays).

#### SK-D3-01 — BSLightingShaderProperty SkinTint/HairTint/EyeEnvmap/etc. colors parsed but discarded
- **Severity**: HIGH | **Dim**: 3 | **Status**: NEW
- **Location**: `crates/nif/src/import/material.rs:286-298`
- `extract_material_info` only matches `EnvironmentMap` arm of `ShaderTypeData`. SkinTint, HairTint, Fo76SkinTint, EyeEnvmap, MultiLayerParallax, SparkleSnow, ParallaxOcc all fall through. `MaterialInfo` has no fields to receive them.
- **Impact**: Every Skyrim NPC head/body/hair shape uses default specular/albedo with no race-tint multiplier; eyes have no cubemap; multi-layer parallax (ice, frosted glass) renders flat.
- **Fix**: Add `skin_tint_color`, `hair_tint_color`, `eye_cubemap_scale`, `eye_left/right_center`, `parallax_*`, `sparkle_parameters`, `multi_layer_*` to `MaterialInfo`. Exhaustively match all variants. Bundle with SK-D3-02.

#### SK-D3-02 — `triangle.frag` has no `material_kind` dispatch — all variants render default lit
- **Severity**: HIGH | **Dim**: 3 | **Status**: NEW
- **Location**: `crates/renderer/shaders/triangle.frag:25-54`, `crates/renderer/src/vulkan/scene_buffer.rs:48-93`
- `GpuInstance` (160 B, std430) carries no `material_kind`/`shader_type`/`material_flags`. Even after SK-D3-01 plumbs the data, the renderer has nowhere to send it. SkinTint, HairTint, MultiLayerParallax POM, SparkleSnow, EyeEnvmap, Dismemberment, TreeAnim — all collapse to default lit.
- **Fix**: Repurpose `_pad1` (offset 156, 4 B unused) as `material_kind: u32`. Keeps struct at 160 B. Update `scene_buffer.rs:824-856` size/offset asserts and **all 3 GLSL files that mirror GpuInstance** (per [Shader Struct Sync](../../home/matias/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_shader_struct_sync.md) feedback memory). Add `switch (instance.materialKind)` in fragment shader.

#### S4-01 — BSEffectShaderProperty rich material fields discarded on import
- **Severity**: HIGH | **Dim**: 4 | **Status**: NEW (related to but distinct from #166, #128)
- **Location**: `crates/nif/src/import/material.rs:300-315`, `crates/nif/src/import/mesh.rs:302-337`
- Parser at `shader.rs:842-1087` extracts every Skyrim+ effect-shader field (falloff_start/stop_angle, falloff_start/stop_opacity, soft_falloff_depth, greyscale_texture, env_map_min_lod, lighting_influence, FO4 env/normal/env_mask textures). Importer reads only `source_texture`, `emissive_color`, `emissive_multiple`, `uv_offset`, `uv_scale`. `MaterialInfo` has no fields for falloff cone or greyscale palette.
- **Impact**: Every magic effect, force field, glow-edged shield, Dwemer steam renders as opaque flat-shaded with no soft-edge falloff, no view-angle modulation. Spell impact decals over-lit at night because `lighting_influence` is dropped.
- **Fix**: Add falloff/soft_falloff/greyscale/lighting_influence/env_map_min_lod fields to `MaterialInfo`. Populate in BSEffectShaderProperty branch and the new BsTriShape branch (S4-02).

#### S4-02 — BsTriShape import path ignores BSEffectShaderProperty entirely (except texture_path)
- **Severity**: HIGH | **Dim**: 4 | **Status**: NEW (broader than #128, which only covers `two_sided`)
- **Location**: `crates/nif/src/import/mesh.rs:279-337` (`extract_bs_tri_shape`)
- Probes only `BSLightingShaderProperty`. When shader is BSEffectShaderProperty, `emissive_color`, `emissive_mult`, `specular_color`, `specular_strength`, `glossiness`, `uv_offset`, `uv_scale`, `mat_alpha`, `normal_map`, `env_map_scale`, `two_sided`, decal flag — all fall back to defaults.
- **Fix**: Mirror the BSLightingShaderProperty `if let Some(shader) = ...` block with a parallel BSEffectShaderProperty branch. Unify with `find_decal_bs` and add `find_two_sided_bs`. Bundles with #128, #129, SK-D3-05.

#### S6-02 — Skyrim ARMO/WEAP/AMMO DATA layouts diverge from FNV — items parser produces garbage stats
- **Severity**: HIGH | **Dim**: 6 | **Status**: NEW
- **Location**: `crates/plugin/src/esm/records/items.rs:124-318`
- Hard-codes FNV/FO3 sub-record schema. Skyrim ARMO uses `BOD2` (not `BMDT`); ARMO `DATA` is `value(u32) + weight(f32)` (8 bytes, no health); ARMO `DNAM = armor rating × 100` (4 bytes, not FNV's 8-byte DT+DR). Skyrim WEAP `DATA` is 10 bytes; `DNAM` is ~100 bytes with different field positions. AMMO uses entirely different sub-records. No game-aware dispatch. `grep BOD2` → 0 hits.
- **Impact**: `EsmIndex.items` map will contain Skyrim entries with zero/garbage stats. Cell rendering unaffected (uses `statics`, not `items`).
- **Fix**: Plumb `EsmVariant` into `parse_esm`, dispatch ARMO/WEAP/AMMO by game. Add `BOD2` parser for Skyrim biped flags.

#### S6-03 — Skyrim FULL is a localized lstring (u32 index), not a zstring — names corrupted
- **Severity**: HIGH | **Dim**: 6 | **Status**: NEW
- **Location**: `crates/plugin/src/esm/records/common.rs:108-110`
- Skyrim ESM uses TES4-record bit `0x80 (Localized)` to flag FULL/DESC as 4-byte u32 references into companion `Strings/<plugin>_<lang>.STRINGS|.DLSTRINGS|.ILSTRINGS` files. `CommonItemFields::from_subs` unconditionally calls `read_zstring` — produces 3-char garbage names. No `.STRINGS` loader exists (`grep -i lstring` → 0 hits). `read_file_header()` doesn't read the localization flag.
- **Impact**: ~12 lstring-bearing sub-records (FULL/DESC/RNAM/ICO2/MICO/...) all corrupted on Skyrim. Cell loader unaffected.
- **Fix**: Multi-week chunk. (1) Read TES4 record flags into `pub localized: bool` on `FileHeader`. (2) When localized, FULL/DESC become u32 lstring indexes. (3) Add `.STRINGS` loader (binary: u32 count + (u32 string_id, u32 offset) pairs + blob).

### MEDIUM (8)

#### S1-03 — BSTriShape tangent/bitangent parsed but discarded; renderer has no tangent attribute
- **Severity**: MEDIUM | **Dim**: 1 | **Status**: NEW
- **Location**: `crates/nif/src/blocks/tri_shape.rs:339,346,362,371-373`; `crates/nif/src/import/mesh.rs:226-250`; `crates/renderer/src/vertex.rs:17-30`
- Bitangent_X (4 or 2 B), Bitangent_Y (1 B normbyte), Tangent (3 B ByteVector3), Bitangent_Z (1 B normbyte) all consumed but assigned to throwaway locals. `Vertex` has no tangent field. Skyrim+ normal-mapped surfaces must reconstruct tangent space from screen-space derivatives — wrong on UV seams and mirrored geometry.
- **Fix**: Extend `BsTriShape` with `tangents: Vec<NiPoint3>` + `bitangent_signs: Vec<f32>`. Add `tangent: [f32; 4]` to renderer `Vertex`. Wire through `ImportedMesh`. Defer to normal-mapping correctness work.

#### B2-01 — Unguarded arithmetic underflow in BSA `extract()` for malformed archives
- **Severity**: MEDIUM | **Dim**: 2 | **Status**: NEW
- **Location**: `crates/bsa/src/archive.rs:227, 236`
- `entry.size as usize - name_prefix_len` underflows if `entry.size < 1 + name_len`. `data_size - 4` underflows if `data_size < 4`. Panic in debug; in release wraps to ~4 GB and aborts on `vec![0u8; ...]`.
- **Impact**: DoS on hostile/corrupt third-party BSA. Vanilla unaffected (verified by 141k-file sweep).
- **Fix**: `checked_sub`, return `io::Error::InvalidData(...)` on `None`. Two-line change.

#### SK-D3-03 — Env-map texture slot (BSShaderTextureSet[4]/[5]) never indexed for EnvironmentMap variant
- **Severity**: MEDIUM | **Dim**: 3 | **Status**: NEW
- **Location**: `crates/nif/src/import/material.rs:259-279`, `crates/nif/src/blocks/shader.rs:154-160`
- Importer reads slots 0/1/2 (diffuse/normal/glow). Slots 3 (parallax), 4 (env cube), 5 (env mask) never touched. For shader_type==1 (EnvironmentMap), env_map_scale is captured but the env cube it should modulate is dropped.
- **Fix**: Add `env_map`, `env_mask`, `parallax_height_map: Option<String>` to `MaterialInfo`. Read slots 3/4/5 in BSLightingShader and BSShaderPPLighting paths. Plumb env_map as fallback when RT reflection misses.

#### SK-D3-06 — Parallax/ParallaxOcc/MultiLayerParallax height-map params parsed but not propagated
- **Severity**: MEDIUM | **Dim**: 3 | **Status**: NEW (subset of SK-D3-01)
- **Location**: `crates/nif/src/blocks/shader.rs:670-688`, `crates/nif/src/import/material.rs:295`
- All four float arms of MultiLayerParallax + both floats of ParallaxOcc dropped at the EnvironmentMap-only match. Skyrim ice walls, snow, parallax-mapped roads/dragon scales render flat.
- **Fix**: Bundle with SK-D3-01.

#### S4-03 — BSEffectShaderProperty alpha not exposed; effect meshes need implicit blend
- **Severity**: MEDIUM | **Dim**: 4 | **Status**: NEW
- **Location**: `crates/nif/src/blocks/shader.rs:847-892`, `crates/nif/src/import/material.rs:300-315`
- BSEffectShaderProperty has no wire `alpha` field — Bethesda gates transparency through falloff_start/stop_opacity + source-texture alpha. Importer only sets `alpha_blend = true` if a sibling NiAlphaProperty exists. Skyrim effect NIFs frequently omit NiAlphaProperty (BGEM is the source of truth) → effect meshes render fully opaque with hard polygon edges.
- **Fix**: Treat presence of BSEffectShaderProperty as implicit `alpha_blend = true`. Default blend mode to ONE/ONE when shader_flags_1 advertises additive. Apply `falloff_start_opacity` as global alpha multiplier when no per-vertex modulation.

#### S4-04 — BSMultiBoundNode `multi_bound_ref` and `culling_mode` never consumed by importer
- **Severity**: MEDIUM | **Dim**: 4 | **Status**: NEW
- **Location**: `crates/nif/src/blocks/node.rs:209-265`, `crates/nif/src/import/walk.rs:46-48`
- Parser reads multi_bound_ref + culling_mode (0 normal / 1 always-visible / 2 always-hidden / 3 force-culled). `as_ni_node` returns `&n.base`, discarding both. BSMultiBound→BSMultiBoundAABB/OBB chain parsed but never consumed. Large interiors (Dragonsreach, Winterhold) lose culling-mode hides + AABB culling hints.
- **Fix**: Surface `multi_bound: Option<MultiBoundData>` on `ImportedNode`. Honor `culling_mode == 2|3` by skipping subtree at walk time. Long-term: feed AABB into renderer culling.

#### S5-02 — All 4 Skyrim shader controllers under-read by 4 bytes (Controlled Variable/Color enum dropped)
- **Severity**: MEDIUM | **Dim**: 5 | **Status**: NEW
- **Location**: `crates/nif/src/blocks/mod.rs:349-364`
- BSEffectShaderProperty{Float,Color}Controller and BSLightingShaderProperty{Float,Color}Controller each add a single trailing `uint` enum field per nif.xml 6253-6266. All four route to `NiSingleInterpController::parse` which doesn't read it. **9,803 occurrences** of "expected 34 consumed 30" warning.
- **Impact**: Every animated waterfall, scrolling cloak, magic glow loses its target-channel selection. Interpolator data imports but Redux doesn't know which shader slot it drives.
- **Fix**: Four parser variants wrapping `NiSingleInterpController::parse` + trailing `read_u32_le()`. Store as tagged `BSShaderController` struct. ~30 LOC.

#### S6-04 — Skyrim CELL extended sub-records (XCLR/XLCN/XCWT/XCAS/XCMO/XCIM/XCLW) silently ignored
- **Severity**: MEDIUM | **Dim**: 6 | **Status**: NEW
- **Location**: `crates/plugin/src/esm/cell.rs:298-401`
- CELL parser handles only EDID/DATA/XCLL. Skyrim XCIM (image-space tone mapping LUT), XCWT (water type), XCLW (water height f32), XCAS (acoustic), XCMO (music), XLCN (location), XCLR (regions) all silently skipped.
- **Impact**: Cells render but with no image-space tone mapping, no per-cell water surface, no acoustic context.
- **Fix**: Extend XCLL match arm with handlers. Add fields to `CellData`.

#### S6-11 — Skyrim TXST has 8 texture slots — only TX00 (diffuse) extracted
- **Severity**: MEDIUM | **Dim**: 6 | **Status**: NEW
- **Location**: `crates/plugin/src/esm/cell.rs:927-955`
- TXST contains TX00 diffuse, TX01 normal, TX02 glow, TX03 height/parallax, TX04 environment, TX05 env_mask, TX06 inner-layer, TX07 specular/back-light. Parser only reads TX00. REFR records use TXST overrides via XTNM — overrides silently degrade to base mesh textures.
- **Fix**: Extract TX00..TX07 into `TextureSet` struct. Wire to (yet-to-be-built) REFR XTNM override path.

#### S6-14 — REFR XESP (enable parent) ignored — default-disabled refs render
- **Severity**: MEDIUM | **Dim**: 6 | **Status**: NEW
- **Location**: `crates/plugin/src/esm/cell.rs:447-494`
- REFR parser handles only NAME/DATA/XSCL. XESP (enable parent: 4-byte FormID + 1-byte flags, bit 0 = inverted) controls initial visibility. Without honoring it, every "spawn after quest stage" REFR renders at cell-load. Many other commonly-seen REFR sub-records (XLCM, XLKR, XAPD, XPRD, XLOC, XPWR, XEZN) also skipped.
- **Impact**: Quest-gated content shows up immediately. Visible clutter in Skyrim cells.
- **Fix**: Add XESP handler. Skip REFR if `enable_parent_form != 0 && !enable_inverted`. Required for clean Skyrim interior cell render.

### LOW (15)

- **S1-02** — `VF_INSTANCE` (bit 9, 0x200) flag constant missing. `tri_shape.rs:267-276`. Bundle with #336 (UVs_2/Land_Data also missing).
- **S1-04** — BSTriShape `data_size` read but never sanity-checked against derived value. `tri_shape.rs:303`. Adding `expected = (vertex_size_quads * num_vertices * 4) + (num_triangles * 6)` assertion would have caught S1-01.
- **S1-05** — Comment misclassifies BSTriShape as Skyrim LE supported. `mesh.rs:521-522,538`. Should say "Skyrim SE".
- **B2-02** — Per-extract `File::open` reopens BSA every call. `archive.rs:210`. Hundreds of syscalls per cell load.
- **B2-03** — Folder/file hashes read but never validated. `archive.rs:98, 131`. Defense-in-depth gap.
- **B2-04** — v105 folder offset field at bytes [16..24] read but unused. `archive.rs:90-102`. Vulnerable to silent corruption on hand-crafted archives.
- **B2-05** — Stored `PathBuf` becomes dead state if B2-02 fixed. Roll into B2-02 patch.
- **SK-D3-04** — Issue #176 premise is wrong — SLSF2 has no decal bit per `nif.xml:6406-6442`. Decals only on SLSF1 bits 26/27, both checked correctly at `material.rs:283`. **Recommend close #176** with nif.xml citation.
- **SK-D3-05** — `find_decal_bs` doesn't check BSEffectShaderProperty. `material.rs:560-569`. Effect-shader decals (blood, gore, magic FX overlays) z-fight against base surfaces. Sibling to #128. Bundle with S4-02.
- **SK-D3-07** — SparkleSnow `sparkle_parameters: [f32; 4]` parsed but dropped. Bundle with SK-D3-01.
- **SK-D3-08** — FO4+ wetness, subsurface_rolloff, rimlight/backlight_power, fresnel_power, grayscale_to_palette_scale parsed but not surfaced. M38-deferred. Skyrim path unaffected (gated `bsver >= 130`).
- **SK-D3-09** — BGSM material reference captured into `material_path` but never resolved. FO4+ only; no Skyrim impact.
- **S4-05** — BSTreeNode `bones_1`/`bones_2` lists discarded. SpeedTree wind sim unreachable (no SpeedTree pass yet).
- **S4-06** — BSRangeNode subclasses (BSBlastNode/BSDamageStage/BSDebrisNode) lose discriminator after walker unwrap. Blocks future destruction system.
- **S4-07** — BSPackedCombinedGeomDataExtra header parses but variable-size pools (per-object data + vertex/triangle pools) skipped. Distant-cell LOD batches contribute no geometry. Existing — referenced by #158.
- **S4-08** — BSEffectShaderProperty `refraction_power` (FO76, BSVER==155) parsed but discarded. Bundle with S4-01.
- **S5-03** — BSTriShape Particle Data Size dropped when Data Size == 0. Same root cause as S5-01; one-line fix covers both. 5,599 warnings.
- **S5-04** — ROADMAP claims sweetroll 1615 FPS without automated benchmark. Only `DebugStats::avg_fps()` in window title. No `cargo bench`, no CI perf-test.
- **S6-01** — `legacy/tes5.rs` is a 14-line `todo!()` stub but never called by runtime. ROADMAP.md:779 misleading. Recommend deleting `legacy/{tes3,tes4,tes5,fo4}.rs`.
- **S6-05** — XCLL 92-byte directional-ambient cube (bytes 40-63: 6×RGBA), specular RGBA (64-67), fresnel power (68-71) all parsed-but-skipped. Per-cell ambient cube is a major Skyrim lighting fidelity element.
- **S6-06** — VMAD (Papyrus script attachment) silently skipped on every record. Behaviorally correct (don't crash) but 0% of script-driven content discoverable. Defer to M30.2/M48.
- **S6-13** — ADDN MODL extracted but DATA/DNAM (master_particle_cap, addonidx, particle config) ignored. Defer until particle-emitter renderer.

---

## Confirmed Working (Positive Findings)

- **BSA v105 LZ4 frame format** — Bethesda emits actual LZ4 frame format (magic `0x184D2204`), not raw block. `lz4_flex::frame::FrameDecoder` at `archive.rs:242` is correct. **Audit prompt's claim of `lz4_flex::block` is wrong.** 141,286 files / 9.5 GB extracted with zero failures.
- **Half-precision IEEE 754 binary16 → f32 decode** (`half_to_f32`, `tri_shape.rs:537-563`): correctly handles ±0, subnormals (normalize loop + exponent rebias), Inf/NaN.
- **Skinning data extraction** (`read_vertex_skin_data`, `tri_shape.rs:524-534`): 4 × hfloat weights + 4 × u8 indices, exactly matches nif.xml `(arg & 0x40) != 0` schema for BSVertexData and BSVertexDataSSE.
- **Skinning import wiring** (#178 closeout): per-vertex weights flow through to `ImportedSkin`; bone palettes resolved via NiSkinInstance+NiSkinData (SSE) or BSSkin::Instance+BSSkin::BoneData (FO4+).
- **Walker `as_ni_node` (`walk.rs:34-66`)** unwraps every parsed Skyrim NiNode subclass: BSFadeNode, BSMultiBoundNode, BSTreeNode, BSOrderedNode, BSValueNode, NiBillboardNode, NiSortAdjustNode, BsRangeNode. Walker is **not** a source of dropped subtrees.
- **Specialty block parsers** — All 13 specialty Skyrim blocks dispatch and parse correctly (see Specialty Block Coverage table above).
- **Sweetroll demo** — Full Vulkan + RT pipeline init, BC1/BC3 textures decoded, BLAS compaction (55% on cube), 112 MB GPU. Renders.
- **Tree LOD** (`dlc2treepineforestlog01ash.nif`) — 4 BSTriShape submeshes, 2,594 verts, 4 textures, batched BLAS 174.5→83.1 KB.
- **fxglow magic effect** — BSEffectShaderProperty parses cleanly (119/119 + 88/88 bytes), 1 effect texture resolved, NiBillboardNode hierarchy formed.
- **Skyrim ESM zlib decompression** — `flate2::ZlibDecoder` at `reader.rs:220-237` handles `FLAG_COMPRESSED` records. Unit-tested but never run on real `Skyrim.esm`.
- **CELL group hierarchy** — Skyrim group-type IDs (1=WRLD children, 2/3=interior, 4/5=exterior, 6/8/9=cell children) handled identically to FO3/FNV.
- **LAND scale** — Skyrim uses same 4096-unit cell, 33×33 grid, 128-unit spacing, ×8.0 VHGT multiplier as FNV. Code path unchanged.
- **MODL extraction** — game-agnostic; works for Skyrim STAT/MSTT/FURN/DOOR/ACTI/CONT/LIGH/MISC/FLOR/TREE/AMMO/WEAP/ARMO/BOOK/KEYM/ALCH/INGR/NOTE/TACT/IDLM/BNDS/ADDN/TERM/NPC_/SCOL/MOVS/PKIN/TXST.
- **NAVM correctly skipped** — `cell.rs:512-513` skips PGRE/PMIS/NAVM via catch-all.
- **HDPT/BPTD/QUST/DIAL/INFO/SCEN/SMQN/PERK/MGEF/SPEL/SHOU groups** all skip without crashing.

---

## Existing Issues Touched

| # | State | Notes |
|---|---|---|
| #106 | OPEN | `BSBehaviorGraphExtraData` u32 vs u8 — produces 922 warnings in archive sweep. Out of dim 5 scope. |
| #109 | OPEN | FO76/Starfield shader property offset drift — adjacent to S4-08, SK-D3-08. |
| #128 | OPEN | BsTriShape `two_sided` misses BSEffectShader. **Subset of S4-02.** |
| #129 | OPEN | BsTriShape duplicates ~130 lines of material extraction. The duplication is what's hiding S4-02; consolidating closes both. |
| #157 | CLOSED | **Regression-adjacent**: dispatch fix works but the underlying base-class header read is off by 4 (S5-01). Test passes against both broken and fixed parsers because `data_size == 0` byte stream is symmetric. Recommend adding non-zero-data variant. |
| #158 | OPEN | BSPackedCombined LOD reconstruction — referenced by S4-07. |
| #166 | OPEN | BSEffectShaderProperty emissive_color/emissive_multiple mislabeled. |
| #176 | OPEN | **Recommend close as not-a-bug** (SK-D3-04). Premise wrong per nif.xml — SLSF2 has no decal bit; SLSF1 26/27 already checked at `material.rs:283`. |
| #178 | CLOSED | Skinning closeout — confirmed working. |
| #331 | OPEN | Havok constraint parsers under-read — produces 13,524 "failed to fill whole buffer" warnings. Recovery via `seeking past block` keeps parse rate at 100%. |
| #336 | OPEN | VF_UVS_2 / VF_LAND_DATA missing flag constants. **S1-02 expands** to also include VF_INSTANCE. |

---

## Suggested Next Action

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-04-16.md
```
