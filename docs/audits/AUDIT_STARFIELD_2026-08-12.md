# Starfield Compatibility Audit — 2026-08-12

*Suite preset: `texture-roles-deep` (Dimensions 3 / 8 / 9 weighted).*

## Executive Summary

Starfield is a first-class `GameKind`: NIF parsing (BSVER 155+, `BSGeometry`
geometry path), BA2 v2/v3 (zlib + LZ4 block, GNRL + DX10), CDB
(`materialsbeta.cdb`) + external BGSM/BGEM materials, ESM at ~99.9% record
parity, and a walkable Cydonia interior. This is a depth/correctness audit of
that bring-up surface. Real game data was available at
`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` (44 archives) and
every dimension except Dimension 4's per-cell resolve rate ran against it.

**All four HIGH findings from `AUDIT_STARFIELD_2026-08-07.md` are fixed and
verified fixed this pass** — bind-pose skinning (#2613), the CDB
`index_chunks` unvalidated reserve (#2614), the `Archive::open` full-file magic
sniff (#2615), the one-word `BSLightingShaderProperty` misalignment (#2616), and
the `BSEffectShaderProperty` stub-guard gap (#2617). No regressions found in any
of them. Parse rate over all five vanilla mesh archives holds at
**89,276/89,276 NIFs, 100% recoverable, 99.993% clean**, with the #2105
truncation tail unchanged at exactly 6 files.

**This pass's headline is a measurement, not a new bug: Starfield's canonical
texture-role fill rate is 0 of 18 roles on 100% of vanilla content, and unlike
every other supported game there is no NIF-side fallback to soften it.** Over
38,930 meshes imported from 12,000 vanilla NIFs, not one populated a single
`MaterialTextureSet` role. The cause is two independently-defensible decisions
composing into a blackout: Starfield NIFs ship **zero `BSShaderTextureSet`
blocks** and every full-body `BSLightingShaderProperty` carries a NULL
`texture_set_ref` (so the NIF walker has nothing to read), while the `.mat`
merge arm returns before touching `material.textures` (so the external side
supplies nothing either). The comparison table in Dimension 8 shows FO4 at 70.9%
`base_color` fill and FO76 at 5.1% under the identical code. This is already
tracked as **#2359** and is *not* re-filed — but #2359 should be re-read as a
total-content-blackout, not the "PBR scalars are approximate" fidelity gap its
description implies.

**Six new findings: 0 CRITICAL, 0 HIGH, 3 MEDIUM, 3 LOW.** The two that matter
most are `RefrTextureOverlay::inner` being a write-only field that silently
drops every inner-layer texture override (SF-2026-08-12-D9-01, hidden from the
`dead_code` lint by a `derive(Debug)`), and the 105 MB `materialsbeta.cdb` being
fully decompressed and immediately discarded on every material-provider rebuild
to read sixteen bytes (SF-2026-08-12-D3-01).

### The findings, in one paragraph

`RefrTextureOverlay::inner` is filled by both TXST paths and read by no one, so
XTXR slot-6 and TXST inner-layer overrides never reach the live
`MaterialTextureSet::inner_layer` role its own doc comment claims it round-trips
to (MEDIUM). The vanilla CDB is inflated from 17.6 MB to 105,037,616 bytes and
dropped on the floor at every cell transition, save-load and debug-load, to read
a 16-byte presence probe (MEDIUM) — a cost hidden by three doc comments that
describe an `Arc`-cached `MaterialProvider::sf_cdbs` field which does not exist
(LOW). `classify_legacy_pbr` stamps a fabricated `Some(0.0)/Some(0.85)` PBR pair
onto 97.9% of Starfield meshes from an input set that is empty by construction,
permanently disabling the NaN-sentinel fallback that #2359's Phase 2 would
otherwise use to detect its own misses (MEDIUM). The REFR-overlay path is a
second, parallel external-material resolver that knows only `.bgsm`/`.bgem`
(LOW), and `merge_external_material`'s `bool` return cannot express "resolved to
nothing" and is discarded at all five production call sites anyway — which is
why a total texture blackout produces no log line (LOW).

## Dimension Findings

### Dimension 1 — BA2 v2/v3 LZ4 Block Decompression

Scope: `crates/bsa/src/ba2.rs`. Real data available (129 Starfield archives).

#### Verified clean (no regression)
- v2 reads an 8-byte header extension, v3 reads 8 + a 4-byte `compression_method`
  at the correct offset (`ba2.rs:221-269`). The version match is exhaustive over
  `{1,2,3,7,8}`; unknown majors are a hard `InvalidData` error, not a fall-through.
- `compression_method` dispatch is a hard error on anything but `0`/`3`
  (`ba2.rs:245-254`) — confirmed a real error return, not a silent default.
- Per-chunk raw-vs-compressed selection is `chunk.packed_size == 0` →
  raw `read_exact`, else `decompress_chunk` (`extract_dx10`, `ba2.rs:815-827`),
  so v3 DX10's mixed raw/LZ4 mip chunks resolve per chunk, as documented.
  GNRL (`extract_general`, `:775-793`) reaches the same unified path.
- `lz4_flex::block::decompress` is given the chunk's declared `unpacked_size`
  and the SF-D1-01 (#2618) under-run warning has landed on the LZ4 arm
  (`ba2.rs:762-769`) with the previously-wrong justifying comment corrected.
- Extract rate: the 5-archive Starfield mesh sweep (Dim 7) extracted and parsed
  89,276/89,276 NIFs with 0 extract failures — 100% holds.

#### Still open, not re-filed
- #2628 (SF-D1-02, DXGI 10/11/31 `pitch_or_linear_size_for`) — partially
  addressed by #2619; the issue is still OPEN, left to its own tracker.
- #2360 (SF-BA2-02, v3 header-boundary log reads stream position 4 B early)
  — verified still present at `ba2.rs:238` (the `log_v2_v3_extra_bytes` call
  runs before the `method_buf` read); diagnostic-only, already tracked.

#### New findings
None.

---

### Dimension 2 — BSGeometry Mesh Extraction

Scope: `crates/nif/src/import/mesh/bs_geometry.rs`, `crates/nif/src/blocks/bs_geometry.rs`,
`crates/nif/src/import/mesh/skin.rs`, `byroredux/src/asset_provider/archive.rs`.

#### Verified clean (no regression)
- **#2613 (bind-pose skinning) is fixed.** `extract_skin_bs_geometry`
  (`skin.rs:272-320`) now routes `mesh_data.skin_weights` through
  `convert_bs_geometry_skin_weights` (`skin.rs:238-267`) into real
  `vertex_bone_indices` / `vertex_bone_weights`, with a length-mismatch
  guard that preserves the empty-vector rigid-fallback sentinel.
- **#1209 / #1828 / #1829 (LOD-slot + sentinel skip) hold on both stages.**
  Stage A's `find_map` (`bs_geometry.rs:44-52`) and Stage B's loop
  (`:88-130`) each require `!vertices.is_empty() && !triangles.is_empty()`
  before accepting a slot, and each iterates every slot.
- **The Stage-A/Stage-B split cannot strand a mesh.** I tried to disprove the
  sentinel fix by constructing a mixed-kind BSGeometry (internal sentinel in
  slot 0, external real geometry in slot 1) — that shape is impossible by
  construction: `BSGeometry::parse` (`crates/nif/src/blocks/bs_geometry.rs:104-110`) passes the
  single block-level `FLAG_INTERNAL_GEOM_DATA` bit to every slot's parse, so all
  slots in one block are the same kind. Finding withdrawn.
- **#1292 (`geometries\<X>.mesh`) intact** — the canonical path is composed in
  the importer (`bs_geometry.rs:91`) and the `geometries\` head is left
  untouched by `normalize_mesh_path`.
- **#2357 (silent resolve failure) fixed** — all three no-geometry exits now log,
  the terminal one at `warn!` (`bs_geometry.rs:131-137`).
- Parse rate over all 5 vanilla mesh archives: 89,276 NIFs, 100% recoverable,
  99.99% clean (measured this pass — see Dim 7).

#### Still open, not re-filed
#2361, #2362, #2098, #2099, #1830, #2105 — all confirmed still-live premises.

#### New findings
None.

---

### Dimension 3 — CDB Material Database Correctness

Scope: `crates/sfmaterial/src/{reader,chunk,string_table,types,value}.rs`,
`byroredux/src/asset_provider/material.rs`. Real data: `Starfield - Materials.ba2`
(17.6 MB on disk; `materials\materialsbeta.cdb` = **105,037,616 bytes uncompressed**,
measured this pass).

#### Verified clean (no regression)
- **#2614 (SF-D3-01) fixed** — `index_chunks` now reserves
  `chunk_count.min(self.bytes.len() / 8)` (`reader.rs:172-190`), so a hostile
  `u32` count can no longer `abort()` before the `ChunkOverflow` guard.
- **#2615 (SF-D3-03) fixed** — `Archive::open` sniffs 4 magic bytes through
  `sniff_magic_from` (`archive.rs:12-40`) instead of `fs::read`ing the whole file.
- `peek_magic` still separates CDB from loose BGSM/BGEM; #762's chunk-overflow
  guard intact; #1571's `is_materialsbeta_cdb_path` predicate
  (`material.rs:13-16`) still matches base + `materials\creations\<plugin>\`.
- Unknown `ChunkType` / `BuiltinType` / `ClassFlags` remain deliberate hard
  errors (`types.rs:37-57`) — correct for a flat, length-prefix-less chunk stream.

#### Texture roles on the CDB path (preset focus)
`crates/sfmaterial/` has **no material-specific extraction of any kind** — it
emits a generic `Value` tree (`value.rs`) with no notion of texture roles, slots,
or `MaterialTextureSet`. Consequently there is **no per-game slot index or
CDB-specific discriminator anywhere on the CDB path**, because the path forwards
nothing at all. The cardinal NIFAL violation the preset asked about is
structurally absent; the actual problem is the opposite (0/18 roles filled — see
Dim 8/9 and #2359).

#### Still open, not re-filed
#2633 (SF-D3-05 duplicate-field last-wins), #2621 (SF-D3-04 `--bsa` numeric-sibling
expansion), #2359 (`.mat` merge forwards zero data). All three premises re-verified
against the current tree and still hold.

#### New findings

##### SF-2026-08-12-D3-01 — The 105 MB `materialsbeta.cdb` is fully decompressed and immediately discarded on every `build_material_provider` call
- **Severity**: MEDIUM
- **Dimension**: 3 — CDB material database
- **Location**: `byroredux/src/asset_provider/material.rs:44-52` (`discover_starfield_cdbs`), `:352-384` (`register_starfield_cdb`)
- **Status**: NEW
- **Description**: `discover_starfield_cdbs` calls `archive.extract(&path)` for every
  discovered CDB, which for a BA2 GNRL entry runs the full zlib inflate into an owned
  `Vec<u8>`. `register_starfield_cdb` then reads exactly the 4-byte magic and the
  12-byte header (`probe_header`), bumps a counter, and the `Vec` is dropped at the end
  of the loop iteration. Nothing retains the bytes. Phase 1 needs 16 bytes; it pays
  105 MB of inflate + allocation for them, per CDB, per provider build.
- **Evidence**: Measured — `materials\materialsbeta.cdb` extracts to 105,037,616 bytes
  from the 17.6 MB `Starfield - Materials.ba2`. `MaterialProvider` has no field holding
  CDB bytes (only `sf_cdb_count: usize`, `material.rs:277`).
  `build_material_provider` runs fresh at boot (`scene.rs:355,395`,
  `byroredux/src/scene/nif_loader.rs:78`), at every door/cell transition (`app_step.rs:514,575`),
  at save-load (`save_io.rs:913`), and at debug-load (`debug_load.rs:125,283,370`) —
  the same call-site set #2615 was filed against, so the CDB extract is now the
  dominant remaining cost of that rebuild.
- **Impact**: ~105 MB transient allocation + a multi-hundred-ms inflate on every cell
  transition on Starfield, for a presence bit. Same class as the (fixed) SF-D3-03
  full-archive sniff, one layer in.
- **Related**: #2615 (SF-D3-03, fixed sibling), #2039 / PERF-D7-02 (provider-rebuild
  caching note in `app_step.rs:445-460`), #2359 (Phase 2 — which *will* need the bytes,
  so the fix should be a cache, not a narrower read).
- **Suggested Fix**: Either (a) add a bounded `Vec<u8>`/`Arc<[u8]>` hold keyed by
  archive+path so the Phase-2 parse and cross-cell rebuilds reuse it — the shape the
  `csg_cache` next to it already uses — or (b) short-circuit discovery when the same
  (archive path, CDB path) pair was already registered this session.

##### SF-2026-08-12-D3-02 — Three doc comments cite a `MaterialProvider::sf_cdbs` `Arc` cache that does not exist, and the claim actively contradicts the code
- **Severity**: LOW
- **Dimension**: 3 — CDB material database
- **Location**: `byroredux/src/asset_provider/material.rs:280`, `:311`, `byroredux/src/app_step.rs:450`
- **Status**: NEW
- **Description**: `csg_cache`'s field doc says it "mirrors the `sf_cdbs` `Arc` hold";
  `geometry_csg`'s doc repeats "mirrors the `sf_cdbs` `Arc` caching"; `app_step.rs`'s
  caching design note lists "`MaterialProvider::sf_cdbs`" among the caches discarded on
  rebuild. `grep -rn sf_cdbs byroredux/src/` returns only those three doc hits — the
  field was replaced by `sf_cdb_count: usize` and no CDB bytes are cached anywhere.
- **Impact**: Documentation-only, but the false claim is load-bearing in the wrong
  direction: a reader auditing provider-rebuild cost would conclude the CDB is already
  `Arc`-cached and stop, which is exactly how SF-2026-08-12-D3-01 stayed unnoticed.
- **Suggested Fix**: Reword all three to reference the real `csg_cache` precedent, or
  land the cache and make the comments true.

---

### Dimension 4 — Starfield ESM Resolve-Rate Baseline

Scope: `crates/plugin/examples/sf_smoke.rs`, `byroredux/src/sf_smoke.rs`,
`docs/engine/starfield-esm-phase0-baseline.md`.

#### Measured this pass
`sf_smoke --recurse` against the real 1.36 GB `Starfield.esm`:

| Metric | This pass | Phase 0/1 baseline |
|---|---|---|
| Distinct top-level GRUP FourCCs | 176 | 176 |
| Handled by dispatch | 76 grups / 1,254,748,494 B / **86.1%** | 86.1% |
| Silently skipped | 100 grups / 202,318,596 B / 13.9% | 13.9% |
| Recursive leaf bytes | 1,454,634,906 (99.8%) | 99.8% |
| Recursive walk errors | **0** | 0 |
| Leaf REFR records | 3,291,860 | — |

**No regression.** Top leaf FourCCs are unchanged in shape (REFR / INFO / RFGP /
DIAL / NAVM / CELL / STAT / LMSW / PKIN / ACHR …); GBFM still appears at 3,141
records, the known conscious Phase-3 stub.

#### Not run
`byroredux --sf-smoke <CELL_EDID>` (the **per-cell base-form resolve rate**) was
**not** run this pass — it lives in the engine binary and the audit brief
prohibits launching it. The 91.2% / 90.8% Cydonia figures in
`AUDIT_STARFIELD_2026-08-07.md` are therefore **carried, not re-measured**; treat
them as of 2026-08-07. Nothing in the intervening commits touches
`crates/plugin/src/esm/cell/` or `records/`, so a regression is unlikely but
unverified.

#### Still open, not re-filed
#1576 (model-less STAT/BNDS/ACTI/ARMO), #1567 (LIGH `DAT2`, verified present),
#1568 (PDCL named skip — verified still a named `skipped_unconsumed_groups` push
at `crates/plugin/src/esm/records/mod.rs:351-357`, not the anonymous catch-all).

#### New findings
None.

---

### Dimension 5 — ESM + Cell Bring-up Regression Surface

Scope: `crates/plugin/src/esm/reader.rs`, `crates/plugin/src/esm/records/mod.rs`,
`crates/plugin/src/esm/cell/walkers.rs`, `byroredux/src/cell_loader/spawn.rs`.

#### Verified clean (no regression)
- `XCLL_SIZES_STARFIELD = &[28, 108]` still split off the Fallout-era bucket
  (`crates/plugin/src/esm/cell/walkers.rs:57`, dispatched at `:95`). Commit `65217327`
  ("name XCLL directional rotation fields") touched this file since the last
  audit; it is a rename of already-decoded fields, size table unchanged.
- #1568 PDCL remains a *named* conscious skip (`crates/plugin/src/esm/records/mod.rs:351-357`) —
  pushed into `index.skipped_unconsumed_groups` with a one-shot warn, so
  coverage tooling still counts it.
- Static-trimesh fallback still gated on `base_layer`, not `final_layer`
  (`spawn.rs:74-85`).

#### Notes
- `IsCollisionOnly` no longer appears anywhere in `byroredux/src/cell_loader/spawn.rs`; the
  synthesized-collider BLAS-exclusion marker referenced by the skill checklist
  now lives elsewhere. Not scored as a finding — the marker component still
  exists in `byroredux/src/components.rs` and the collider path was reworked by
  `8ee151e0`/`716b7ee9` (#2355) after the checklist text was written. **The
  `/audit-starfield` SKILL.md Dimension 5 checklist line citing
  `spawn.rs`+`IsCollisionOnly` is stale and should be re-pointed next time that
  file is edited.**

#### New findings
None. (The one texture-role bug found on the REFR spawn path is filed under
Dimension 9, where the role taxonomy lives.)

---

### Dimension 6 — NIF Shader Blocks, BSVER 155+

Scope: `crates/nif/src/blocks/shader.rs`, `crates/nif/src/shader_flags.rs`.

#### Verified clean (no regression)
- **#2616 (SF-D6-01, the one-word `BSLightingShaderProperty` misalignment) is
  fixed.** `parse_fo76_plus` now reads `shader_type` unconditionally
  (`shader.rs:1152`) and gates `root_material_path` on
  `bsver < STARFIELD` (`:1170-1174`), with the full corpus-verification
  rationale inline. The two compensating 4-byte errors are gone.
- **#1606 tail capture is to-`block_size`, not a hardcoded 38.**
  `read_starfield_tail` (`shader.rs:760-778`) computes
  `block_size - (position - block_start)`, returns empty when `remaining == 0`,
  and returns empty for `bsver < STARFIELD` or `block_size == None` — no
  over-read possible.
- CRC32 flag arrays: `num_sf1`/`num_sf2` + per-element u32 reads
  (`shader.rs:1153-1156`); the 32-entry name↔hash table in
  `shader_flags.rs::bs_shader_crc32` is unchanged and still pinned by
  `bs_shader_crc32_matches_nif_xml_literals`.
- **`NiUnknown` count for Starfield shader blocks: 0.** Measured over 12,000
  vanilla NIFs (Meshes01+02): 35,588 `BSLightingShaderProperty` blocks parsed as
  typed (35,248 material-reference stubs + 340 full-body) plus 158
  `BSEffectShaderProperty`. Aggregate parse rate 100% recoverable (Dim 7).

#### Measured, new corpus facts (not defects, but they change what other
#### dimensions can assume)
- The Starfield stub discriminator (`!name.is_empty()`, `shader.rs:1125-1132`)
  captures **99.04%** of `BSLightingShaderProperty` blocks (35,248 / 35,588).
- **All 340 full-body Starfield `BSLightingShaderProperty` blocks carry
  `texture_set_ref == BlockRef::NULL`**, and there are **zero
  `BSShaderTextureSet` blocks in the entire 12,000-NIF sample.** The
  texture-set branch in `apply_bs_lighting_shader`
  (`import/material/dedicated_shader.rs:96-...`) is therefore unreachable on
  Starfield content. Feeds the Dim 8/9 finding.
- **All 148 full-body `BSEffectShaderProperty` blocks carry an empty
  `source_texture`.**

#### Still open, not re-filed
#2639 (BSVER band 168-171 has no Starfield handling), #2624 (tautological
Starfield shader test fixtures), #2622 (the 38-byte tail is `BSSPLuminanceParams`
at documented defaults), #2105 (residual truncation tail).

#### New findings
None.

---

### Dimension 7 — Real-Data Validation

Game data confirmed present at
`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` (44 archives incl.
Shattered Space + Creations). All dimensions had real-data access except the
per-cell resolve-rate run (Dim 4, prohibited by the brief).

#### Measured this pass
`cargo test --release -p byroredux-nif --test parse_real_nifs
parse_rate_starfield_all_meshes -- --ignored` (7.79 s):

| Archive | NIFs | Clean | Truncated | Failed | Recoverable |
|---|---|---|---|---|---|
| `Starfield - Meshes01.ba2` | 31,058 | 100.00% | 0 | 0 | 100% |
| `Starfield - Meshes02.ba2` | 7,552 | 100.00% | 0 | 0 | 100% |
| `Starfield - MeshesPatch.ba2` | 29,849 | 99.98% | **6** | 0 | 100% |
| `Starfield - LODMeshes.ba2` | 19,535 | 100.00% | 0 | 0 | 100% |
| `Starfield - FaceMeshes.ba2` | 1,282 | 100.00% | 0 | 0 | 100% |
| **Total** | **89,276** | **99.993%** | 6 | 0 | **100%** |

**The #2105 residual truncation tail has NOT grown**: still exactly 6/29,849
`MeshesPatch` files, each losing 1 block, all terrain-object LOD NIFs
(`lc174world.1.0.1.nif`, `cydoniacity.2.-2.-2.nif`, `cydoniacity.8.-6.-6.nif`, +3).

Import-side, over 12,000 NIFs from Meshes01+02 with a real external-`.mesh`
resolver chain: **38,930 meshes imported** (`import_nif_with_resolver`), 0 hard
failures.

#### New findings
None. (The 0/18 texture-role fill measurement made with this harness is filed
under Dimension 8.)

---

### Dimension 8 — NIFAL Canonical Material Translation for Starfield

Scope: `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`,
`crates/nif/src/import/material/{mod,dedicated_shader}.rs`,
`byroredux/src/asset_provider/material.rs`.

#### Verified clean (no regression)
- `translate_material` is still the single raw→canonical boundary; no `GameKind`
  branch inside it; `Material.metalness`/`roughness` remain plain resolved `f32`.
- **#2617 fixed** — the #2353 material-reference-stub guard now exists on the
  `BSEffectShaderProperty` walker too (`dedicated_shader.rs:419`), alongside the
  `BSLightingShaderProperty` one (`:86`). The invisible-effect-surface bug is closed.
- All 22 `MaterialTextureSet` roles (18 named + 4 decals) survive to the GPU:
  `map_secondary_texture_handles` (`byroredux/src/asset_provider/texture.rs:433-453`) resolves
  every one, and `GpuMaterial` (`crates/renderer/src/vulkan/material.rs:100-296`) carries a matching
  index for each — no role dead-ends at the GPU boundary.

#### MEASURED: Starfield's canonical texture-role fill rate is 0/18, with no fallback

Harness: `import_nif_with_resolver` over 12,000 vanilla NIFs
(`Starfield - Meshes01.ba2` + `Meshes02.ba2`) with a real external-`.mesh`
resolver chain; 38,930 imported meshes.

| | Starfield | FO4 (`Fallout4 - Meshes.ba2`, 4k NIFs) | FO76 (`SeventySix - Meshes.ba2`, 4k NIFs) |
|---|---|---|---|
| imported meshes | 38,930 | 11,281 | 10,425 |
| `.mat` material path | 38,120 (97.9%) | 0 | 0 |
| `.bgsm`/`.bgem` path | 392 (1.0%) | 8,642 | 9,807 |
| **no** material path | 418 (1.1%) | 2,639 | 618 |
| meshes with ANY role filled | **0** | 8,002+ | 535+ |
| `base_color` filled | **0** | 8,002 | 535 |
| `normal` filled | **0** | 7,369 | 312 |
| every other role | **0** | env 2,851 · height 626 · glut 294 · tint 142 | env 104 · glut 110 · lighting 76 · reflectance 76 |

Root cause is a *pair* of independently-correct decisions that compose into a
total blackout, and neither is visible from its own side:
1. Starfield NIFs carry **zero `BSShaderTextureSet` blocks**, and all 340
   full-body `BSLightingShaderProperty` blocks carry
   `texture_set_ref == NULL` (Dim 6, measured). There is nothing for the NIF
   walker to read even when the #2353 stub guard does *not* fire.
2. The `.mat` merge arm (`byroredux/src/asset_provider/material.rs:726-739`) returns before
   touching `material.textures`, so the external side supplies nothing either.

**This is already tracked as #2359** (OPEN, "Starfield `.mat` merge forwards zero
authored material data"). Not re-filed. What is new is the *measurement* and the
corollary that the NIF-side belt-and-braces fallback that keeps FO4 (70.9%
`base_color` fill) and FO76 (5.1%) partially textured **does not exist at all on
Starfield** — every Starfield surface renders on the placeholder/fallback texture
today. #2359 should be re-read as a total-content-blackout, not a
"scalars are approximate" fidelity gap.

#### New findings

##### SF-2026-08-12-D8-01 — `classify_legacy_pbr` stamps a fabricated `Some(0.0)/Some(0.85)` PBR pair onto 97.9% of Starfield meshes from an input set that is empty by construction, permanently disabling the NaN-sentinel fallback
- **Severity**: MEDIUM
- **Dimension**: 8 — NIFAL canonical material translation
- **Location**: `crates/nif/src/import/material/mod.rs:1194-1218` (`classify_legacy_pbr`), `:1269-1270` (the unconditional `Some(...)` write), `crates/core/src/ecs/components/material.rs:816-842` (`resolve_pbr`), `byroredux/src/asset_provider/material.rs:726-739` (the `.mat` early return)
- **Status**: NEW — distinct from #2359, which is about the *merge* forwarding nothing; this is about the *importer* asserting a resolved value it did not derive from anything
- **Description**: On a Starfield material-reference stub the walker returns at
  `dedicated_shader.rs:86` before writing a single `MaterialInfo` field, so
  `into_imported_material` calls `classify_legacy_pbr` on an all-defaults
  `MaterialInfo`: `texture_path = None` → `path = ""` (no keyword can match),
  `specular_authored = false`, `has_normal_map = false`, `has_gloss_map = false`,
  `env_map_scale = 0.0` (the `MaterialInfo::default`, `mod.rs:1061`, which fails the
  `> 0.3` arm). Every classifier arm falls through to the terminal
  `PbrMaterial { roughness: 0.85, metalness: 0.0 }` (`material.rs:757-759`). That
  constant is then written as `metalness_override: Some(0.0)`,
  `roughness_override: Some(0.85)` — indistinguishable downstream from an authored
  value.
- **Evidence**: 38,120 of 38,930 sampled Starfield meshes (97.9%) take this exact
  path. Because both overrides are `Some`, `translate_material` never seeds the NaN
  sentinel, so `Material::resolve_pbr`'s backstop (`material.rs:817`) is unreachable
  for Starfield — the `merge_external_material` comment at `material.rs:730-737`
  states this outcome explicitly ("the NaN-sentinel path in `Material::resolve_pbr`
  never fires for Starfield content") but frames it as a benign fact rather than a
  fabrication.
- **Impact**: (a) Today: a single invented matte-dielectric constant on essentially
  all Starfield content, presented to the Disney BSDF lobe as resolved data.
  (b) After #2359 Phase 2 lands: any `.mat` the CDB index *misses* will silently keep
  the fabricated `0.0/0.85` instead of falling back to the sentinel — the failure
  becomes permanently invisible rather than merely current. This is the NIFAL
  no-fabrication rule (`docs/engine/nifal.md`) applied at the boundary.
  Scored MEDIUM rather than the HIGH the severity table assigns to "divergent
  Material out of NIFAL", because the value is not *divergent* from a competing
  authored value — there is none — and the immediate rendering harm is wholly
  contained by #2359.
- **Related**: #2359, #2353, #2330 (second spawn-time roughness write outside the boundary).
- **Suggested Fix**: When `MaterialInfo` carries no authored signal at all (the
  stub-guard case), leave `metalness_override`/`roughness_override` as `None` so
  `translate_material` seeds the NaN sentinel and `resolve_pbr` owns the default —
  one code path for "unknown", instead of a fabricated `Some` that outranks Phase 2's
  own miss-detection.

---

### Dimension 9 — BGSM/BGEM External Material Flow

Scope: `crates/bgsm/src/{bgsm,bgem}.rs`, `byroredux/src/asset_provider/material.rs`
(`merge_external_material`), `byroredux/src/cell_loader.rs`
(`pack_imported_material_flags`), `byroredux/src/cell_loader/refr.rs`,
`byroredux/src/cell_loader/spawn.rs`.

#### Verified clean (no regression)
- `merge_external_material` still takes `&mut ImportedMaterial` — the NIFAL
  narrowing holds; nothing in the body reaches geometry/skin/transform.
- BGSM/BGEM dispatch is on file **magic** with extension as fallback and a
  one-shot mismatch warn (`material.rs:768-793`).
- Role assignment on the BGSM arm is correct against the preset's specific
  concerns: `smooth_spec ← bgsm.smooth_spec_texture` (`material.rs:904-909`) and
  `specular ← bgsm.specular_texture` (`:952-957`) are distinct fills into
  distinct roles; `environment ← bgsm.envmap_texture` (`:935-940`). BGEM fills
  `environment`/`environment_mask` from its own two distinct fields.
- Starfield `.mat` still cannot leak a CDB slot index — it forwards nothing
  (#2359).
- `pack_imported_material_flags` bits still derive from the right
  `ImportedMaterial` fields, including #2108's enable-bit gate.

#### Still open, not re-filed
#2626 (BGEM `refraction` as unconditional glass signal — re-verified present at
`material.rs:110-113`), #2627 (BGSM `inner_layer_texture` never wired — re-verified:
no `inner_layer` fill exists in the BGSM block at `material.rs:881-975`), #2642,
#2643, #2594, #2595, #2601, #2606-#2610, #2533.

#### New findings

##### SF-2026-08-12-D9-01 — `RefrTextureOverlay::inner` is written by both TXST paths and read by nobody: every XTXR slot-6 / TXST inner-layer override is silently dropped at the overlay→`MaterialTextureSet` boundary
- **Severity**: MEDIUM
- **Dimension**: 9 — external material flow / texture roles
- **Location**: `byroredux/src/cell_loader/refr.rs:62-65` (field + its own contradicting doc), `:120` (`merge_from_texture_set` write), `:157,172` (`apply_slot_swap` write), `byroredux/src/cell_loader/spawn.rs:1139-1234` (`resolve_mesh_paths` — the only overlay consumer, no `inner` read)
- **Status**: NEW
- **Description**: `RefrTextureOverlay` carries an `inner` slot, filled from
  `TextureSet.inner` by the full-TXST merge and from `slot_index == 6` by the XTXR
  per-slot swap. `resolve_mesh_paths` is the sole place an overlay is folded into the
  canonical `MaterialTextureSet`, and it reads `diffuse`, `normal`, `glow`,
  `specular`, `height`, `env`, `env_mask`, `wrinkle`, `material_path` and
  `model_space_normals` — but never `inner`. `grep -rn 'o\.inner\|ov\.inner\|overlay\.inner' byroredux/src/`
  returns zero hits, and `.inner` appears nowhere in `cell_loader/` outside `refr.rs`.
- **Evidence**: The field's own doc comment asserts the opposite — *"Preserved for
  parity with `TextureSet.inner` so the slot_index=6 XTXR swap round-trips"* — and the
  round-trip does not happen. The sink exists and is fully live: `MaterialTextureSet`
  has an `inner_layer` role (`crates/nif/src/import/types.rs:322`), the NIF
  multi-layer-parallax path populates it, `map_secondary_texture_handles` resolves it
  to a bindless handle (`byroredux/src/asset_provider/texture.rs:444`), and it reaches
  `GpuMaterial.inner_layer_map_index` (`crates/renderer/src/vulkan/material.rs:304`). The compiler cannot
  flag the dead write because `#[derive(Debug, Default, Clone)]` on the struct
  (`refr.rs:51`) suppresses the `dead_code` field lint.
- **Impact**: A REFR that overrides its base mesh's inner/multi-layer-parallax
  texture — ice/glass panes, layered display cases, Skyrim SE and FO4 multi-layer
  content, and any Starfield REFR once a `.mat` overlay path exists — renders with the
  base mesh's inner layer, or none. Silent: no warn, no telemetry, and the
  regression-test file `refr_texture_overlay_tests.rs` never asserts on `inner`.
- **Related**: #2627 (the BGSM merge's sibling `inner_layer` gap — same role, the other
  producer; fixing one without the other still leaves the role unreachable from REFR
  overrides), #2594 (`fill_from_bgsm` role coverage).
- **Suggested Fix**: In `resolve_mesh_paths`, add
  `textures.inner_layer = resolve_to_owned(&pool, ov.and_then(|o| o.inner).or(mesh.material.textures.inner_layer));`
  next to the existing `wrinkle` fill, and extend
  `refr_texture_overlay_tests.rs` with a slot-6 XTXR round-trip assertion so the
  derive-suppressed dead write can't come back.

##### SF-2026-08-12-D9-02 — The REFR-overlay material resolver is a second, parallel external-material path that knows only `.bgsm`/`.bgem`, so Starfield `.mat` overlays resolve to nothing even after CDB Phase 2 lands
- **Severity**: LOW
- **Dimension**: 9 — external material flow
- **Location**: `byroredux/src/cell_loader/refr.rs:192-233` (`fill_from_bgsm`)
- **Status**: NEW
- **Description**: `fill_from_bgsm` dispatches on `path.ends_with(".bgsm")` /
  `".bgem")` and returns silently for anything else. Its own doc says "No-op when the
  path isn't a `.bgsm` / `.bgem`", so the omission is deliberate — but it means the
  engine has **two** external-material resolvers with divergent format coverage:
  `merge_external_material` (BGSM + BGEM + a `.mat` arm) and this one (BGSM + BGEM
  only). A Starfield REFR whose XATO/MSWP supplies a `.mat` path gets the path
  propagated into `ov.material_path` (and thence onto the spawned material) but no
  role fills, and there is no place for a future CDB lookup to hook in on this side.
- **Impact**: Zero today — Starfield content resolves no textures from either path
  (#2359), and `.mat` overlays on vanilla Starfield REFRs are rare. It becomes a real,
  silent per-REFR divergence the moment #2359 Phase 2 lands and the two resolvers
  disagree about what a `.mat` yields.
- **Related**: #2359, #2594, SF-2026-08-12-D9-01.
- **Suggested Fix**: Note the format gap in the doc comment now, and route both
  resolvers through one shared "resolve external material → roles" helper when Phase 2
  lands, rather than adding a second `.mat` arm here.

##### SF-2026-08-12-D9-03 — `merge_external_material`'s `bool` return cannot distinguish "resolved and populated" from "resolved and forwarded nothing", and all five production call sites discard it anyway
- **Severity**: LOW
- **Dimension**: 9 — external material flow
- **Location**: `byroredux/src/asset_provider/material.rs:667-739`; call sites `byroredux/src/cell_loader/references/import.rs:113`, `byroredux/src/cell_loader/partial.rs:115`, `byroredux/src/scene/nif_loader.rs:273`, `byroredux/src/cell_loader/precombined.rs:275`
- **Status**: NEW
- **Description**: The function documents `touched` as "flips to `true` on any merged
  field", but the `.mat` arm returns `true` after setting only `is_pbr` and forwarding
  no textures, scalars, or alpha state. Every production call site ignores the result
  (only the tests in `byroredux/src/asset_provider/tests.rs` assert on it), so there is no telemetry
  anywhere distinguishing "this cell's materials resolved" from "this cell's materials
  resolved to nothing" — which is precisely the state 97.9% of Starfield content is in.
- **Impact**: Diagnostics only, but it is the reason a total texture blackout (Dim 8)
  produces no log line and no counter. There is no `tex.missing`-style signal on the
  material side.
- **Suggested Fix**: Either mark the fn `#[must_use]` and have callers accumulate a
  per-cell "materials resolved / of which empty" counter, or return a small enum
  (`Unresolved` / `Merged { fields: usize }` / `PresenceOnly`) so the `.mat`
  presence-gate case is nameable.

---

## CRC32 Flag Table

`crates/nif/src/shader_flags.rs::bs_shader_crc32` carries the complete 32-entry
name to CRC32 table for the FO76+/Starfield shader-flag arrays
(`sf1_crcs` gated BSVER >= 132, `sf2_crcs` >= 152), pinned against
`docs/legacy/nif.xml` by `bs_shader_crc32_matches_nif_xml_literals`. It was
re-verified unchanged this pass and is reproduced in full in
`docs/audits/AUDIT_STARFIELD_2026-08-07.md`; nothing here is an opaque raw hash,
and no new hashes were observed in the 12,000-NIF corpus walk. The name-string
to hash derivation algorithm remains unknown, which is irrelevant to correctness
— matching Bethesda's wire literals is what matters, and nif.xml documents them.

## Remaining-Work Chain

Per `docs/engine/starfield-esm-roadmap.md`, ESM Phases 0+1 are done and Phases
2-4 are invalidated by the 99.9%-parity measurement. This is **not** a
"BGSM parser first / ESM very far" chain — both have shipped. Genuine remaining
work, in order:

1. **Per-field CDB material extraction (#2359 / #1289 Phase 2).** Now measured
   as a total texture blackout on 100% of Starfield content rather than a
   scalar-fidelity gap, and confirmed to have no NIF-side fallback. This is the
   single highest-value remaining Starfield item by a wide margin. Two findings
   in this report are its prerequisites: SF-2026-08-12-D3-01 (the CDB bytes must
   be cached, not re-inflated per cell, before anything parses them) and
   SF-2026-08-12-D8-01 (the fabricated `Some(...)` PBR pair must become `None`
   or Phase 2 cannot detect its own index misses).
2. **Exterior worldspace tiles** — Cydonia is an interior; not yet in scope.
3. **Space-cell / planet / GBFM records** — GBFM (3,141 leaf records measured
   this pass) / PNDT / STDT / BIOM remain conscious Phase-3 stubs, not defects.
4. **#2105 residual truncation tail** — 6 of 29,849 `MeshesPatch.ba2` files,
   each losing one block, all terrain-object LOD NIFs. Re-measured this pass:
   unchanged, not growing, root cause still unexplained.

## Deduplication Summary

All findings were checked against `/tmp/audit/issues.json` (258 issues) and the
twelve prior *docs/audits/AUDIT_STARFIELD_\*.md* reports plus
`docs/audits/SF_FIRST_RENDER_2026-05-28.md` before filing.

**Verified fixed, not re-filed** (all closed since 2026-08-07): #2613, #2614,
#2615, #2616, #2617, #2618, #2619, #2620, #2629-#2632, #2357.

**Confirmed still-open, premise re-verified against current code, not re-filed**:
#2359, #2360, #2361, #2362, #2621, #2622, #2624, #2626, #2627, #2628, #2633,
#2639, #2642, #2643, #2594, #2595, #2601, #2606, #2607, #2608, #2609, #2610,
#2533, #2330, #2105, #1576, #1568, #1567.

**Withdrawn during this audit** (premise disproved before filing): a suspected
Stage-A/Stage-B stranding in `extract_bs_geometry` when an internal sentinel slot
precedes an external real slot — impossible by construction, since
`BSGeometry::parse` applies one block-level `FLAG_INTERNAL_GEOM_DATA` bit to
every slot.

## Skill-file drift noted (not filed as findings)

- `/audit-starfield` SKILL.md Dimension 5 cites `IsCollisionOnly` in
  `byroredux/src/cell_loader/spawn.rs`; the symbol is no longer in that file
  after the #2355 collision rework (`8ee151e0` / `716b7ee9`).
- SKILL.md Dimensions 7 and the merge section cite "#746/#747" for the
  Meshes01/MeshesPatch truncation tail. Both are CLOSED and unrelated; the real
  tracker is **#2105**. (Also flagged in the 2026-08-07 report and still unfixed.)

## Finding Count Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 3 |
| LOW | 3 |
| **Total** | **6** |

**MEDIUM**: SF-2026-08-12-D3-01 (105 MB CDB inflate-and-discard per provider
build), SF-2026-08-12-D8-01 (fabricated PBR pair disables the NaN sentinel),
SF-2026-08-12-D9-01 (`RefrTextureOverlay::inner` write-only, inner-layer
overrides dropped)

**LOW**: SF-2026-08-12-D3-02 (stale `sf_cdbs` `Arc`-cache doc claims),
SF-2026-08-12-D9-02 (REFR-overlay resolver has no `.mat` arm),
SF-2026-08-12-D9-03 (`merge_external_material` return value cannot express
"resolved to nothing", discarded everywhere)

Per-dimension breakdown: Dim1 clean - Dim2 clean - Dim3 1M/1L - Dim4 clean -
Dim5 clean - Dim6 clean - Dim7 clean - Dim8 1M - Dim9 1M/2L.

---

Suggested next step: `/audit-publish` `docs/audits/AUDIT_STARFIELD_2026-08-12.md`
