# Starfield Compatibility Audit — 2026-06-23

**Auditor**: Claude (inline, all 9 dimensions, no nested sub-agents)
**Repo state**: `main` @ `2d4c350d`
**Game data**: `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` — **present**
(all 5 vanilla mesh archives + v2/v3 texture archives + DLC ESMs). All real-data
dimensions (1, 2, 4, 7) were exercised against live archives this round.

---

## Executive Summary

Starfield remains a first-class `GameKind`: NIF + BA2 v2/v3 (incl. LZ4 block),
CDB + BGSM/BGEM materials, and a walkable Cydonia interior all ship. This is a
depth/correctness regression audit of that bring-up surface, not a gap inventory.

**The bring-up surface is healthy and has, if anything, *improved* since the
last audit (2026-06-18):**

- **BA2 v3 / LZ4 decompression** — validated end-to-end on real vanilla v3 archives.
  200/200 textures from `Starfield - Textures01.ba2` (v3, `compression_method = 3`)
  decompressed to valid DDS. 15 v3 archives + 22 v2 DX10 + 92 v2 GNRL confirmed in
  vanilla data; unsupported `compression_method` is a hard error (verified). **No findings.**
- **NIF mesh parse rate** — the full 5-archive `parse_rate_starfield_all_meshes`
  sweep now reports **clean 100% on Meshes01 / Meshes02 / LODMeshes / FaceMeshes**
  and **98.91% on MeshesPatch** (325 truncated / 0 failed), **recoverable 100%
  across all five**. This is *higher* than the ROADMAP / compat-matrix figures
  (Meshes01 recorded as 97.21%, MeshesPatch 98.11%). → **doc-rot finding SF-D7-01 (LOW)**.
- **Cydonia ESM resolve rate** — `--sf-smoke citycydoniamainlevel` reports
  **88.8% (24 781 / 27 898 REFRs)** — bit-identical to the 2026-06-14 baseline.
  **No regression.** The two known renderable gaps (ESM-placed LIGH #1567,
  PDCL decals) remain open and unchanged.
