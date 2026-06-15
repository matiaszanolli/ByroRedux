# AUDIT — The Elder Scrolls V: Skyrim Special Edition

**Date**: 2026-06-14
**Branch**: `main`
**Scope**: Per-game Skyrim SE compatibility — BSTriShape packed geometry + SSE skinned
reconstruction, BSLightingShader/BSEffectShader shader-type dispatch, NPC equip +
FaceGen (M41), multi-master load order + TES5 cell-load, BSA v105 (LZ4), specialty-block
dispatch + real-data rendering, NIFAL canonical material translation.
**Methodology**: Seven-dimension orchestrated audit; per-dimension subagents
(legacy / renderer / general-purpose). Each finding re-verified against the live tree by
re-reading the code path and attempting to disprove it. Deduplicated against
`/tmp/audit/issues.json` (23 issues) and `docs/audits/AUDIT_SKYRIM_*.md`.
**Game data**: Skyrim SE Data dir present
(`/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/`,
Skyrim.esm + Dawnguard/Dragonborn/HearthFires + Meshes0/Textures0-8 BSAs) — real-data
portions exercised.

---

## Executive Summary

**Skyrim SE is the engine's renderer control bench** — single-plugin Whiterun
BanneredMare renders end-to-end (3,216 entities / 362.8 FPS per ROADMAP Bench-of-record
`1c26bc25`) with 6 equipped NPCs. This audit is regression coverage plus the
Skyrim-specific geometry/shader/equip/load-order risk surface. The control bench is
**not regressed**: no commit on the single-plugin cell-load path changes which REFRs
get spawned, and Whiterun is immune to the one new HIGH (which only triggers under
multi-master DLC override).

The big new finding is in the **multi-master path**: cross-plugin cell overrides
(`--master Skyrim.esm --esm Dawnguard.esm --cell <overridden>`) currently stomp the
base-game cell's entire REFR list with the DLC's partial-record handful, rendering
overridden interiors near-empty. The second HIGH is a hard-crash hazard on the SSE
skinned-body reconstruction path (unbounded slice indexing) — not reachable on
consistent vanilla data, but a process crash on malformed/modded content.

