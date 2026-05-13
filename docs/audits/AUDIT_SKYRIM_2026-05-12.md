# AUDIT — The Elder Scrolls V: Skyrim Special Edition

**Date**: 2026-05-12
**Branch**: `main`
**Scope**: NIF parser + import, BSA v105 reader, BSLightingShader / BSEffectShader render coverage, specialty-node dispatch, real-data validation, ESM readiness, forward blockers.
**Methodology**: Six-dimension orchestrated audit; per-dimension subagents (some retried inline); dedup against `/tmp/audit/issues.json` and `docs/audits/AUDIT_SKYRIM_*.md`.

---

## Executive Summary

**Skyrim SE individual-mesh rendering is production-ready.** Cell-load is unblocked
end-to-end; the WhiterunBanneredMare interior already renders at **3,209 entities /
217.3 FPS** on the current build, and the prerequisite gates for "interior cell
renders" are all landed.

This audit re-confirms the 2026-05-11 audit's verdict and surfaces **one new
MEDIUM finding** (a missing import-side arm for `NiLodTriShape` post-#838) plus
**two LOW findings** in the ESM layer (a missing `.STRINGS` loader for display-name
fidelity, and a missing unit test for compressed-record decompression).

| Dim | Surface | NEW findings | Existing OPEN | Notes |
|----|---------|--------------|---------------|-------|
| 1 | BSTriShape vertex format | 0 | 0 | 100.00% Meshes0 clean baseline holds (#836/#838) |
| 2 | BSA v105 + LZ4 | 0 | 0 | Brute-force 18,862 NIFs / 1.96 GB / 0 errors |
| 3 | BSLightingShader (21 variants) | 0 | 0 | 3 informational stubs (multi-layer-parallax, eye-envmap, face-tint) — all in-code-tagged |
| 4 | BSEffectShader + specialty nodes | 0 | 1 | #946 (BSDynamicTriShape WARN-spam comment is stale) |
| 5 | Real-data validation & rendering | **1 MEDIUM** | 0 | **SK-D5-NEW-09** — `BSLODTriShape` geometry dropped by import walker |
| 6 | ESM readiness | 2 LOW + 1 informational | 0 | SK-D6-NEW-01 (.STRINGS loader), SK-D6-NEW-02 (zlib test) |

**Totals**: 1 MEDIUM + 2 LOW NEW findings; 1 existing OPEN intersects.

---

## Forward Blocker Chain — Interior Cell Renders

**Status: NOTHING BLOCKS.** All 13 gates are landed and verified against current `main`.

1. Per-record zlib decompression — `reader.rs:454-477` (#FLAG_COMPRESSED + `flate2::ZlibDecoder`). Not directly tested (SK-D6-NEW-02), but exercised on vanilla data.
2. Tes5Plus 24-byte record/group headers — `reader.rs:401-447`. ROADMAP: M32.5.
3. `GameKind::Skyrim` HEDR routing — `reader.rs:135` (band 1.6..=1.8), pinned by `game_kind_from_header_maps_real_master_hedr_values`.
4. CELL group walker — `cell/walkers.rs:11` (group types 2/3/6/8/9).
5. REFR/ACHR/ACRE record walker — `cell/walkers.rs:289`; full sub-record coverage (NAME/DATA/XSCL/XESP/XTEL/XPRM/XLKR/XRMR/XPOD/XRDS/XATO/XTNM/XTXR/XEMI/XMSP/XOWN/XRNK/XGLB). ROADMAP: M32.5 + #692.
6. CELL display name + lighting — `cell/walkers.rs:85-243` (FULL/XCLL 28-40-92-byte tiers, LTMP fallback, XCLW water height, XCIM/XCWT/XCAS/XCMO/XCMT/XCCM/XLCN/XCLR). ROADMAP: #348 / #356 / #379 / #389 / #566 / #624 / #692 / #693.
7. STAT/LIGH/MSTT/FURN/DOOR/FLOR/IDLM/BNDS/ADDN/TACT base records (MODL-only) — `records/mod.rs:750-753`.
8. WEAP/ARMO/MISC/KEYM/ALCH/INGR/BOOK/NOTE/CONT (visible item bases) — `records/mod.rs:777-867`. ROADMAP: M24 Phase 1.
9. LGTM lighting templates — `records/mod.rs:975`. ROADMAP: #566.
10. TXST texture sets — `records/mod.rs:739`. ROADMAP: #584.
11. NIF: `BSBehaviorGraphExtraData` parses without error — `crates/nif/src/blocks/extra_data.rs:565`. Pinned by `extra_data_tests.rs:122-179`.
12. Multi-plugin FormID remap — `reader.rs:240-278` + `parse_esm_with_load_order`. Required for Dawnguard / Dragonborn / HearthFires DLC. ROADMAP: M46.0 / #445 / #561.
13. Cell loader entry — `byroredux/src/cell_loader/load.rs:114` → `parse_record_indexes_in_load_order` → `cell_loader::references::load_references`. ROADMAP: M32.5.

**Cosmetic-only gap**: `.STRINGS` file loader (SK-D6-NEW-01) — display names ride as `<lstring 0x…>` placeholders. Does not block rendering.

**Out of scope (tracked elsewhere)**:
- Skyrim exterior grid streaming (WRLD walker in place but grid-streamer is a separate milestone).
- Papyrus VMAD evaluation (M30.2 / M47.2).
- Havok `.hkx` behavior graph runtime (M41.x; NIF reference parses cleanly today).
- FaceGen runtime morph/blend (M41+).

---

## BSLightingShaderProperty — Variant Coverage Matrix

All 21 `BSLightingShaderType` enum values (0..20 per nif.xml) dispatch through
`crates/nif/src/blocks/shader.rs:1164-1240` (Skyrim) /
`shader.rs:1245-1327` (FO4) / `shader.rs:1332+` (FO76). Import-side
`material_kind` flows through `GpuInstance` and reaches `triangle.frag`.

| # | nif.xml name | Parse | Import (`material_kind`) | Render | Notes |
|---|--------------|-------|--------------------------|--------|-------|
| 0 | Default | ✓ | ✓ | ✓ | base PBR |
| 1 | Environment Map | ✓ env_map_scale (+ FO4 SSR bools) | ✓ | ✓ | env reflection via `env_map_index` |
| 2 | Glow Shader | ✓ | ✓ | ✓ | glow texture in TS3 |
| 3 | Parallax | ✓ | ✓ | ✓ | height TS4 (#453) |
| 4 | Face Tint | ✓ | ✓ | ⚠ partial | TS4 detail + TS7 tint reach `GpuInstance`; no dedicated branch (FaceGen runtime is M41+) |
| 5 | Skin Tint | ✓ Color3 | ✓ (FO76 type 4 remapped to 5, #570) | ✓ | `triangle.frag:1296-1302` |
| 6 | Hair Tint | ✓ Color3 | ✓ | ✓ | `triangle.frag:1303-1308` |
| 7 | Parallax Occ | ✓ max_passes + scale | ✓ | ✓ | POM (#453) |
| 8 | Multitexture Landscape | ✓ | ✓ | ⚠ | terrain LTEX splat (M32 Phase 2); no dedicated kind=8 branch |
| 9 | LOD Landscape | ✓ | ✓ | ✓ | base PBR |
| 10 | Snow | ✓ | ✓ | ⚠ | base PBR; no dedicated snow SSS branch |
| 11 | MultiLayer Parallax | ✓ 5 floats | ✓ — fields on `GpuInstance` | DEFERRED | second-sample blend stubbed (`triangle.frag:1328-1334`) |
| 12 | Tree Anim | ✓ | ✓ | ✓ | base PBR; wind-bone metadata on BSTreeNode |
| 13 | LOD Objects | ✓ | ✓ | ✓ | base PBR |
| 14 | Sparkle Snow | ✓ Color4 | ✓ | ✓ | hash-driven glint (`triangle.frag:1309-1322`) |
| 15 | LOD Objects HD | ✓ | ✓ | ✓ | base PBR |
| 16 | Eye Envmap | ✓ cubemap + L/R eye centers | ✓ — fields on `GpuInstance` | DEFERRED | per-instance cubemap binding (`triangle.frag:1335-1340`) |
| 17 | Cloud | ✓ | ✓ | ⚠ | base PBR; no dedicated sky-projected branch |
| 18 | LOD Landscape Noise | ✓ | ✓ | ✓ | base PBR |
| 19 | Multitexture Landscape LOD Blend | ✓ | ✓ | ✓ | base PBR |
| 20 | FO4 Dismemberment | ✓ | ✓ | N/A | FO4-only sentinel |

Parse + import + flag-bit handling for every variant is complete and matches nif.xml.
The three DEFERRED / ⚠ entries are documented in-source via stub comments and are
roadmap items, not regressions.

---

## Dimension Findings (by severity)

### SK-D5-NEW-09: `BSLODTriShape` geometry silently dropped by import walker — #838 parser fix landed without the matching import arm

- **Severity**: MEDIUM
- **Dimension**: Real-Data Validation & Rendering (Dim 5)
- **Location**: `crates/nif/src/import/walk.rs:328-407` (`walk_node_local`) and `crates/nif/src/import/walk.rs:649-713` (`walk_node`). Both branches downcast to `NiTriShape`, `BsTriShape`, and `BSGeometry`; no `NiLodTriShape` arm.
- **Status**: NEW
- **Description**: #838 (2026-05-10) introduced a dedicated `NiLodTriShape` Rust type for Skyrim DLC tree LODs (`BSLODTriShape` per nif.xml, `inherit="NiTriBasedGeom"` — distinct from FO4's `BSMeshLODTriShape` which inherits BSTriShape). The parser dispatch at `blocks/mod.rs:306` correctly routes `BSLODTriShape` to `NiLodTriShape::parse`, the new block reports `as_any() = &NiLodTriShape`, and the inner body is accessible via `.base` (`crates/nif/src/blocks/tri_shape.rs:217-255`). **However the importer's two walkers each have only three downcast arms — `NiTriShape`, `BsTriShape`, `BSGeometry`. A `NiLodTriShape` matches none of them.** Pre-#838 the importer picked these up as `BsTriShape` (the parser misrouted them through `parse_lod`); post-#838 they fall through silently.
- **Evidence**:
  - Static: `grep -rn 'NiLodTriShape' crates/nif/src/import/` returns zero results.
  - Dynamic: 23 `BSLODTriShape` blocks parse clean on the Meshes0 sweep; the walker yields zero `ImportedMesh` from any of them.
- **Impact**: Distant-LOD geometry on 23 Skyrim SE meshes (mostly DLC/architecture distant-LOD shells) doesn't render. Total import-side regression — not "wrong LOD selected" but "shape entirely missing."
- **Suggested Fix**: Add an `NiLodTriShape` downcast arm in both `walk_node_local` and `walk_node`. Body identical to the existing `NiTriShape` branch but operates on `&lod.base`. ~6 LOC + a regression test that loads a Skyrim BSLODTriShape NIF and asserts non-empty `ImportedMesh` output.

### SK-D6-NEW-01: No `.STRINGS` companion-file loader — Skyrim localized names render as `<lstring 0xNNNNNNNN>` placeholders

- **Severity**: LOW (cosmetic — does not block rendering)
- **Dimension**: ESM Readiness & Forward Blockers (Dim 6)
- **Location**: `crates/plugin/src/esm/records/common.rs:105-114`; also `crates/plugin/src/esm/reader.rs:557` (`FileHeader.localized`)
- **Status**: NEW. Issue #348 (Phase 1, placeholder) is CLOSED; the Phase 2 `.STRINGS` loader follow-up has no dedicated open ticket.
- **Description**: TES4 record `0x80` Localized flag is captured into `FileHeader.localized` and stashed via `LocalizedPluginGuard`. Every `read_lstring_or_zstring` call site routes a 4-byte payload to the placeholder `<lstring 0xNNNNNNNN>` string. Phase 2 — actually loading `Strings/Skyrim_English.STRINGS` / `.DLSTRINGS` / `.ILSTRINGS` and resolving the placeholder — was promised in the doc comment but has no open issue.
- **Impact**: Display names (cell title, NPC name, book titles, faction names) appear as `<lstring 0x000...>`. UI legibility blocker, not a rendering one.
- **Suggested Fix**: Land `crates/plugin/src/esm/strings_table.rs` honouring the 8-byte header + count × (id, offset) + string-blob format. ~150 LOC + test fixtures. File under `legacy-compat`.

### SK-D6-NEW-02: Per-record zlib decompression has no unit-test coverage

- **Severity**: LOW
- **Dimension**: ESM Readiness & Forward Blockers (Dim 6)
- **Location**: `crates/plugin/src/esm/reader.rs:451-477`
- **Status**: NEW
- **Description**: The `read_sub_records` `FLAG_COMPRESSED = 0x00040000` branch decompresses via `flate2::ZlibDecoder`. The codepath is exercised at runtime on vanilla Skyrim/FO4 masters (which ship compressed records for large bodies), but no synthetic regression test guards a future flate2 swap, `decompressed_size` off-by-one, or the `header.data_size >= 4` panic guard. `grep -rn "FLAG_COMPRESSED" crates/plugin/src/esm/` returns three hits in `reader.rs` and zero in any test file.
- **Impact**: Latent — a backend swap or minor refactor could silently break vanilla-master parsing, only surfacing when a real ESM with a compressed record loads.
- **Suggested Fix**: Add a `tests` module entry that builds a synthetic record with `FLAG_COMPRESSED` set, zlib-encodes a known sub-record payload via `flate2::write::ZlibEncoder`, prepends the 4-byte decompressed-size header, and asserts `read_sub_records` round-trips correctly. ~40 LOC.

---

## Existing OPEN issue cross-references (re-verified, not refiled)

- **#946 SK-D5-NEW-08** — BSDynamicTriShape "vanilla never fires" WARN-spam comment is empirically false (21,140 fires/Meshes0). Logging-noise classification; mesh data is correct. Intersects Dim 4 + Dim 5.
- **#945 SK-D1-NEW-04** — duplicate `half_to_f32` between `tri_shape.rs:1268` and a sibling site. Cosmetic dedup; verified Dim 5 still observes the duplication.
- **#979 NIF-D5-NEW-03** — `bhkBallSocketConstraintChain` truncation on Meshes1 (99.81% clean / 6 trap NIFs). Out-of-Skyrim-band but corroborated by the Dim 5 sweep.

---

## Informational deferrals (not findings — already tracked in-source)

- **SK-D3-OBS-01** — `BSLightingShaderType` kind=11 (MultiLayer Parallax) second-sample blend deferred; data plumbed end-to-end (`triangle.frag:1328-1334` stub comment / #562).
- **SK-D3-OBS-02** — `BSLightingShaderType` kind=16 (Eye Envmap) cubemap reflection deferred; needs per-instance cubemap descriptor binding (`triangle.frag:1335-1340` stub / #562).
- **SK-D3-OBS-03** — `BSLightingShaderType` kind=4 (Face Tint) has no dedicated render branch; static-baked FaceGen heads render correctly via base PBR, runtime FaceGen is M41+.
- **SK-D6-NEW-03** — `EsmReader::read_record_header` and zlib-payload sizing verified correct; filed for the record so a future audit doesn't reinvestigate.

---

## Audit guards (correct by design — all held)

These design decisions surfaced in prior audits and **must not be reverted**:

- **#836** — BSTriShape `data_size` warning gate on `num_vertices != 0` (silences 67 false-positive WARN on the SSE skinned-body reconstruction path).
- **#837** — `BsLagBoneController` + `BsProceduralLightningController` have dedicated parsers (silences ~120 by-design `block_size` WARNs per Meshes0 sweep).
- **#838** — `NiLodTriShape` is distinct from `BSTriShape` (per nif.xml `inherit="NiTriBasedGeom"` — folding it back would re-introduce a 23-byte over-read on every Skyrim tree LOD).
- **Meshes0 baseline post-#836–#838**: 100.00% clean / 0 truncated / 0 recovered / 0 realignment WARNs. Any audit observing realignment WARNs on a clean Skyrim Meshes0 corpus has hit a regression.

---

## Recommended next steps

1. **Land SK-D5-NEW-09** — add `NiLodTriShape` import-walker arm. ~6 LOC, unblocks distant-LOD geometry on 23 Skyrim SE meshes. **Highest-leverage fix in this audit.**
2. **Land SK-D6-NEW-02** — synthetic zlib-record test for `reader.rs:454-477`. ~40 LOC; closes a latent regression hazard with no functional risk.
3. **Plan SK-D6-NEW-01** — `.STRINGS` loader for display-name fidelity. ~150 LOC; cosmetic but high-visibility UX win.
4. **Tighten #946** — flip the BSDynamicTriShape WARN to `log::debug` and correct the stale comment.

---

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-05-12.md`
