# Skyrim SE Compatibility Audit — 2026-06-23

**Type**: Per-game compatibility audit (Skyrim Special Edition)
**Branch**: `main` · HEAD at audit time `2d4c350d`
**Scope**: 7 dimensions — BSTriShape packed geometry + SSE skinned reconstruction,
BSLightingShaderProperty/BSEffectShaderProperty shader-type dispatch, NPC equip + FaceGen (M41),
multi-master load order + TES5 cell-load regression, BSA v105 (LZ4), specialty blocks + real-data
rendering, NIFAL canonical material translation (Skyrim slice).
**Dedup baseline**: `/tmp/audit/issues.json` (28 open issues) + prior `docs/audits/AUDIT_SKYRIM_2026-06-18.md`.
**Real data**: `Skyrim Special Edition/Data/` present (Skyrim.esm, Dawnguard.esm, Dragonborn.esm,
HearthFires.esm, Meshes0–1 + Textures0–8 BSAs). Heavy corpus re-burned this session.

---

## Executive Summary

Skyrim SE is the engine's renderer **control bench** — Whiterun BanneredMare loads as a full cell
with 6 named equipped NPCs, and both loose-mesh and cell rendering work. This audit is therefore
**regression coverage** plus the Skyrim-specific geometry/shader/equip risk surface, not readiness
scoping.

**The audit is clean. No CRITICAL or HIGH findings.** Both heavy parse dimensions (BSTriShape
packed geometry, shader-type dispatch) were re-read field-for-field against the live code and
re-validated on the real `Skyrim - Meshes0.bsa` + `Meshes1.bsa` corpus: **22,047 NIFs parsed,
100% clean, 0 truncated, 0 failures, 0 recovered, 0 unknown blocks across 87 distinct block
types, and ZERO realignment/mismatch WARNs**. `Skyrim.esm` walks cleanly (590 cells, all with
XCLL + Skyrim extended lighting; `SolitudeWinkingSkeever` resolves).

**Notable positive delta since the 2026-06-18 audit**: three issues that the prior audit
recorded as real-and-unfixed have **landed fixes in the interim and are no longer in the open
set**:
- **#1553** (`.STRINGS` loader never wired into the multi-plugin path) → fixed `db5bb149`.
- **#1554** (ESL `0x0200` light-master FormID undecoded) → fixed `59d3f007`.
- **#1661** (numeric-sibling BSA auto-load skipped the `Textures0`-suffixed base) → fixed
  `821a425b`. **This issue is still OPEN on GitHub but the fix has landed** — flagged below as a
  stale-open dedup item, not a new finding.

