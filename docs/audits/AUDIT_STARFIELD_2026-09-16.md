# Starfield Compatibility Audit — 2026-09-16

**Scope**: A depth and correctness regression audit of the Starfield `GameKind`
bring-up surface. It covers all nine dimensions of `/audit-starfield`, run as
part of the `texture-roles-deep` audit-suite preset. The preset follows the
2026-07-27 cross-game texture-role unification (`MaterialTextureSet`,
`ImportedMaterial`, `merge_external_material`, `translate_material`), so the
CDB `.mat` role mapping got extra scrutiny.

**Method**: One agent read live source at HEAD (`7996edf61`) with no nested
sub-agents. Findings were deduplicated against `gh issue list` (the 200 most
recent issues, all 94 open issues, and a 572-issue Starfield keyword search)
and against the prior `AUDIT_STARFIELD_*` reports. The engine binary was not
launched. Live measurements (all read-only against the installed game):

- **Parse-rate gate**: `parse_rate_starfield_all_meshes -- --ignored` on all 13
  mesh archives.
- **BA2 texture census**: every file in the 14 `Starfield - Textures*.ba2`
  archives (45,756 files).
- **Shader-property census**: a scratchpad tool (outside the repo) that uses
  `byroredux-nif` to classify every Starfield `BSLightingShaderProperty` /
  `BSEffectShaderProperty` name and inline texture string across 16 archives
  (120,543 NIFs).
- **TXST census**: a Python pass over the TXST sub-records of all 13 Starfield
  masters.
- **ESM coverage**: `crates/plugin/examples/sf_smoke.rs` on `Starfield.esm`
  (a GRUP walk, 1.4 GB RSS).
- **Unit tests**: the targeted suites (`byroredux-bsa` ba2, `byroredux-sfmaterial`,
  `byroredux-bgsm`, `byroredux-nif` bs_geometry, and the `byroredux` bin
  `starfield_mat` / `bgsm_merge` / `material_translate` filters). All green.

**Result**: **0 CRITICAL, 0 HIGH, 1 MEDIUM, 8 LOW**. All 9 findings are NEW.
There are no regressions in any bring-up milestone:

- BA2 v3 method dispatch and the `packed_size == 0` selector are intact.
- #1292, #1209, #1828/#1829, #3777 and #4394 (the BSGeometry bound unit fix) are intact.
- XCLL `[28, 108]`, the PDCL named skip and the spawn-path guards are intact.
- The #1510 / #1606 / #1881 shader-block guards are intact.
- ESM GRUP coverage is 86.2%, against a Phase 0/1 baseline of 86.1%.

The NIF parse rate *improved*: it is now **100.00%** clean (see D7-01).

---

## Executive Summary

### Premise correction for this preset — the CDB `.mat` path produces **zero** texture roles

The preset brief calls the CDB `.mat` path "the second-densest texture-role
producer". That is not true at HEAD. Measured over the full vanilla corpus
(120,543 NIFs):

