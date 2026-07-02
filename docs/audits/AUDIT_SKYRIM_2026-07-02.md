# Skyrim SE Compatibility Audit — 2026-07-02

**Scope**: Regression coverage of ByroRedux's Skyrim Special Edition support plus
the Skyrim-specific geometry / shader / equip / load-order risk surface.
**Type**: Per-game audit (`/audit-skyrim`), all 7 dimensions.
**Method**: Each dimension re-verified against live source; corpus test suites
executed; real `Skyrim.esm` parsed on-disk.

## Executive Summary

Skyrim SE is the engine's **renderer control bench** (Whiterun BanneredMare,
6 equipped NPCs); both loose-mesh and cell rendering work today. This audit is
regression coverage, and the result is **clean**: every documented Skyrim-specific
guard (#838 BSLODTriShape routing, #559 SSE skinned reconstruction, #795/#796
inline-tangent convention, #1201/#1202 alpha cascade, #1280 EmissiveSource,
#1510/#1552 shader-type gating, #1554 ESL FormID decode, `db5bb149` STRINGS
wiring) is intact and covered by passing tests.

**No NEW findings survived verification.** All Skyrim-adjacent defects are
pre-existing, correctly-tracked OPEN issues (see the Existing-Issue table).

### Verification evidence

| Signal | Result |
|--------|--------|
| `cargo test -p byroredux-nif --lib` | **846 passed / 0 failed** |
| `cargo test -p byroredux-plugin --lib` | **522 passed / 0 failed** (13 ignored = data-gated) |
| `cargo test -p byroredux-facegen --lib` | **24 passed / 0 failed** |
| `cargo test -p byroredux-bsa --lib` | **52 passed / 0 failed** (11 ignored = data-gated) |
| `parse_real_skyrim_esm` (real on-disk `Skyrim.esm`) | **590/590 cells; Winking Skeever found w/ full XCLL lighting** |
| Skyrim SE `Data/` present | ✓ (`Skyrim - Meshes*.bsa`, `Textures*.bsa`, DLC ESMs) |

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction — CLEAN
`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`,
`crates/nif/src/import/mesh/sse_recon.rs`

- All `VF_*` attribute bits (VERTEX / UVS / UVS_2 / NORMALS / TANGENTS / COLORS /
  SKINNED / LAND / EYE / INSTANCE / FULL_PRECISION) are declared and mapped to
  the correct bit positions; half-precision `half_to_f32` decode is IEEE-754
  binary16 correct.
