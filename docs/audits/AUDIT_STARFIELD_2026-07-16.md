# Starfield Compatibility Audit — 2026-07-16

Depth/correctness regression audit of ByroRedux's shipped Starfield support.
Starfield is a first-class `GameKind`: NIF + BA2 v2/v3 (LZ4-block), CDB +
BGSM/BGEM materials, and a walkable Cydonia interior all ship today. This is
**not** a from-scratch gap inventory — it targets regressions in the bring-up
surface (BA2 v3 decompress, CDB chunk index, BSGeometry `.mesh` resolution,
spawn gates, NIFAL translation) plus the remaining tracked ESM phase work.

9 dimensions audited, each by an independent sub-agent tracing real code paths
and, where feasible, running against real game data at
`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/`.

**Total: 14 findings — 0 CRITICAL, 1 HIGH, 4 MEDIUM, 9 LOW.**
Of these, 1 (SF2D2-03) restates an already-tracked open issue (#1827) found
intact-but-still-open during regression verification; 13 are NEW.

## Executive Summary

The Starfield bring-up surface is in good shape. Six of nine dimensions found
**zero or near-zero live defects** — BA2 v2/v3 LZ4 decompression, BSGeometry
mesh extraction's seven previously-fixed regression guards (#1292/#1209/#1828/
#1829/#1203/#1232), CDB parsing correctness (line-by-line matches the Gibbed
reference on a real 105 MB / 1.44M-instance vanilla database), ESM/cell
bring-up regression guards (7 named spawn-path fixes), and NIF shader-block
parsing at BSVER 155+ (0 `NiUnknown` across 202,586 real `BSLightingShaderProperty`
blocks) all verified intact with no regressions.

The two dimensions that surfaced real, previously-unknown defects were the
ones that ran the deepest real-data tracing:

- **Dimension 7 (Real-Data Validation)** found the project's own documented
  Starfield launch command silently drops single-LOD-slot geometry (measured
  17.9% sub-mesh loss on a real weapon NIF) because the archive
  auto-sibling-loading heuristic doesn't recognize Starfield's two-digit
  zero-padded mesh-series naming (`Meshes01`/`Meshes02`) — **HIGH**, and
  separately found the documented "#746/#747" citation for the MeshesPatch
  truncation tail is wrong; the real, still-open cause is an unfixed
  `BSWeakReferenceNode` garbage-skip bug on populated weak-ref lists —
  **MEDIUM**.
- **Dimension 9 (BGSM/BGEM)** found `EFFECT_PALETTE_COLOR`/`ALPHA` is derived
  from greyscale-texture-slot *presence* rather than the authoritative
  `grayscale_to_palette_color` enable bit (which is parsed and never
  consumed) — asymmetric with the correctly-gated inline-NIF path — **MEDIUM**.

The remaining findings are lower-severity: a memory/startup-cost waste in the
CDB Phase-1 loader (parses and retains 1.44M material instances just to
answer one boolean — MEDIUM), a known/tracked skinning gap (#1827, bind-pose
only for BSGeometry actors), and a cluster of doc-rot findings (stale
comments describing superseded gate predicates or non-existent code paths)
that carry no runtime impact but risk misleading a future engineer into
reintroducing a fixed bug.

No CRITICAL findings. No wrong/divergent `Material` was found escaping the
`translate_material` NIFAL boundary (Dimension 8 confirmed clean).

---

## Dimension Findings

### Dimension 1: BA2 v2/v3 — LZ4 Block Decompression

6 of 7 checklist items verified OK with no defects: v2/v3 header offset math,
compression-method dispatch (hard error on unknown codec), per-chunk
raw-vs-LZ4 selection in mixed v3 DX10 mip chains, the unified GNRL+DX10
decompress path, and the version-independent DX10 chunk layout. Real-data
sweep: 129/129 archives extract cleanly; a scoped full-extraction probe of all
15 real v3/LZ4 archives recovered 104,818/104,818 entries with zero errors.