| Dim | Surface | NEW | Notes |
|----|---------|-----|-------|
| 1 | BSTriShape packed geom + SSE recon | 1 MED + 1 LOW | half_to_f32, VF_* flags, tangent sign (#1516 guard), alpha-gate all CLEAN |
| 2 | BSLightingShader / BSEffectShader dispatch | 1 LOW | Skyrim path complete; one FO4-band over-read unreachable from Skyrim |
| 3 | NPC equip + FaceGen (M41) | 1 HIGH + 1 LOW | equip chain / LVLI / armor-mesh / FaceGen parse all CLEAN |
| 4 | Multi-master load order + TES5 cell-load | 1 HIGH + 2 LOW | remap math + real-ESM walk + compressed-record + record-set all CLEAN |
| 5 | BSA v105 (LZ4) | 1 LOW | frame-codec / stride / flags / sweep all CLEAN (checklist premise was wrong) |
| 6 | Specialty blocks + real-data render | 0 | #837/#838 guards hold; Meshes0 sweep 0 unknowns |
| 7 | NIFAL canonical material (Skyrim slice) | 0 | single-boundary / no-render-fallback / EmissiveSource all CLEAN |

**Totals**: CRITICAL=0 · HIGH=2 · MEDIUM=1 · LOW=6 · **TOTAL=9**.

---

## Dimension Findings (by severity)

### SK-D4-01: Cross-plugin cell override discards all base-game REFRs

- **Severity**: HIGH
- **Dimension**: Multi-Master Load Order + TES5 Cell-Load (Dim 4)
- **Location**: `crates/plugin/src/esm/cell/mod.rs:944-952` (`EsmCellIndex::merge_from`)
- **Status**: NEW (no matching open/closed issue; no prior audit mention)
- **Description**: `merge_from` folds each plugin's cell index into the running index
  with `self.cells.extend(other.cells)` — a whole-value `HashMap<String, CellData>`
  overwrite keyed by editor-id. The doc comment states the intent explicitly: "DLC
  redefining a base cell wins the entire CellData (REFRs, lighting, water level)." But a
  Bethesda DLC override cell is a **partial** record: the engine is supposed to merge
  *per-REFR by FormID*, keeping the base game's references and applying only the DLC's
  added/changed/deleted ones. Replacing the whole `references` vec drops every base REFR
  the DLC didn't re-emit. The exterior per-grid path has the same defect at
  `mod.rs:947-952`.
- **Evidence**:
  - `mod.rs:945` — `self.cells.extend(other.cells);` (whole-value overwrite).
  - Reproduced (temporary probe against real `Skyrim.esm` + `Dawnguard.esm`, since
    reverted): 57 interior cells overlap by editor-id, and in all 57 the DLC re-emits
    far fewer refs — `riftenraggedflagon` base=826 → dg=5; `chillwinddepths01`
    3153 → 28; `kagrenzel01` 1017 → **0**.
  - Runtime spawn (`cell_loader/load.rs:225,237`) consumes the stomped
    `cell.references` directly — no per-REFR re-resolution downstream to mask it.
- **Impact**: `--master Skyrim.esm --esm Dawnguard.esm --cell <overridden>` renders
  near-empty / empty cells for the 57 DLC-overridden interiors (and the exterior
  equivalent). Multi-master DLC is M46.0's headline use case. Single-plugin cells
  (Whiterun control bench) are unaffected, which is why prior audits — which validated
  the FormID *remap* as green — never caught the *merge* stomp.
- **Related**: M46.0 / #561 (the remap this merge sits beside). Remap math is correct
  (verified, 41 tests); the defect is purely the merge granularity.
- **Suggested Fix**: Merge interior cells per-REFR by FormID instead of whole-value —
  start from the base cell's `references`, then apply the override's adds/changes/deletes
  keyed on REFR FormID (last-write-wins per REFR, not per cell). Apply the same to the
  exterior per-grid table.

### SK-D3-01: `decode_sse_packed_buffer` panics (OOB slice index) on malformed SSE skinned geometry

- **Severity**: HIGH
- **Dimension**: NPC Equip + FaceGen (Dim 3)
- **Location**: `crates/nif/src/import/mesh/sse_recon.rs:279-285, 306-309, 337-342` (direct indexing inside the per-vertex decode loop)
- **Status**: NEW
- **Description**: The function slices each vertex into a fixed-length sub-slice
  `bytes[base..base+vertex_size]`, then walks it with an `off` cursor. Position/UV/
  skin-weight reads use the bounds-checked `read_f32_le`/`read_u16_le` (which return
  `None` via `?`), but the **normal, tangent, vertex-color, and skin-index** reads use
  direct slice indexing (`bytes[off]`, `bytes[off+1]`, … `bytes[off+11]`), which panics
  on OOB. `vertex_size` (`skin.rs:224`) and `vertex_desc` (`skin.rs:225`) are independent
  raw file fields with no cross-validation; the only structural check is
  `is_multiple_of(vertex_size)`, which guarantees the stride *divides* the buffer but not
  that it's *large enough* for the declared attribute mask. The function's own
  `if off > vertex_size { return None; }` guard fires only at end-of-loop, after the
  indexing has already panicked.
- **Evidence**: Confirmed in live code — `bytes[off]`/`bytes[off+1]`/`bytes[off+2]`/
  `bytes[off+3]` at the normal block (279-285), `bytes[off..off+3]` at vertex-colors
  (306-309), and `bytes[off+8..off+11]` at skin-indices (337-342) are all unguarded,
  while the interleaved `read_u16_le(bytes, off)?` weight reads are guarded. Contrast: the
  inline decoder (`bs_tri_shape.rs`) reads through `NifStream`'s bounds-checked methods,
  so it yields `Err` (never panics) on bad input.
- **Impact**: This is the SSE NPC-body / FaceGeom reconstruction path that M41 drives for
  every Skyrim+ NPC body. A truncated or corrupt partition buffer crashes the cell loader
  (hard process panic) instead of skipping the shape. Not reachable on consistent vanilla
  data, so day-to-day risk is low, but the failure mode is a hard crash, and modded /
  LE→SE-converted content is the realistic trigger.
- **Related**: SK-D1-AUDIT-01 (a *different* SSE-recon defect — strip-authored partition
  drop). Distinct sites, distinct symptoms.
- **Suggested Fix**: Replace the raw byte reads with `bytes.get(off)…?` (or add an
  `off + needed > vertex_size` check at the top of each attribute block) so malformed
  geometry returns `None` and the shape is skipped, matching the inline path's
  fail-soft behaviour.

### SK-D1-AUDIT-01: SSE skin-partition *strips* silently drop reconstructed triangles

- **Severity**: MEDIUM
- **Dimension**: BSTriShape Packed Geometry + SSE Reconstruction (Dim 1)
- **Location**: `crates/nif/src/blocks/skin.rs:289-300` (`NiSkinPartition::parse`), consumed at `crates/nif/src/import/mesh/sse_recon.rs:105-138` (`try_reconstruct_sse_geometry`)
- **Status**: NEW
- **Description**: `NiSkinPartition::parse` only fills `SkinPartitionEntry.triangles`
  when `num_strips == 0`; strip-authored partitions are `stream.skip`-ed, leaving
  `triangles` empty. `try_reconstruct_sse_geometry` builds indices solely from
  `part.triangles`, so a fully strip-authored skinned shape produces `indices.is_empty()`
  → `return None` → the whole NPC/creature body fails to reconstruct, with no diagnostic.
- **Evidence**: `skin.rs:289-300` skips the strip arrays without de-stripping;
  `sse_recon.rs:105-138` has no strip fallback. Vanilla SSE ships indexed triangles
  (`num_strips == 0`), so vanilla content is unaffected — but LE→SE-converted and modded
  meshes that retain strips drop geometry wholesale.
- **Impact**: A strip-authored skinned body (creature or NPC) renders as nothing, with no
  WARN to point at the cause. Realistic on modded / converted content; not on vanilla SSE.
- **Suggested Fix**: De-strip partition strips into triangles during parse (standard
  triangle-strip → triangle-list expansion), or at minimum emit a WARN when
  `num_triangles > 0 && part.triangles.is_empty()` so the silent drop becomes diagnosable.

### SK-D2-01: `parse_shader_type_data_fo4` over-reads `env_map_scale` (shader_type==1) at BSVER 140–154

- **Severity**: LOW (Skyrim — unreachable) / MEDIUM if re-scoped to FO76 152–154
- **Dimension**: BSLightingShader / BSEffectShader Dispatch (Dim 2)
- **Location**: `crates/nif/src/blocks/shader.rs:1330` (the read), dispatched from `:982`, band selected at `:809-818`
- **Status**: NEW
- **Description**: `BSLightingShaderProperty::parse` routes on raw header BSVER: `>= 155`
  → `parse_fo76_plus`; `>= 130` → `parse_fo4` (band **130..=154**); else → `parse_skyrim`.
  Inside `parse_shader_type_data_fo4`, `env_map_scale` is read **unconditionally** for
  `shader_type == 1`, while nif.xml (L6619) gates it `#NI_BS_LTE_FO4#` = `BSVER <= 139`.
  The two SSR bools one line below ARE correctly gated to 130–139 — only the
  `env_map_scale` read lacks the matching upper bound, so for BSVER 140–154 an
  EnvironmentMap (type-1) BSLSP over-reads 4 bytes.
- **Evidence**: `shader.rs:1330` reads `env_map_scale` with no `bsver <= 139` guard; the
  SSR bools immediately below are gated `(FALLOUT4..FO4_DLC_UPPER)`. Routing band confirmed
  at `shader.rs:813`.
- **Impact**: **Skyrim SE (BSVER 100) and LE (83) both route through `parse_skyrim`, not
  `parse_fo4` — so Skyrim content is completely unaffected.** BSVER 140–151 is a dead band
  (no shipping game). BSVER 152–154 is an FO76-era edge (retail FO76 BSLSP is exactly 155,
  correctly routed elsewhere; 152–154 is an early/dev-build edge). Hence LOW for the
  Skyrim scope; MEDIUM only if an FO76 audit re-scopes the 152–154 reach.
- **Related**: Cross-game; surfaced here because the band is adjacent to the Skyrim path.
  Distinct from #1330 (BSShaderNoLightingProperty over-read on FO3/FNV bsver≤26).
- **Suggested Fix**: Add `if bsver <= 139` to the `env_map_scale` read to mirror the SSR
  bool gate immediately below it.

### SK-D4-02: `.STRINGS` loader written + tested but never wired in — localized names render as placeholders

- **Severity**: LOW (cosmetic — does not block rendering)
- **Dimension**: Multi-Master Load Order + TES5 Cell-Load (Dim 4)
- **Location**: `crates/plugin/src/esm/strings_table.rs` (complete, 7 tests); zero non-test call sites for `StringTableSet::load` / `StringsTableGuard::new`
- **Status**: Evolution of prior **SK-D6-NEW-01** (2026-05-12 audit: "no `.STRINGS`
  loader exists"). The loader now *exists* but is unwired, so the user-visible symptom is
  unchanged. No open GitHub issue.
- **Description**: `crates/plugin/src/esm/strings_table.rs` implements the
  `.STRINGS`/`.DLSTRINGS`/`.ILSTRINGS` table format and is fully unit-tested, but
  `StringTableSet::load` / `StringsTableGuard::new` have **no production call sites** —
  only doc-comment and test references (`grep` confirms). With no guard installed,
  `resolve_lstring` (`records/common.rs:169`) always returns `None`, so every localized
  name emits the `<lstring 0xNNNNNNNN>` placeholder.
- **Evidence**: `common.rs:9,28,91-126` reference `StringTableSet` only via the
  thread-local + RAII guard; nothing calls `::load`/`Guard::new` outside docs/tests. All
  seven vanilla/DLC/CC Skyrim SE masters are Localized-flagged → hits 100% of Skyrim
  content at runtime.
- **Impact**: Cell titles, NPC names, book/faction names display as
  `<lstring 0x000…>`. UI legibility, not a rendering blocker.
- **Suggested Fix**: Install `StringsTableGuard` per-plugin during ESM load when
  `header.localized` is set (resolve the language tag, call `StringTableSet::load`,
  hold the guard across the record walk). ~20 LOC of wiring; loader + tests already exist.

### SK-D4-03: ESL `0x0200` light-master flag undecoded — `0xFE` FormID prefix treated as a flat mod-index

- **Severity**: LOW
- **Dimension**: Multi-Master Load Order + TES5 Cell-Load (Dim 4)
- **Location**: `crates/plugin/src/esm/reader.rs:256-297` (`FormIdRemap::remap`); `read_file_header` never reads the TES4 `0x0200` ESL flag
- **Status**: NEW
- **Description**: `FormIdRemap::remap` treats the top byte of a FormID as a flat
  mod-index. ESL / light-master plugins use the `0xFE` prefix with a 12-bit sub-index
  (`0xFExxx`), so an ESL's FormIDs need a different decode. The file-header parser never
  reads the `0x0200` ESL header flag, so ESL plugins would remap incorrectly.
- **Evidence**: `reader.rs:256-297` is a flat top-byte index; no `0x0200` flag read in
  `read_file_header`. Verified **none of the seven vanilla/DLC/CC Skyrim SE masters are
  ESL-flagged**, so the blast radius for vanilla Skyrim compat is zero — this is a
  forward-looking gap for third-party ESL mods (and FO4/SF, which ship ESL CC content).
- **Impact**: Third-party ESL mod FormIDs would resolve to wrong records. No impact on
  vanilla Skyrim SE.
- **Suggested Fix**: Read the `0x0200` header flag; for ESL-flagged plugins, decode
  FormIDs as `0xFE` + 12-bit load-order sub-index + 12-bit local ID per the Creation
  Engine spec.

### SK-D5-01: BSA v105 real-data tests are `#[ignore]`'d — default CI never gates the frame-codec / 24-byte-stride path

- **Severity**: LOW
- **Dimension**: BSA v105 (LZ4) (Dim 5)
- **Location**: `crates/bsa/src/archive/tests.rs:533-655` (the 5 real-archive tests carry `#[ignore]`)
- **Status**: NEW
- **Description**: The five tests that exercise the real Skyrim v105 archives (frame-codec
  LZ4 decompression, 24-byte folder-record stride, embed-name path) are all `#[ignore]`'d
  (they require on-disk game data), so default `cargo test` never gates them. The
  unconditional synthetic tests encode+decode with the *same* `lz4_flex::frame` codec, so
  they cannot catch a wrong-codec regression (e.g. an accidental swap to the block codec
  that Starfield's BA2 uses).
- **Evidence**: `tests.rs:533-655` carry `#[ignore]`; ran manually
  `cargo test -p byroredux-bsa --lib -- --ignored skyrim` → 5/5 pass against real
  archives; full lib suite 50 passed / 0 failed. The frame-vs-block distinction is real:
  raw inspection of `Skyrim - Meshes0.bsa` shows the LZ4 frame magic `0x184D2204` after
  the 4-byte size prefix, and the reader correctly uses `lz4_flex::frame::FrameDecoder`
  (`extract.rs:128-132`) — Starfield's BA2 (`ba2.rs:694`) correctly uses the block codec.
- **Impact**: A future refactor swapping the v105 codec would pass default CI and only
  surface when a real Skyrim BSA loads. Latent regression hazard, no current defect.
- **Suggested Fix**: Either commit a tiny synthetic v105 archive fixture that round-trips
  through the frame codec unconditionally, or add a CI job that runs the `--ignored skyrim`
  subset when game data is mounted.

### SK-D1-AUDIT-02: `has_tangents` gate diverges between inline and SSE-recon decoders

- **Severity**: LOW (no behavioral impact — flagged for consistency)
- **Dimension**: BSTriShape Packed Geometry + SSE Reconstruction (Dim 1)
- **Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:926` (inline) vs `crates/nif/src/import/mesh/sse_recon.rs:216` (SSE-recon)
- **Status**: NEW
- **Description**: The inline decoder gates the tangent quad on `VF_TANGENTS` alone
  (matching the nif.xml `BSVertexData` condition `0x11`); the SSE-recon decoder requires
  `VF_TANGENTS && VF_NORMALS`. No behavioral difference in practice — the byte stride is
  identical and the 5-tuple tangent assembly needs NORMALS anyway — but the two paths
  should use the same predicate so a future maintainer doesn't infer a real semantic
  divergence.
- **Evidence**: `bs_tri_shape.rs:926` = `VF_TANGENTS`; `sse_recon.rs:216` =
  `VF_TANGENTS & VF_NORMALS`.
- **Impact**: None today; code-review consistency only.
- **Suggested Fix**: Align both to `VF_TANGENTS` (or both to `VF_TANGENTS && VF_NORMALS`)
  with a shared helper.

### SK-D3-02: M41 equip smoke test soft-warns on zero equip components; no 6-named-NPC count guard

- **Severity**: LOW
- **Dimension**: NPC Equip + FaceGen (Dim 3)
- **Location**: `docs/smoke-tests/m41-equip.sh:186-195`
- **Status**: NEW
- **Description**: The only HARD assertions in the M41 equip smoke test are cell-wide
  `entities`/`draws` floors. The actual equip signals — `Inventory` / `EquipmentSlots`
  entity counts — are emitted as WARN only and never affect the exit code. The named-NPC
  count (6: saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda) is never asserted
  anywhere. A regression dropping all NPC gear would still pass as long as the static-mesh
  count stays above its floor.
- **Evidence**: `m41-equip.sh:186-195` — zero-Inventory / zero-EquipmentSlots emit WARN,
  not a non-zero exit; no reference to the 6 NPC names or the count 6 in the test.
- **Impact**: The one smoke test that exercises the full outfit chain wouldn't catch a
  silent equip regression. (The equip code itself is correct — see clean items below.)
- **Suggested Fix**: Promote the zero-Inventory / zero-EquipmentSlots WARN to a HARD fail
  with a small floor (e.g. `>= 6` entities carrying both components).

---

## Shader-Type Coverage Matrix (Skyrim `parse_shader_type_data`)

All arms verified field-for-field against nif.xml (`crates/nif/src/blocks/shader.rs:1242-1318`).
The catch-all `_ => None` (`:1316`) reads **zero** trailing bytes, so every "None" type is
a confirmed no-over-read.

| Numeric type | ShaderTypeData arm | Trailing fields | Parse | Import | Render |
|---|---|---|---|---|---|
| 0 Default | None | — | ✓ | ✓ | ✓ (base PBR) |
| 1 Environment Map | EnvironmentMap | env_map_scale | ✓ | ✓ | ✓ |
| 2 Glow | None | — | ✓ | ✓ | ✓ |
| 3 Parallax | None | — | ✓ | ✓ | ✓ |
| 4 Face Tint | None | — | ✓ | ✓ | ⚠ base PBR (FaceGen runtime is M41+) |
| 5 Skin Tint | SkinTint | Color3 | ✓ | ✓ | ✓ |
| 6 Hair Tint | HairTint | Color3 | ✓ | ✓ | ✓ |
| 7 Parallax Occ | ParallaxOcc | max_passes + scale | ✓ | ✓ | ✓ |
| 8 Multitexture Landscape | None | — | ✓ | ✓ | ⚠ terrain splat path |
| 9 LOD Landscape | None | — | ✓ | ✓ | ✓ |
| 10 Snow | None | — | ✓ | ✓ | ⚠ base PBR (no SSS branch) |
| 11 MultiLayer Parallax | MultiLayerParallax | inner-layer fields | ✓ | ✓ | DEFERRED (#562) |
| 12 Tree Anim | None | — | ✓ | ✓ | ✓ |
| 13 LOD Objects | None | — | ✓ | ✓ | ✓ |
| 14 Sparkle Snow | SparkleSnow | 4 params | ✓ | ✓ | ✓ |
| 15 LOD Objects HD | None | — | ✓ | ✓ | ✓ |
| 16 Eye Envmap | EyeEnvmap | cubemap + L/R centers | ✓ | ✓ | DEFERRED (#562) |
| 17 Cloud | None | — | ✓ | ✓ | ⚠ base PBR |
| 18 LOD Landscape Noise | None | — | ✓ | ✓ | ✓ |
| 19 Multitexture LOD Blend | None | — | ✓ | ✓ | ✓ |
| 20 FO4 Dismemberment | None (Skyrim) | — | ✓ | ✓ | N/A (FO4 sentinel) |

FO76 (`parse_shader_type_data_fo76`, `shader.rs:1414-1449`) uses the distinct
`BSShaderType155` numbering (4 → `Fo76SkinTint` Color4, 5 → HairTint Color3) via a separate
function and dedicated enum variant — **no cross-contamination** with the Skyrim/FO4
numbering. The DEFERRED / ⚠ render entries are documented in-source and are roadmap items,
not regressions.

---

## Cell-Load Regression Status

- **TES5 cells parse** through the unified `esm/cell/` walker. `parse_real_skyrim_esm`
  ran green: **590 interior cells**, `SolitudeWinkingSkeever` = 981 refs, 590/590 with the
  Skyrim 92-byte XCLL lighting tier.
- **Compressed-record decompression** now has 3 unit tests (prior SK-D6-NEW-02 is
  RESOLVED — the codepath is no longer untested).
- **Minimum interior-render record set** (CELL/REFR/STAT/LIGH/WEAP/ARMO + LAND/LTEX/
  TXST/ADDN) all dispatch in production; LAND VHGT uses the correct universal `*8.0`
  height scale. Out-of-scope NAVM/HDPT/`BSBehaviorGraphExtraData` parse without error.
- **Control bench**: Whiterun BanneredMare 3,216 entities / 362.8 FPS (ROADMAP
  Bench-of-record `1c26bc25`). No single-plugin cell-load-path commit changes which REFRs
  spawn; single-plugin Whiterun is immune to SK-D4-01. **No control-bench regression.**

---

## Specialty-Block Dispatch Status (Dim 6 — clean, evidenced on real Meshes0)

`nif_stats` sweep over the real `Skyrim - Meshes0.bsa` returned **0 unknowns** across the
Skyrim-specific specialty blocks — every #837/#838 guard holds:

| Block | Parsed | Unknown | Guard |
|---|---|---|---|
| BSLODTriShape | 23 | 0 | #838 — routed via `NiLodTriShape`, NOT BSTriShape; import arm present (`walk/mod.rs:442-448, 1029`) |
| BSLagBoneController | 163 | 0 | #837 dedicated parser |
| BSProceduralLightningController | 3 | 0 | #837 dedicated parser |
| BSTreeNode | 20 | 0 | SpeedTree wind-bones |
| BSMultiBoundNode | 180 | 0 | unwrapped by import walker |
| BSFadeNode | (folded) | 0 | dispatched-as-NiNode, counted in 109,145 NiNode total |

Prior **SK-D5-NEW-09** (`BSLODTriShape` geometry dropped by the import walker, MEDIUM,
2026-05-12) is **RESOLVED**: the `NiLodTriShape` downcast arm landed in both walkers
(`crates/nif/src/import/walk/mod.rs:442-448` and `:1029`), with a regression test at
`crates/nif/src/import/tests.rs:1757-1769`.

---

## NIFAL Canonical Material Status (Dim 7 — clean)

- `byroredux/src/material_translate.rs:73` `translate_material` is the **only** production
  ECS-`Material` producer; both spawn sites (`cell_loader/spawn.rs:857` /
  `scene/nif_loader.rs:796`) build through it. SpeedTree + precombine build *import-tier*
  `MaterialInfo`, never an ECS Material.
- `Material::metalness`/`roughness` are plain `f32` (`material.rs:217,223`); the per-draw
  `Material::classify_pbr` is **deleted** (only `classify_pbr_keyword` + `resolve_pbr`
  survive). Seeding uses `unwrap_or(f32::NAN)` then `resolve_pbr()`; for shipped content
  the NaN sentinel never fires (Skyrim SE writes `Some(...)` at `bs_tri_shape.rs:246-247`).
- Ordering holds: `resolve_pbr()` runs **before** `classify_glass_into_material`
  (`material_translate.rs:160-161`), so forced-glass roughness wins.
- EmissiveSource (#1280): Skyrim BSLightingShaderProperty emissive routes through the
  `Lighting` variant (`walker.rs:308-339`, sets `has_material_data = true` so the
  `Effect`/`NiMaterial` blocks are gated out). `Effect` only fires on effect-shader-only
  meshes — its correct semantic.

The MAT_FLAG_PBR_BSDF Disney/Burley regression guard (Dim 2) also holds: the flag's only
production set site is `pack_bgsm_material_flags` (`byroredux/src/cell_loader.rs:215-217`),
gated on `mesh.is_pbr`, which is false on every NIF import path and flips true only on a
BGSM/BGEM/.mat sidecar merge. Vanilla Skyrim SE ships no BGSM sidecars → the lobe stays
unreachable on vanilla content.

---

## Verified-clean checklist items (could not disprove)

**Dim 1**: VF_* flag constants match nif.xml bit-for-bit; `half_to_f32` is IEEE-754
binary16-correct (subnormals, inf/NaN, signed zero); triangle indices always 3×u16 on
disk; SSE/FO4 precision split correct; tangent/bitangent sign routes through the shared
`bitangent_sign` helper with identical operand order — **#1516 inversion is NOT present**
(regression test `bitangent_sign_swapped_operands_invert` pins it); `alpha_property_consumed`
set once (`material/mod.rs:1052`), consumed exactly once per shape (`walker.rs:118-122`,
gated at `:496`/`:572`).

**Dim 2**: Skyrim `parse_shader_type_data` arms all correct; FO76 numbering isolated;
BSEffectShaderProperty (soft_falloff_depth / greyscale_texture / lighting_influence /
env_map_min_lod / falloff angle+opacity / env-map slot / alpha) all reach MaterialInfo;
#1241 PBR scalars forwarded.

**Dim 3**: 6 BanneredMare NPCs land Inventory + EquipmentSlots and spawn equipped;
`resolve_armor_mesh` ARMO→ARMA→worn-mesh with race/gender/fallback (7 tests); body
pre-scan uses the per-game main-body bit correctly (Skyrim has no separate upperbody.nif,
so it correctly no-ops there); LVLI `expand_leveled_form_id` highest-eligible single-pick
/ flag-bit multi-pick, level-filtered, depth-capped, empty/below-floor/nested/circular all
test-covered; FaceGen EGM/EGT/TRI parsers do exact-size validation before indexing — zero
unwrap/panic in non-test code; BSDismemberSkinInstance partition data routes into the
skinning pipeline (local→global bone remap, drop-on-OOB).

**Dim 4**: FormID remap math correct (41 tests); unresolved-REFR loud-fail names the
missing plugin; compressed-record decompression tested; minimum + out-of-scope record set
dispatches.

**Dim 5**: v105 frame-codec decompression correct (proved via raw byte inspection of real
Meshes0 + the `sweetroll01.nif` extract = exactly 10,245 bytes); 24-byte folder stride;
embed-name order verified byte-for-byte against real Textures0; compression toggle XORs
archive default with per-file bit winning on disagreement; `TexturesN` auto-load miss is
documented-by-design (`--textures-bsa` is repeatable).

**Dim 6**: Meshes0 sweep 0 unknowns (table above); #837/#838 guards hold.

**Dim 7**: single-boundary / no-render-fallback / EmissiveSource→Lighting all clean
(detailed above).

---

## Audit guards (correct by design — must not be reverted)

- **#836** — BSTriShape `data_size` warning gate on `num_vertices != 0` (silences SSE
  skinned-body reconstruction false positives).
- **#837** — `BsLagBoneController` + `BsProceduralLightningController` dedicated parsers
  (163 + 3 fires on Meshes0; folding back re-introduces the WARN burst).
- **#838** — `NiLodTriShape` distinct from `BSTriShape` (folding `BSLODTriShape` back =
  23-byte over-read on every Skyrim tree LOD). The import arm is now present in both
  walkers; any proposal to "fold BSLODTriShape into BSTriShape" is a regression.
- **#1516** — inline + SSE-recon tangent paths use the **non-inverted** bitangent-sign
  convention via the shared `bitangent_sign` helper. Re-inverting either re-introduces the
  magenta/chrome failure mode.
- **Meshes0 baseline**: 100% clean / 0 truncated / 0 recovered / 0 realignment WARNs. Any
  realignment WARN on a clean Skyrim Meshes0 corpus is a regression.

---

## Recommended next steps

1. **Land SK-D4-01** — per-REFR cell merge by FormID (interior + exterior). Highest-impact
   fix; unblocks the M46.0 multi-master DLC headline use case (57 interiors stomped today).
2. **Land SK-D3-01** — fail-soft the SSE-recon byte reads (`bytes.get(off)?`). Closes a
   hard-crash hazard on modded/converted skinned bodies; ~10 LOC.
3. **Land SK-D1-AUDIT-01** — de-strip skin partitions or WARN on the silent drop.
4. **Wire SK-D4-02** — install `StringsTableGuard` per localized plugin; loader + tests
   already exist (~20 LOC of wiring) for a high-visibility UX win.
5. **Gate SK-D2-01** — one-line `bsver <= 139` guard on the FO4 env_map_scale read
   (latent for Skyrim, real for the FO76 152–154 edge).
6. **SK-D5-01 / SK-D3-02 / SK-D4-03 / SK-D1-AUDIT-02** — test/coverage + forward-looking
   hardening; no current vanilla-Skyrim defect.

---

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-06-14.md`
