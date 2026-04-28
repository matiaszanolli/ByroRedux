# Starfield Compatibility Audit — 2026-04-27

**Scope**: ByroRedux readiness for Starfield content (BSVER ≥ 155 / 168+, BA2 v2/v3, BGSM/BGEM/.mat materials, ESM).

**Methodology**: 6 parallel dimension agents, dedup against 81 open issues (`/tmp/audit/issues.json`). Each finding verified against current code at `main` (HEAD `0fc1e03`) before reporting. Sources merged from `/tmp/audit/starfield/dim_{1..6}.md`.

---

## Executive Summary

Two production-correct subsystems (NIF parser + BA2 reader) sit on top of a renderer that **extracts zero geometry from any vanilla Starfield NIF**. The headline "97.19% / 100% recoverable" parse rate is real but disconnected from rendering: every Starfield mesh imports as nodes-only because `BSGeometry` has a parser but no importer consumer. Beneath that gap, every `BSLightingShaderProperty` / `BSEffectShaderProperty` block reads incorrectly because four trailing-field gates use `bsver == 155` instead of `bsver >= 155` — Starfield reports `bsver = 172`, so ~24-50 bytes per shader block silently skip on every Starfield NIF.

**Effective Starfield rendering today: 0 meshes.**

The audit produced **8 HIGH / 6 MEDIUM / 7 LOW findings** plus 5 forward-looking planning items. None are CRITICAL — the gaps are correctness drift and missing-format work, not GPU-crashing or memory-corrupting bugs.

### Severity tally (cross-dimension, deduped)

| Severity | Count | Headline IDs |
|----------|-------|--------------|
| CRITICAL | 0 | — |
| HIGH     | 8 | SF-D1-01..04 (bsver gates), SF-D3-01..03 (stopcond + .mat fallback), SF-D5-02 / SF-D4-03 (BSWeakReferenceNode) |
| HIGH (cross-dim, deduped) | 3 | SF-D5-01 / SF-D4-01 / SF-D6-01 (BSGeometry import gap), SF-D4-02 / SF-D6-02 (.mesh parser absent), and the BSWeakReferenceNode dup |
| MEDIUM   | 6 | SF-D1-06 (CRC32 table), SF-DIM2-02..03, SF-D3-04..05, SF-D5-03 |
| LOW      | 7 | SF-DIM2-01, SF-DIM2-04, SF-D3-06, SF-D6-02..05 |

### State of play