#### LZ4-01: LZ4 decompress relies on undocumented-safe dependency behavior the crate itself disclaims as "may panic"
- **Severity**: LOW
- **Location**: `crates/bsa/src/ba2.rs:692-696` (comment), `:717-724` (call site)
- **Status**: NEW
- **Description**: A comment asserts the LZ4 branch "inherently size-checks" and
  hard-errors on a size mismatch, but pinned `lz4_flex 0.11.6`'s own docs state
  the function "may panic" if `min_uncompressed_size` undershoots the true
  decompressed size — a stronger guarantee than the dependency's public
  contract promises. Empirical fuzzing (constructed LZ4 payloads, undersized
  from 1 byte to 0) found zero panics on the currently pinned version — not an
  active bug, but an unpinned assumption that could silently regress on a
  future `lz4_flex` upgrade.
- **Impact**: None today. A future dependency bump that tightens/loosens the
  internal bounds-check discipline (within its still-compatible public
  contract) could crash the engine on a malformed/adversarial v3 BA2 chunk
  record, with no code change on this side to explain why.
- **Suggested Fix**: Wrap the call in `catch_unwind` and convert a caught panic
  into the existing `Err` path, or pin the safety claim to `lz4_flex 0.11.6`
  with a version-gated regression test.

### Dimension 2: BSGeometry Mesh Extraction