- **BSGeometry `.mesh` resolution, NIFAL `translate_material`, BGSM/BGEM merge,
  CDB chunk index, spawn-path gates** — every regression guard from the bring-up
  arc (#1209/#1232/#1292/#1294/#1295/#1510/#1606/#1569) verified in place.

**Net new findings this round: 1 LOW (doc-rot).** Two existing OPEN issues
(#1567 LIGH, #1580 BGEM palette-alpha) re-verified as still-valid and unchanged.
The per-field CDB extraction gap (#1289 Phase 2) is confirmed as the top
remaining-work item, scoped — not re-filed.

---

## Findings by Severity

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 1 (NEW) |

Existing OPEN issues re-verified (not re-filed): #1567 (HIGH), #1576, #1580.

---

## LOW

### SF-D7-01: ROADMAP / compat-matrix Starfield parse rates understate current state
- **Severity**: LOW (documentation; the code is *better* than the doc claims)
- **Dimension**: 7 — Real-Data Validation
- **Location**: `ROADMAP.md` (Starfield compat-matrix row, line ~206 + per-game NIF
  clean-parse-rate row ~736)
- **Status**: NEW
- **Description**: The ROADMAP compat matrix records "Starfield 98.6% aggregate,
  Meshes01 97.21%, MeshesPatch 98.11%, sweep date 2026-04-27". The live
  `parse_rate_starfield_all_meshes` sweep this round (git `2d4c350d`) reports
  **Meshes01 100.00%, Meshes02 100.00%, MeshesPatch 98.91%, LODMeshes 100.00%,
  FaceMeshes 100.00%**, recoverable 100% on all five. The aggregate clean rate is
  now ≈ 99.6%, not 98.6%. The intervening parser work (≥ #1510 BSShaderType155 tail,
  #1606 starfield_tail, #754 BSWeakReferenceNode, #722 cloth) lifted the rate but
  the matrix was never refreshed.
- **Evidence**:
  ```
  [Starfield/Meshes01.ba2]    clean 100.00% (31058 clean / 0 trunc / 0 failed)
  [Starfield/Meshes02.ba2]    clean 100.00% (7552 clean / 0 trunc / 0 failed)
  [Starfield/MeshesPatch.ba2] clean  98.91% (29524 clean / 325 trunc / 0 failed)
  [Starfield/LODMeshes.ba2]   clean 100.00% (19535 clean / 0 trunc / 0 failed)
  [Starfield/FaceMeshes.ba2]  clean 100.00% (1282 clean / 0 trunc / 0 failed)
  ```
- **Impact**: None at runtime. Stale published figures only — risks a future
  audit "discovering" an improvement that already happened, or under-selling SF
  support in status reporting.
- **Related**: #746/#747 (residual MeshesPatch truncation tail — now 325 NIFs / 1.09%,
  has *not* grown).
- **Suggested Fix**: Refresh the Starfield compat-matrix row + the per-game
  clean-parse-rate row in `ROADMAP.md` with the 2026-06-23 figures; note the
  MeshesPatch truncation tail at 325/29 849.

---

## Dimension Findings (verification log)

### Dimension 1 — BA2 v2/v3 + LZ4 block decompression — PASS, no findings
`crates/bsa/src/ba2.rs`. The two-axis compression model is correct:
- **Archive-level codec** picked once at header parse. `version` match
  (`Ba2Archive::open`): v1/v7/v8 → `Zlib`; v2 reads the +8 extra header bytes,
  stays `Zlib`; v3 reads +8 then the `compression_method` u32 → `0 ⇒ Zlib`,
  `3 ⇒ Lz4Block`, **any other value is a hard `InvalidData` error** (no
  silent fall-through). Unknown major versions also hard-error.
- **Per-chunk on/off** — both `extract_general` and `extract_dx10` branch on
  `packed_size == 0` → raw copy, else `decompress_chunk`. v3 DX10's mixed
  raw + LZ4 mips within one texture are handled by this per-chunk branch.
- **LZ4 max_size** — `lz4_flex::block::decompress(packed, unpacked_size)` is
  given the explicit `unpacked_size` from the chunk record; failure maps to
  `InvalidData`. (Block LZ4 has no embedded size, so the explicit bound is
  required and correctly supplied.)

**Real-data validation**: scanned all `.ba2` in vanilla `Data/` → 15× v3 DX10
(`compression_method = 3`), 22× v2 DX10, 92× v2 GNRL, **0 v3 GNRL** (matches the
module doc's claim). Extracted 200/200 textures from `Starfield - Textures01.ba2`
(v3) and 42/42 from `OldMars - Textures.ba2` (v2) — all produced valid `DDS `
magic, 0 errors. Unit tests `decompress_chunk_lz4_roundtrip`,
`decompress_chunk_lz4_corrupt_data_fails`, `v3_unknown_compression_method_rejected`
all green.

### Dimension 2 — BSGeometry mesh extraction — PASS, no findings
`crates/nif/src/import/mesh/bs_geometry.rs` + `byroredux/src/asset_provider.rs`.
- Stage A (internal) and Stage B (external `.mesh`) both **iterate every LOD
  slot** via `find_map` / loop — the #1209 `meshes.first()` short-circuit is
  gone, in both stages.
- #1292 canonical path: external resolution composes `geometries\{mesh_name}.mesh`
  in the importer; `normalize_mesh_path` (`asset_provider.rs:158`) leaves
  `geometries\` / `geometries/` heads untouched (verified the byte-compare gate),
  prepending `meshes\` only otherwise.
- #1232 tangent fallback: empty `tangents_raw` with populated normals/uvs/positions
  routes to `synthesize_tangents_yup` (Mikkelsen, Y-up), not `Vec::new()`.
- #1203 skin: `extract_skin_bs_geometry` resolves the `BSSkin::Instance` +
  `BSSkin::BoneData` chain.
- PBR scalars `metalness_override` / `roughness_override` forwarded from the
  classifier'd `legacy_pbr` (overwritten downstream by BGSM merge when a `.mat`
  is present). `bs_lod_cutoffs` / `bs_sub_index` correctly `None` (BSGeometry has
  no FO4-era LOD discriminator).

### Dimension 3 — CDB material database — PASS (parse correct); per-field extraction is the scoped #1289 follow-up
`crates/sfmaterial/src/reader.rs` + `byroredux/src/asset_provider.rs`.
- `ComponentDatabaseFile::parse` walks header → STRT → TYPE → N×CLAS → instance
  stream. `index_chunks` bounds every chunk against the remaining buffer
  (`ChunkOverflow`), the #762 chunk-index regression guard. `peek_magic`
  distinguishes CDB (`BETH` signature) from a loose BGSM. Unknown chunk FourCC /
  builtin tag / class flag all hard-error **with diagnostics naming the offending
  class** (#1569) — pinned by `chunk_type_recognized_set_is_pinned` and siblings (5 tests green).
- #1571 DLC/Creation discovery: `discover_starfield_cdbs` scans each materials
  archive for **every** `materials\...materialsbeta.cdb` (base *and*
  `materials\creations\<plugin>\...`) via `is_materialsbeta_cdb_path`, appended in
  load order to `sf_cdbs` — no hardcoded single base path.
- **Per-field gap (scoped, not new)**: the `.mat` arm in `merge_bgsm_into_mesh`
  (`asset_provider.rs:1279`) flips `mesh.is_pbr = true` (gated on
  `has_starfield_cdb()`) and returns — `metalness_override`/`roughness_override`
  stay `None`, texture slots stay NIF-derived. The CDB is loaded and *validated to
  parse* but its per-`.mat` authored values are not walked into the `ImportedMesh`.
  This is exactly the #1289 Phase 2 follow-up the code comment documents
  ("Phase 2 will walk the CDB to extract authored values"). Routing to the Disney
  lobe with NIF defaults is better than Lambert but approximate. **Confirmed and
  scoped per skill instruction — not re-filed.**

### Dimension 4 — Starfield ESM resolve-rate baseline — PASS, no regression
`byroredux/src/sf_smoke.rs`. `--sf-smoke citycydoniamainlevel` against vanilla
`Starfield.esm`:
```
references : 27898 REFRs
resolved   : 24781 / 27898 (88.8%)
unresolved :  3117 / 27898 (11.2%)
by type: STAT 22758 (81.6%), MSTT 466, MISC 454, PKIN 370, FURN 292, ACTI 130,
         IDLM 95, ALCH 93, DOOR 41, CONT 37, FLOR 25, TERM 8, BOOK 6, ARMO 4, WEAP 2
```
Bit-identical to the published 2026-06-14 Cydonia baseline (88.8%, 24 781/27 898).
LIGH is **absent** from the resolved-types list (#1567, below). The per-record
breakdown shows no new Starfield-only base type leaking in where a parser is
missing. `parse_modl_group` does route `LIGH` into `statics` (records/mod.rs:335),
but SF LIGH forms ship no MODL/DATA (geometry+light params live in a BFCB component
block) so they resolve to a model-less static — the #1567 root cause stands.

### Dimension 5 — ESM + cell bring-up regression surface — PASS, no findings
- HEDR-0.96 → `GameKind::Starfield` classifier intact (`reader.rs`).
- #1291 `XCLL_SIZES_STARFIELD = [28, 108]` (`walkers.rs:51`), selected for
  `GameKind::Starfield`; doc comment correctly states the 108-byte body is
  decoded in full against xEdit SF1, *not* "Skyrim 92 + 16 tail".
- Spawn-path gates (`cell_loader/spawn.rs`): #1294 static-trimesh fallback gated on
  `base_layer == Architecture` (not `final_layer`) — the post-escalation
  misclassification that zeroed Cydonia colliders is fixed; #1212 `FormIdComponent`,
  #1213 `LocalBound`, #1214 `BSXFlags`, #1235 `SceneFlags::from_nif`, #1295
  `DoorTeleport` from XTEL all attached at spawn.
- **Synthesized colliders stay out of the BLAS**: the trimesh fallback spawns a
  **separate MeshHandle-free ghost entity** (`Transform + GlobalTransform +
  CollisionShape + RigidBodyData`), mirroring the bhk path — no MeshHandle ⇒ no
  BLAS entry ⇒ no TLAS instance (R6a-stale-14 fix). `IsCollisionOnly` is therefore
  unnecessary on the ghost (it carries no renderable geometry); the gate is
  structural, not flag-based. Correct.

### Dimension 6 — NIF shader blocks BSVER 155+ — PASS, no findings
`crates/nif/src/blocks/shader.rs`.
- CRC32 flag arrays: `parse_skyrim_shader_base` reads `sf1_crcs` for
  `bsver >= FO4_CRC_FLAGS (132)` and `sf2_crcs` for `bsver >= FO76_SF2_CRCS (152)`,
  each a length-prefixed `Vec<u32>`. The hashes are **not opaque** — there is a
  `bs_shader_crc32` name→hash table in `crates/nif/src/shader_flags.rs`, pinned
  against nif.xml's `BSShaderCRC32` enum literals by
  `bs_shader_crc32_matches_nif_xml_literals` (all 32 entries, #712/#748).
- #1510 regression: `BSShaderType155` dispatch + luminance/translucency/texture-array
  tail intact — the full mesh sweep reports **0 failed NIFs** across all 5 archives,
  so the ~1036 full-body BSLightingShaderProperty blocks are not truncating to
  `NiUnknown`.
- #1606 starfield tail: `read_starfield_tail` consumes **exactly
  `block_size − consumed`** (no hardcoded 38, no over-read), gated on
  `bsver >= STARFIELD` **and** `Some(block_size)`; the legacy `parse(None)` path
  yields an empty tail. Test `parse_bs_lighting_starfield_tail_empty_without_size_or_drift`
  green. The sibling BSEffectShaderProperty +32 B under-read is the known scoped
  follow-up — not re-filed.

### Dimension 7 — Real-data validation — PASS → see SF-D7-01 (doc-rot)
Full 5-archive mesh sweep: clean 100/100/98.91/100/100, recoverable 100% across
the board, **0 hard failures**. Texture archives extract cleanly (dim 1). The
only artifact is the stale ROADMAP figure (SF-D7-01).

### Dimension 8 — NIFAL canonical material translation — PASS, no findings
`byroredux/src/material_translate.rs` + `crates/core/src/ecs/components/material.rs`.
- `translate_material` is the **single** `ImportedMesh → Material` boundary.
  `metalness` / `roughness` land as plain resolved `f32` (NaN sentinels filled
  once by `Material::resolve_pbr`); there is **no per-draw `Option<f32>`
  `classify_pbr` plumbing** (removed by the NIFAL refactor, #1480/#1522).
- `EmissiveSource` discriminator (#1280) set by the material walker and by the
  BGEM merge (`EmissiveSource::Effect`).
- NIFAL particle slice reaches SF: typed `NiPSysEmitter` →
  `extract_emitter_params` (`walk/mod.rs:518`) → `apply_emitter_params`
  (`systems/particle.rs:29`).
- NIFAL collision slice: `BhkMultiSphereShape` (collision.rs:566) and
  `BhkConvexListShape` (collision.rs:684) both translate to `CollisionShape`
  (ConvexList → `Compound`), dispatch↔resolve parity held.

### Dimension 9 — BGSM/BGEM external material flow — re-confirms #1580 (existing OPEN)
`crates/bgsm/src/{bgsm,bgem}.rs` + `asset_provider.rs::merge_bgsm_into_mesh`.
- BGEM is dispatched distinctly from BGSM (magic-wins-over-extension, with a
  one-shot mismatch warn). BGEM forwards base/normal/glow/grayscale-LUT/env/
  specular/lighting textures, base_color×scale as `EmissiveSource::Effect`,
  UV/alpha/two-sided/decal/alpha-test, the #1651 `(One,One)` blend translation,
  `glass_enabled → mesh.bgem_glass` (#1280, the authoritative glass signal,
  consumed in `helpers.rs`), and the soft-particle `effect_shader` payload.
- `pack_bgsm_material_flags` derives `PBR_BSDF`, `TRANSLUCENCY`,
  `MODEL_SPACE_NORMALS`, `EFFECT_PALETTE_COLOR`, `BGSM_AUTHORED` from the right
  `ImportedMesh` fields.
- **#1580 (existing OPEN, re-verified valid)**: BGEM's `grayscale_to_palette_alpha`
  bool (`bgem.rs:49`, parsed at `bgem.rs:140`) is **not forwarded** — the
  `BsEffectShaderData` the BGEM arm constructs (`asset_provider.rs:1771`) sets
  `effect_soft`/`effect_lit`/falloff but leaves `effect_palette_alpha` at its
  `Default` (false), and a repo-wide grep finds zero consumers of
  `grayscale_to_palette_alpha` outside the parser. Unchanged since filed. Note:
  BGEM has no `grayscale_to_palette_color` field, so #1580 is the complete gap.

---

## CRC32 Flag Table

The FO76/Starfield CRC32-hashed shader flag arrays (`sf1_crcs` / `sf2_crcs`) are
**fully named**, not opaque — `crates/nif/src/shader_flags.rs::bs_shader_crc32`
mirrors nif.xml's `BSShaderCRC32` enum (pinned, 32 entries). Sample
(name → u32 hash):

| Flag | CRC32 (u32) |
|------|-------------|
| `DECAL` | 3849131744 |
| `DYNAMIC_DECAL` | 1576614759 |
| `TWO_SIDED` | 759557230 |
| `CAST_SHADOWS` | 1563274220 |
| `ZBUFFER_TEST` | 1740048692 |
| `ZBUFFER_WRITE` | 3166356979 |
| `VERTEX_COLORS` | 348504749 |
| `PBR` | 731263983 |
| `SKINNED` | 3744563888 |
| `ENVMAP` | 2893749418 |
| `VERTEX_ALPHA` | 2333069810 |
| `GRAYSCALE_TO_PALETTE_COLOR` | 442246519 |
| `HAIRTINT` | 1264105798 |

No new/unknown hashes were observed in this round's parse (0 failed NIFs).

---

## Remaining-Work Chain (per `starfield-esm-roadmap.md`)

Phases 0+1 are done; Phases 2–4 were invalidated by the 99.9%-record-parity
measurement. In priority order, the remaining renderable-fidelity gaps:

1. **Per-field CDB extraction** (#1289 Phase 2) — `.mat`-resolved Starfield
   materials currently reach the Disney lobe with NIF-default metalness/roughness
   and texture slots; the CDB is parsed but not walked. Top item. (Dim 3.)
2. **ESM-placed LIGH** (#1567, HIGH, OPEN) — Cydonia's 656 interior LIGH REFRs
   (across 62 forms) resolve to a model-less static because SF LIGH stores light
   data in a BFCB component block, not DATA/MODL; cell renders under-lit. (Dim 4.)
3. **PDCL projected decals** + **model-less BFCB-geometry base forms** (#1576) —
   the bulk of the remaining 11.2% Cydonia resolve gap; needs a decal-projection
   system and BFCB component-block geometry decode respectively. (Dim 4/5.)
4. **Exterior worldspace tiles / space-cell / planet / GBFM** — out of scope for
   the interior-first bring-up; GBFM confirmed stubbable for Cydonia (2 REFRs).
5. **#746/#747 NIF truncation tail** — MeshesPatch residual 325/29 849 (1.09%),
   recoverable 100%; has not grown.

Both the BGSM/BGEM parser and the CDB consumer have **shipped** — this chain is
fidelity refinement, not a "parser-first / ESM-very-far" bring-up.

---

## Disproof Log (regressions investigated and cleared)

- **BA2 v3 LZ4 silent-fallthrough on unknown method** — disproved; `compression_method`
  other than 0/3 hard-errors (`ba2.rs:243`), unit-tested.
- **BSGeometry `meshes.first()` LOD short-circuit (#1209)** — disproved; both stages iterate.
- **`normalize_mesh_path` prepending `meshes\` to `geometries\` (#1292)** — disproved; byte-gate present.
- **#1510 BSLightingShaderProperty → NiUnknown truncation** — disproved; 0 failed NIFs in full sweep.
- **#1606 starfield_tail over-read / hardcoded 38** — disproved; consumes exactly `block_size − consumed`.
- **Synthesized SF colliders entering BLAS (R6a-stale-14)** — disproved; ghost entity carries no MeshHandle.
- **Cydonia resolve-rate regression** — disproved; 88.8% bit-identical to baseline.
- **CDB DLC discovery re-hardcoded to single base path (#1571)** — disproved; scan-based `discover_starfield_cdbs`.
- **NIFAL per-draw classify_pbr fallback** — disproved; metalness/roughness are resolve-once `f32`.

---

## Notes / Out-of-scope observations

- A pre-existing **compile error** in `crates/scripting/src/fragment.rs:244`
  (`mod tests;` with no `tests.rs` on disk) and untracked
  `crates/scripting/src/{fragment.rs,translate/effects.rs}` are present in the
  working tree from a concurrent (non-Starfield) session. They block
  `cargo test --workspace` but are **unrelated to Starfield** — all SF-relevant
  crates (`bsa`, `sfmaterial`, `nif`, `plugin`, `byroredux` binary) build and test
  clean (835 + 5 + 500 + 5 + 8 tests green). Not a finding of this audit; flagged
  for whoever owns that branch.
- `--sf-smoke` emits a burst of `#1620 — ARMO …: corrupt MODL mesh path (control
  bytes)` warnings on Cydonia load; this is the existing tolerant-decode path for
  SF ARMO records whose geometry lives in a BFCB block (kin to #1576), already
  warned-and-skipped. Not new.

---

## References

- Prior: `docs/audits/AUDIT_STARFIELD_2026-06-18.md`, `…2026-06-14.md`
- Specs: `docs/engine/starfield-esm-roadmap.md`, `…-phase0-baseline.md`,
  `docs/engine/nifal.md`
- Open issues re-verified: #1567 (HIGH), #1576, #1580

Suggested next step:

```
/audit-publish docs/audits/AUDIT_STARFIELD_2026-06-23.md
```
