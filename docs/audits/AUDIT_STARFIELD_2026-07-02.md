# Starfield Compatibility Audit — 2026-07-02

**Scope**: All 9 dimensions. Engine HEAD `1b4e8e84` on `main` (post-#1567
Starfield LIGH DAT2 decode, post-#1606 BSLightingShaderProperty opaque tail,
post-#1571 DLC/Creation CDB discovery, post-#1291..#1295 walkable-Cydonia
bring-up). Depth/correctness audit of the existing Starfield bring-up surface —
not a from-scratch gap inventory.

**Method**: Every finding re-verified against current source this session. Dims
1-3 reused prior worker drafts *after* re-confirming each site at HEAD; Dims 4-9
run inline. Dim 4 validated against live vanilla `Starfield.esm` via the
`--sf-smoke` harness; Dim 1 (prior) validated against live vanilla `*.ba2`.

---

## Executive Summary

Starfield is a first-class `GameKind` and the bring-up surface is healthy. The
depth/correctness sweep confirms:

- **BA2 v2/v3 + LZ4 block (Dim 1)** — correct and live-validated (200/200 DDS,
  200/200 `.mesh` extracted; v3 `compression_method` at byte 32; unknown-method
  is a hard error). No new defects.
- **CDB materials (Dim 3)** — parse path is bounds-checked, multi-CDB DLC/Creation
  discovery (#1571) works, `peek_magic` disambiguates CDB↔BGSM. The per-field CDB
  *value* extraction is still the known #1289 Phase-2 follow-up (`.mat` materials
  reach the Disney lobe with NIF defaults) — confirmed, not re-filed.
- **ESM resolve rate (Dim 4)** — **improved** to **91.2% (25 437 / 27 898 REFRs)**
  on `CityCydoniaMainLevel`, up from the 2026-06-14/06-23 baseline of 88.8%
  (24 781). The +656-REFR delta is exactly the #1567 LIGH-light indexing fix
  landing. **No regression.**