The remaining surface is carry-over only: one MEDIUM defense-in-depth gap (deleted-REFR
tombstones, #1660), one MEDIUM import-completeness gap (BSDismember partition flags discarded,
#1659), and the test-coverage / hardening LOWs already on file.

### Finding Tally

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 new (2 carry-over existing issues confirmed still present) |
| LOW      | 0 new (1 stale-open dedup item: #1661 fix landed, issue not closed) |

No new findings filed. All Skyrim-specific risk surfaces re-verified clean.

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction — CLEAN

Re-read `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`,
`crates/nif/src/import/mesh/bs_tri_shape.rs`, `crates/nif/src/import/mesh/sse_recon.rs`.

- **`VF_*` flag mapping** is complete and matches nif.xml `BSVertexDesc.VertexAttribute`
  (VERTEX/UVS/UVS_2/NORMALS/TANGENTS/COLORS/SKINNED/LAND/EYE/INSTANCE/FULL_PRECISION). Every bit
  has a const even where the decoder is deferred; the trailing
  `consumed < vertex_size_bytes` skip absorbs deferred-bit bytes so the per-vertex cursor stays
  aligned. Half→f32 decode (`half_to_f32`) is IEEE-754 binary16.
- **`decode_bs_vertex_stream`** handles every flag combination; the consumed-vs-stride guard
  hard-errors on over-read rather than wrapping. Index read is the bulk `read_u16_triple_array`.
- **SSE skinned reconstruction tangent path** (#559/#1204): positions/normals are Z-up→Y-up
  converted (`[x, z, -y]`), and the on-disk "bitangent" triplet is correctly routed as the Y-up
  tangent (∂P/∂U) at `sse_recon.rs` lines ~370–381, with the bitangent sign derived from the
  on-disk tangent (∂P/∂V) via `crate::types::bitangent_sign` (operand order shared with the
  authored producer, #1516). No magenta/chrome regression. The #1559 fix (tangent gated on
  `VF_TANGENTS` alone, not `&& VF_NORMALS`) is in place and matches the inline parser.
- **Alpha-property cascade** is gated on `alpha_property_consumed` at both walker sites
  (`material/walker.rs:530`, `:606`), set once in `material/mod.rs:1101`. Skinned geometry
  inherits the parent `NiAlphaProperty` exactly once. Regression tests in
  `material/alpha_flag_tests.rs` pin the gate.

**Real-data**: 52,196 `BSTriShape` + 21,140 `BSDynamicTriShape` + 26,708 `NiSkinPartition`
(SSE recon carriers) in Meshes0 — 0 unknown.

### Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch — CLEAN

Re-read `crates/nif/src/blocks/shader.rs` (full file).

- **Skyrim dispatch** (`parse_shader_type_data`) handles types 1 (EnvironmentMap),
  5 (SkinTint Color3), 6 (HairTint Color3), 7 (ParallaxOcc), 11 (MultiLayerParallax),
  14 (SparkleSnow ×4), 16 (EyeEnvmap = scale + two reflection centers). All other types
  (0/2/3/4/8–10/12/13/15/17–20) fall through to `None` with no over-read — verified against the
  Skyrim `BSLightingShaderType` enum.
- **FO76 vs Skyrim/FO4 enum separation** holds: `parse_shader_type_data_fo76` admits only
  {0,2,3,4,5,12,17}, with type 4 = `Fo76SkinTint` Color4 and type 5 = HairTint Color3 — the
  distinct `BSShaderType155` numbering. No cross-contamination between the three dispatch tables.
- The three per-BSVER parsers (`parse_skyrim` 83–129, `parse_fo4` 130–154, `parse_fo76_plus`
  ≥155) split at the boundaries the wire format actually changes shape; the Skyrim parser reads
  `lighting_effect_1/2` and dispatches through the legacy table. `BSEffectShaderProperty` carries
  `soft_falloff_depth`, `greyscale_texture`, `lighting_influence`, `env_map_min_lod`, falloff
  angle/opacity, base color (the #166 emissive→base_color rename is semantic-only).
- **Disney/Burley lobe regression guard — CONFIRMED HELD**: `MAT_FLAG_PBR_BSDF` (`PBR_BSDF =
  1 << 5`, `material.rs:461`) is set only by `cell_loader::pack_bgsm_material_flags`, which ORs
  the bit only when `mesh.is_pbr` is true. `is_pbr` is flipped exclusively by the BGSM/BGEM/.mat/
  CDB merge paths in `asset_provider.rs` (FO4+/Starfield). Vanilla Skyrim BSLightingShaderProperty
  meshes have no BGSM, so the flag stays clear and the principled BRDF in
  `crates/renderer/shaders/include/pbr.glsl` is unreachable for vanilla content. Modded BGSM that
  opts into PBR is the one legitimate flip.

**Real-data**: 67,105 `BSLightingShaderProperty` in Meshes0 — 0 unknown.

### Dimension 3 — NPC Equip + FaceGen (M41) — PARSE-VALIDATED (runtime equip-count not exercised)

Re-read `byroredux/src/npc_spawn.rs`.

- The equip path populates `Inventory` + `EquipmentSlots` per NPC, walks OTFT.items + LVLI via
  `byroredux_plugin::equip::expand_leveled_form_id` (level-gated), and dispatches armor meshes via
  `resolve_armor_mesh` (ARMO → ARMA → worn-mesh).
- The body-slot pre-scan (Phase A.2, lines ~482–543) skips the hardcoded vanilla `upperbody.nif`
  when an equipped main-body armor covers it, killing the documented z-fight + double bone-palette
  overhead.
- `BSDynamicTriShape` facegen heads parse (21,140 in Meshes0, 0 unknown); fidelity is
  parse-only by design (no render-time FaceGen morph).
- **Not exercised**: the live 6-named-NPC equip-count guard (`saadia`, `brenuin`, `mikael`,
  `sinmir`, `amaundmotierreend`, `hulda`) requires a Vulkan device + cell render, out of
  `cargo test` scope. This is the gap tracked by existing issue **#1560** (M41 equip smoke test
  soft-warns on zero equip components; no 6-named-NPC count guard) — confirmed still open, not
  re-filed.

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression — CLEAN (with positive delta)

Re-read `byroredux/src/cell_loader/load_order.rs`, `crates/plugin/src/esm/reader.rs`,
`crates/plugin/src/esm/cell/tests/integration.rs`.

- **Repeatable `--master` FormID remap** (M46.0/#561): `build_remap_for_plugin` resolves each
  plugin's TES4 `master_files` to global load-order slots; missing or misordered masters fail
  loudly with the named plugin. `plugin_for_form_id` names the missing master on unresolved REFRs.
- **`.STRINGS` wired in — #1553 NOW FIXED** (`db5bb149`): `install_strings_guard` is invoked per
  plugin inside `parse_record_indexes_in_load_order` (line 200) with a RAII guard held across each
  plugin's record walk, so DLC-owned localized names resolve. End-to-end tests
  (`localized_plugin_resolves_names_through_load_order` + negative control) pin the wiring.
- **ESL `0x0200` light-master decode — #1554 NOW FIXED** (`59d3f007`): TES4 flag `0x0200` →
  `FileHeader::light_master` → `GlobalSlot::Light`; `GlobalSlot::compose` produces
  `0xFE00_0000 | ((sub & 0x0FFF) << 12) | (raw & 0x0FFF)`. Tests
  (`esl_plugin_own_forms_land_in_light_space`, `compose` unit tests) confirm forms land in the
  0xFE space, not at a flat top-byte index.
- **Control-bench guard**: `parse_real_skyrim_esm` (real `Skyrim.esm`) walks 590 cells, finds
  Winking Skeever, 590/590 cells with XCLL + Skyrim extended lighting fields. TES5 compressed
  records decompress; the unified `esm/cell/` walker stays green.

### Dimension 5 — BSA v105 (LZ4) — CLEAN (with positive delta)

Re-read `crates/bsa/src/archive/{open,extract}.rs`.

- **v105 header + folder records** (24-byte v105 vs 16-byte v103/v104) parse correctly;
  embedded-name flag (`0x100`, version-gated to FO3+) handled; compression-toggle XOR
  (`compressed_by_default != entry.compression_toggle`) selects per-file compression.
- **LZ4 decode**: v105 uses `lz4_flex::frame::FrameDecoder` (LZ4 *frame* format), v103/v104 uses
  zlib. Post-decompression size-mismatch sanity check at WARN. *(Note: the skill's prose says
  "LZ4 block via `lz4_flex::block`" — the code correctly uses LZ4 **frame**, which is what
  Skyrim SE v105 actually ships. This is a skill-doc imprecision, not a code bug.)*
- **Real-data extraction**: `meshes\clutter\ingredients\sweetroll01.nif` lists and extracts from
  `Skyrim - Meshes0.bsa` (LZ4-frame round-trip confirmed by the 100%-clean parse of all 18,862
  internal NIFs).
- **Zero-based sibling auto-load — #1661 NOW FIXED** (`821a425b`): `numeric_sibling_paths`
  handles the `…0` series-start case (`Textures0` → strip the `0`, offer `…1`..`…9`), guards
  against `…10` (explicit member), and offers `…2`..`…9` for the unnumbered FNV base. Regression
  test `siblings_skyrim_zero_start_offers_1_through_9` pins it. **#1661 is still OPEN on GitHub
  but the code is fixed** — see Dedup Notes.

### Dimension 6 — Specialty Blocks + Real-Data Rendering — CLEAN

Re-read the block dispatch + ran the full Meshes0/Meshes1 sweep.

- **`BSLODTriShape` routed through `NiLodTriShape`, NOT BSTriShape** (#838 guard): 23
  `BSLODTriShape` blocks in Meshes0, all 0 unknown — distinct body from `BSMeshLODTriShape` /
  `BSSubIndexTriShape` (15,726 `BSDismemberSkinInstance` also clean).
- **`BsLagBoneController` (163) + `BsProceduralLightningController` (3)** (#837): dedicated
  parsers live, 0 unknown — no by-design `block_size` WARN burst.
- **`BSTreeNode` (20)** SpeedTree wind-bone list parses clean.
- **Meshes0 baseline**: 18,862 NIFs, 100.00% clean, 0 truncated, 0 failures, 0 recovered, 0
  realignment WARNs. **Meshes1 baseline**: 3,185 NIFs, 100.00% clean. The clean-corpus invariant
  holds.

### Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice) — CLEAN

Re-read `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`.

- **Single canonical boundary**: `translate_material` is the only `ImportedMesh → Material`
  path; `Material.metalness`/`roughness` are plain `f32` seeded from override or `f32::NAN`
  sentinel and filled by `Material::resolve_pbr` (which delegates to `classify_pbr_keyword`).
  **The per-draw `Material::classify_pbr` is deleted** — only `classify_pbr_keyword` (the
  classifier) and `resolve_pbr` (the once-at-spawn resolver) exist. No render-time PBR
  classification.
- **Ordering**: `material.resolve_pbr()` (line 160) runs **before**
  `crate::helpers::classify_glass_into_material` (line 161), so forced-glass roughness wins over
  the keyword default.
- **`EmissiveSource` discriminator (#1280)**: Skyrim `BSLightingShaderProperty.emissive_multiple`
  routes through `EmissiveSource::Lighting` (`material/walker.rs:344`), and
  `BSEffectShaderProperty` routes through `EmissiveSource::Effect` (`:401`). Tests in
  `material/emissive_source_tests.rs` pin both — Skyrim emissive is `Lighting`, never `Effect`.

---

## Shader-Type Coverage Matrix (`ShaderTypeData`, Skyrim `BSLightingShaderType`)

| # | Type | Variant | Parse | Trailing fields | Notes |
|---|------|---------|-------|-----------------|-------|
| 0 | Default | `None` | ✓ | none | falls through |
| 1 | EnvironmentMap | `EnvironmentMap` | ✓ | env_map_scale (f32) | |
| 2 | Glow | `None` | ✓ | none | **no `GlowShader` variant** by design |
| 3 | Parallax | `None` | ✓ | none | |
| 4 | Face Tint | `None` | ✓ | none | |
| 5 | SkinTint | `SkinTint` | ✓ | Color3 | |
| 6 | HairTint | `HairTint` | ✓ | Color3 | |
| 7 | ParallaxOcc | `ParallaxOcc` | ✓ | max_passes + scale | |
| 8–10 | Landscape | `None` | ✓ | none | |
| 11 | MultiLayerParallax | `MultiLayerParallax` | ✓ | thickness + refr_scale + inner UV scale + envmap_strength | |
| 12–13 | Tree/LOD | `None` | ✓ | none | |
| 14 | SparkleSnow | `SparkleSnow` | ✓ | ×4 params | |
| 15 | LOD HD | `None` | ✓ | none | |
| 16 | EyeEnvmap | `EyeEnvmap` | ✓ | cubemap scale + L/R reflection centers | |
| 17–20 | Cloud/Noise | `None` | ✓ | none | |

FO76 path uses the separate `BSShaderType155` table (4=`Fo76SkinTint` Color4, 5=HairTint Color3),
no cross-contamination.

---

## Cell-Load Regression Status

- TES5 cells parse through the unified `esm/cell/` walker; compressed records decompress.
- `Skyrim.esm`: 590 cells, 18,113 statics, 37 worldspaces; `SolitudeWinkingSkeever` resolves
  (981 refs, lighting=true, ambient/directional/fog all populated); 590/590 cells carry XCLL +
  Skyrim extended lighting fields.
- Multi-master, `.STRINGS`, and ESL `0xFE` decode all wired and tested (#1553/#1554 fixed).
- **Control-bench note**: Whiterun BanneredMare entity-count + FPS vs the ROADMAP Bench-of-record
  (R6a-stale-14) requires a Vulkan device + live cell render — **not exercised** this session
  (headless audit). The parse/index half of the bench (cell + record walk) is green.

---

## Dedup Notes

- **#1553 / #1554** — recorded as real-and-unfixed by the 2026-06-18 audit; **both now fixed**
  in the interim (`db5bb149`, `59d3f007`) and absent from the open set. No action.
- **#1661 (SKY-D5-01, LOW, OPEN)** — the numeric-sibling skip it describes is **fixed** by
  `821a425b`; the live `numeric_sibling_paths` handles the `Textures0` zero-based start and a
  regression test pins it. **The GitHub issue is stale-open and should be closed** (or verified
  against `siblings_skyrim_zero_start_offers_1_through_9`).
- **#1660 (SKY-D4-01, MEDIUM, OPEN)** — deleted-REFR tombstones (0x20 Deleted flag) still not
  captured; `crates/plugin/src/esm/cell/mod.rs:936` documents the gap explicitly. Carry-over,
  confirmed still present.
- **#1659 (SKY-D3-03, MEDIUM, OPEN)** — `BSDismemberSkinInstance` per-partition body-part flags
  are parsed but not routed into the skin import. Carry-over, confirmed still present (no
  `bs_sub_index`/body-part flag consumer in `import/mesh/skin.rs`).
- **#1560 (SK-D3-02, OPEN)** — M41 equip smoke test lacks a 6-named-NPC count guard. Carry-over,
  confirmed still present (Dimension 3).

---

## Verification Commands Run

```
cargo build --release                                   # exit 0
cargo test -p byroredux-plugin parse_real_skyrim_esm -- --ignored   # 1 passed
cargo run --release -p byroredux-nif --example nif_stats -- "Skyrim - Meshes0.bsa"  # 18862 / 100% clean / 0 unknown
cargo run --release -p byroredux-nif --example nif_stats -- "Skyrim - Meshes1.bsa"  # 3185 / 100% clean / 0 unknown
RUST_LOG=warn ... nif_stats Meshes0                     # 0 realignment/mismatch WARNs
cargo test -p byroredux-nif                             # all green (835 + sibling suites)
cargo test -p byroredux-bsa -p byroredux-plugin -p byroredux  # all green
```

---

*Suggested next step:* `/audit-publish docs/audits/AUDIT_SKYRIM_2026-06-23.md` (note: there are
no new findings to file; the publish step would only confirm dedup state and could close the
stale-open #1661).