- The `data_size`-derived stride fallback (#621) and the `num_vertices != 0`
  warning gate (#836) are both present, so SSE skinned bodies (`data_size` on a
  sister `NiSkinPartition`) don't emit false-positive WARNs and the per-vertex
  loop stays aligned.
- **SSE skinned-tangent path (#559 / #795 / #796) is correct** — the on-disk
  "bitangent" triplet (`bitangent_x/y/z`) is reassembled into the `tangents.xyz`
  slot as ∂P/∂U, Z-up→Y-up converted via `math::coord::zup_to_yup_pos`
  (`sse_recon.rs:380`), with the sign derived from the on-disk tangent (∂P/∂V)
  through the shared `bitangent_sign` helper (sign is rotation-invariant). This
  is the exact regression guard against magenta/chrome reconstructed bodies; it
  is intact and mirrors the inline `decode_bs_vertex_stream` path bit-for-bit.
- **Alpha-property cascade (#1201 / #1202)** is gated on `alpha_property_consumed`
  — set once in `import/material/mod.rs:1101`, consulted at the two gate sites in
  `import/material/walker.rs:533` / `:609`. Covered by `alpha_flag_tests.rs`.

### Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Dispatch — CLEAN
`crates/nif/src/blocks/shader.rs`

- The BSVER-split dispatch (`parse_skyrim` 83–129 / `parse_fo4` 130–154 /
  `parse_fo76_plus` ≥155) routes Skyrim SE (bsver 100) through `parse_skyrim`.
  `parse_shader_type_data` handles the Skyrim `BSLightingShaderType` numbering:
  types 1/5/6/7/11/14/16 read their trailing fields; **types
  0,2,3,4,8,9,10,12,13,15,17,18,19,20 fall through to `None`** with no over-read —
  matches nif.xml. There is no `GlowShader` variant (type 2 → `None`), as
  documented.
- FO76 (`parse_shader_type_data_fo76`) uses the distinct `BSShaderType155`
  numbering (4 = `Fo76SkinTint` Color4, 5 = HairTint Color3) and does not
  cross-contaminate the Skyrim/FO4 enum. Verified against
  `nif.xml:1425-1434` per the in-code #623 note.
- `BSEffectShaderProperty` carries `soft_falloff_depth` / `greyscale_texture`
  (aliased through the effect fields), `lighting_influence`, `env_map_min_lod`,
  and the falloff angle/opacity quartet.
- **Disney/Burley lobe stays unreachable for vanilla Skyrim.** `MAT_FLAG_PBR_BSDF`
  (`#define … 32u`, shader `material_flag::PBR_BSDF = 1<<5`) is set **only** via
  `ImportedMesh.is_pbr` on the BGSM merge path (`renderer/src/vulkan/material.rs:688`).
  BGSM is FO4+; vanilla Skyrim authors no BGSM, so `is_pbr` is never set on the
  Skyrim.esm material universe and the lobe stays gated off. Regression guard intact.

### Dimension 3 — NPC Equip + FaceGen (M41) — CLEAN (one Existing-issue gap)
`byroredux/src/npc_spawn.rs`, `crates/facegen/src/`

- The 6-NPC Whiterun outfit chain is present: `resolve_armor_mesh` (ARMO→ARMA→
  worn-mesh, `npc_spawn.rs:485`), `expand_leveled_form_id` LVLI flattening
  gated on actor level (`:437` / `:457` / `:653`), and the `upperbody.nif`
  body-slot pre-scan skip (`:233`). ROADMAP Bench-of-record confirms the 6 named
  NPCs (saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda) land
  `Inventory` + `EquipmentSlots`.
- FaceGen `.tri`/`.egt`/`.egm` parse cleanly (24 facegen tests green); render-time
  morph fidelity is out of scope by design.
- **Existing #1659 (SKY-D3-03)**: `BsDismemberSkinInstance` geometry *is* consumed
  for skinning (`import/mesh/skin.rs:36/135/311/386`), but the per-partition
  body-part *flags* (`BodyPartInfo`, `blocks/skin.rs:377`) are discarded at the
  import boundary. Not a regression — dismemberment is an unshipped feature; the
  enhancement is correctly tracked.

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load — CLEAN
`byroredux/src/cell_loader/load_order.rs`, `crates/plugin/src/esm/`

- Repeatable `--master` FormID remap (M46.0 / #561): per-plugin TES4
  `master_files` header drives the merged global FormID space; last-write-wins.
- **`.STRINGS` loader wired per-plugin (`db5bb149`)** — `install_strings_guard`
  (`load_order.rs:200`) is invoked in the per-plugin record walk gated on the
  TES4 `0x80` Localized flag, so DLC-owned localized names resolve. Covered by
  `localized_plugin_resolves_names_through_load_order`.
- **ESL / light-master decode (#1554)** — `reader.rs:295` decodes
  `0xFE00_0000 | ((sub & 0x0FFF) << 12) | (raw & 0x0FFF)` driven by
  `light_master` (TES4 `0x0200`). Covered by
  `esl_self_ref_form_remaps_into_light_space`.
- Real `Skyrim.esm` walks through the unified `esm/cell/` walker: 590 cells,
  18113 statics, 37 worldspaces, all 590 with XCLL; Winking Skeever resolves.

### Dimension 5 — BSA v105 (LZ4) — CLEAN
`crates/bsa/src/archive/`

- v105 header + 24-byte folder records + u64 offsets + LZ4 block decompression
  (`archive/mod.rs:36` `BSA_V_SKYRIM_SE = 105`; extract dispatches zlib v103/v104
  vs LZ4 v105). 52 BSA tests green.
- Zero-based sibling auto-load (`821a425b`): `open_with_numeric_siblings`
  (`asset_provider/archive.rs:306`) auto-opens `<stem>2.bsa`..`<stem>9.bsa`,
  so `Textures0.bsa` drags in `Textures1..8`. ROADMAP confirms Whiterun loads
  246 unique textures across the sibling set with this fix.

### Dimension 6 — Specialty Blocks + Real-Data Rendering — CLEAN
`crates/nif/src/blocks/mod.rs`, `byroredux/src/cell_loader/terrain_lod_btr.rs`

- **#838 guard holds**: `"BSLODTriShape"` → `NiLodTriShape::parse`
  (`blocks/mod.rs:452`), `"BSMeshLODTriShape"` → `BsTriShape::parse_meshlod`
  (`:453`), `"BSSubIndexTriShape"` → structured segmentation decode (`:466`).
  The three share no body confusion.
- `BsLagBoneController` (`:840`) + `BsProceduralLightningController` (`:842`)
  have dedicated parsers (#837) — no by-design `block_size` WARN burst.
- M35 prebaked `.btr` distant terrain (`9384d4c2`): `terrain_lod_btr::spawn_btr_block`
  is wired from `terrain_lod.rs:306`, gated on `GameKind::Skyrim | Fallout4` and
  `mask == 0`, with the loose-mesh fallback preserved.

### Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice) — CLEAN
`byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`

- `translate_material` is the single boundary; `resolve_pbr()`
  (`material_translate.rs:160`) runs **before** `classify_glass_into_material`
  (`:161`), so forced-glass roughness wins over the keyword default. `Material`
  metalness/roughness are plain resolved `f32`; no render-time `classify_pbr`.
- **EmissiveSource (#1280)**: Skyrim `BSLightingShaderProperty.emissive_multiple`
  is tagged `EmissiveSource::Lighting` (`import/material/walker.rs:347`), while
  `BSEffectShaderProperty` is tagged `EmissiveSource::Effect` (`:404`) — the two
  are NOT conflated. Covered by `emissive_source_tests.rs`.

## Shader-Type Coverage Matrix (Skyrim `BSLightingShaderType`)

| Type | Name | Trailing data | Parse | Import | Render |
|------|------|---------------|-------|--------|--------|
| 0 | Default | None | ✓ | ✓ | ✓ |
| 1 | Environment Map | `env_map_scale` f32 | ✓ | ✓ | ✓ |
| 2 | Glow | None (no GlowShader variant) | ✓ | ✓ | ✓ |
| 3 | Parallax | None | ✓ | ✓ | ✓ |
| 4 | Face Tint | None | ✓ | ✓ | ✓ |
| 5 | Skin Tint | Color3 | ✓ | ✓ | ✓ |
| 6 | Hair Tint | Color3 | ✓ | ✓ | ✓ |
| 7 | Parallax Occ | max_passes + scale | ✓ | ✓ | partial |
| 8–10 | Landscape | None | ✓ | ✓ | ✓ |
| 11 | Multi-Layer Parallax | 4 inner-layer fields | ✓ | ✓ | partial |
| 12–13 | Tree / LOD | None | ✓ | ✓ | ✓ |
| 14 | Sparkle Snow | 4 params | ✓ | ✓ | partial |
| 15 | LOD HD | None | ✓ | ✓ | ✓ |
| 16 | Eye Envmap | cubemap scale + 2 centers | ✓ | ✓ | partial |
| 17–19 | Cloud / Noise | None | ✓ | ✓ | ✓ |

("partial" render = trailing params parsed and available in `MaterialInfo` but
the dedicated render path for that effect is not yet a distinct shader branch;
these are pre-existing feature gaps, not parse defects.)

## Cell-Load Regression Status

TES5 cells parse through the unified `esm/cell/` walker; compressed records
decompress; XCLL lighting + Skyrim extended fields fill 590/590 on real
`Skyrim.esm`. Whiterun BanneredMare control-bench figures (3216 ent / 362.8 FPS /
fence 0.98) remain the ROADMAP Bench-of-record (R6a-stale-14) — **note this bench
is 437 commits stale** (ROADMAP Session 53), so any current FPS claim is gated on
a fresh R6a-stale-15 run. This audit made no code changes and observed no entity-
count or parse-rate regression.

## Existing Open Issues Touching Skyrim (not re-reported)

| # | Title | Dimension | Note |
|---|-------|-----------|------|
| #1698 | RT-1: Skyrim Dragonsreach FPS collapse (321→8.7), scheduler stalls ~140 ms/frame | 4/6 (perf) | Live perf regression on a Skyrim exterior-adjacent cell; tracked separately. |
| #1659 | SKY-D3-03: BSDismemberSkinInstance per-partition body-part flags discarded at import | 3 | Skinning geometry works; only dismemberment slot metadata is dropped. |
| #1731 | LC-D7-02: VWD "Has Distant LOD" record-header flag (0x00010000) not parsed | 6 | Distant-LOD record flag; affects LOD selection metadata. |

## Findings Total

- CRITICAL: 0
- HIGH: 0
- MEDIUM: 0
- LOW: 0
- **NEW findings: 0** — all Skyrim-adjacent defects are pre-existing OPEN issues.

Skyrim SE support is in a healthy, well-guarded state. The dimension-level guards
this audit exists to protect are all present and test-covered.

---
*Recommended next step: refresh the R6a-stale bench (per ROADMAP) — it is the only
Skyrim signal currently stale enough to hide a perf regression.*