| Starfield material source | Occurrences | Texture roles it fills today |
|---|---:|---|
| `BSLightingShaderProperty` stub, `.mat` name | 478,691 | **0** — `.mat` arm → `apply_cdb_pbr_fallback` → `PresenceOnly` |
| `BSLightingShaderProperty` stub, `.bgsm` name | 1,679 | **0** — no `.bgsm` file exists in any Starfield archive → resolver miss → CDB PBR flip |
| `BSEffectShaderProperty` stub, `.bgem` name | 104 | **0** — same as above |
| stub, suffix-less name (`Materials\` / `\Materials`) | 387 | **0** — `material_path` is `None`, so `Unresolved` (see D6-01) |
| full-body `BSLightingShaderProperty` (empty name) | 3,080 | **0** — `texture_set_ref` is NULL on all 3,080 |
| full-body `BSEffectShaderProperty` (empty name) | 879 | **2** — `source_texture = textures\Effects\ColorWhiteAlphaUtility_d.dds` on 2 blocks. All 8 texture strings are empty on the other 877, and `reflectance` / `lighting` / `emit_gradient` are empty on all 879. |

Starfield's live texture-role output is therefore **2 role fills across the
entire vanilla corpus**. CDB Phase 2 (#3398) is still a step function, as
#3398 itself says. The texture-role question for Starfield is not "is the CDB
mapping correct". It is **"does the canonical role vocabulary have somewhere to
put what the CDB will carry?"** The answer is no (SF-2026-09-16-D3-01).

### Starfield texture vocabulary vs. the canonical role set

Measured from the texture archives (file-name suffix; 45,756 files in
`Starfield - Textures01..11/Patch01..02` and `GeneratedTextures`):

| Suffix | Files | Canonical role today |
|---|---:|---|
| `_color` | ~12,580 | `base_color` |
| `_normal` | 8,590 | `normal` |
| `_rough` | 7,548 | **none** |
| `_ao` | 4,990 | **none** |
| `_opacity` | 2,790 | **none** |
| `_metal` | 2,357 | **none** |
| `_mask` | 2,229 | none (semantics unmeasured) |
| `_emissive` | 434 | `emissive` |
| `_transmissive` | 313 | **none** |
| `_height` | 295 | `height` |
| `_flow` | 35 | `flow` |

`Starfield.esm` TXST confirms these kinds occupy their own slots: TX00
`_color` ×19, TX01 `_normal` ×19, TX08 `_metal` ×11, TX09 `_rough` ×11, TX17
`_ao` ×1, TX19 `_opacity` ×16.

### What moved since 2026-09-11

- **Fixed and verified**:
  - #4270–#4276: skin-name short-circuit, Stage A logging, duplicate-class
    reject, tolerant `probe_header`, CDB test limits, `Field::offset` guard,
    and a citation fix.
  - #4278: `DISPATCH_HANDLED_FOURCCS` is now derived next to the dispatch
    match and pinned by a test.
  - #4284: partially fixed; see D8-01.
  - #4286–#4289: BGSM greyscale OR, overlay BGEM parity and its fixture, and
    `MergeOutcome` trace.
  - #4394: the BSGeometry bounding sphere is now scaled by `HAVOK_SCALE`. The
    sibling check found no other verbatim-unit consumer; skin-bone spheres
    have no consumer.
- **Still open, re-confirmed against code**: #3659, #4268, #4269, #4277,
  #4279, #4282, #4283, #4285, #3398, #1576.
- **Split since the last audit**: `eaa94b49d` split
  `crates/nif/src/blocks/shader.rs` into `crates/nif/src/blocks/shader/` and
  split the ESM records barrel. The #1606 / #1881 tail capture and its
  `opaque_tail_len` scan moved with the code and are still non-vacuous. The
  audit skill's own path references did not move (META-01).

---

## Dimension Findings

### Dimension 1 — BA2 v2/v3 LZ4 Block Decompression
**Verdict**: Clean. No new finding.

- **v3 header**: the `compression_method` read is at the correct offset. Any
  method other than 0/3 is a hard `InvalidData` error
  (`crates/bsa/src/ba2.rs:265-277`). Unknown versions are also hard errors.
- **Decompression**: GNRL (`extract_general`) and DX10 (`extract_dx10`) both
  select raw vs. codec per chunk on `packed_size == 0` and share
  `decompress_chunk`.
- **LZ4**: the LZ4 arm passes `unpacked_size` as the output bound (a hard
  bound on the pinned `safe-decode` build). Under-runs warn.
- **Size limits**: sizes are capped at record read (`checked_chunk_size`,
  lines 519-520 and 641-642).
- **Real data**: the 45,756-file texture listing across 14 archives opened
  with zero errors. The parse-rate gate extracted all 120,543 NIFs cleanly.

| Severity | ID | Title | Status |
|---|---|---|---|
| MEDIUM | — | `Ba2Archive::extract` holds its `Mutex<File>` across inflate | **Existing: #3659** (OPEN, re-confirmed) |

### Dimension 2 — BSGeometry Mesh Extraction
**Verdict**: Clean apart from open issues.

- **Bounding sphere**: #4394 is correct and pinned by
  `authored_bounding_sphere_is_converted_to_decoded_position_units`. The
  #2098 cross-check now reports only genuine divergence.
- **Earlier fixes**: #1292, #1209, #1828/#1829, #3777 and #1232 are unchanged
  since the 09-11 verification. There are no commits to
  `crates/nif/src/import/mesh/bs_geometry.rs` other than #4394.
- **Tests**: the `byroredux-nif` `bs_geometry` filter has 68 tests, all
  passing.

| Severity | ID | Title | Status |
|---|---|---|---|
| MEDIUM | — | `convert_bs_geometry_skin_weights` passes `.mesh` bone indices through unbounded | **Existing: #4268** |
| LOW | — | #3777 trailer gate is undecidable on the inline (Stage A) path | **Existing: #4269** |

### Dimension 3 — CDB Material Database Correctness (texture-role focus)
**Verdict**: 1 MEDIUM and 2 LOW, all NEW. The CDB reader fixes #4272, #4273
and #4275 are present:

- #4272: duplicate class names now error.
- #4273: `probe_header` → `count_chunks_tolerant`.
- #4275: `fields_are_offset_ordered` warns on XMCOLOR-class divergence. It
  only runs inside the full `parse`, never in production.

XMCOLOR is still decoded in declaration order; it is tracked in #3398. The
CDB discovery scan (`discover_starfield_cdbs`) still scans every
`materials\...materialsbeta.cdb` in both `--materials-ba2` and `--bsa`
numeric-sibling archives (#1571 / #2621).

#### SF-2026-09-16-D3-01: The canonical texture-role vocabulary has no role for Starfield's roughness / metalness / AO / opacity / transmissive maps — CDB Phase 2 has nowhere correct to put ~39% of Starfield's textures
- **Severity**: MEDIUM
- **Dimension**: 3 (CDB material correctness) / 9 (`.mat` → `MaterialTextureSet` roles)
- **Location**:
  - `crates/nif/src/import/types.rs:335-369` (`MaterialTextureSet`, 22 roles + 4 decals)
  - `crates/renderer/src/vulkan/material.rs:569-585` (`supplemental_texture_slot`, 16 lanes)
  - `crates/renderer/shaders/triangle.frag:1437-1452` (gloss-map sampling)
  - `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md:160-163, 226-229`
- **Status**: NEW. #3398 item 2 asks for "a field-name → `MaterialTextureSet`
  role … mapping" and the spike tabulates `MRTextureFile`/`TextureFile` →
  "texture roles". Neither records that the role vocabulary itself is missing
  the destinations. The `nifal.md` parked-roles table does not list them either.
- **Description**: Starfield authors separate single-channel PBR maps. Its
  texture archives and its own TXST records show `_rough`, `_metal`, `_ao`,
  `_opacity` and `_transmissive` as first-class kinds, each in its own TXST
  slot (TX09 / TX08 / TX17 / TX19). `MaterialTextureSet<T>` has 22 roles and
  none of them is roughness, metalness, ambient occlusion, opacity or
  transmission. `GpuMaterial` has no lane for them, and `triangle.frag`
  samples none of them.

  The only nearby role is `smooth_spec` (legacy gloss / BGSM smooth-spec). The
  shader consumes it as **gloss**: `roughness = mix(1.0, roughness,
  glossTexel.r)`. That is the inverse of a roughness map, so routing `_rough`
  there is a silent sign flip. `specular` is a colour map, not metalness.

  So when Phase 2 lands per-texture extraction, each non-colour/normal texture
  has three options, and all three break a project rule:
  1. drop it silently (NIFAL "no silent drop");
  2. misroute it into a role with different semantics ("no fabrication", and
     the inverted gloss);
  3. store it under a CDB-specific index, which the checklist forbids ("never
     a CDB slot index").

  The spike's §4 step 5 ("The merge arm … mirrors the BGSM arm directly; ~80
  lines") therefore understates the work. The canonical roles, the
  `GpuMaterial` lanes (a lockstep `bindings.glsl` change against the 428 B
  pin) and the shader consumers are a prerequisite, not a follow-up.
- **Evidence**:
  - **Texture-archive suffix census** (45,756 files): `_rough` 7,548, `_ao`
    4,990, `_opacity` 2,790, `_metal` 2,357, `_transmissive` 313. That is
    17,998 files (39%) with no canonical role.
  - **`Starfield.esm` TXST census** (23 records): TX08 → `_metal` (11), TX09 →
    `_rough` (11), TX17 → `_ao` (1), TX19 → `_opacity` (16).
  - **`roles()`** (`types.rs:384-413`) enumerates every role. None of these
    kinds appears.
  - **Merge-arm comment**: `merge.rs:544-545` already records the metalness
    half for FO4 ("Per-texel metalness from the spec map … deferred — needs a
    metalness-map shader binding"). It was scoped as an FO4 refinement, not as
    a Starfield Phase-2 blocker.
- **Impact**:
  - **Today**: none. Zero Starfield texture roles are produced (see the
    Executive Summary).
  - **At Phase 2**: the fidelity step #3398 exists to deliver would land at
    most `_color` / `_normal` / `_emissive` / `_height`. Every Starfield
    surface would keep scalar-only roughness and metalness and would have no
    AO and no opacity mask. Opacity-driven decals and cutouts, such as the 16
    `_opacity` TXST decals (`decalpuddlemd01_opacity.dds`), have no coverage
    source.
- **Related**:
  - #3398 (Phase 2 tracker).
  - #4277 (loose `.mat` JSON resolver, which will need the same roles).
  - SF-2026-09-16-D5-01 (the TXST decode drops the same four slots).
  - `mat_path_forwards_no_texture_roles_until_cdb_phase_2_lands` pins
    zero-forwarding only. Phase 2 must rewrite it, so the "never a CDB slot
    index" half of the invariant has no guard that survives Phase 2.
- **Suggested Fix**: Before the Phase-2 merge arm, add canonical roles
  (roughness, metalness, ambient_occlusion, opacity, transmissive) with
  documented channel semantics, their `GpuMaterial` lanes and shader
  consumers. Or record them explicitly as parked in `docs/engine/nifal.md`
  with #3398 as the unblocking consumer. In the same change, add a guard that
  a CDB-sourced role lands by name.

#### SF-2026-09-16-D3-02: The Phase-2 spike (the source of truth for #3398) points at three examples deleted four days after it was written; the CDB key derivation has no in-tree reproduction
- **Severity**: LOW
- **Dimension**: 3
- **Location**: `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md:18-25, 222-231, 234-242`
- **Status**: NEW
- **Description**: The spike's "Reproduce with" block and its Artifacts list
  name the following. All three were removed by `a823c13a1` (#3150,
  2026-09-02, "delete stale _tmp_ scratch examples"). The spike is dated
  2026-08-29.
  - `crates/sfmaterial/examples/_tmp_cdb_phase2_spike.rs`
  - `crates/sfmaterial/examples/_tmp_cdb_hash_probe.rs`
  - `crates/nif/examples/_tmp_sf_matpath_dump.rs`

  The hash probe is the only tool that established the 3,032 / 3,084 (98.3%)
  key match and the rotated `BSResource::ID` columns. Phase 2's lookup key
  rests on that result.

  The same section also cites stale locations:
  - §4 step 4: `byroredux/src/asset_provider/material.rs`, which is now a
    directory (#3857).
  - §4 step 6: `starfield_mat.rs:177-188`. The pinned test is now at lines
    168-209.

  (The XMCOLOR check the spike also housed now has a committed guard, #4275.)
- **Evidence**: `git show a823c13a1 --stat` lists all three deletions, and
  `crates/sfmaterial/examples/` no longer exists.
- **Impact**: A Phase-2 implementer who follows the tracker's source doc hits
  dead commands. The key derivation can only be re-verified by digging up
  `a823c13a1^`.
- **Suggested Fix**: Restore the hash probe as a named, documented diagnostic
  (the convention `sf_smoke.rs` follows). Or at minimum, rewrite the
  Reproduce and Artifacts sections to `git show a823c13a1^:<path>` and fix the
  two stale locations.

#### SF-2026-09-16-D3-03: `MaterialProvider::sf_cdb_count`'s Phase-2 plan prescribes the re-parse the spike ruled out, and the `.mat` fallthrough cites a closed tracker
- **Severity**: LOW
- **Dimension**: 3
- **Location**:
  - `byroredux/src/asset_provider/material/provider.rs:160-167`
  - `byroredux/src/asset_provider/material/merge.rs:326-329`
- **Status**: NEW
- **Description**:
  - **Phase-2 plan**: the field doc, which is what a Phase-2 implementer reads
    first, says: "Phase 2 (future, SF-D3-01 #1289): re-`parse` each CDB on
    demand and walk the instance trees … so per-material metalness /
    roughness / texture paths flow into `ImportedMesh`". The spike §3 measured
    `parse` at 9.19 GB per full-size CDB (~18 GB across the 13 discovered)
    and concluded "Calling `parse` on the cell-load path is not viable; an
    indexed reader is the project".
  - **Wrong target and tracker**: the same doc names the wrong sink
    (`ImportedMesh`, where the boundary takes `&mut ImportedMaterial`) and a
    closed tracker (#1289, where #3398 owns the work).
  - **Stale `.mat` pointer**: `merge.rs:326-329` says ".mat format is not yet
    parsed (tracked in SF-D6-03)". SF-D6-03 is the closed #762. The open
    tracker for the loose `.mat` resolver gap is #4277.
- **Evidence**: The quoted doc text above, against spike §3 lines 189-203.
- **Impact**: The doc steers the next implementer toward the approach already
  measured as non-viable.
- **Suggested Fix**: Rewrite the Phase-2 paragraph to "indexed/streaming
  reader (#3398)" and target `ImportedMaterial`. Repoint `merge.rs` at #4277.

### Dimension 4 — Starfield ESM Resolve-Rate Baseline
**Verdict**: 1 LOW (NEW).

- **Resolve rate**: `--sf-smoke` (the per-cell REFR resolve rate) was **not**
  re-run, because it is an engine-binary mode and this preset forbids
  launching the engine. The last live figure is 25,433 / 27,898 = 91.2% (09-11).
- **GRUP byte coverage**: `crates/plugin/examples/sf_smoke.rs` on vanilla
  `Starfield.esm` reports **86.2% handled** (80 GRUPs / 1,255,402,403 B),
  13.8% skipped, 0 byte-level errors, and 0 orphans. The Phase 1 baseline is
  86.1%, so there is no regression.
- **PDCL**: reported as skip (706 records), which is correct per the
  `DISPATCH_HANDLED_FOURCCS` doc.
- **GBFM**: skip (3,141 records), the known stub gap. It is not re-reported.

#### SF-2026-09-16-D4-01: Starfield ESM coverage docs point at the pre-split dispatch site, and the #4278 drift history lists LCTN as a live arm when it has none
- **Severity**: LOW
- **Dimension**: 4
- **Location**:
  - `crates/plugin/examples/sf_smoke.rs:10-12`
  - `crates/plugin/src/esm/records/parse.rs:33-35`
  - `docs/engine/starfield-esm-phase0-baseline.md:9, 23, 165, 175, 232`
  - `docs/engine/starfield-esm-roadmap.md:73, 239`
- **Status**: NEW
- **Description**:
  - **Stale dispatch site**: `sf_smoke`'s module doc still locates the
    catch-all at "`records/mod.rs:925`". `eaa94b49d` moved the dispatch to
    `records/parse.rs`, and `records/mod.rs` is now a 145-line barrel. The
    Phase 0 baseline doc still calls `DISPATCH_HANDLED_FOURCCS` "the
    hand-maintained slice in `sf_smoke.rs`", which #4278 made false.
  - **LCTN is not a live arm**: `DISPATCH_HANDLED_FOURCCS`'s own doc (and
    #4278's body) says the hand list "drifted three times (LCTN, then …),
    each time reporting live dispatch arms as 'skip'". LCTN has **no**
    top-level dispatch arm. `rg 'b"LCTN"' crates/plugin/src/esm/records`
    finds nothing, `sf_smoke` correctly reports `LCTN … 6017 … skip` today,
    and `starfield-esm-phase0-baseline.md:223` agrees: "Locations (LCTN)
    silently skipped at the top level".
- **Impact**: The doc drift misleads the coverage tooling's readers. The LCTN
  line invites someone to "fix" the list by re-adding LCTN, which would make
  the tool over-report coverage (the regression class #4278 closed, inverted).
- **Suggested Fix**: Repoint the three doc sites at `records/parse.rs` and
  replace LCTN with SECH/AOPF + OMOD/LVSP/SCEN in the drift history.

### Dimension 5 — ESM + Cell Bring-up Regression Surface
**Verdict**: 1 LOW (NEW). The following all verified intact:

- HEDR 0.96 → `GameKind::Starfield` (`sf_smoke`: `game_kind: Starfield`).
- `XCLL_SIZES_STARFIELD = [28, 108]` (`walkers.rs:71, 113`).
- The PDCL named skip into `skipped_unconsumed_groups` (`parse.rs`, `b"PDCL"` arm).
- #1294 `base_layer` gating (`spawn.rs:79-90`).
- Structural BLAS exclusion: `spawn_trimesh_collider_ghost` /
  `spawn_packed_havok_proxy` spawn without a `MeshHandle`.

#### SF-2026-09-16-D5-01: `parse_txst_group` silently drops Starfield TXST TX08 / TX09 / TX17 / TX19 (metal / rough / AO / opacity)
- **Severity**: LOW
- **Dimension**: 5 (this game's data through the shared parser)
- **Location**: `crates/plugin/src/esm/cell/support.rs:572-604`
- **Status**: NEW
- **Description**: The TXST decode matches `TX00`–`TX07` (plus `MNAM`,
  `DNAM`, `DODT`). Starfield TXST records author PBR maps in higher slots
  that fall through the match with no warning and no census entry:
  - TX08 `_metal`
  - TX09 `_rough`
  - TX17 `_ao`
  - TX19 `_opacity`

  The adjacent TX02 comment ("Starfield's xEdit definition has not seen TX02
  in shipped content") is consistent with the census (0 TX02). The
  higher-slot lanes simply are not modelled.
- **Evidence**: The `Starfield.esm` TXST census found 23 records, with sub-record counts
  TX00 19, TX01 19, **TX08 11, TX09 11, TX17 1, TX19 16**, DODT 20, MNAM 2.
  That is 39 sub-records dropped. Sample:
  `TX19 = decals\puddles\decalpuddlemd01_opacity.dds`. The 12 other
  Starfield masters (ShatteredSpace, SFBGS*, Constellation, OldMars,
  BlueprintShips) carry 0 TXST records.
- **Impact**: Small today. `TextureSet` consumers are LTEX terrain and the
  REFR texture overlays (`byroredux/src/cell_loader/refr.rs:401-417`), and
  Starfield exterior terrain is not live. 20 of the 23 records are decals
  (DODT), so the dropped `_opacity` is exactly the coverage map those decals
  depend on. There is also no canonical sink (D3-01), so this is the ESM-side
  twin of that gap.
- **Related**: SF-2026-09-16-D3-01.
- **Suggested Fix**: Capture the four slots once D3-01's roles exist. Until
  then, warn once per unmodelled `TX*` FourCC, as the parser already does for
  era-gated groups, so the drop is visible.

### Dimension 6 — NIF Shader Blocks, BSVER 155+
**Verdict**: 1 LOW (NEW). The regression guards are intact after the
`shader/` split:

- `parse_skyrim_shader_base` keeps the gap-band / `FO76_SF2_CRCS` gates
  (`shader/mod.rs:93-147`).
- `read_starfield_tail` is shared by both families (`mod.rs:155-173`) and is
  called from `lighting.rs:303` and `effect.rs`.
- `every_tail_capturing_block_reports_it_and_parse_nif_records_it` scans all
  five family files.
- The parse gate reports 0 truncated and 0 recovered blocks corpus-wide (so
  the #1510 NiUnknown count is 0).

#4279 (Own_Emit is typed-word-only) is still open.

#### SF-2026-09-16-D6-01: The #1510 "content-hash path" premise has no vanilla evidence — every suffix-less Starfield material reference is the degenerate string `Materials\`
- **Severity**: LOW
- **Dimension**: 6
- **Location**:
  - `crates/nif/src/blocks/shader/lighting.rs:579-588`
  - `crates/nif/src/blocks/shader/effect.rs:160-168`
  - `crates/nif/src/blocks/shader_tests/starfield.rs:274-313`
  - `crates/nif/src/import/mesh/material_path.rs:10-17`
- **Status**: NEW
- **Description**: The stub discriminator (`!name.is_empty()` for
  `bsver >= STARFIELD`) is justified in three places by the claim that
  "Starfield material references are content-hash paths with NO suffix
  (`<hash>\<hash>`)". The tests use the synthetic name
  `"8f3a91c4\\b27e5d06"`. The census found no such name.

  The gate itself is still correct: a non-empty name on vanilla content always
  means a stub. The premise is false, though, and it hides a downstream fact.
  The suffix-less stubs that do exist get `material_path = None`, because
  `material_path_from_name` keeps the suffix gate. `merge_external_material`
  then returns `Unresolved` for them, so they are the only Starfield stubs
  that never reach the CDB PBR route.
- **Evidence**: The census covered 120,543 NIFs and 480,861 non-empty-name
  shader stubs:

  | Name kind | Count |
  |---|---:|
  | `.mat` | 478,691 |
  | `.bgsm` | 1,679 |
  | `.bgem` | 104 |
  | suffix-less | **387** |

  Every suffix-less name is `"Materials\\"` (384) or `"\\Materials"` (3):
  editor markers, conveyor belts, pedestals and similar
  (`meshes\markers\editormarkers\editormarkerrocksmall.nif`, …). There are
  zero hex-hash paths.
- **Impact**: The comments are wrong, which misdirects Phase 2: a CDB lookup
  cannot key a hash-path name that does not exist. 387 shapes (0.08%) render
  on the legacy path with NIF-stub defaults, and no decision has been recorded
  on whether that is intended.
- **Suggested Fix**: Correct the three comments and the test fixture name to
  the measured `Materials\` form. Decide explicitly, with a test, whether
  directory-only references should take the CDB PBR fallback or stay
  `Unresolved`.

**CRC32 note**: Nothing changed since `AUDIT_STARFIELD_2026-09-11.md`. See the
table below.

### Dimension 7 — Real-Data Validation
**Verdict**: 1 LOW (NEW, good direction). This audit ran the gate:
`parse_rate_starfield_all_meshes -- --ignored`.

| Archive | Clean |
|---|---|
| Meshes01 | 31,058 / 31,058 |
| Meshes02 | 7,552 / 7,552 |
| **MeshesPatch** | **29,849 / 29,849** |
| LODMeshes | 19,535 / 19,535 |
| FaceMeshes | 1,282 / 1,282 |
| LODMeshesPatch | 19,540 / 19,540 |
| **ShatteredSpace-Main01** | **9,198 / 9,198** |
| SFBGS003 / 004 / 008 / 00D / 047 / 050 | 85 / 6 / 5 / 934 / 4 / 1,495 |

That is **120,543 / 120,543 = 100.00% clean**, with 0 truncated and 0 failed.
The independent census tool agrees: 0 `truncated`, 0 `recovered_blocks`, 0
NiUnknown. The texture archives list cleanly. No new block types were found.

#### SF-2026-09-16-D7-01: Starfield NIF parse is 100.00% clean, but ROADMAP, the compatibility docs, this skill and the 09-11 audit still report 99.98% with a 19-file residual tail
- **Severity**: LOW
- **Dimension**: 7
- **Location**:
  - `ROADMAP.md:519, 686, 1528`
  - `docs/engine/game-compatibility.md:19, 44, 201`
  - `docs/engine/nif-parser.md:676`, which also gives a third total, "120 836",
    and says "residual truncation tail tracked at #2105/#3524"
  - `.claude/commands/audit-starfield/SKILL.md` Dimension 7
  - `docs/audits/AUDIT_STARFIELD_2026-09-11.md:167-176`
- **Status**: NEW
- **Description**: The 6 MeshesPatch and 13 ShatteredSpace-Main01 truncations
  no longer reproduce. #3524 (the characterised `BSWeakReferenceNode`
  residual) closed on 2026-09-08. The 09-11 audit wrote "confirmed unchanged"
  but live-measured only the two LODMeshes archives, which were already at
  100%.
- **Evidence**: The gate output quoted above.
- **Impact**: Stale status only. The skill still tells auditors to confirm a
  19-file tail "has not grown", which is now unfalsifiable busywork.
- **Suggested Fix**: `/session-close` should refresh the matrix rows to
  100.00% (120,543 / 120,543) and retire the residual-tail language. Update
  the skill's Dimension 7 to "confirm it stays at 0".

### Dimension 8 — NIFAL Canonical Material Translation for Starfield
**Verdict**: 1 LOW (NEW). Invariants hold:

- `translate_material` is the single boundary.
- `metalness` / `roughness` seed as `f32` from the overrides or NaN, and
  `resolve_pbr` runs once (`material_translate.rs:691-760`).
- The Starfield stub path correctly reaches the classifier backstop via
  `has_no_pbr_classifier_signal` (`import/material/mod.rs:1459-1465, 1542-1543`).

#4282, #4283 and #4285 are still open.

#### SF-2026-09-16-D8-01: #4284 corrected one of three "classifier arm is only a future backstop" statements; the boundary's own doc and the corrected block still misstate the live NaN producers
- **Severity**: LOW
- **Dimension**: 8
- **Location**:
  - `byroredux/src/material_translate.rs:528-537`
  - `crates/core/src/ecs/components/material.rs:1307, 1334-1337`
- **Status**: NEW (a missed sibling of CLOSED #4284)
- **Description**: The inaccurate statements:
  - **Boundary doc**: `translate_material`'s contract doc still says
    `resolve_pbr`'s "classifier arm is a sentinel-backstop (only fires when
    the override is `NaN`, i.e. for future non-NIF paths). BGSM/BGEM content
    also arrives pre-classified as `Some`." This is exactly the text #4284
    fixed in `Material::resolve_pbr`.
  - **The block #4284 corrected**: `material.rs:1307` still says "For
    **BGSM/BGEM** content the authored scalars also arrive as `Some`."
  - **Inline comment**: `material.rs:1334-1337` says the backstop "is
    unreachable for every pre-classified current producer (both NIF import …
    and BGSM/BGEM leave metalness/roughness non-NaN)".

  All three are false for BGEM. `merge_bgem_arm` deliberately leaves both
  overrides `None` (`merge.rs:993-995`: "metalness and roughness are left as
  NaN sentinels so resolve_pbr runs the keyword classifier"), and the stub it
  merges into has no classifier signal. The claims are also false for every
  Starfield stub (#2707).
- **Evidence**: The quoted text above.
- **Impact**: The boundary's own contract still describes the live arm (the
  majority Starfield path, and every FO4 / Starfield BGEM) as future-proofing.
  That is the same deletion hazard #4284 was filed for.
- **Suggested Fix**: Apply #4284's correction to `translate_material`'s doc
  and drop "BGEM" from the two `material.rs` sentences. Consider a test that
  asserts a BGEM-merged stub reaches `translate_material` with NaN scalars.

### Dimension 9 — BGSM/BGEM External Material Flow
**Verdict**: No new finding beyond D3-01, which is shared.

- **Merge signature**: `merge_external_material` still takes
  `&mut ImportedMaterial`. The single-export invariant is pinned by
  `merge_external_material_is_the_only_exported_fn_in_this_file`.
- **BGEM vs. BGSM**: they stay distinct, with different texture conventions,
  `glass_enabled` / `bgem_uses_glass_behavior`, and the `env_mapping_enabled`
  gate.
- **Provenance**: `record_external_texture_sources` walks every role via
  `zip_map_ref`.
- **Flag packing**: `pack_imported_material_flags` derives `BGSM_AUTHORED`
  from `from_bgsm`, `PBR_BSDF` from `is_pbr`, `TRANSLUCENCY` from
  `has_translucency` and `MODEL_SPACE_NORMALS` from `model_space_normals`
  (`cell_loader.rs:257-300`).
- **CDB fallback**: #3230's try-then-fall-through is intact. The `.mat`-only
  early return is present, and `.bgsm` / `.bgem` reach
  `apply_cdb_pbr_fallback` only at resolver misses. Tests
  `registered_cdb_does_not_shadow_a_resolvable_bgsm` and
  `unresolvable_bgsm_still_falls_back_to_cdb_pbr` are green.
- **Starfield scope**: there are 0 `.bgsm` / `.bgem` files in vanilla
  archives, so 1,783 `.bgsm` / `.bgem`-named stubs take the fallback. The
  BGEM `glass_enabled` path is unreachable for vanilla Starfield until CDB
  Phase 2. The open issues #4283 and #4400 cover the related glass signal.

### Cross-cutting — audit infrastructure

#### SF-2026-09-16-META-01: The audit path gate fails — `crates/nif/src/blocks/shader.rs` is backticked 10 times across 6 skills (including this one) after the `shader/` split
- **Severity**: LOW
- **Dimension**: Audit infrastructure (tech-debt)
- **Location**:
  - `.claude/commands/audit-starfield/SKILL.md:43, 315`
  - `.claude/commands/audit-fo3/SKILL.md:92`
  - `.claude/commands/audit-fo4/SKILL.md:121`
  - `.claude/commands/audit-nif/SKILL.md:65, 115`
  - `.claude/commands/audit-skyrim/SKILL.md:77, 87, 126`
  - `.claude/commands/audit-tech-debt/SKILL.md:129`
- **Status**: NEW (`eaa94b49d`, 2026-09-15; #4339 recorded the split but not
  the skill fallout)
- **Description**: `SKIP_SYMBOL_CHECK=1 .claude/commands/_audit-validate.sh`
  ends with `FAIL: 10 stale path reference(s)`. Every hit is the pre-split
  file, which is now `crates/nif/src/blocks/shader/{mod,legacy,sky_water,lighting,effect}.rs`.
- **Impact**: The gate is red for every audit skill edit. The per-game skills'
  Dimension 6 entry points send auditors to a missing file (the family now
  lives in `shader/lighting.rs` / `shader/effect.rs`).
- **Suggested Fix**: Repoint each reference at the family file that holds the
  cited symbol: `parse_skyrim_shader_base` → `shader/mod.rs`,
  `BSLightingShaderProperty` → `shader/lighting.rs`,
  `BSEffectShaderProperty` → `shader/effect.rs`.

---

## CRC32 Flag Table

There were no changes this cycle, and there is no new unknown hash. The
derivation is unchanged: reflected CRC-32, poly `0xEDB88320`, init 0, no final
XOR, over the **uppercase** nif.xml flag name. It is the same parameterisation
as `crates/bsa/src/csg.rs` and the CDB key hash, differing only in case fold.
`crates/nif/src/shader_flags.rs` (`bs_shader_crc32`) names all 32 nif.xml
entries.

| Flag | CRC32 | Read by import | Vanilla SF occurrences¹ |
|---|---|:-:|---:|
| `ZBUFFER_TEST` | `0x67B70934` | yes | 74 |
| `ZBUFFER_WRITE` | `0xBCBAC5F3` | yes | 74 |
| `TWO_SIDED` | `0x2D45EC6E` | yes | 1 |
| `VERTEXCOLORS` | `0x14C5C2AD` | no | 1,396 |
| `SKINNED` | `0xDF3182B0` | yes | 3 |
| `GRAYSCALE_TO_PALETTE_COLOR` | `0x1A5C2577` | yes | 1 |
| `DECAL` | `0xE56D16E0` | yes | 10 |
| `DYNAMIC_DECAL` | `0x5DF93B67` | yes | 10 |
| `REFRACTION` | `0x74AAC97E` | no | 1 |
| `NOFADE` | `0xB2757B8C` | no | 10 |
| `EMIT_ENABLED` | `0x86DBD392` | no (#4279) | 0 |

¹ From the 2026-08-30 census, carried in `AUDIT_STARFIELD_2026-09-11.md`, which
has the full 32-row table. This audit measured only the CRC array lengths
on the 879 full-body `BSEffectShaderProperty` blocks (`sf1` 1–3, `sf2` 0–1),
not the hash values. Stubs carry no arrays.

---

## Remaining-Work Chain

Per `starfield-esm-roadmap.md`, Phases 0 and 1 are done, and Phases 2–4 were
invalidated by the 99.9%-parity measurement. In priority order:

1. **CDB Phase 2: per-field `.mat` extraction** (#3398). The lookup key and
   field vocabulary are solved. What remains:
   - **(a) The canonical texture-role vocabulary** for roughness / metalness /
     AO / opacity / transmissive, with lanes and shader consumers
     (**SF-2026-09-16-D3-01**, new prerequisite). Without it Phase 2 can
     deliver colour, normal, emissive and height only.
   - **(b) The indexed or streaming reader** that avoids the corpus-wide
     ~18 GB `parse` peak (13 CDBs, two of them full-size), plus the XMCOLOR
     field-offset fix.
   - **(c) Restoring the key-derivation probe** the spike depends on
     (SF-2026-09-16-D3-02).
   - Phase 2 is a step function: Starfield produces **2** texture-role fills
     corpus-wide today.
2. **PDCL ahead of GBFM**. PDCL is 706 records and 74.9% of unresolved Cydonia
   REFRs. GBFM is 3,141 records and 0.081%.
3. **Exterior worldspace tiles**. This also unblocks the LTEX / TXST consumers
   that D5-01 feeds.
4. **Space-cell / planet / GBFM records**: SFTR, PNDT, STDT and BIOM are still
   `skip`.
5. ~~The #2105/#3524 NIF truncation tail~~: **cleared**. Starfield is
   100.00% clean (SF-2026-09-16-D7-01). Only the docs need to catch up.

Both the BGSM parser and the ESM parser have shipped. This chain is the
accurate ordering, not "BGSM parser first / ESM very far".

---

## Coverage Notes

- **Not re-run this cycle**: `--sf-smoke` (the per-cell resolve rate needs the
  engine binary) and any runtime or bench capture. #3540's frame-0 fix is
  still unconfirmed on-device for Cydonia.
- **Un-owned subsystems**: the Gameplay slice, the SDK, the Launcher, FaceGen,
  Mod Runtime, the FSR3 FFI, the Havok packfile reader and the Debug server
  were **not** examined. None is Starfield-specific.
- **Scratch tooling**: census scripts lived in the session scratchpad (outside
  the repo). No source files were modified.

## Total Findings Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 8 |
| **Total** | **9** |

| ID | Sev | Title |
|---|---|---|
| SF-2026-09-16-D3-01 | MEDIUM | Canonical texture-role vocabulary has no role for Starfield rough / metal / AO / opacity / transmissive maps (~39% of SF textures) |
| SF-2026-09-16-D3-02 | LOW | Phase-2 spike's reproduce commands and artifacts point at examples deleted by `a823c13a1` |
| SF-2026-09-16-D3-03 | LOW | `sf_cdb_count` Phase-2 doc prescribes the ruled-out re-parse; `.mat` fallthrough cites a closed tracker |
| SF-2026-09-16-D4-01 | LOW | ESM coverage docs cite the pre-split dispatch site; #4278 history names LCTN as a live arm it never had |
| SF-2026-09-16-D5-01 | LOW | `parse_txst_group` silently drops Starfield TXST TX08 / TX09 / TX17 / TX19 |
| SF-2026-09-16-D6-01 | LOW | #1510 "content-hash path" premise unsupported; all 387 suffix-less refs are `Materials\` and skip the CDB route |
| SF-2026-09-16-D7-01 | LOW | Starfield NIF parse is 100.00% clean; docs still say 99.98% / 19 residual |
| SF-2026-09-16-D8-01 | LOW | #4284 sibling: `translate_material` doc and `resolve_pbr` still say BGEM arrives pre-classified |
| SF-2026-09-16-META-01 | LOW | Audit path gate red: `blocks/shader.rs` stale in 6 skills (10 refs) |

Suggested next step: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-09-16.md`.
Label every finding `game:starfield` + `legacy-compat`, plus its domain:
- D3-01: `nifal`, `renderer`
- D3-02 and D3-03: `doc-rot`
- D4-01: `esm-plugin`, `doc-rot`
- D5-01: `esm-plugin`
- D6-01: `nif-parser`, `doc-rot`
- D7-01: `doc-rot`
- D8-01: `nifal`, `doc-rot`
- META-01: `tech-debt`