All seven referenced regression guards (#1292, #1209, #1828, #1829, #1203,
#1232, plus #1830 hint-mismatch) are **intact**, backed by live tests
exercising the real `extract_bs_geometry` entry point. Index/attribute-length
safety verified — out-of-range indices are clamped downstream, no OOB/panic
risk on malformed `.mesh` data.

#### SF2D2-01: BSGeometry block bounding-sphere scale not cross-checked against havok-scaled vertices
- **Severity**: LOW
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:233-249`
- **Status**: NEW (low-confidence, needs real-data spot-check)
- **Description**: The block's raw `bounding_sphere` is used verbatim as the
  mesh's local bound whenever `radius > 0`, with no cross-check that it's
  expressed in the same havok-scaled units as the decoded vertices. If units
  diverge, the bound could be ~70x too small, causing off-axis culling pop.
  Cydonia renders correctly today, which is evidence against a gross
  mismatch — hence LOW and flagged as needs-verification rather than confirmed.
- **Suggested Fix**: Add a debug-only sanity check comparing the sphere radius
  against the actual max vertex extent (mirroring the existing
  `bs_geometry_hint_mismatch` pattern); spot-check one vanilla Cydonia `.mesh`.

#### SF2D2-02: Secondary UV channel (`uvs1`) parsed then dropped by the importer
- **Severity**: LOW
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:160`
- **Status**: NEW
- **Description**: `BSGeometryMeshData` decodes both `uvs0` and `uvs1`, but
  `extract_bs_geometry` only consumes `uvs0`. Starfield uses the second UV set
  for detail/decal and some shader layering; affected content renders with the
  primary UV set only. Not actionable until `Vertex`/`ImportedMesh` grow a
  second UV slot.
- **Suggested Fix**: Track as an enhancement; thread `uvs1` through once the
  vertex format supports a second UV channel.

#### SF2D2-03: Starfield BSGeometry skinned meshes get bones but no per-vertex weights (won't deform)
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/import/mesh/skin.rs:261-275`
- **Status**: Existing: #1827 (OPEN)
- **Description**: `extract_skin_bs_geometry` returns empty
  `vertex_bone_indices`/`vertex_bone_weights`, so every Starfield BSGeometry
  actor falls to the rigid path and renders in bind pose, even though
  `BSGeometryMeshData` does decode `skin_weights`. Bones/bind-inverses resolve
  correctly; only the per-vertex densify is missing. Reported here only for
  regression-guard completeness — already tracked, not a new defect.

### Dimension 3: CDB Material Database Correctness

Line-by-line diff against the authoritative Gibbed reference confirms the CDB
parser is a faithful, correct port; the real 105 MB vanilla
`materialsbeta.cdb` parses cleanly to 97 classes / 1,438,780 instances. All
named issues (#762, #1289, #1571, #1569, #1290, #1831) are CLOSED and their
fixes verified in place, including the `index_chunks` underflow/overflow guard
(#762) and the multi-CDB DLC/Creation discovery scan (#1571). Confirmed CDB is
the sole vanilla Starfield material source (zero loose BGSM/BGEM in the
archive). The per-field CDB→Material extraction gap is the known, tracked
Phase-2 follow-up (deferred half of #1289) — scoped here, not re-filed.

#### SF-D3-AUDIT-01: Full 1.44M-instance CDB tree parsed and retained for the whole session purely to answer a boolean presence check
- **Severity**: MEDIUM
- **Location**: `byroredux/src/asset_provider/material.rs:236-267` (`load_starfield_cdb`/`has_starfield_cdb`); `crates/sfmaterial/src/reader.rs:29-90` (`parse`)
- **Status**: NEW
- **Description**: The only Phase-1 consumer of the parsed CDB is
  `has_starfield_cdb() { !self.sf_cdbs.is_empty() }`, yet `load_starfield_cdb`
  runs the entire parse, materializing a 1,438,780-element tree (each entry
  carrying a cloned `class_name` String plus a `BTreeMap` of fields) and
  retaining it in an `Arc` for the provider's whole lifetime. Nothing else
  walks it.
- **Evidence**: Measured 9.70s parse time (debug) for the real vanilla CDB; the
  retained structure is hundreds of MB to low-GB of host RAM, entirely unread.
- **Impact**: Multi-second startup stall plus steady-state RAM retention of a
  dead structure on every Starfield launch, worsening as DLC CDBs are added
  (each is another full tree). Not a crash, but material against the project's
  tight memory budget.
- **Suggested Fix**: Either probe presence header-only (parse just the
  header/chunk-index without walking instances), or drop `instances` after the
  non-empty check and re-parse when Phase 2's per-field index is actually
  built.

#### SF-D3-AUDIT-02: `read_primitive_string` omits the reference's trailing-NUL trim
- **Severity**: LOW
- **Location**: `crates/sfmaterial/src/reader.rs:535-539`
- **Status**: NEW
- **Description**: Gibbed reads inline CDB strings with `trimNull=true`; the
  Rust port reads exactly `len` bytes with no NUL trimming, so a
  length-prefixed string whose window includes a terminating/embedded NUL
  would yield a `String` with an embedded `\0`, diverging from the reference.
  Not reachable in Phase 1 (no inline strings are read from the retained tree
  yet); vanilla data produced clean names in this run, so risk is latent, not
  active.
- **Suggested Fix**: Truncate at the first `0x00` within the read window before
  the lossy UTF-8 decode, matching the reference semantics.

#### SF-D3-AUDIT-03: `ComponentDatabaseFile::peek_magic` is test-only, not wired into production discovery
- **Severity**: LOW
- **Location**: `crates/sfmaterial/src/reader.rs:95-101`; `byroredux/src/asset_provider/material.rs:23-45`
- **Status**: NEW
- **Description**: CDB discovery is purely path-based; a discovered path goes
  straight to the full `parse()`, which relies on `parse_header`'s `BadMagic`
  rejection rather than the cheaper dedicated `peek_magic` probe (only
  exercised by a test). Correctness is unaffected — a mis-named non-CDB file is
  still rejected, just via the heavier path.
- **Suggested Fix**: Gate discovery with `peek_magic` before the full
  extract/parse for a cheap reject, or document it as an intentional public
  probe for external callers.

### Dimension 4: Starfield ESM Resolve-Rate Baseline

Live `--sf-smoke` run against the real, currently-patched `Starfield.esm`
reproduces the Cydonia resolve rate exactly: **91.2% (25,437/27,898 REFRs)**,
byte-identical to the 2026-07-02/07-03 audit baseline. Top-level GRUP byte
coverage (86.1%), leaf REFR/CELL/GBFM counts, and the #1567 LIGH `DAT2` decode
(656 Cydonia lights, all resolving) all reproduced exactly with no regression.

#### DIM4-STARFIELD-01: Phase 0/1 baseline doc's interior-REFR capture count is stale by 16 records (explained, non-code)
- **Severity**: LOW
- **Location**: `docs/engine/starfield-esm-phase0-baseline.md:174`
- **Status**: NEW
- **Description**: The doc's 2026-05-28-captured interior-REFR count
  (1,971,151) is 16 higher than a live re-run today (1,971,135). Traced to
  commit `2dc43106` (2026-06-26, post-dating the doc), which correctly skips
  deletion-tombstone REFRs — 16 vanilla Starfield.esm interior REFRs carry the
  Deleted flag and are now correctly excluded. This is the intended, more
  correct behavior of that fix, not a bug; the doc is simply dated.
- **Suggested Fix**: Add a "superseded by #1660" note next to the stale table,
  or refresh the captured count. Doc-only, low priority.

### Dimension 5: ESM + Cell Bring-up Regression Surface

All seven named spawn-path regression guards (#1294, #1235, #1295, #1212,
#1213, #1214, #1284), the HEDR-0.96 classification bands, the PDCL
conscious-skip telemetry (#1568), the corrected Starfield 108-byte XCLL
field-by-field decode (#1291), and per-cell NAVM collection (#1272) all
verified intact with no regressions. Note: the original checklist's item 7
premise (`IsCollisionOnly` marker) is stale — that component was deliberately
deleted under closed #1570 in favor of a ghost-entity (no-`MeshHandle`)
pattern that achieves the same BLAS-exclusion goal more robustly; confirmed
the replacement mechanism works correctly.

#### DIM5-01: `walkers.rs` module-doc XCLL comment still says the Starfield gate is `== 108`, but the live gate (and its own regression test) use `>= 108` since #1579
- **Severity**: LOW
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:38-46,159-160` (doc) vs `:576-582` (live code)
- **Status**: NEW
- **Description**: #1579 hardened the Starfield XCLL dispatch gate from exact
  `== 108` to `>= 108` so a future-DLC cell with trailing pad bytes still takes
  the dedicated Starfield decode arm. The fix is correct and test-pinned, but
  the module-level doc comment (predating #1579) still states `== 108` in two
  places, directly contradicting the code three lines below it.
- **Impact**: No functional bug today, but a future engineer skimming the
  doc-first comment could "fix" the code back to exact equality, silently
  reintroducing the #1579 regression on any future-DLC or modded cell with
  trailing padding.
- **Suggested Fix**: Update both doc-comment occurrences from `== 108` to
  `>= 108`, matching the live predicate and the inline `#1579` comment already
  adjacent to it.

### Dimension 6: NIF Shader Blocks — BSVER 155+ (regression guard)

**Zero findings.** Every checklist item verified OK against `nif.xml` and real
Starfield data. CRC32 flag arrays gate correctly at `>=132`/`>=152`; a real
CRC32 hash→name table (33 constants) exists and spot-checks match `nif.xml`
exactly. The #1510 regression guard is intact — measured **0 `NiUnknown`**
across 202,586 real `BSLightingShaderProperty` and 179
`BSEffectShaderProperty` blocks, all 42 shader unit tests pass. The #1606
Starfield tail is captured to `block_size` with a `saturating_sub` over-read
guard (no fabricated field semantics). The sibling `BSEffectShaderProperty`
+32B tail asymmetry, previously scoped out, is confirmed **resolved** under
#1881 (tests present, 0-unknown confirmed) — not a live gap. All related
issues (#1510, #1606, #1881, #1721, #1552, #746/#747/#749, #403, #1223, #1901)
confirmed CLOSED with fixes verified in place.

### Dimension 7: Real-Data Validation

Parse rate holds exactly at the documented compat-matrix figure: **99.64%**
aggregate clean / **100%** recoverable across all 5 vanilla mesh archives
(88,951/89,276 clean), matching ROADMAP.md precisely. Five representative
meshes (clutter, weapon, character head, ship module, landscape) traced
through `import_nif_scene` with **zero `NiUnknown`** placeholders — the block
dispatcher is clean. All content loss found in this dimension was at the
import-time external-mesh-resolution layer, not NIF parsing.

#### SF-D7-NEW-01: MeshesPatch truncation tail mis-attributed to closed #746/#747; real cause is an unfixed `BSWeakReferenceNode` garbage-skip bug
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/blocks/node.rs:857-931` (`BsWeakReferenceNode::parse_inner`); doc citations at `ROADMAP.md:210`, `docs/engine/game-compatibility.md:38,196,396`
- **Status**: NEW
- **Description**: ROADMAP.md and `docs/engine/game-compatibility.md` cite
  closed #746/#747 for the residual 325/29,849 (1.09%) MeshesPatch truncation
  tail. Those issues actually fixed an unrelated `bsver == 155` vs `>= 155`
  gate bug in `shader.rs` (closed 2026-04-28). Re-tracing the real 325 files
  with debug logging shows every one fails inside `BSWeakReferenceNode` parsing
  — confirmed via the per-block histogram (`BSWeakReferenceNode
  parsed=7227 unknown=325`, the only type in the archive with any unknown
  count). This is distinct from closed #1882, which only fixed the simpler
  empty-weak-ref-list byte-drift case; the populated-list case (real
  terrain-overlay files) was never covered and reproduces an identical magic
  garbage skip value (`skip(2359296)`) across three unrelated files of very
  different sizes — a structural fixed-offset misread, not per-file
  corruption.
- **Impact**: 325 vanilla `meshes\terrain\*` NIFs lose their entire
  `BSWeakReferenceNode` payload to `NiUnknown` substitution. Currently benign
  at runtime (the payload feeds a not-yet-built LOD-streaming system either
  way), but the stale doc citation actively misdirects anyone trying to close
  out the real bug toward already-closed, unrelated shader code.
- **Suggested Fix**: Update the ROADMAP/compat-doc citations to point at a new
  issue instead of #746/#747; byte-diff a populated-list
  `BSWeakReferenceNode` against the `parse_inner` field sequence — the
  constant garbage value across unrelated files strongly suggests a
  fixed-size miscalculation early in the per-`BSWeakReference` record.

#### SF-D7-NEW-02: Starfield's two-digit mesh-archive series (`Meshes01`/`Meshes02`) is not covered by numeric-sibling auto-load — silent, unrecoverable geometry loss on single-LOD BSGeometry blocks
- **Severity**: HIGH
- **Location**: `byroredux/src/asset_provider/archive.rs:333-367` (`numeric_sibling_paths`); documented repro command at `docs/engine/game-compatibility.md:243-249`
- **Status**: NEW
- **Description**: `numeric_sibling_paths` recognizes an FNV-style unsuffixed
  series and a single-digit zero-based Skyrim-style series, but Starfield's
  actual naming is a two-digit zero-padded series (`Meshes01`, `Meshes02`).
  Because `"Meshes01"` ends in `'1'` (not `'0'`), it falls into the
  mid-series "don't expand" bucket — the same bucket as
  `"Skyrim - Textures2.bsa"` — so zero siblings auto-load. The project's own
  documented Starfield launch command passes only
  `--bsa "Starfield - Meshes01.ba2"`, with no explicit `Meshes02`. BSGeometry's
  external-mesh importer already tries every LOD slot and falls back
  gracefully when a mesh has multiple slots, but blocks with exactly **one**
  LOD slot (the common case for close-range detail parts — weapon internals,
  ship-module panels) have no fallback.
- **Evidence**: Measured 5/28 (17.9%) unrecoverable sub-meshes on a real
  weapon (`ar99.nif`) and 1/25 BSGeometry blocks fully lost on a ship
  cargo-bay module, using only `Starfield - Meshes01.ba2` as documented.
  Confirmed the missing geometry exists only in `Starfield - Meshes02.ba2`
  (direct archive search) and that no other code path auto-loads it. `--bsa`
  can be repeated, so the fix is tractable at either the CLI-doc or
  sibling-heuristic level.
- **Impact**: Any real cell load using the project's own documented Starfield
  launch command silently drops close-range detail geometry (measured
  15-18% in the two gameplay-relevant samples checked) with no warning
  surfaced at the engine's normal log level. Same bug class as already-fixed
  #1292 and #1661, but for a naming shape neither fix covers.
  `MeshesPatch.ba2`/`LODMeshes.ba2`/`FaceMeshes.ba2` are never loaded at all
  under the documented single-archive command, independent of the
  sibling-heuristic gap (those names can't be numeric-sibling-derived under
  any scheme).
- **Related**: Closed #1292 (same silent-drop symptom, different root cause),
  Closed #1661 (same function, fixed the single-digit Skyrim case this
  finding's two-digit Starfield case wasn't reached by).
- **Suggested Fix**: Extend `numeric_sibling_paths` with a Starfield-shaped
  two-digit zero-padded case (`Meshes01` → auto-load `Meshes02..`); since
  `MeshesPatch`/`LODMeshes`/`FaceMeshes` can't be derived by any numeric rule,
  also update the documented Starfield repro command to pass all 5 archives
  explicitly via repeated `--bsa` flags, and add a `numeric_sibling_paths`
  unit test for the `Meshes01` → `Meshes02` shape.

### Dimension 8: NIFAL Canonical Material Translation for Starfield

All 5 checklist items verified OK. Confirmed zero `if game == Starfield`
branching anywhere in the renderer/shaders (only comments) — the single
`translate_material` boundary holds. BSGeometry/BSTriShape import sets
`metalness_override`/`roughness_override` as `Some(classify_legacy_pbr(...))`;
`translate_material`→`resolve_pbr` emits plain resolved `f32` scalars once,
with no `Option<f32>` per-draw plumbing surviving. The `.mat` arm correctly
flips `is_pbr`→`MAT_FLAG_PBR_BSDF` at the boundary, not per-draw. `EmissiveSource`
tags `Lighting`/`Effect` correctly. The particle chain is game-agnostic by
block-type downcast with finite/positive guards. Collision translation
(`BhkMultiSphereShape`/`BhkConvexListShape`) is correct and unit-tested;
Starfield's actual `BhkNPCollisionObject` blobs correctly return `None` and
defer to the synthesized-trimesh fallback (a documented PHYSAL deferral, not a
NIFAL defect).

#### SF-D8-01: `.mat`-arm comment claims metalness/roughness overrides become `NaN` in `translate_material`; they are always `Some(classify_legacy_pbr)` from import
- **Severity**: LOW
- **Location**: `byroredux/src/asset_provider/material.rs:596-605`
- **Status**: NEW
- **Description**: The comment states overrides "stay `None` until Phase 2
  walks the CDB" and become `f32::NAN`, filled by `resolve_pbr`'s NaN-sentinel
  classifier. This is factually wrong for every real Starfield mesh: import
  (`bs_geometry.rs:310-311` and siblings) unconditionally sets
  `Some(legacy_pbr.metalness/roughness)` *before* the `.mat` arm runs, which
  returns early without touching those fields. The NaN-sentinel path never
  fires for Starfield content.
- **Impact**: Documentation only — the emitted `Material` is correct (and
  arguably better than the sentinel-backstop path the comment describes). Risk
  is to a future CDB Phase-2 implementer reasoning about the wrong mechanism.
- **Suggested Fix**: Correct the comment to state overrides are already
  `Some(classify_legacy_pbr(...))` from NIF import; Phase 2 must *overwrite*
  those `Some` values with CDB-authored ones rather than relying on an
  unreachable NaN-sentinel path.

### Dimension 9: BGSM/BGEM External Material Flow

Vanilla Starfield materials are almost entirely CDB + `.mat` (per Dimension
3); BGSM/BGEM appear only for mod-added or specific content. BGEM is
dispatched distinctly from BGSM via magic (not extension); `glass_enabled` is
confirmed the authoritative glass signal with a working opaque-misclassify
regression test. `BGSM_AUTHORED`/`PBR_BSDF`/`TRANSLUCENCY`/`MODEL_SPACE_NORMALS`
flags all derive from the correct `ImportedMesh` fields. Disney BSDF
attribution (GLSL-PathTracer MIT + Burley 2012) is present in
`triangle.frag`.

#### SF-D9-01: `EFFECT_PALETTE_COLOR`/`ALPHA` derived from LUT-texture presence, not the authored palette-enable flag
- **Severity**: MEDIUM
- **Location**: `byroredux/src/asset_provider/material.rs:790-793` (BGSM), `:1001-1008` (BGEM); `byroredux/src/cell_loader.rs:244-250` (packer)
- **Status**: NEW
- **Description**: The packer sets `EFFECT_PALETTE_COLOR`/`ALPHA` whenever
  `bgsm_greyscale_lut_path.is_some()` — populated purely on the greyscale
  texture *slot* being non-empty, with no reference to the authoritative
  `grayscale_to_palette_color` enable flag (parsed, zero consumers elsewhere).
  This is asymmetric with the inline NIF effect-shader path
  (`pack_effect_shader_flags`), which correctly gates the same flag on the
  real SLSF enable bit. A BGSM/BGEM that fills the greyscale slot (a legal,
  serialized-always slot) but leaves the remap flag off is given a palette LUT
  remap it should not receive.
- **Impact**: Wrong diffuse colors (unwanted palette-LUT remap) on any
  BGSM/BGEM material with an authored-but-disabled greyscale slot — likely on
  inherited-from-template slots and mis-authored mod content. Blast radius
  narrow for vanilla (creature/NPC variants that use the LUT do enable the
  flag) but silent and cross-game (FO4/FO76/Starfield-BGEM).
- **Suggested Fix**: Forward the parsed `grayscale_to_palette_color` (BGEM:
  `|| grayscale_to_palette_alpha`) bool onto a new `ImportedMesh` enable field
  and gate the flag pack on it, mirroring the inline-path gate.

#### SF-D9-02: BGEM v21/v22 glass-overlay params + envmap-mask-scale + v11 emittance dropped in merge
- **Severity**: LOW
- **Location**: `byroredux/src/asset_provider/material.rs:973-1102`; fields at `crates/bgsm/src/bgem.rs:31-77`
- **Status**: NEW
- **Description**: The BGEM merge arm forwards `glass_enabled` but drops
  `glass_fresnel_color`, `glass_refraction_scale_base`, `glass_blur_scale_base`,
  `glass_blur_scale_factor`, `glass_roughness_scratch`, `glass_dirt_overlay`
  (all FO76/Starfield-era), plus `environment_mapping_mask_scale` and
  `emittance_color` (the latter already explicitly deferred in-code). No
  `ImportedMesh` sink exists for any of these.
- **Impact**: Mod-added Starfield/FO76 BGEM glass renders with engine-default
  refraction/tint instead of authored values. Low severity: the renderer
  currently has no binding to consume these fields even if forwarded, so this
  is a deferred-consumer gap, not an active miswrite.
- **Suggested Fix**: Track as a deferred renderer-binding follow-up
  (glass refraction/fresnel/blur), paired with the already-noted
  `emittance_color` second-emissive-slot deferral. No parser change needed.

---

## CRC32 Flag Table (BSVER ≥ 132/152 shader flag arrays)

A real CRC32 hash→flag-name table exists at
`crates/nif/src/shader_flags.rs:235-310` (33 named constants, pinned by
`bs_shader_crc32_matches_nif_xml_literals`). Spot-checked entries, all
confirmed matching `nif.xml`'s `enum BSShaderCRC32` (nif.xml:6520):

| Flag Name | CRC32 |
|---|---:|
| `DECAL` | 3849131744 |
| `TWO_SIDED` | 759557230 |
| `CAST_SHADOWS` | 1563274220 |
| `PBR` | 731263983 |
| `NO_EXPOSURE` | 3707406987 |
| `VERTEX_COLORS` | 348504749 |

The remaining 27 constants were not individually re-derived in this audit pass
(the table's existence and the sample above were sufficient to confirm the
hashes are resolvable, not opaque); see `shader_flags.rs` for the full set.

---

## Remaining-Work Chain

Per `starfield-esm-roadmap.md` (Phases 0+1 done; Phases 2-4 invalidated by the
measured 99.9% ESM record parity), the ordered remaining work is:

1. **Per-field CDB extraction** (#1289 Phase 2 follow-up) — `.mat`-resolved
   materials currently reach the Disney lobe with NIF-keyword-classified
   defaults (`classify_legacy_pbr`), not CDB-authored roughness/metalness/
   texture values. The Phase-1 fallback mechanism itself is implemented
   correctly (Dimension 8); only the CDB→field wiring is unbuilt. Note the
   Phase-1 CDB loader currently pays a multi-second parse + retains the full
   1.44M-instance tree just to answer a presence boolean (SF-D3-AUDIT-01) —
   worth revisiting alongside Phase 2, since Phase 2 will need the tree
   anyway.
2. **Exterior worldspace tiles** — not yet built.
3. **Space-cell / planet / GBFM records** — `GBFM` remains a zero-dispatch
   stub (3,141 leaf records, confirmed still below the detection threshold for
   Cydonia's interior resolve rate); `PNDT`/`STDT`/`BIOM` planet/star/biome
   procgen records are correctly out of scope for interior-cell work.
4. **NIF truncation tail** — the residual MeshesPatch tail is NOT the
   previously-cited #746/#747 (both closed, unrelated shader-gate fixes); it
   is a genuinely separate, still-open `BSWeakReferenceNode` populated-list
   parse bug (SF-D7-NEW-01), newly identified in this audit. Recommend opening
   a fresh issue and correcting the ROADMAP/compat-doc citations.

Both BGSM/BGEM parsing and the ESM pipeline are fully shipped — this is a
depth/correctness list, not a "BGSM first / ESM far behind" framing.

---

## Deduplication Note

Cross-referenced all findings against `gh issue list --repo matiaszanolli/ByroRedux`
(200-issue snapshot) and `docs/audits/` prior reports. One finding
(SF2D2-03) restates the still-open #1827; all others are genuinely new. No
finding duplicates an already-open issue under a different name.