- **ESM/cell spawn gates (Dim 5)** — every regression guard present:
  `XCLL_SIZES_STARFIELD = [28, 108]` (with the #1579 `>= 108` decoder refinement),
  PDCL named-skip (#1568), `base_layer`-gated trimesh fallback (#1294),
  `SceneFlags`/`DoorTeleport`/`FormIdComponent` at spawn, ghost-entity colliders
  (no BLAS entry).
- **NIF shader BSVER 155+ (Dim 6)** — #1510 over-read stays fixed; the #1606
  opaque `starfield_tail` is captured to `block_size` with saturating arithmetic
  (no over-read, no hardcoded 38). The sibling BSEffectShaderProperty +32 B
  under-read remains the known scoped-out follow-up (not re-filed).
- **NIFAL material translation (Dim 8)** — `translate_material` is the single
  boundary; metalness/roughness are plain resolved `f32`; `bgem_glass` forwarded.
  Clean.
- **BGSM/BGEM flow (Dim 9)** — every `material_flag::*` bit derives from the right
  `ImportedMesh` field; BGEM `glass_enabled` → `mesh.bgem_glass`. The BGEM
  `grayscale_to_palette_alpha` bool remains parsed-but-unforwarded (existing open
  **#1580**).

The one genuinely new correctness risk is in **BSGeometry mesh extraction
(Dim 2)**: the external `.mesh` slot loop accepts the first slot that merely
*parses*, not the first that carries geometry — a re-introduction of the #1209
failure class on the external branch (SF2-01, HIGH).

**Findings**: 1 HIGH, 2 MEDIUM, 3 LOW (4 NEW, 2 surfaced-existing).

---

## Dimension Findings

### Dimension 2 — BSGeometry Mesh Extraction

#### SF2-01: Stage B external `.mesh` loop short-circuits on first *parsed* slot, not first slot *with geometry* (re-introduces the #1209 class on the external path)
- **Severity**: HIGH
- **Dimension**: BSGeometry mesh
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:52-82`
- **Status**: NEW (regression-of-concept of #1209, which fixed Stage A only)
- **Description**: Stage B iterates `shape.meshes` external slots and `break`s on
  the first slot whose `.mesh` companion parses `Ok(...)` (lines 58-62). But a
  vanilla-Starfield `.mesh` slot with the `scale <= 0.0` sentinel parses `Ok`
  with **empty** `vertices` (populated `triangles`, empty everything else) — that
  is exactly the "segment-only / skin-weight-only slot that shares a parent
  BSGeometry with a populated slot" case documented at
  `crates/nif/src/blocks/bs_geometry.rs:388-390`. The loop takes that empty result
  as `found`, breaks, then the guard at line 80
  (`if mesh_data.vertices.is_empty() || mesh_data.triangles.is_empty() { return None }`)
  drops the entire BSGeometry — even when a later slot carries full geometry.
  `extract_bs_geometry` returning `None` makes the walker push nothing
  (`import/walk/mod.rs`), so the visible mesh silently vanishes.
- **Evidence**:
  - Break-on-first-`Ok`: `bs_geometry.rs:58-62`.
  - Sentinel returns empty-but-`Ok` body: `blocks/bs_geometry.rs:391-408`
    (`if scale <= 0.0 { return Ok(Self { … vertices: Vec::new(), triangles … }) }`);
    doc lines 388-390 confirm vanilla SF uses multi-slot BSGeometry with sentinel
    slots.
  - Post-loop drop: `bs_geometry.rs:80-82`.
  - Stage A (line 32-35, #1209 fix) already iterates all slots but likewise does
    not skip empty ones — see SF2-02.
- **Impact**: Any Starfield BSGeometry whose slot order places a `scale<=0`
  sentinel external `.mesh` before the populated one renders as nothing. This is
  the failure mode #1209 was filed to kill, on the external branch (the ~99%
  Starfield case per `blocks/bs_geometry.rs:9-11`). Blast radius depends on how
  often vanilla LOD-slot ordering puts a sentinel first; the #1209 precedent shows
  sentinel-first ordering does occur in practice.
- **Suggested Fix**: In the Stage B loop, don't `break` on the mere `Ok`; keep
  iterating past empty results —
  `if !data.vertices.is_empty() && !data.triangles.is_empty() { found = Some(data); break; }`
  (still `log::debug!` skipped empties). Add a regression test with
  `meshes = [External(sentinel scale<=0), External(populated)]` asserting a mesh
  is returned.

#### SF2-02: Stage A internal-geom `find_map` accepts the first `Internal` slot even when its body is empty (same short-circuit, internal branch)
- **Severity**: MEDIUM
- **Dimension**: BSGeometry mesh
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:32-35`
- **Status**: NEW (sibling of SF2-01 / #1209)
- **Description**: The Stage A `find_map` returns the first slot whose `kind` is
  `Internal { mesh_data }` regardless of whether `mesh_data.vertices` /
  `triangles` is empty. If an inline `Internal` slot is itself a `scale<=0`
  sentinel (the format permits inline sentinel slots) and a later `Internal` slot
  is populated, line 80 drops the mesh. Lower severity than SF2-01 because vanilla
  Starfield ships external `.mesh` (inline is authoring-tool / port only per
  `blocks/bs_geometry.rs:208-212`), so the trigger is rarer.
- **Evidence**: `shape.meshes.iter().find_map(|m| match &m.kind { Internal { mesh_data } => Some(mesh_data.as_ref()), External { .. } => None })?`
  — no emptiness check (`bs_geometry.rs:32-35`). Sentinel-empty body applies to
  inline too (`blocks/bs_geometry.rs:391-408`, reached via
  `BSGeometryMeshData::parse`).
- **Impact**: Inline-geometry Starfield / ported meshes with a sentinel-first slot
  order silently drop. Rare in vanilla, realistic in modded/ported content.
- **Suggested Fix**: Add the emptiness check to the `find_map` closure:
  `Internal { mesh_data } if !mesh_data.vertices.is_empty() && !mesh_data.triangles.is_empty() => Some(mesh_data.as_ref())`.
  Fold into the SF2-01 test module.

#### SF2-03: `BSGeometryMesh.tri_size` / `num_verts` hints parsed then never validated against the resolved geometry
- **Severity**: LOW
- **Dimension**: BSGeometry mesh
- **Location**: `crates/nif/src/blocks/bs_geometry.rs:188-198`; consumer
  `crates/nif/src/import/mesh/bs_geometry.rs`
- **Status**: NEW
- **Description**: Each `BSGeometryMesh` slot carries `tri_size` (triangle-index
  byte-size hint) and `num_verts` (vertex-count hint), "always present regardless
  of internal/external". The importer never cross-checks these against the
  actually-parsed `mesh_data.vertices.len()` / `triangles.len()`. A slot's hint
  disagreeing with its resolved `.mesh` body is a strong signal of a wrong-file
  resolve (hash collision, stale archive) or a truncated companion — currently
  undiagnosable.
- **Evidence**: fields declared `bs_geometry.rs:188-192`, read into the struct,
  never read again outside `Debug`. No comparison site anywhere in `import/mesh`.
- **Impact**: Defense-in-depth gap only; no incorrect render on its own.
- **Suggested Fix**: After Stage B parse succeeds, `log::debug!` (or
  `debug_assert`) when `data.vertices.len() != num_verts as usize` or the
  `tri_size`-derived triangle count disagrees, to surface bad resolves during
  bring-up.

### Dimension 3 — CDB Material Database

#### SF3-01: Per-field CDB extraction not implemented — `.mat` materials reach the Disney lobe with NIF defaults
- **Severity**: MEDIUM
- **Dimension**: CDB materials (correctness)
- **Location**: `byroredux/src/asset_provider/material.rs` (`.mat` arm in
  `merge_bgsm_into_mesh`; `MaterialProvider::sf_cdbs`)
- **Status**: Existing — #1289 Phase 2 (do NOT re-file). Tracked as the current
  top material blocker per `starfield-esm-roadmap.md`.
- **Description**: The CDB is parsed and held (`sf_cdbs`) but ONLY for presence
  (`has_starfield_cdb()`). The `.mat` arm flips `mesh.is_pbr = true` to route
  Starfield content through the Disney BSDF, then returns without reading any
  authored value out of the CDB. `metalness_override` / `roughness_override` stay
  `None` (→ NaN sentinel → `Material::resolve_pbr` keyword classifier), and
  texture-slot paths are never forwarded from the CDB. Every vanilla Starfield
  `.mat` material renders with NIF-derived defaults on the Disney lobe, not its
  authored parameters.
- **Evidence**: The `.mat` arm sets `is_pbr` and returns; no `sf_cdbs` value walk
  exists anywhere in the crate (used only in `has_starfield_cdb` /
  `load_starfield_cdb` / the accumulate test). Contrast the FO4 BGSM arm which
  does full spec-glossiness → metallic-roughness translation.
- **Impact**: Starfield materials are shaded better than legacy Lambert but remain
  approximate. Correctly deferred and documented; confirmed here that current
  state matches the roadmap claim.
- **Suggested Fix (scope only — #1289 Phase 2, already tracked)**: Walk `sf_cdbs`
  in load order once at load, build a `material_path → MaterialFields` index from
  the `BSMaterial::*` instance classes, and forward
  metalness/roughness/texture-slot/emissive into `ImportedMesh` in the `.mat` arm,
  at the same parser→Material boundary as the BGSM arm. Do NOT open a new issue.

#### SF3-02: `.mat` arm silently no-ops (falls to "unsupported format" warn) when the CDB fails to parse
- **Severity**: LOW
- **Dimension**: CDB materials (defense-in-depth / diagnosability)
- **Location**: `byroredux/src/asset_provider/material.rs` (`load_starfield_cdb`
  warn+drop; `.mat` gate; unknown-extension fallback warn)
- **Status**: NEW
- **Description**: `load_starfield_cdb` warns and drops on parse failure, leaving
  `sf_cdbs` possibly empty. If the ONLY CDB fails (e.g. a future patch bumps CDB
  fileVersion past 4 → `UnsupportedVersion`, a hard bail per the #1569 pins),
  `has_starfield_cdb()` returns false, the `.mat` gate is skipped, and every
  `.mat` mesh falls through to the unknown-extension arm — logging "unsupported
  format (Starfield .mat?)" per path. The operator sees generic per-material spam
  that does not point back at the single upstream CDB failure (logged once, far
  earlier).
- **Evidence**: The `load_starfield_cdb` failure branch logs once and returns; the
  downstream `.mat` warning never references CDB state. The two log sites are
  disjoint.
- **Impact**: Diagnosability only. Content still renders (NIF-default Lambert). A
  future Starfield update changing the CDB version would present as thousands of
  "unsupported .mat" warnings rather than one clear degradation line.
- **Suggested Fix**: In the `.mat` fallback, when the path ends `.mat` AND
  `!has_starfield_cdb()`, emit a distinct once-only warning naming the likely
  cause ("Starfield .mat encountered but no CDB loaded/parsed — check
  --materials-ba2 and CDB version").

### Dimension 1 — BA2 v2/v3 LZ4

#### SF1-01: `Dx10Chunk::end_mip` set-but-never-read (dead field)
- **Severity**: LOW
- **Dimension**: BA2 v2/v3 LZ4
- **Location**: `crates/bsa/src/ba2.rs` (`Dx10Chunk` field declarations + reader)
- **Status**: Existing: #1761 (TD8-004)
- **Description**: `Dx10Chunk::start_mip` is now read (used in the #1176
  monotonicity guard) so its `#[allow(dead_code)]` is redundant, and `end_mip` is
  parsed but never consumed. Both are placeholders for the M40 mip-range streaming
  milestone. This is exactly #1761.
- **Impact**: None functional — code hygiene only.
- **Suggested Fix**: Track under #1761. No action required for Starfield
  correctness.

---

## Dimensions With No New Findings

- **Dim 4 (ESM resolve rate)** — live-run 91.2% (25 437 / 27 898) on Cydonia, up
  from 88.8% baseline; the +656 delta = #1567 LIGH fix. No regression. Remaining
  8.8% dominated by non-mesh REFRs + the known PDCL-decal (~1846) and #1576
  model-less-STAT (geometry-in-BFCB) gaps — both already-open, not re-filed.
- **Dim 5 (ESM/cell spawn regression surface)** — all guards verified present:
  `XCLL_SIZES_STARFIELD = [28, 108]` + `>= 108` decoder gate (#1579), PDCL named
  skip in `skipped_unconsumed_groups` (#1568), `base_layer`-gated trimesh fallback
  (#1294) spawning MeshHandle-free ghost colliders (no BLAS entry — the
  R6a-stale-14 fix), `SceneFlags::from_nif` + `DoorTeleport`(XTEL) +
  `FormIdComponent` at spawn, `SkinSlotPool` ceiling (#1284).
- **Dim 6 (NIF shader BSVER 155+)** — #1510 over-read stays fixed; the #1606
  `starfield_tail` capture (`read_starfield_tail`) uses saturating `block_size -
  consumed` with no over-read and no hardcoded 38; the dispatcher passes
  `block_size` only to `BSLightingShaderProperty::parse_with_size`. The
  BSEffectShaderProperty +32 B under-read (no `block_size` plumbed) is the known
  scoped-out follow-up — noted, not re-filed.
- **Dim 7 (real-data NIF parse rate)** — held at the ROADMAP compat-matrix figure:
  **98.6% aggregate, recover 100%** across all 5 vanilla mesh archives (Meshes01
  97.21%, Meshes02 100%, MeshesPatch 98.11%, LODMeshes 99.92%, FaceMeshes 100%).
  Residual truncation tail tracked at #746/#747 (not grown). Corroborated by
  Dim 1's live extraction (200/200 `.mesh` with BSGeometry magic). The published
  compat-matrix/ROADMAP under-statement of per-archive figures is doc-rot, already
  tracked as **#1717 (SF-D7-01)**.
- **Dim 8 (NIFAL material translation)** — `translate_material` is the single
  `ImportedMesh → Material` boundary; `Material.metalness`/`roughness` are plain
  resolved `f32` filled by `resolve_pbr` (no per-draw `classify_pbr`); `bgem_glass`
  forwarded to `classify_glass_into_material`. Emitter/collision NIFAL slices
  wired. Clean.
- **Dim 9 (BGSM/BGEM external material flow)** — `pack_bgsm_material_flags` derives
  each `material_flag::{BGSM_AUTHORED, PBR_BSDF, TRANSLUCENCY, MODEL_SPACE_NORMALS,
  EFFECT_PALETTE_COLOR, TRANSLUCENCY_THICK_OBJECT, TRANSLUCENCY_MIX_ALBEDO}` bit
  from the correct `ImportedMesh` field; BGEM `glass_enabled` → `mesh.bgem_glass`
  (#1280). The BGEM `grayscale_to_palette_alpha` bool is parsed but has no consumer
  — existing open **#1580 (SF-D9-02)**, not re-filed.

---

## CRC32 Shader-Flag Table

No new empirical CRC32 hash → flag-name mappings were derived this sweep. The
`sf1_crcs` / `sf2_crcs` arrays (`crates/nif/src/blocks/shader.rs`,
`parse_skyrim_shader_base`) are still stored as opaque `Vec<u32>` — there is no
hash→name table in-repo, and none is required for correct byte consumption
(the arrays are length-prefixed and consumed by count). Building an empirical
table remains future enhancement work, not a correctness gap.

---

## Remaining-Work Chain (per `starfield-esm-roadmap.md`)

Phases 0+1 done; Phases 2-4 invalidated by the ~99.9%-record-parity measurement.
In order of value, the outstanding items — all tracked, none re-filed here:

1. **Per-field CDB extraction** (#1289 Phase 2 — SF3-01 above): `.mat`-resolved
   materials currently reach the Disney lobe with NIF defaults.
2. **ESM-placed content gaps in Cydonia**: PDCL decals (~1846 REFRs, named-skip)
   and #1576 model-less STAT/BNDS/ACTI/ARMO (geometry in a BFCB component block).
3. **Exterior worldspace tiles / space-cell / planet / GBFM records** — out of
   scope for the walkable-interior milestone; GBFM confirmed non-dominant for
   Cydonia (stub-and-skip).
4. **#746/#747 NIF truncation tail** in Meshes01 / MeshesPatch — residual drift,
   not grown.
5. **BSEffectShaderProperty +32 B under-read** (Dim 6) — plumb `block_size` into
   `BSEffectShaderProperty::parse` mirroring the #1606 `parse_with_size` pattern.

---

## Deduplication Notes

- Dedup baseline: `gh issue list --limit 300` (21 open issues) captured to
  `/tmp/audit/issues.json` this session.
- Surfaced-existing (not re-filed): **#1761** (SF1-01), **#1289** (SF3-01),
  **#1580** (Dim 9 BGEM grayscale), **#1576** (Dim 4/5 model-less STAT), **#1717**
  (Dim 7 doc-rot).
- NEW: SF2-01 (HIGH), SF2-02 (MEDIUM), SF2-03 (LOW), SF3-02 (LOW).

---

_Suggested next step_: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-07-02.md`