| Subsystem | Verdict |
|-----------|---------|
| BA2 v2/v3 extraction | **Production-correct.** 108/108 vanilla archives open, 520/520 sampled extracts succeed in 1.25s (78 v2 GNRL + 15 v2 DX10 + 15 v3 DX10). 30 DX10 archives / 136 598 textures (ROADMAP's "~128K across 22" was an undercount). |
| NIF parse rate | **Misleading.** 31 058/31 058 *of `Meshes01.ba2`* via shader-blocks route. Gate test covers 35% of the corpus. `Meshes02.ba2` is **0% clean** (7552/7552 truncated by a missing `BSWeakReferenceNode` parser); `MeshesPatch.ba2` is 74%. Aggregate vanilla mesh-archive clean rate ≈ 80.6%. |
| BSGeometry parser | Correct (parses inline `BSGeometryMeshData` + reads external mesh refs). |
| BSGeometry importer | **Missing.** Zero consumers. All 5 sampled Starfield meshes (clutter, ship hull, character, weapon, rock) extract zero `ImportedMesh`. |
| `.mesh` companion-file parser | **Missing entirely.** Vanilla Starfield always uses external `geometries/<sha1>/<sha1>.mesh` files; the inline-geom path is rare. |
| Shader property tail-field parsing | **Broken on Starfield.** Four `bsver == 155` sites silently skip on `bsver = 172`. |
| BGSM / BGEM parser | Production-correct for FO4/SSE/FO76 (v1–v22). Crate doc-comment misadvertises Starfield support. |
| Starfield `.mat` JSON / `.cdb` parser | **Missing.** No fallback warn when stopcond captures a `.mat` path. |
| Starfield ESM parser | Untested. Falls through to FO4 dispatch under `GameKind::Starfield`. New record types (`PNDT`, `STDT`, `BIOM`, `SFBK`, `SUNP`, `GBFM`, `GBFT`) have no constants nor parsers. |
| Procedural ship assembly | Out of scope (gated on ESM + `.mat` + `.cdb`). |

---

## HIGH Severity Findings

### SF-D1-01..04 — Shader property tail-field gates use `==` instead of `>=`

Four sites in `crates/nif/src/blocks/shader.rs` gate FO76+ trailing fields on `bsver == 155` when nif.xml gates them on `BSVER >= 155`. Starfield reports `bsver = 172` per `version.rs:129`, so all four sites silently skip on every Starfield shader block. Every subsequent block parse for the same NIF drifts.

| ID | Location | Skipped bytes | Fix |
|---|---|---|---|
| SF-D1-01 | `shader.rs:947-985` (BLSP LuminanceParams + TranslucencyParams + texture_arrays) | ~24 + ≤22 + variable | `bsver == 155 → bsver >= 155` |
| SF-D1-02 | `shader.rs:923-927` (WetnessParams `unknown_2`) | 4 | `bsver == 155 → bsver >= 155` |
| SF-D1-03 | `shader.rs:799 / 827 / 990` (BSShaderType155 dispatch — skin tint / hair tint enum) | 12 + mis-routed semantics | All three become `>= 155`. Verify `parse_shader_type_data_fo76` is BSVER-agnostic. |
| SF-D1-04 | `shader.rs:1418-1422 / 1462-1477` (BSEffectShaderProperty refraction_power + reflectance/lighting/emittance/emit_gradient + Luminance) | ≥40 + 4 sized strings | Both gates become `>= 155` |

**Status**: Existing **#109** umbrella-covers SF-D1-01/02/04. SF-D1-03 (dispatch gate) is logically separate and worth its own subticket alongside #108. Once landed, expect Starfield clean parse rate to climb out of 97.19% across all five mesh archives.

### SF-D3-01 — BGSM/BGEM stopcond fires on ANY non-empty Name (no suffix gate)

- **Severity**: HIGH
- **Location**: `crates/nif/src/blocks/shader.rs:774-780, 1361-1368`
- **Status**: NEW
- **Description**: Both BLSP and BSEffect parsers short-circuit on `bsver >= 155 && !name.is_empty()`. nif.xml's stopcond is supposed to fire only when `Name` is a material-file reference (`.bgsm` / `.bgem` / `.mat`). On Starfield, blocks may carry a non-path editor label in `Name`; if so, the entire trailing body (shader_flags_1/2, CRC arrays, texture set ref, all PBR scalars) defaults to zero and downstream `block_size` skip is the only thing keeping the stream aligned.
- **Suggested Fix**: Tighten gate to suffix-aware `name.to_ascii_lowercase().ends_with(".bgsm" | ".bgem" | ".mat")`, matching the helper at `crates/nif/src/import/mesh.rs:756`. Strip trailing whitespace / `\0` first.

### SF-D3-02 — `bgsm` crate misadvertises Starfield support

- **Severity**: HIGH (correctness contract / documentation drift)
- **Location**: `crates/bgsm/src/lib.rs:1`
- **Status**: NEW (related to SF-D6-05)
- **Description**: lib.rs reads `Fallout 4 / Skyrim SE / FO76 / Starfield external material file`. The crate caps at v22 (BGSM + BGEM, FO4/SSE/FO76 only). **Starfield does not ship `.bgsm` or `.bgem` files** — it uses `.mat` JSON descriptors plus a global `materialsbeta.cdb` component database, both unparsed today.
- **Suggested Fix**: Remove "Starfield" from the doc-comment header. Add explicit "Not supported: Starfield (.mat JSON + .cdb)" note. Cross-link to a tracking issue for the Starfield material parser.

### SF-D3-03 — No `.mat` branch in `merge_bgsm_into_mesh` → silent fallback to NIF defaults

- **Severity**: HIGH
- **Location**: `byroredux/src/asset_provider.rs:435-535`
- **Status**: NEW
- **Description**: Dispatch tests `path.ends_with(".bgsm")` then `.bgem`, else `return false`. A Starfield NIF whose stopcond captured `materials/foo.mat` lands in the else branch with no warn / log — the renderer pulls whatever defaults the NIF stopcond stub left (empty texture_path, empty normal_map, default PBR scalars, no two-sided, no alpha test).
- **Suggested Fix**: Either (a) once-per-path warn log so missing `.mat` parser is visible during cell loads, or (b) wire a stub Starfield `.mat` parser that at least extracts texture paths. (a) is cheap and unblocks (b)'s effort sizing.

### SF-D5-01 / SF-D4-01 / SF-D6-01 — `BSGeometry` parses but never imports — every Starfield mesh extracts zero geometry

- **Severity**: HIGH (single root cause, three audit perspectives)
- **Location**: `crates/nif/src/blocks/bs_geometry.rs` parses; `crates/nif/src/import/{walk.rs,mesh.rs,mod.rs}` has zero `BSGeometry` arms.
- **Status**: NEW (parser side closed by **#708**; importer side is the unfiled follow-up)
- **Evidence**: 5 representative meshes traced through `import_nif_scene`:

| Sample | NIF | Blocks | BSGeometry blocks | ImportedMesh |
|--------|-----|--------|-------------------|--------------|
| Clutter | `meshes\setdressing\exotic_clutter\exoticplayingcard_heart_q.nif` | 16 | 4 | **0** |
| Ship hull | `meshes\ships\modules\hab\smod\smod_hab_hope_3l2w1h.nif` | 97 | 31 | **0** |
| Character skeleton | `meshes\actors\bipeda\characterassets\skeleton.nif` | 25 | 0 | **0** |
| Weapon | `meshes\weapons\maelstrom\6.nif` | 5 | 1 | **0** |
| Landscape rock | `meshes\landscape\rocks\rough\rockroughboulder02.nif` | 7 | 1 | **0** |

37 `BSGeometry` blocks across the 5 samples; zero converted to `ImportedMesh`. The mesh importer covers only `NiTriShape`, `NiTriStripsData`, `BSTriShape`, `BSDynamicTriShape`.

**Vertex format that needs to land** (from `bs_geometry.rs:250-313`, parser already correct):

| Channel | Wire format | vs FO4 BSTriShape |
|---|---|---|
| Position | 3× i16 NORM × `havok_scale 69.969` | new (was f16 / f32) |
| UV0/UV1 | 2× f16 each | dual-UV is new |
| Normal | u32 UDEC3 (10:10:10:2) | new packing |
| Tangent | u32 UDEC3 with W = bitangent sign | new packing + new sign encoding |
| Skin weights | variable count `(u16 bone, u16 weight NORM)` | was fixed 4× |
| Indices | u16 only | FO4 BSTriShape used u32 |
| LOD lists | `Vec<Vec<[u16; 3]>>` per LOD | new |
| Meshlets | DX12 `(vert_count, vert_offset, prim_count, prim_offset)` × n | new |
| Cull data | `(center: vec3, expand: vec3)` per meshlet | new |

- **Suggested Fix**: Two-stage rollout:
  1. **Issue A** (small): `BSGeometry::has_internal_geom_data()` → emit `ImportedMesh` from inline `BSGeometryMeshData`. Material falls back to checkerboard handle 0. Unblocks debug NIFs that ship inline geometry.
  2. **Issue B** (medium, see SF-D4-02 / SF-D6-02): external `.mesh` companion-file decoder. Vanilla Starfield always takes this path (`FLAG_INTERNAL_GEOM_DATA = 0x200` is never set), so visible content needs Issue B too.

### SF-D4-02 / SF-D6-02 — External `geometries/<sha1>/<sha1>.mesh` companion-file format has no parser

- **Severity**: HIGH
- **Location**: missing — no `crates/nif/src/mesh_file/`, no `MeshFile` parser, no decoder.
- **Status**: NEW
- **Description**: Vanilla Starfield separates vertex/index data into external files inside the same BA2. `BSGeometry::BSGeometryMesh::External { mesh_name }` is a dead leaf. Reference: nifly's `MeshFile.cpp` / `MeshFile.hpp`. Same UDEC3 / i16 NORM / half-UV layout as the inline `BSGeometryMeshData`, but as a standalone binary with its own header.
- **Suggested Fix**: New `crates/nif/src/mesh_file/` (or sibling `crates/sfmesh/`) module. Plumb the BA2 handle from the NIF importer to a sibling `.mesh` reader; extraction itself is solved.

### SF-D5-02 / SF-D4-03 — `BSWeakReferenceNode` undispatched (Starfield-only)

- **Severity**: HIGH
- **Location**: not in `crates/nif/src/blocks/mod.rs` dispatch; not in `reference/nifxml/nif.xml`; zero references in the codebase.
- **Status**: NEW
- **Evidence**: `nif_stats 'Starfield - Meshes02.ba2' --unknown-only` reports total 7552, **clean 0**, **truncated 7552 (100%)**, single unparsed type `BSWeakReferenceNode`. Plus 7552 hits in `MeshesPatch.ba2`.
- **Description**: Likely a packin / composite-LOD reference node. Hidden today because `parse_rate_starfield` only walks `Meshes01.ba2` (see SF-D5-03).
- **Suggested Fix**: Bake parser from `crates/nif/examples/trace_block.rs` against `meshes\terrain\packinnodes\composite\packin\fpiexoticsmineralformationssmall04_lod_2.nif`. nif.xml entry will need to be authored from observation since upstream nifxml doesn't have it yet.

---

## MEDIUM Severity Findings

### SF-D1-06 — `BSShaderCRC32` flag-name table covers ~32 of nif.xml's ~120 entries

- **Location**: `crates/nif/src/shader_flags.rs:206`
- **Status**: NEW (extension of #712)
- **Description**: 32 `pub const` entries cover the high-impact render-routing subset (DECAL, TWO_SIDED, CAST_SHADOWS, ZBUFFER_*, VERTEX_COLORS, SKIN_TINT, ENVMAP, EMIT_ENABLED, GLOWMAP, REFRACTION, MODELSPACENORMALS, GREYSCALE_TO_PALETTE_COLOR, HAIRTINT, PBR, etc.). Missing flags include LANDSCAPE, MULTIPLE_TEXTURES, FIRE_REFRACTION, EYE_ENVIRONMENT_MAPPING, CHARACTER_LIGHTING, SOFT_EFFECT, TESSELLATE, SCREENDOOR_ALPHA_FADE, LOCALMAP_HIDE_SECRET, LOD_LANDSCAPE, LOD_OBJECTS, plus all SLSF1/SLSF2 bits 16-31 not in the table. Raw u32s preserve on disk — but `bs_shader_crc32::contains_any` checks for missing flags silently miss-route.
- **Suggested Fix**: Mechanically generate the full ~120 entries from nif.xml `<enum name="BSShaderCRC32">`.

### SF-DIM2-02 — v3 unknown `compression_method` silently warns and falls back to zlib

- **Location**: `crates/bsa/src/ba2.rs:176-186`
- **Status**: NEW
- **Description**: An unknown `compression_method` (e.g. 1, 2, 4 for a future codec) emits a single warn line and proceeds to deflate-decode garbage bytes — drowning operators in N "decompression failed" lines per archive instead of one "unsupported codec" up front.
- **Suggested Fix**: Convert fallback to hard `Err(io::Error::InvalidData, "BA2 v3: unsupported compression method N (expected 0=zlib or 3=lz4_block)")`. Surfaces once at archive-open instead of per-extract.

### SF-DIM2-03 — Zero real-data integration tests for Starfield BA2 paths

- **Location**: `crates/bsa/tests/ba2_real.rs`
- **Status**: NEW (parallel to closed FO4 #587)
- **Description**: File ships three FO4-gated tests, zero Starfield equivalents. Session-7's v2/v3 sweep was external + uncommitted. A future regression in `compression_method` parsing or v2/v3 header-extension offset slips through `cargo test`.
- **Suggested Fix**: Three sibling tests gated on `BYROREDUX_STARFIELD_DATA`:
  1. `starfield_meshes01_ba2_v2_gnrl_extracts_nif_with_starfield_magic`
  2. `starfield_textures01_ba2_v3_dx10_extracts_lz4_block_dds`
  3. `starfield_constellation_textures_ba2_v2_dx10_extracts_zlib_dds` (proves we don't gate on type_tag)

### SF-D3-04 — Stopcond captures Name unmodified; downstream re-lowercases per block

- **Location**: `crates/nif/src/import/mesh.rs:750-761`
- **Status**: NEW
- **Description**: Two issues — (1) one allocation per shader block during import for the lowercase suffix test; (2) test coverage for `.MAT` / `.mat` is missing because there's no Starfield branch yet. Cosmetic; track alongside SF-D3-01's gate tightening.
- **Suggested Fix**: Use `eq_ignore_ascii_case` on the last 5 bytes; allocation-free.

### SF-D3-05 — BGEM vs BGSM dispatch gates on extension only, not file magic

- **Location**: `byroredux/src/asset_provider.rs:435, 504`
- **Status**: NEW
- **Description**: `merge_bgsm_into_mesh` dispatches purely on `path.ends_with(".bgsm" | ".bgem")`. A modded asset with mismatched magic vs suffix silently picks the wrong override semantics. Bethesda tooling enforces the suffix in vanilla; risk is in mods.
- **Suggested Fix**: Add a 4-byte magic sanity check before dispatching to the matching parser branch.

### SF-D5-03 — `parse_rate_starfield` covers 1 of 5 vanilla mesh archives

- **Location**: `crates/nif/tests/common/mod.rs:101`
- **Status**: NEW
- **Description**: Per-archive results from this audit:

  | Archive | NIFs | Clean % | Notes |
  |---------|------|---------|-------|
  | `Meshes01.ba2` | 31 058 | 98.17% | covered |
  | `Meshes02.ba2` | 7 552 | **0.00%** | uncovered; SF-D5-02 |
  | `MeshesPatch.ba2` | 29 849 | 74.37% | uncovered |
  | `LODMeshes.ba2` | 19 535 | 99.92% | uncovered |
  | `FaceMeshes.ba2` | 1 282 | 100.00% | uncovered |

  Aggregate vanilla mesh-archive clean rate: ~80.6%. The "100% recoverable / 31 058" headline conceals the Meshes02 0% reality.
- **Suggested Fix**: Extend `Game::Starfield` to expose all five archives, or add `parse_rate_starfield_all_meshes`. Document per-archive clean rates in ROADMAP.

---

## LOW Severity Findings

| ID | Location | Summary |
|---|---|---|
| SF-DIM2-01 | `ba2.rs:11-13, 163-166` | Module docstring claims `v2 = GNRL only / v3 = DX10 only`. Vanilla ships **15 v2 DX10 archives** (Constellation/OldMars/SFBGS-Textures/ShatteredSpace/CC). Code is variant-agnostic — comment is misleading only. |
| SF-DIM2-04 | `ba2.rs:78` | `compression` field threaded through every dispatch site but only ever flips on v3. Cosmetic. |
| SF-D3-06 | `shader.rs:708-742` | `material_reference_stub` hard-codes `texture_clamp_mode = 3` (WRAP/WRAP). Acceptable default; document the intent. |
| SF-D6-02 | (planning) | External `.mesh` companion-file decoder absent — same as SF-D4-02 from a planning angle. |
| SF-D6-03 | (planning) | Starfield `.mat` JSON parser absent. |
| SF-D6-04 | (planning) | `Starfield.esm` walked under FO4 dispatch — never validated. Recommend a smoke-test binary that walks the smallest interior CELL and reports per-record-type resolve rate before committing to a SF ESM parser. |
| SF-D6-05 | `crates/bgsm/src/lib.rs:1` | Documentation footnote on SF-D3-02. |

---

## Verified (no action required)

- **BA2 reader** (Dim 2): 108/108 vanilla archives open, 520/520 sample extractions succeed in 1.25s. Single dispatch site for GNRL + DX10 via `decompress_chunk()`. v3 12-byte extension at correct offset (hexdump-confirmed: `Starfield - Textures01.ba2` at 0x18-0x23). DX10 chunk layout identical for v1/v2/v3. Zero `unsafe` in `crates/bsa/src/`. All 41 BA2 unit tests pass.
- **BGSM/BGEM stopcond gate** (Dim 1, 3): `bsver >= 155 && !name.is_empty()` matches nif.xml at both call sites.
- **No trailing Phong reads after stopcond**: both shader parse fns `return Ok(material_reference_stub(net))` immediately.
- **Name → ImportedMesh.material_path → MaterialInfo.material_path continuity**: capture at `material/walker.rs:86-87, 176-178`; tests `bgsm_on_lighting_shader_still_captured`, `bgem_on_effect_shader_is_captured`, `bgsm_on_effect_shader_also_captured` cover all three crossings.
- **BGEM vs BGSM downstream branches distinct**: `merge_bgsm_into_mesh` correctly omits BGEM template walking; BGEM has no `root_material_path`.
- **SF1 / SF2 array gating**: `bsver >= 132` (line 808) and `bsver >= 152` (lines 810, 1384) — both correct per nif.xml.
- **WetnessParams `unknown_1`**: gated on `bsver >= 130` (was `> 130` pre-#403); fix landed.
- **NifVersion classification**: `version.rs:112-113` `uv2 < 170 → FO76, uv2 ≥ 170 → Starfield`. The `bsver >= 132` family is fine; the `bsver == 155` family (SF-D1-01..04) is the live bug.
- **BSTriShape vertex format (FO4 / FO76)** (Dim 4): all 9 checks pass — half-float position gate, VF_* flag set, tangent/bitangent decode, u32 indices on FO4+, `data_size`-derived stride override, strict `bsver == 155` Bound Min Max gate (correctly excludes Starfield), BSGeometry parser itself.
- **`BSMeshLODTriShape` / `BSSubIndexTriShape` not emitted by Starfield**: replaced wholesale by `BSGeometry` in vanilla content. Old parsers remain dormant-but-correct.
- **Starfield ESH (Medium Master) plug-in slot 0xFD** plumbed via `LegacyFormId::is_esh()` / `esh_index()` / `esh_local()` in `crates/plugin/src/legacy/mod.rs:89-104` — preparatory work, no follow-up needed.

---

## CRC32 Flag Table — Coverage

`crates/nif/src/shader_flags.rs::bs_shader_crc32` defines **32 `pub const` entries** covering the render-critical subset. nif.xml `<enum name="BSShaderCRC32">` defines roughly **120 entries** spanning all SLSF1 + SLSF2 bit positions. **~88 entries unmapped.**

**Covered (high-impact subset)**: DECAL, DYNAMIC_DECAL, TWO_SIDED, CAST_SHADOWS, ZBUFFER_TEST/WRITE, VERTEX_COLORS/ALPHA, SKIN_TINT, ENVMAP, FACE, EMIT_ENABLED, GLOWMAP, REFRACTION, REFRACTION_FALLOFF, NOFADE, INVERTED_FADE_PATTERN, RGB_FALLOFF, EXTERNAL_EMITTANCE, MODELSPACENORMALS, TRANSFORM_CHANGED, GREYSCALE_TO_PALETTE_COLOR, HAIRTINT, PBR (and ~10 others).

**Unmapped (silently miss-route)**: LANDSCAPE, MULTIPLE_TEXTURES, FIRE_REFRACTION, EYE_ENVIRONMENT_MAPPING, CHARACTER_LIGHTING, SOFT_EFFECT, TESSELLATE, SCREENDOOR_ALPHA_FADE, LOCALMAP_HIDE_SECRET, LOD_LANDSCAPE, LOD_OBJECTS, all SLSF1 / SLSF2 bits 16-31 not yet listed. Raw u32 values preserve on disk; importer-side `contains_any` calls return false.

**Action**: SF-D1-06 — generate missing entries mechanically from nif.xml.

---

## Forward Blocker Chain

Two end-state milestones with their dependency graphs.

### Milestone A — "A Starfield mesh renders with real material"

| Step | Status | Effort | Gates |
|---|---|---|---|
| 1. NIF parse (`BSGeometry` et al) | DONE (#708) | — | — |
| 2. BA2 v2/v3 + LZ4 block extract | DONE (Session 7) | — | — |
| 3. Fix `bsver == 155` → `>= 155` (SF-D1-01..04) | NOT DONE | small | — |
| 4. `BSGeometry → ImportedMesh` (internal-data path) | NOT DONE (SF-D5-01 / SF-D4-01 / SF-D6-01) | small | 3 |
| 5. External `.mesh` companion-file decoder | NOT DONE (SF-D4-02 / SF-D6-02) | medium | 4 |
| 6. `BSWeakReferenceNode` parser | NOT DONE (SF-D5-02 / SF-D4-03) | small | — |
| 7. Starfield `.mat` JSON parser + `MaterialProvider` integration | NOT DONE (SF-D6-03) | medium | 5 |
| 8. `materialsbeta.cdb` component-database reader | NOT DONE (SF-D6-03) | medium | 7 (deferred — loose `.mat` covers `Tools/ContentResources.zip` immediately) |

**Smallest visible win**: steps 3 + 4 + a checkerboard fallback. Lands inline-geom debug meshes with no new format work. With step 5 added, vanilla Starfield content (which exclusively uses external `.mesh`) becomes renderable — untextured.

### Milestone B — "A Starfield interior cell renders"

Adds, on top of A:

| Step | Status | Effort | Gates |
|---|---|---|---|
| 9. `Starfield.esm` CELL group walk verified | UNVERIFIED (SF-D6-04) | medium-large | A |
| 10. Starfield REFR resolution (FormID → STAT → mesh) | UNVERIFIED | medium | 9 |
| 11. New record types: `PNDT`, `STDT`, `BIOM`, `SFBK`, `SUNP`, `GBFM`, `GBFT` | NOT DONE | medium | 9 |
| 12. Evolved-from-FO4 record validation: `STAT`, `CELL`, `REFR`, `LIGH`, `DOOR`, `MSTT`, `LGTM` | UNVERIFIED | medium | 9 |
| 13. Space-cell concept (interiors are 1g rooms; spaces are different) | NOT DONE | large | 9-12 |
| 14. Procedural ship assembly (SCOL-like + module-snap graph + form-linker) | NOT DONE | very large | 9-13 |

**Recommended first concrete deliverable** (gates everything else): a `cargo run -- --sf-smoke <interior CELL EDID>` smoke test that walks `Starfield.esm`, picks the smallest interior cell, and reports per-record-type the percentage of REFRs whose base form is resolvable. That single number tells us whether SF ESM is "FO4 works for 80% of records" or "completely different schema." Without it, milestone B sizing is guessing.

**Procedural ship assembly without working ESM form-linker: confirmed not possible.** Out of scope until 9-13 land.

### Starfield ESM format — documentation status

| Source | Status |
|---|---|
| **SF1Edit (xEdit fork)** | Active (4.1.5o, 2025-10-01). De-facto reference; "spec" is its `.pas` definition files. ~5 GB working set. |
| Wrye Bash | Tracking issue #667 open since 2023; no merged support. |
| BAE | BA2 v2/v3 only; ESM out of scope. |
| Gibbed.Starfield | Reverse-engineering project; CDB material database covered. |
| niftools/nifskope | Tracking issue #232; partial NIF, no ESM. |

**Verdict**: no human-readable record-by-record spec analogous to UESP for TES4/TES5. Implementation will mean reading SF1Edit's Pascal definitions and hand-validating against `Starfield.esm` — larger reverse-engineering investment than FO4 was.

---

## Dedup Notes

Cross-dimension duplicates merged in this report:

| Dup pair | Tracked under |
|---|---|
| SF-D5-01 + SF-D4-01 + SF-D6-01 (BSGeometry import) | SF-D5-01 (single ticket; D4-01 is the vertex-format-mapping sub-axis worth flagging in PR description) |
| SF-D5-02 + SF-D4-03 (BSWeakReferenceNode) | SF-D5-02 |
| SF-D4-02 + SF-D6-02 (.mesh decoder) | SF-D4-02 |
| SF-D3-02 + SF-D6-05 (bgsm crate doc) | SF-D3-02 (SF-D6-05 is the documentation-fix angle) |
| SF-D1-01/02/04 (== 155 → >= 155 value gates) | Existing **#109** umbrella |
| SF-D1-03 (BSShaderType155 dispatch gate) | Related to #108; logically separate from #109 |
| SF-D1-06 (CRC table completion) | Extension of closed **#712** |
| SF-DIM2-03 (SF BA2 integration tests) | Parallel to closed FO4 **#587** |

No CRITICAL findings; no GPU-unsafe paths; no FFI lifetime issues; no missing AS / sync barriers identified in the audited surface.

---

## Audit Helpers

Throwaway debug examples added to `crates/nif/examples/` (d5_ prefix shared with prior audits) — keep or delete:

- `d5_listba2.rs` — list NIFs in a BA2 matching a pattern.
- `d5_starfield_import.rs` — extract NIFs, parse, dump per-block-type histogram, run `import_nif_scene`, report nodes/meshes/material_path/vertex counts.
- `d5_ba2_extract_check.rs` — open every BA2 archive, report version+variant, attempt extracts.

Plus `crates/bsa/examples/sf_sweep.rs` (audit-only) for the 108-archive BA2 sweep.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_STARFIELD_2026-04-27.md
```

This will create GitHub issues for SF-D1-01..04, SF-D3-01..03, SF-D5-01..03, SF-D4-02, SF-DIM2-02..03, and the LOW-tier doc fixes, deduped against the existing 81-issue tracker.
