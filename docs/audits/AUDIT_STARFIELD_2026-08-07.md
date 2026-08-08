# Starfield Compatibility Audit — 2026-08-07

## Executive Summary

Starfield is a first-class `GameKind` in ByroRedux: NIF parsing (BSVER 155+,
`BSGeometry` geometry path) at 99.99% aggregate clean-parse rate, BA2 v2/v3
archives (zlib + LZ4 block, GNRL + DX10) at 100% extract, CDB
(`materialsbeta.cdb`) + external BGSM/BGEM materials, ESM parsing at ~99.9%
record-parity, and a walkable Cydonia interior (cell resolve rate
91.2%/90.8% of REFRs on the two largest cells, unchanged from the Phase 0/1
baseline). This is a depth/correctness audit of that bring-up surface, not a
from-scratch gap inventory. All 9 dimensions ran with heavy real-data
validation this pass: an independent Python re-implementation of the BA2
reader over all 129 on-disk archives (Dimension 1), a line-by-line reader
cross-check against the Gibbed.Starfield reference (Dimension 3), live
`--sf-smoke` runs against the real 1.36 GB `Starfield.esm` (Dimension 4), and
an 87,994-NIF corpus walk with per-byte alignment scoring (Dimension 6).

**Overall verdict: the load/spawn/parse pipeline is sound, but this pass
surfaces the most significant Starfield rendering-correctness findings of
any audit to date — three independent HIGH bugs that each corrupt or hide a
large, distinct class of Starfield content, discovered by three different
dimension agents working independently, plus a HIGH-severity allocation-
safety gap in the CDB reader's live code path. 31 findings total: 5 HIGH,
10 MEDIUM, 16 LOW.**

### The four findings to read first

1. **Starfield skinned meshes render in bind pose (HIGH).** `extract_skin_bs_geometry`
   (`crates/nif/src/import/mesh/skin.rs:233-275`) hardcodes
   `vertex_bone_indices: Vec::new(), vertex_bone_weights: Vec::new()`, even
   though the parser already fully decodes per-vertex skin weights into
   `BSGeometryMeshData.skin_weights` (`crates/nif/src/blocks/bs_geometry.rs:283,479-495`,
   `BoneWeight { bone_index: u16, weight: u16 }`) — the data is sitting in
   scope at the call site and is simply never passed in. Every Starfield NPC,
   creature, and skinned apparel/armor mesh is affected (confirmed on two
   real production meshes — `naked_f.nif`, 6,616 verts/38 bones, and
   `femalehead_facebones.nif`, 15,370 verts/50 bones — both report
   `vbi_len=0 vbw_len=0` unconditionally). Independently found by **Dimension 2**
   and corroborated with production-scale evidence by **Dimension 7**; merged
   below as one finding. The stale #1827 tracker (CLOSED) and a stale test
   assertion (`crates/nif/src/import/mesh/bs_geometry_skin_tests.rs:118-121`,
   citing an already-resolved "#1203 deferred scope" rationale) both need to
   be revisited alongside the fix.

2. **`BSLightingShaderProperty` is misaligned by one 4-byte word on 100% of
   Starfield full-body blocks (HIGH).** Found independently by **Dimension 6**.
   `parse_fo76_plus` (`crates/nif/src/blocks/shader.rs:1142-1161`) skips the
   Starfield `shader_type` u32 and reads a `root_material_path` that
   Starfield doesn't carry — two 4-byte errors that cancel in total byte
   count, so drift histograms, `NiUnknown` counts, parse rate, and all three
   existing Starfield shader tests read green. Underneath, every field from
   `num_sf1` through `emissive_multiple` is read one word early. All 2,538
   inline-authored `BSLightingShaderProperty` blocks in the vanilla corpus
   emit a **NaN emissive colour**, a `texture_set_ref` that can never
   resolve, a UV transform with a **zero U-scale**, and a shader-flag CRC set
   that is wrong 57% of the time (invalid on 1,446/2,538 blocks) — corrupting
   decal/two-sided/PBR/vertex-colour classification downstream. Corpus-wide
   corrected-alignment validation (CRC membership, finite non-negative
   emissive, resolvable texture-set ref) scores **0/2,538 under the shipped
   alignment, 2,538/2,538 under the corrected one.**

3. **Every externally-referenced Starfield effect shader renders fully
   invisible (HIGH).** Found by **Dimension 8**. The #2353 material-reference-
   stub guard was added to `BSLightingShaderProperty`'s walker
   (`crates/nif/src/import/material/dedicated_shader.rs:85-88`) but never to
   its `BSEffectShaderProperty` sibling (`apply_bs_effect_shader`,
   `dedicated_shader.rs:365-500`). On Starfield the stub discriminator is
   "name is non-empty," which is the *dominant* authoring path (materials
   live in `materialsbeta.cdb`, referenced by name) — so nearly every
   Starfield effect-shader surface gets the placeholder's
   `falloff_start_opacity = falloff_stop_opacity = 0.0` copied in instead of
   the correct 1.0 identity default. `triangle.frag`'s cone-fade math
   degenerates (`denom == 0` because `falloff_start_angle == falloff_stop_angle == 1.0`
   in the stub too) and `finalAlpha = texColor.a * coneFade` evaluates to
   **0.0** — the surface is fully transparent, with no visual signal that
   anything is wrong.

4. **The CDB allocation-safety pair (HIGH + latent MEDIUM).** Found by
   **Dimension 3**, cross-checked line-by-line against the Gibbed.Starfield
   reference. `index_chunks` (`crates/sfmaterial/src/reader.rs:172-179`)
   pre-reserves a `VecDeque` sized directly from an unvalidated on-disk `u32`
   chunk count — a truncated/corrupted `materials\**\materialsbeta.cdb`
   (mod-shipped, partial download, bit rot) can request a ~103 GB allocation,
   which `abort()`s the process rather than returning the `Err` the
   surrounding code is written to handle. This is **on the live path today**:
   `register_starfield_cdb` calls exactly `parse_header()` + `index_chunks()`
   at every cell load. A structurally identical bug in `LIST`/`MAPC` element
   counts (`reader.rs:372-373,389-390`, a negative `i32` sign-extending to a
   ~1.8e19 request) is currently unreachable — `ComponentDatabaseFile::parse`
   is not called in production yet — but is scored MEDIUM-escalating-to-HIGH
   because it activates the moment #2359/#1289 Phase 2 per-field CDB
   extraction lands, so it is cheaper to fix in the same patch as the live
   bug. Dimension 3 also found a third, independent HIGH: `Archive::open`
   (`byroredux/src/asset_provider/archive.rs:10-27`) reads an **entire**
   multi-GB archive into RAM just to sample 4 magic bytes, and Starfield CDB
   discovery (`build_material_provider`) re-opens every `--bsa` path a
   *second* time purely for this — despite its own comment claiming
   otherwise.

None of these four issues were caught by existing tests, drift telemetry, or
parse-rate metrics — each is a case of the byte-accounting staying correct
while the *semantic* content is wrong, which is exactly the failure mode
this audit exists to catch.

### Everything else, in one paragraph

The remaining 27 findings are real but narrower: two MEDIUM-severity BA2
robustness gaps (a silently-truncating LZ4 under-run with a factually wrong
justifying comment; three DXGI formats — chargen normal maps and interior
cubemaps — that fail to load, 78 records / 0.06% of the corpus); three
MEDIUM findings on the Starfield shader tail (a genuinely-undocumented 30
remaining bytes after `BSSPLuminanceParams` is recovered; tautological test
fixtures that mirror the parser's own field order and could never have
caught finding #2; opaque-tail capture that disables the drift telemetry
that would otherwise have raised finding #2 for free); two MEDIUM BGSM/BGEM
classification gaps (the BGEM `refraction` bit over-firing as a glass signal
ahead of its version-gated guards; a populated `inner_layer` texture role
that BGSM's merge never wires up); a MEDIUM CDB-discovery gap where the
material provider's `--bsa` archive scan misses the numeric-sibling
expansion the texture provider already performs, silently dropping
DLC/Creation CDBs one layer above the #1571 fix; and a long tail of LOW
diagnostic/doc-staleness/test-coverage items. Two collision-related commits
(`8ee151e0`, `716b7ee9`) landed on this exact bring-up surface *the same day*
as this audit and were traced end-to-end by Dimension 5 as a correct,
fully-tested fix for #2355 (Starfield's packed-Havok collision gap) — not a
finding, a confirmed win.

## Dimension Findings

### Dimension 1 — BA2 v2/v3 LZ4 Block Decompression

Scope: `crates/bsa/src/ba2.rs` (1,470 lines). Verified with an independent
Python re-implementation over all 129 on-disk Starfield archives (vanilla +
Shattered Space + installed DLC/mods): 129/129 open, 4,519/4,519 sampled
extracts OK, 137,383 DX10 chunks walked with 0 anomalies, 2,822 LZ4 chunks
independently re-decoded with 0 mismatches, 3,830 DX10 payloads byte-exact
against their synthesized headers.

**Confirmed correct, no regression:** v2 (8-byte)/v3 (12-byte) header offset
math, verified independently on all 129 archives; `Ba2Compression` dispatch
(0=zlib/3=LZ4/other=hard `InvalidData`, not silent fallback); `lz4_flex`'s
`max_size` supplied from the chunk's own `unpacked_size` (required — BA2 LZ4
is raw block, not frame); per-chunk raw-vs-compressed selection via
`packed_size == 0` (not an equality comparison — confirmed correct against
137,383 real chunks where **zero** compressed chunks have
`packed_size == unpacked_size`); GNRL + DX10 both funnel into one
`decompress_chunk`; DX10 chunk struct layout unchanged since FO4 v1 (the v3
fix really is header-offset-only); the cubemap flag bit (`0x1`, not `0x800`)
holds on the full Starfield cubemap set. **#2356/SF-BA2-01 (DX10 chunk-sum
cap) is CLOSED and the fix is verified present** (`checked_chunk_total`,
`ba2.rs:624-633`).

> **Datapoint correction**: `AUDIT_STARFIELD_2026-08-03.md` states "13 v3 LZ4
> DX10 texture archives"; the current on-disk count is **15**
> (`Textures01..11`, `TexturesPatch01/02`, plus the two `LODTextures01/02`
> the prior count missed). Not a code defect.

#### SF-D1-01: LZ4 branch silently truncates on an under-run, and the in-code comment claims the opposite
- **Severity**: MEDIUM
- **Location**: `crates/bsa/src/ba2.rs:738-746` (LZ4 arm), `:712-735` (the misleading comment)
- **Status**: NEW — partial overlap with **#2097 (LZ4-01, OPEN, LOW)**, opposite failure direction, different fix
- **Description**: `lz4_flex::block::decompress(packed, unpacked_size)` allocates the declared size, decodes, then `truncate`s to the actual decoded length and returns `Ok` — so a record that declares *more* than the stream contains gets a silent short buffer, no error, no log. The zlib arm handles the identical condition with `log::warn!` (#812); the comment claiming the LZ4 branch "hard-errors on the same condition" is factually wrong — `lz4_flex` only hard-errors in the *other* direction (declared < actual).
- **Evidence**: measured against the pinned `lz4_flex 0.11.6`: under-run → `Ok(len=13)` for a declared 4096, no error; over-run → hard `Err`. Vanilla corpus is clean (0/2,822 sampled chunks), so this is a robustness gap on malformed/mod-repacked archives, not an active bug.
- **Impact**: LZ4 is the only codec for all 15 Starfield v3 texture archives; a DX10 texture is a concatenation of per-mip chunks, so a short decode on a non-final chunk shifts every subsequent mip, and the synthesized DDS header then misdescribes its own payload — garbled/offset mip data in the renderer with no error signal.
- **Related**: #2097, #812, #2360.
- **Suggested Fix**: Compare `out.len()` against `unpacked_size` post-decode in the LZ4 arm and `log::warn!` (or hard-error for chunk chains, where a short mid-chain chunk is unrecoverable). Fix the comment. Add an under-run unit test.

#### SF-D1-02: `pitch_or_linear_size_for` has no arm for DXGI 10/11/31 — 78 vanilla textures get an invalid `dwPitchOrLinearSize`
- **Severity**: LOW
- **Location**: `crates/bsa/src/ba2.rs:952-1002`
- **Status**: NEW — same defect class as #594/FO4-DIM2-03 (CLOSED), on formats that fix never enumerated
- **Description/Evidence**: `dxgi_format` histogram across all 137,383 DX10 records shows fmt 31 (`R8G8B8A8_SNORM`, 63 records — 62 chargen face normal maps), fmt 10 (`R16G16B16A16_FLOAT`, 13 records — 12 cubemaps + the LTC area-light LUT), fmt 11 (`R16G16B16A16_UNORM`, 2 records — gas-giant gradient textures) all fall to the legacy `(total_bytes, DDSD_LINEARSIZE)` branch instead of the correct `DDSD_PITCH` form.
- **Impact**: Invisible in-engine (the DX10 extended header is read, not the legacy field), so the blast radius is external tooling / texture dumps — same standard #594 was fixed under.
- **Related**: #594 (CLOSED), SF-D1-03 (same records, renderer side).
- **Suggested Fix**: Add `10 | 11 => Some(8)`, `31 => Some(4)` to the `bpp` match with matching tests.

#### SF-D1-03: The same 78 records hard-fail the renderer's `map_dxgi_format` — every Starfield interior cubemap and chargen face normal map falls back to the placeholder texture
- **Severity**: MEDIUM
- **Location**: `crates/renderer/src/vulkan/dds.rs:508-552` (`map_dxgi_format`)
- **Status**: NEW
- **Description**: BA2 extraction of these 78 textures is byte-exact correct; the renderer's DXGI table simply has no arm for 10/11/31 and bails at parse time.
- **Evidence**: the same 78-record set as SF-D1-02 — 12 interior ambient/reflection-probe cubemaps (`cell_cavecube`, `cell_shipinteriorcube`, …) + the LTC LUT + 62 chargen head normal maps + 2 gas-giant gradients.
- **Impact**: Missing textures, not a crash — but per the project's own "chrome/posterized ⇒ missing textures" diagnosis rule, this is exactly the defect class that costs hours downstream, concentrated on interior ambient lighting and every chargen face.
- **Related**: SF-D1-02, `feedback_chrome_means_missing_textures`.
- **Suggested Fix**: Add core-Vulkan-1.0 format arms for DXGI 10/11/31 with matching tests.

#### SF-D1-04: `log_v2_v3_extra_bytes` documents a "compressed name-table size" field that is always the constant `1` on every real archive — the malformed-header heuristic built on it is dead code
- **Severity**: LOW
- **Location**: `crates/bsa/src/ba2.rs:431-474`
- **Status**: NEW — sibling of #2360 (SF-BA2-02, OPEN, LOW), different defect in the same helper
- **Description/Evidence**: All 129 archives have `hdr[24..32] == 0100000000000000` byte-identical. A value of 1 is not a size; the `stream_pos + size > name_table_offset` malformed-header branch derived from reading it as one can never fire on real data.
- **Impact**: Documentation/diagnostic only.
- **Suggested Fix**: Rename to `unknown_1`/`unknown_2` (or `name_table_format`), recording the observed constant; drop or replace the dead heuristic.

#### SF-D1-05: No test covers the v3-zlib path, the LZ4 under-run, or a real-data-derived header fixture
- **Severity**: LOW
- **Location**: `crates/bsa/src/ba2.rs:1009-1470` (`mod tests`)
- **Status**: NEW
- **Description**: `compression_method == 0` on a v3 archive (v3+zlib) has zero test coverage and does not occur in vanilla; no under-run test exists (SF-D1-01); the header-offset tests build their fixture to mirror the parser's own layout assumption, so a wrong offset would move in lockstep with the bug rather than being caught.
- **Impact**: A future header-layout edit could pass the whole suite while breaking every v3 archive, surfacing only on a manual run against game data.
- **Suggested Fix**: Synthesize a v3+method-0 fixture; add the LZ4 under-run test; add a byte-literal fixture built from the documented v3 header layout with post-parse content assertions.

**Existing, reconfirmed and not re-filed**: #2097 (LZ4-01), #2360 (v3 log stream position), #1761 (`Dx10Chunk::start_mip` stale `#[allow(dead_code)]`).

---

### Dimension 2 — BSGeometry Mesh Extraction

Scope: `crates/nif/src/import/mesh/{bs_geometry,skin,tangent}.rs`,
`crates/nif/src/blocks/bs_geometry.rs`,
`byroredux/src/asset_provider/{archive,texture,tests}.rs`.

**Confirmed correct, no regression:** #1292 canonical `geometries\<X>.mesh`
path preservation; #1209 full-LOD-slot iteration (no `.first()`
short-circuit in either stage); #1828/#1829 sentinel-slot skip in both
Stage A and Stage B, with all four permutations regression-tested; #1203
skin *bone* chain (bind inverses + bone bounding spheres) resolved
correctly — the per-vertex half is the finding below; #1232 tangent
synthesis correctly gated on the `normals_authored` flag, not the
always-populated fallback vector; downstream length-mismatch safety
(`i < vec.len()` guards + `MeshRegistry::sanitize_scene_indices` hard-clamp
before both raster upload and BLAS build — no OOB/AS-corruption path found);
`metalness_override`/`roughness_override` forwarding runs through the same
shared boundary as every other extractor, no Starfield bypass.

#### SF2D2-D2-01 / merged with Dimension 7 — HIGH — Starfield skinned-mesh per-vertex bone weights are fully decoded and thrown away; every skinned mesh renders in bind pose
- **Severity**: HIGH
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:249-260` (call site — `mesh_data` is live but not passed), `crates/nif/src/import/mesh/skin.rs:233-275` (`extract_skin_bs_geometry`, canonical fix site), `crates/nif/src/blocks/bs_geometry.rs:283,479-495` (`BSGeometryMeshData.skin_weights: Vec<Vec<BoneWeight>>`, fully decoded)
- **Status**: NEW — #1827 (CLOSED) carries the same, now-incorrect, premise ("the packed BSGeometry vertex bone channel is not decoded") and sizes the remaining work as a separate milestone; it is materially smaller — a plumbing change, not a decode change. No open tracker.
- **Description**: `extract_skin_bs_geometry` hardcodes
  `vertex_bone_indices: Vec::new(), vertex_bone_weights: Vec::new()`. Its
  own comment ("the BSGeometry parser doesn't surface them yet") is
  factually wrong: `BSGeometryMeshData.skin_weights` is fully decoded at
  parse time via `read_pod_vec::<BoneWeight>`, grouped by
  `weights_per_vert`, and `BoneWeight { bone_index: u16, weight: u16 /* NORM/65535 */ }`
  already indexes the same `BsSkinInstance.bone_refs` array
  `extract_skin_bs_geometry` walks to build `ImportedSkin.bones`. A
  repo-wide grep shows zero consumers of `skin_weights` outside the parser
  and its own unit test — the data is decoded, unit-tested, and unused.
- **Evidence**: Dimension 2 confirmed the gap at the source (the call site
  has `mesh_data` in scope and simply doesn't pass it). **Dimension 7
  independently corroborated with production-scale real-data evidence**,
  tracing two real vanilla meshes end-to-end through `import_nif_scene`:
  `naked_f.nif` (6,616 verts, 38 bones) and `femalehead_facebones.nif`
  (15,370 verts, 50 bones) both report `has_skin=true` with correctly
  resolved bones/bind matrices but `vbi_len=0 vbw_len=0 vbw_nonzero=0`
  unconditionally regardless of vertex count. Dimension 7 additionally
  flagged that the existing regression test
  (`crates/nif/src/import/mesh/bs_geometry_skin_tests.rs:118-121`) *asserts
  the empty arrays as correct*, citing a stale "#1203 deferred scope"
  rationale that was already resolved by the time `skin_weights` decoding
  landed — the gap has gone silent rather than failing loud.
- **Impact**: Every Starfield skinned mesh — all NPCs, all creatures, all
  skinned armor/apparel, all FaceMeshes content (`Starfield -
  FaceMeshes.ba2` is 14.27% `BSGeometry` blocks) — renders in bind pose.
  `nif_loader.rs`'s `.filter(|s| !s.vertex_bone_indices.is_empty() && ...)`
  silently drops every vertex to the rigid path. This is a
  rendering-correctness defect on the largest animated-content class in the
  game, confirmed on two independent real production meshes, not a
  synthetic-fixture artifact.
- **Related**: #1827 (CLOSED, stale premise), the FO4 `BsTriShape` path
  (already implements top-4-by-weight + renormalize for the same contract —
  reuse, don't re-derive).
- **Suggested Fix**: Change `extract_skin_bs_geometry`'s signature to accept
  `mesh_data`; when `weights_per_vert > 0 && !skin_weights.is_empty()`, map
  each row to `[u16; 4]` indices + `[f32; 4]` weights (top-4-by-weight when
  `> 4`, zero-pad when `< 4`, `weight as f32 / 65535.0`, renormalize through
  the existing `crates/nif/src/blocks/tri_shape/mod.rs::renormalize_skin_weights`
  helper shared with FO4). Guard on `skin_weights.len() == vertices.len()`,
  fall back to bind-pose on mismatch. Update the stale test at
  `bs_geometry_skin_tests.rs:118-121` to assert the new non-empty behavior.

#### SF2D2-D2-02 — MEDIUM — `weights_per_vert == 0` with a non-zero `n_total_weights` consumes zero bytes, drifting the rest of the `.mesh` parse
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/blocks/bs_geometry.rs:479-495`
- **Status**: NEW
- **Description**: `n_total_weights.checked_div(weights_per_vert)` returns `None` only for `weights_per_vert == 0`, and that arm reads **zero bytes** regardless of `n_total_weights`. If a `.mesh` body ever ships `weights_per_vert == 0` with `n_total_weights > 0`, the undrained `BoneWeight` payload shifts every subsequent field (`n_lods`/`n_meshlets`/`n_cull_data`) into garbage, driving `read_u16_triple_array` off a corruption-controlled count.
- **Impact**: Parse-position drift on malformed/atypical `.mesh` bodies (per the severity table, MEDIUM for "stream position off"). Bounded by `check_alloc` (no OOM/UB), but the mesh silently loses its LOD/meshlet/cull tables or fails, surfacing only as "REFR spawned with zero meshes" with no diagnostic — Stage B's error arm is `log::debug!`-only.
- **Related**: the deliberate remainder case (`n_total_weights % weights_per_vert != 0`) is correctly pinned by `skin_weights_bulk_read_matches_per_element_semantics`; that test does not cover the `== 0` arm.
- **Suggested Fix**: Treat `weights_per_vert == 0` as "skip the payload, still advance the cursor" (`stream.skip(n_total_weights * 4)`), not "read nothing." Add a unit test with `weights_per_vert = 0`, `n_total_weights = 2`, a non-zero `n_lods` following.

#### SF2D2-D2-03 — LOW — `BSGeometryMeshData.lods`/`meshlets`/`cull_data` and the slot's own LOD index are decoded and dropped by the importer
- **Severity**: LOW
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:140-144,325`, `crates/nif/src/blocks/bs_geometry.rs:107-112`
- **Status**: NEW
- **Description**: Three signals are parsed and discarded: (1) `mesh_data.lods` — full reduced triangle lists per LOD level (importer reads only LOD 0); (2) `meshlets`/`cull_data` — cluster-culling primitives; (3) the slot loop index itself is lost at parse time (`BSGeometry::parse`'s `for _ in 0..4` loop discards its own counter), so `meshes[0]` is the first *present* slot, not necessarily LOD 0 — combined with the sentinel-slot skip, a future LOD selector has no way to know which level it actually loaded.
- **Impact**: No LOD switching possible for Starfield content today (missing-feature, nothing renders wrong) — but item (3) is cheap now, expensive to retrofit later.
- **Suggested Fix**: Store the loop index as `BSGeometryMesh.lod_slot: u32` at parse time and carry it into `ImportedMesh` alongside `bs_lod_cutoffs`. `lods`/`meshlets` consumption itself is fine as EXAL follow-up work.

#### SF2D2-D2-04 — LOW — UDEC3-decoded normals feed unnormalized into `synthesize_tangents_yup`'s Gram-Schmidt, which assumes unit N
- **Severity**: LOW
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:150-162,217`, `crates/nif/src/blocks/bs_geometry.rs:569-580`, `crates/nif/src/import/mesh/tangent.rs:442-505`
- **Status**: NEW
- **Description**: `unpack_udec3_xyzw`'s raw remap has no normalization (unit-length only to 10-bit quantization); the Gram-Schmidt projection is only correct for `|n| == 1`, and the degenerate fallback branch (`t_y = [n[1], n[2], n[0]]`) is neither normalized nor orthogonalized against `n`.
- **Impact**: Quantization error (~0.1%) is visually negligible on the non-degenerate path (shader renormalizes); the degenerate branch's non-orthogonality is a pre-existing shared divergence (AUDIT_INCREMENTAL_2026-05-22 ID-4), sub-pixel in practice.
- **Suggested Fix**: `normalize_inplace` the copy fed to `synthesize_tangents_yup`; orthogonalize the degenerate `t_y` against `n` with Gram-Schmidt + normalize before the cross product.

**Existing, not re-filed**: #2361 (SF2D2-04, non-idempotent `.mesh` path composition), #2357 (SF2D2-03, silent external-`.mesh` resolve failure), #2362 (SF2D2-05, resolver-less call sites), #2098/#2099/#1830 (bounding-sphere/uvs1/hint-mismatch diagnostics — present and matching).

---

### Dimension 3 — CDB Material Database Correctness

Scope: `crates/sfmaterial/src/{reader,chunk,string_table,types,value}.rs`,
`byroredux/src/asset_provider/material.rs`. Cross-checked line-by-line
against `Gibbed.Starfield.FileFormats/ComponentDatabaseFile.cs`.

**Confirmed correct, no regression:** unknown `ChunkType`/`BuiltinType`/`ClassFlags`
are deliberately hard errors, not warn-and-skip — correct for a flat
chunk stream with no per-record length prefix, where skipping would
desynchronise the cursor for the remaining ~1.44M entries (each recognised
set is baseline-pinned by test); #762's chunk-overflow guard is intact
(just preceded by the unvalidated reserve — see SF-D3-01); `peek_magic`
correctly distinguishes CDB from loose BGSM/BGEM; #1571's multi-CDB
discovery predicate (`materials\**\materialsbeta.cdb`, covering DLC/Creation
namespaces) is genuinely fixed — the residual gap is at archive selection,
not path matching (SF-D3-04); header/chunk-table/class/object/list/map/diff
structural parity with the Gibbed reference holds field-for-field.

#### SF-D3-01: `index_chunks` pre-reserves from an unvalidated on-disk `u32` chunk count — a truncated/corrupt CDB aborts the process before any bounds check runs
- **Severity**: HIGH
- **Location**: `crates/sfmaterial/src/reader.rs:172-179`
- **Status**: NEW
- **Description**: `chunk_count = (chunk_count_incl_beth - 1) as usize; VecDeque::with_capacity(chunk_count)` — a `0xFFFF_FFFF` read requests ~103 GB *before* the per-chunk `ChunkOverflow` guard at line 186 ever runs. `with_capacity` on a request this large panics with "capacity overflow" or calls `handle_alloc_error` → `abort()` — not catchable by the caller.
- **Evidence**: **On the live path today** — `register_starfield_cdb` (`material.rs:344-376`) is wired to call exactly `probe_header()` = `parse_header()` + `index_chunks()`. Any file at `materials\**\materialsbeta.cdb` inside a loaded BA2 starting with `BETH` (mod-shipped, partially-downloaded, or bit-rotted) kills the engine at cell-load with no log line, defeating the "malformed payload is warned and dropped" contract the surrounding code documents and `discovered_cdbs_accumulate_in_load_order` tests for — that test only exercises `b"not a cdb"`, rejected earlier by `peek_magic`, so the reserve path is untested. Gibbed's reference has no equivalent exposure (a `Queue<Chunk>` with no pre-reserve fails on the first stream read past EOF instead).
- **Impact**: Corrupt/truncated CDB → process abort instead of `Err`, on the live cell-load path.
- **Related**: SF-D3-02 (same class, latent).
- **Suggested Fix**: `chunks.reserve(chunk_count.min(self.bytes.len() / 8))` (each chunk costs ≥8 bytes of header) or drop `with_capacity` entirely, matching the reference. The existing EOF/`ChunkOverflow` checks then produce a proper `Err`.

#### SF-D3-02: `LIST`/`MAPC` element counts are read as `i32` and cast to `usize` — a negative count becomes a ~1.8e19 allocation panic where Gibbed yields an empty collection
- **Severity**: MEDIUM (escalates to HIGH once #2359/#1289 Phase 2 starts calling `parse` on real archives)
- **Location**: `crates/sfmaterial/src/reader.rs:372-373,389-390`
- **Status**: NEW
- **Description**: `let count = cur.read_i32()? as usize;` sign-extends a negative count to ~1.8e19; `Vec::with_capacity` panics. Even a plausible positive corruption (`count = 100_000_000`) reserves ~5.6 GB before a single element is read, with no bound against `payload.len()` (which is known and tightly bounds the count — every element consumes ≥1 byte). Gibbed's `ConsumeList`/`ConsumeMap` use no reserve and a bounded `for` loop, so a negative count there just produces an empty collection.
- **Impact**: `ComponentDatabaseFile::parse` — the only caller that reaches `consume_list`/`consume_map` — is not invoked in production today (Phase 1 stops at `probe_header`), hence MEDIUM not HIGH. This is the second half of "the CDB allocation-safety pair" flagged in the executive summary — same root cause as SF-D3-01, cheaper to fix in the same patch.
- **Related**: SF-D3-01.
- **Suggested Fix**: `usize::try_from(cur.read_i32()?).map_err(...)?` plus `Vec::with_capacity(count.min(payload.len()))`.

#### SF-D3-03: `Archive::open` reads the entire archive file into RAM to sample 4 magic bytes; CDB discovery pays that cost a second time on every mesh archive
- **Severity**: HIGH
- **Location**: `byroredux/src/asset_provider/archive.rs:10-27`, `byroredux/src/asset_provider/material.rs:194-205` (`build_material_provider`)
- **Status**: NEW
- **Description**: `std::fs::read(path)` allocates and fills a `Vec<u8>` the size of the whole archive to extract 4 magic bytes. `Starfield - Meshes01.ba2`/`Meshes02.ba2` are multi-GB; `Starfield - Materials.ba2` carries the ~105 MB CDB. `build_material_provider`'s comment says the archive is "re-opened here purely to read its file table (the entry data isn't touched)" — but `Archive::open` reads all the entry data anyway, so each mesh archive is fully read twice per provider build, from six call sites (`app_step.rs:462,523`, `scene.rs:355,395`, `scene/nif_loader.rs:54`, `save_io.rs:851`, `debug_load.rs:125`), several re-running on save-load/debug-load.
- **Impact**: On a `--bsa`-heavy Starfield invocation, several GB transiently allocated per archive and page cache thrashed before a single byte of the file table is parsed.
- **Suggested Fix**: Replace the `fs::read` sniff with `let mut m = [0u8; 4]; File::open(path)?.read_exact(&mut m)?;` — one-line-scoped, no behavioral change, and makes the "purely to read the file table" comment true.

#### SF-D3-04: CDB discovery on `--bsa` skips numeric-sibling archives, missing exactly the split-archive layout `numeric_sibling_paths` was written for Starfield
- **Severity**: MEDIUM
- **Location**: `byroredux/src/asset_provider/material.rs:194-205` vs `byroredux/src/asset_provider/texture.rs:166-172`
- **Status**: NEW
- **Description**: The texture provider opens mesh archives via `open_with_numeric_siblings` (`Foo01.ba2` → `Foo02..09.ba2`, added specifically for Starfield's zero-padded series). The material provider's `--bsa` arm calls bare `Archive::open(path)` with no sibling expansion, so an invocation naming only `Meshes01.ba2` gets `Meshes02…09` auto-loaded for meshes/textures but **never scanned** for `materials\creations\<plugin>\materialsbeta.cdb`.
- **Impact**: This is precisely #1571's original failure mode ("a missed DLC CDB") reappearing one level up, at archive selection rather than path selection. Silent — `sf_cdb_count` just stays lower, and if no CDB is found at all, every `.mat` mesh in the cell falls through to NIF-default rendering.
- **Related**: #1571 (CLOSED — the path-matching half; this is the archive-selection half).
- **Suggested Fix**: Route the material provider's `--bsa` arm through the same `open_with_numeric_siblings`, into a scratch `Vec<Archive>` scanned and dropped. Also LOW, same site: a loose `Data\materials\materialsbeta.cdb` (the natural mod-override shape) is never discovered at all — not a regression, worth documenting.

#### SF-D3-05: Duplicate field names silently last-wins where the reference hard-fails
- **Severity**: LOW
- **Location**: `crates/sfmaterial/src/reader.rs:444,453,473,493`
- **Status**: NEW
- **Description**: Field values accumulate via `BTreeMap::insert` (silent overwrite); Gibbed uses `Dictionary.Add` (throws on duplicate key). A `CLAS` declaring the same field name twice, or a `DIFF` naming the same field index twice, silently keeps the second value — worst-case outcome for a Phase 2 material index (silent wrong value, not a parse error).
- **Suggested Fix**: `debug_assert!(insert(...).is_none())` or a real `Err`.

#### SF-D3-06: `StringTable::get` doc comment contradicts its own (correct) offset-0 behaviour
- **Severity**: LOW
- **Location**: `crates/sfmaterial/src/string_table.rs:26-28`
- **Status**: NEW
- **Description**: The comment claims empty string at `offset == 0`; the code correctly reads the NUL-terminated string *at* offset 0 (matching Gibbed), and a synthetic fixture (`reader.rs:813`) depends on that being right. The comment describes a behaviour that would break class-name resolution if "fixed."
- **Suggested Fix**: Delete the clause.

**Also LOW (not separately numbered)**: `Value::Ref` wraps its referent in `Value::Ref { type_ref, inner }`; Gibbed's `ReadPrimitiveRef` returns the inner value directly. A strict superset, but a future Phase 2 walker ported from the reference will miss one unwrap level — worth one sentence in the doc comment.

**Already tracked, not re-reported**: per-`.mat` field forwarding (#2359/#1289 Phase 2) — confirmed the `.mat` merge arm still sets `is_pbr = true` and returns immediately, forwarding zero roughness/metalness/texture/alpha/emissive data from the CDB.

---

### Dimension 4 — Starfield ESM Resolve-Rate Baseline

Scope: `byroredux/src/sf_smoke.rs`, `crates/plugin/examples/{sf_smoke,sf_parse_check}.rs`.
Live runs against the real 1.36 GB `Starfield.esm`, cross-checked with two
independent Python re-walks of the raw ESM byte stream.

**Baseline verdict: no regression.** `--sf-smoke citycydoniamainlevel`
resolves 25,437/27,898 REFRs (91.2%), `citycydoniamainlevel02` resolves
8,784/9,679 (90.8%) — two independent large cells landing within 0.4pp of
each other. `sf_smoke --recurse` top-level GRUP byte coverage (86.1%) and
`sf_parse_check` interior REFR capture (1,971,135) both match the Phase 0/1
baseline docs exactly. #1567 (LIGH `DAT2`) holds at 656/656 (100%). #1576
(model-less STAT/BNDS/ACTI/ARMO) is unchanged at the issue's own evidence
counts. #1568 (PDCL) is intact — 1,846 REFRs still route through the named
warned-once skip, not the anonymous catch-all. `SOUN`/`ASPC` are correctly,
intentionally excluded from `statics` (typed-captured elsewhere, no `MODL`
by design).

#### SF-D4-05: `SECH`/`AOPF` have zero dispatch — no typed capture, no skip telemetry, unlike every sibling audio-metadata type
- **Severity**: LOW
- **Location**: `crates/plugin/src/esm/records/mod.rs:423-451`
- **Status**: NEW
- **Description**: `SOUN`/`ASPC` are captured via `dispatch_misc_stub_group` into typed `EsmIndex` collections; `SECH` (`BGSSoundEcho`) and `AOPF` (`BGSAudioOcclusionPrimitive`) are absent from every match arm and fall to the bare `_ => skip_group` catch-all — no counter, no `skipped_unconsumed_groups` entry.
- **Evidence**: `citycydoniamainlevel` alone has 190 `SECH` + 30 `AOPF` REFRs (0.8% of the cell). Both are genuine Starfield-era FourCCs (Gibbed `FormType.cs` confirms), not garbage.
- **Impact**: No visible-content loss (neither type has a mesh) — purely diagnostic: a future FourCC repurposing or content patch would be invisible.
- **Related**: #1568 (the precedent this should follow).
- **Suggested Fix**: Add `b"SECH" | b"AOPF"` to the `dispatch_misc_stub_group` arm alongside `SOUN`/`ASPC`.

#### SF-D4-06: `sf_smoke`'s "unresolved" report conflates by-design exclusions with real gaps, overstating the headline number ~5×
- **Severity**: LOW
- **Location**: `byroredux/src/sf_smoke.rs:154-176`
- **Status**: NEW
- **Description**: The tool's hint text ("parser gap — schema diverged or record type missing") applies uniformly, but this run's 2,461 unresolved `citycydoniamainlevel` REFRs decompose into ~140 (0.5%) real #1576 gap, 1,846 (6.6%) intentionally-unconsumed PDCL, and ~369 (1.3%) by-design-excluded audio markers.
- **Impact**: Purely a diagnostic-tool clarity gap — but it's exactly the failure mode the tool exists to catch: a real regression (PDCL doubling, or the BFCB gap widening) would today hide inside the same undifferentiated bucket as two already-understood causes.
- **Related**: SF-D4-05, #1576, #1568.
- **Suggested Fix**: Thread the FourCC through the existing skip telemetry into a per-type unresolved-REFR counter; separate known-tracked buckets from the residual "unattributed" count in the report.

---

### Dimension 5 — ESM + Cell Bring-up Regression Surface

Scope: `crates/plugin/src/esm/{reader,records/mod,cell/walkers}.rs`,
`byroredux/src/cell_loader/spawn.rs`.

**Confirmed correct, no regression:** HEDR-0.96 → `GameKind::Starfield`
tolerance-band classification (35/35 pinned tests pass); FourCC dispatch
coverage cross-checked against Dimension 4's baseline; `XCLL_SIZES_STARFIELD = [28, 108]`
correctly a distinct bucket sharing only bytes 0-27 with every other game
(not "Skyrim + 16-byte tail," per the #1293 correction), gated `>= 108` not
`== 108`; per-cell NAVM collection (#1272); #1294 static-trimesh fallback
correctly gated on `base_layer`, not the post-escalation `final_layer`;
#1235 `SceneFlags::from_nif`, #1295 `DoorTeleport`, #1212/#1213/#1214
`FormIdComponent`/`LocalBound`/`BSXFlags` all attach at spawn as documented;
#1284 `SkinSlotPool` ceiling (196,608 total / 144 per-mesh → 1,364 skinned
slots) structurally confirmed wired.

**Headline (not a finding, a confirmed fix landing on this exact dimension's
watch):** two collision commits (`8ee151e0`, `716b7ee9`) landed the same day
as this audit, implementing `CollisionAuthoringSummary` /
`summarize_collision_authoring` and a layer-aware
`missing_collision_fallback()` — the fix for **#2355** ("NIFAL collision
slice never fires on Starfield — all colliders route to undecoded
`BhkSystemBinary`"). Traced end-to-end: exhaustive over `RenderLayer`'s 4
variants (pinned by `collision_authoring_selects_packed_proxy_only_for_safe_layers`),
correctly scoped per-NIF-file (a mesh's authored collision kind is a file
property, shared across every REFR placement), spawns a conservative
`Keyframed` AABB proxy for Clutter/Actor content with no decoded shape,
parents it to the placement root, and carries distinguishing telemetry for
"approximated" vs. "no safe render geometry to approximate from." A sibling
audit (`AUDIT_LEGACY_COMPAT_2026-08-07.md`, Dimension 4/PHYSAL) independently
reached the same conclusion. Full `cargo test` sweep of touched crates: all
green, no regressions.

#### SF-D5-2026-08-07-01: Two audit-infrastructure docs still tell auditors to verify a component removed 8 weeks of sessions ago
- **Severity**: LOW
- **Location**: `.claude/commands/audit-starfield/SKILL.md:202-203`, `.claude/commands/_audit-common.md:86`
- **Status**: NEW (elevates an informal note from the 2026-08-03 report that survived a second pass unfixed)
- **Description**: `IsCollisionOnly` was removed as dead code by `e5868bac` (#1570, CLOSED 2026-06-15) — zero hits in any tracked `.rs` file. The real BLAS-exclusion mechanism (still correct) is structural: `spawn_trimesh_collider_ghost`/`spawn_packed_havok_proxy` both spawn colliders with deliberately no `MeshHandle`, so they can never enter `blas_specs` regardless of any marker component. `IsCollisionOnly` is PascalCase, so `_audit-validate.sh`'s advisory-symbol heuristic (which only scans snake_case tokens ≥7 chars) doesn't flag it — a second, smaller gap.
- **Impact**: None on shipped behavior; purely repeat-work for future audit passes re-deriving "this component doesn't exist" from scratch.
- **Related**: #1570 (CLOSED), #2355 (the functional fix, landed today, unrelated to this doc issue).
- **Suggested Fix**: Replace the `IsCollisionOnly` reference in both docs with the actual mechanism (`spawn_trimesh_collider_ghost`/`spawn_packed_havok_proxy` are `MeshHandle`-free by construction); drop it from `_audit-common.md`'s Project Layout line. Optionally widen the validate-script's advisory regex to also catch PascalCase identifiers.

**Existing, reconfirmed, not re-filed**: #2364 (SF-D5-2026-08-03-01, stale test-failure-message framing at `walkers.rs:1102-1103`, unfixed 4 days later, decode logic unaffected); SF-D4-05 (cross-referenced from Dimension 4).

---

### Dimension 6 — NIF Shader Blocks, BSVER 155+

Scope: `crates/nif/src/blocks/shader.rs`, `crates/nif/src/shader_flags.rs`.
Corpus: 87,994 real Starfield NIFs across all 5 vanilla mesh archives, 0
hard parse failures; 2,538 full-body `BSLightingShaderProperty` + 831
full-body `BSEffectShaderProperty` blocks analyzed with per-byte alignment
scoring.

**Confirmed correct, no regression:** the #1510 regression guard holds
*structurally* — `NiUnknown` count is 0 for every shader type across all
four mesh archives, `read_starfield_tail` never over-reads and is not
hardcoded to a fixed length (it produced both 38B and 42B tails, proving
it's data-driven), the stub path's tail is empty on all 389,849
material-reference stubs (positive evidence the #1510 discriminator is
correct); the 32-value CRC32 → flag-name table (`bs_shader_crc32`) is
complete against nif.xml, not opaque; `BSEffectShaderProperty`'s Starfield
byte consumption is empirically *correct* (four `BSSPLuminanceParams`
defaults land in four consecutive correct slots on every full-body block —
not coincidence); `WetnessParams` gating matches nif.xml for the era. **But
this structural correctness was measuring block framing, not block
contents** — see SF-D6-01.

#### SF-D6-01: `BSLightingShaderProperty` is misaligned by one 4-byte word on Starfield — 100% of full-body blocks emit a NaN emissive colour, a bogus texture-set ref, a zero-U-scale UV transform, and the wrong CRC flag set
- **Severity**: HIGH
- **Location**: `crates/nif/src/blocks/shader.rs:1142-1161` (`BSLightingShaderProperty::parse_fo76_plus`)
- **Status**: NEW — not covered by #1510, #1606, #1721, #1881, or #2353
- **Description**: `parse_fo76_plus` makes two 4-byte compensating errors for `bsver >= STARFIELD`: it skips the `shader_type` u32 that Starfield *does* carry (`shader.rs:1142-1146`), and it reads `root_material_path` unconditionally, which Starfield does *not* carry (`shader.rs:1161`). Total block consumption stays right — no drift reported — which is precisely why this survived #1510, #1606, and two prior Starfield audits. Every field between the two errors is read one word early: `num_sf1`, `num_sf2`, both CRC arrays, `uv_offset`, `uv_scale`, `texture_set_ref`, `emissive_color`, `emissive_multiple`. Fields from `texture_clamp_mode` onward re-converge and are unaffected.
- **Evidence**: Real-block dump, `Starfield - LODMeshes.ba2`, `shiplandingmarker_lod_3.nif` block 6 (bsver 173, block_size 166): the shipped parser reads `sf2[0]` = CRC `0` (not a valid `BSShaderCRC32` value — the real `num_sf2` word), `uv_scale.y`/`texture_set_ref` from the same word pair (`texture_set_ref = 1065353216`, unresolvable), `emissive_color.r` = **NaN** (from the `0xFFFFFFFF` word that is really `texture_set_ref`'s NULL sentinel). Corpus-wide corrected-alignment scoring (CRC membership in the 32-value set, resolvable texture-set ref, finite non-negative emissive) across LODMeshes/Meshes01/MeshesPatch: **0/2,538 valid under the shipped alignment, 2,538/2,538 valid under the corrected one.** Under the corrected alignment the previously-bimodal tail length `{38: 1868, 42: 11}` collapses to a uniform `{38: 1879}` — the 11 outliers were an artifact of the misalignment itself. Downstream, `dedicated_shader.rs` copies all of `emissive_color`, `uv_offset`/`uv_scale`, `texture_set_ref`, `root_material_path`, and `sf1_crcs` (decal/two-sided/PBR/vertex-colour classification) directly from these fields.
- **Impact**: All 2,538 inline-authored Starfield `BSLightingShaderProperty` meshes receive a **NaN** emissive colour propagated through `translate_material` into the ECS `Material` and `GpuMaterial` SSBO (poisoning any lighting term it multiplies into); a `texture_set_ref` that can never resolve (texture slots silently empty); a UV transform with a **zero U-scale**; and a shader-flag CRC set invalid on 1,446/2,538 blocks (57%), making decal/two-sided/PBR/vertex-colour classification arbitrary. Per the severity table, "Wrong/divergent Material out of the NIFAL boundary" is HIGH minimum.
- **Related**: #1510 (`c2778fc5`, introduced the `bsver < STARFIELD` shader-type gate), #1606 (`497700e7`, built the opaque tail on top of the misalignment), SF-D6-02, SF-D6-03.
- **Suggested Fix**: Read `shader_type` unconditionally for `bsver >= FO76` (revert the `< STARFIELD` gate); gate `root_material_path` on `bsver < STARFIELD` instead. Net byte count is unchanged. Add a real-data-derived fixture rather than editing the synthetic builder to match (see SF-D6-03).

#### SF-D6-02: The "opaque 38-byte Starfield tail" is not fully opaque — its first four words are the FO76 `BSSPLuminanceParams` block at documented defaults, on 100% of blocks
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/blocks/shader.rs:1183-1187` (the `bsver < STARFIELD` luminance/translucency/texture-array gate), `:1276-1303` (`read_wetness_block`), `:740-751` (the tail doc comment)
- **Status**: NEW
- **Description**: #1510 concluded the FO76 luminance tail is absent on Starfield; #1606 then declared the residual 38 bytes opaque. The tuple `(wetness.metalness, wetness.unknown_1, tail_f32[0], tail_f32[1])` takes exactly two values across 1,879 Meshes01 blocks: `1868× (100.0, 13.5, 2.0, 3.0)` and `11× (-1.0, 100.0, 13.5, 2.0)` (the SF-D6-01-shifted outliers). `(100.0, 13.5, 2.0, 3.0)` are, in order, nif.xml's documented `BSSPLuminanceParams` defaults — the same quad that is the *decoded, aligned* `luminance` on every Starfield `BSEffectShaderProperty`. Four documented defaults appearing as an invariant contiguous quad across 1,879 materially different blocks is not coincidence.
- **Impact**: `LuminanceParams` is `None` for every Starfield `BSLightingShaderProperty` (exposure-offset/emittance authoring unavailable for the era's HDR path), and `WetnessParams.metalness` is populated with `100.0` (an emittance value, not consumed downstream today, but a live trap for whoever wires Starfield wetness up).
- **Related**: SF-D6-01 (must land first — the +4 shift in 11 blocks is its artifact); #1510; #1606.
- **Suggested Fix**: After SF-D6-01, re-enable the `BSSPLuminanceParams` read for `bsver >= STARFIELD` and re-derive the wetness field count from the corpus. Do not name the remaining ~30 undocumented bytes.

#### SF-D6-03: The Starfield shader test fixtures are tautological — they encode the parser's own field order, so no existing test could ever have caught SF-D6-01
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/blocks/shader_tests/mod.rs:414-449` (`build_starfield_bs_lighting_minimal`), consumed by `crates/nif/src/blocks/shader_tests/starfield.rs:16,50,82`
- **Status**: NEW
- **Description**: The fixture builder mirrors `parse_fo76_plus` line for line, comments included (`// NO BSShaderType155 (FO76 == 155 only)`, `// root_material (>= 130)`) — it emits nothing where the real stream carries `shader_type` and emits a `root_material` word the real stream doesn't carry. Every test built on it (`parse_bs_lighting_starfield_captures_trailing_tail`, `..._tail_empty_without_size_or_drift`, `..._minimal_omits_fo76_only_tail`) therefore asserts "the parser reads what the parser writes" — field *order* is unfalsifiable by construction, and all three pass today against a parser that mis-decodes 100% of real Starfield blocks.
- **Impact**: The regression guard is real for `NiUnknown` count but hollow for field-level correctness; two prior audits and three fixes (#1510, #1606, #1881) shipped on top of it.
- **Related**: SF-D6-01; the same pattern (correctly) does not apply to `build_starfield_bs_effect_minimal`.
- **Suggested Fix**: Add one fixture captured verbatim from retail data (the 166-byte block 6 of `shiplandingmarker_lod_3.nif` is ideal — constant across the LOD corpus) and assert semantic invariants: `sf1_crcs == [VERTEX_COLORS]`, `texture_set_ref.is_null()`, `emissive_color == [1.0,1.0,1.0]`, `uv_scale == [1.0,1.0]`, all-finite emissive. Any one of these would have caught SF-D6-01.

#### SF-D6-04: The opaque-tail mechanism disables the drift telemetry that would otherwise have surfaced this exact class of bug for free
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/blocks/shader.rs:760-778` (`read_starfield_tail`), `crates/nif/src/lib.rs:460-508` (drift accounting)
- **Status**: NEW
- **Description**: `read_starfield_tail` consumes `block_size − consumed` *before* `parse_nif` compares consumed against `block_size`, so any Starfield shader-block under-read is converted into tail bytes and never reaches `drift_histogram`. Measured: shader-block drift is `{}` (empty) on all four archives while tail lengths were simultaneously bimodal `{38: 1868, 42: 11}` — exactly the signal a drift histogram exists to raise, invisible to `nif_stats --drift-histogram`.
- **Impact**: Blind spot on precisely the block types with the most Starfield churn; one-directional (over-reads still surface via `saturating_sub` → empty tail), but under-read is the failure mode these parsers actually exhibit.
- **Related**: SF-D6-01, SF-D6-02.
- **Suggested Fix**: Have `read_starfield_tail` also record captured length into a per-type `opaque_tail_histogram` sibling of `drift_histogram`, surfaced on `NifScene`.

#### SF-D6-05: BSVER band 168–171 has no Starfield handling, while `STARFIELD = 172`'s own doc comment claims retail starts at 168
- **Severity**: LOW
- **Location**: `crates/nif/src/version.rs:413-415`
- **Status**: NEW
- **Description**: Every Starfield-vs-FO76 branch keys off `172`; content at 168–171 would take the full FO76 path and skip tail capture. Observed bsver distribution across 87,994 retail NIFs is `{172,173,174,175}` only — latent, not live, but the doc comment invites a future "fix" that would silently re-break the era split.
- **Suggested Fix**: Correct the doc-comment to the observed retail range (172–175); note 168–171 is unattested.

#### SF-D6-06: `SF_WEAK_REF_GAP`'s doc claim that bsver 174 is unobserved is falsified by 13 real files, which also close the gate boundary at exactly 175
- **Severity**: LOW
- **Location**: `crates/nif/src/version.rs:420-436`
- **Status**: NEW (correction to a doc claim, not a code defect)
- **Description**: `Starfield - MeshesPatch.ba2` contains 13 bsver-174 terrain files, all parsing with 0 `NiUnknown` under the current `SF_WEAK_REF_GAP = 175` gate — i.e. 174 carries `form_id` but no 2-byte gap. The current constant is right; the doc understates the confidence available and could invite a wrong future widening to 174.
- **Suggested Fix**: Update the version table row for 174 to `yes / no / MeshesPatch.ba2 (13 files)`, note the boundary is pinned at 175.

**Scoped out by the brief, verified not a defect**: the `BSEffectShaderProperty` "+32B under-read" is the #1881 opaque tail, present on 831/831 full-body blocks at a constant 32B, provably correct via documented-default triangulation — not the same defect class as SF-D6-01.

---

### Dimension 7 — Real-Data Validation

Scope: `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`,
end-to-end trace of 5 representative meshes through `import_nif_scene`.

**No new findings — a clean corroboration pass, with one significant
cross-check.** Mesh parse rate matches the compat matrix exactly (Meshes01
31,058/31,058 100.00%; Meshes02 7,552/7,552 100.00%; MeshesPatch
29,849/29,849 99.98% clean / 100% recoverable, 6 truncated — matching the
tracked #2105 residual tail, not grown); all 13 vanilla texture archives
extract at 100% (full corpus swept, not sampled); zero unexpected
`NiUnknown` block types anywhere in the 89,276-file mesh corpus, with the
one known exception (`BSFaceGenNiNode` trailing-bytes skip, #727 CLOSED,
accepted scope) reconfirmed unchanged. Five representative meshes (clutter,
ship hull, character body, weapon, landscape) traced end-to-end all parse
clean with internally-consistent geometry.

**This dimension's real contribution is the skinning corroboration merged
into Dimension 2 above** — production-scale confirmation of SF2D2-D2-01 on
two independent real meshes, plus the pointer to the stale test assertion
at `bs_geometry_skin_tests.rs:118-121`.

**Tech-debt note (not filed, LOW)**: `audit-starfield/SKILL.md` lines 242
and 316 cite "#746/#747" as the tracker for the Meshes01/MeshesPatch
truncation tail; both are CLOSED and are actually about an unrelated
BSVER-155 shader-property gating regression. The real tracker is #2105.
Worth a 1-line fix next time that skill file is touched.

---

### Dimension 8 — NIFAL Canonical Material Translation for Starfield

Scope: `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`,
`crates/nif/src/import/material/{dedicated_shader,legacy_properties}.rs`.

**Checklist items 1–2 confirmed:** `translate_material` is genuinely the
single raw→canonical boundary — exactly two production call sites
(`scene/nif_loader.rs:889`, `cell_loader/spawn.rs:1506`), structurally
symmetric, no `GameKind` branch anywhere inside; `Material.metalness`/`roughness`
are plain resolved `f32` set once via `resolve_pbr`, with the old per-draw
`classify_pbr` fallback fully deleted and confirmed absent from every render
call site.

#### SF-D8-2026-08-07-01 — HIGH — The #2353 material-reference-stub guard was added to `BSLightingShaderProperty` but never to `BSEffectShaderProperty`; every externally-referenced Starfield effect shader (the dominant authoring path) renders fully invisible
- **Severity**: HIGH
- **Location**: `crates/nif/src/blocks/shader.rs:1616-1650` (`BSEffectShaderProperty::material_reference_stub`), `:1681-1698` (Starfield stub discriminator), `crates/nif/src/import/material/dedicated_shader.rs:365-500` (`apply_bs_effect_shader`, no guard) vs `:86` (the BSLSP guard that exists), `crates/renderer/shaders/triangle.frag:790-799`
- **Status**: NEW — not covered by #2359 (which tracks the `.mat`/CDB merge forwarding zero authored data, an approximate-not-invisible outcome) or #2354 (particles)
- **Description**: `#2353` added `if shader.material_reference { return; }` to the `BSLightingShaderProperty` walker (`dedicated_shader.rs:85-88`) with the rationale that a material-reference stub's fields are parser placeholders, not authored data, and copying them would falsely suppress the external CDB values. `apply_bs_effect_shader` has no equivalent guard — `grep material_reference crates/nif/src/import/` returns exactly one production hit. For a stub it copies the full placeholder set into `MaterialInfo`: `base_color=[1,1,1,1]` → fabricated emissive tint, `emissive_source` wrongly set to `Effect` (nothing was authored), and — the lethal one — `falloff_start_opacity = falloff_stop_opacity = 0.0`.
- **Evidence**: `triangle.frag:790-799`'s cone-fade math: `float coneFade = mat.falloffStartOpacity; float denom = mat.falloffStartAngle - mat.falloffStopAngle; if (denom > 1e-5) {...} ... finalAlpha = texColor.a * coneFade;`. The in-shader comment asserts the identity default is `start_op = stop_op = 1.0` ("the math reduces to a no-op"). The stub hardcodes `0.0`, and with `start_angle == stop_angle == 1.0` (also stub defaults), `denom == 0` skips the branch entirely — `coneFade` stays `0.0` → `finalAlpha = 0.0` on every affected surface. Scope: the stub discriminator on Starfield is `!name.is_empty()`, and Starfield FX materials are authored in `materialsbeta.cdb` and referenced by name — i.e. this is the **dominant** path for Starfield effect geometry, not an edge case. Full-body (non-stub) blocks are the ones with an *empty* name.
- **Impact**: Every externally-referenced Starfield `BSEffectShaderProperty` surface renders fully transparent, with zero visual signal that anything is wrong — a content-visibility failure with no workaround. Per the severity table, "wrong/divergent Material out of NIFAL" is HIGH minimum; this is also flatly worse than divergent — it's invisible.
- **Related**: #2353 (the guard this mirrors, on the sibling type), #2359, #2354.
- **Suggested Fix**: Mirror the #2353 guard in `apply_bs_effect_shader`: after `info.material_path` capture, `if shader.material_reference { info.material_kind = 101; return; }` (keep the kind tag, drop the placeholder payload). Add a test asserting a stub yields `emissive_source == EmissiveSource::None` and `effect_falloff == None`.

#### SF-D8-2026-08-07-02 — LOW — `EmissiveSource::None`'s doc comment claims a "non-zero emissive" condition that no producer checks
- **Severity**: LOW
- **Location**: `crates/core/src/ecs/components/material.rs:452-460`
- **Status**: NEW
- **Description**: The doc says materials land in `None` "or where none of them authored a non-zero emissive"; all three writers (`dedicated_shader.rs:300,397`, `legacy_properties.rs:149`, `asset_provider/material.rs:1230`) set their variant unconditionally once their property class is bound, so a BSLSP with `emissive_multiple = 0.0` reports `Lighting`, not `None`.
- **Impact**: Harmless today — `emissive_source` has no consumer anywhere in `crates/renderer/` — but a trap for the future BSEffect render path #1280 exists to enable.
- **Suggested Fix**: Fix the doc, or add the `!= 0.0` gate.

**Checklist item 4 (particle emitters on Starfield)**: reachability is
already tracked as **#2354** ("NIFAL particle slice structurally
unreachable on Starfield") — confirmed nothing contradicts that premise;
not re-derived, no new finding.

**Checklist item 5** (`BhkMultiSphereShape`/`BhkConvexListShape` →
`CollisionShape`): not completed this pass (investigation cut short); the
relevant Starfield collision gap (#2355) was independently closed the same
day and verified by Dimension 5 — any future pass on this item should read
against `8ee151e0`/`716b7ee9`, not the pre-08-07 tree.

---

### Dimension 9 — BGSM/BGEM External Material Flow

Scope: `crates/bgsm/src/{bgsm,bgem}.rs`, `byroredux/src/asset_provider/material.rs`
(`merge_external_material`), `byroredux/src/cell_loader.rs`
(`pack_imported_material_flags`).

**Confirmed correct, no regression:** BGEM dispatched distinctly from BGSM
on file magic (not extension), with genuinely distinct texture-set
orderings matching `BGSM.cs`/`BGEM.cs` field order; `merge_external_material`
holds the NIFAL boundary strictly at `&mut ImportedMaterial` (no
geometry/skin/transform reach anywhere in the 680-line body); every
`pack_imported_material_flags` bit derives from the correct
`ImportedMaterial` field, including the #2108 enable-bit gate on both merge
arms; `glass_enabled`'s stuck-flag misclassification guard
(`has_transparent_coverage && !is_decal && ...`) is correctly fenced and
covered by three dedicated regression tests. Starfield `.mat` cannot leak a
CDB slot index — it forwards zero textures (the flip side, already tracked
as #2359).

#### SF-D9-2026-08-07-01 — MEDIUM — `bgem_uses_glass_behavior` treats the raw `refraction` bit as an unconditional glass signal, ahead of every version-gated guard, capturing every heat-haze/distortion BGEM as glass
- **Severity**: MEDIUM
- **Location**: `byroredux/src/asset_provider/material.rs:110-113`
- **Status**: NEW
- **Description**: `if bgem.glass_enabled || bgem.base.refraction { return true; }`. `glass_enabled` is a v21+ field authored specifically to mean glass; the careful v<21 feature-bundle heuristic below it (`hard_transparent_shell && reflective_surface_maps && lit_fresnel_falloff`) exists precisely because the pre-v21 format has no such field. `base.refraction` is a different, shared `BaseMaterial` screen-distortion bit — authored on heat shimmer, cloaking shells, force-field ripple, fire/plasma distortion — none of which are glass, and it is neither gated behind the alpha/decal/conductor guards nor version-gated, so it fires on v2 through v22 alike.
- **Impact**: `material.bgem_glass = true` and (since distortion cards are typically `non_occluder`) `THIN_GLASS` too; in `helpers.rs:73-85`, `bgem_glass` makes the mesh an `effect_glass_carrier`, one of the few things allowed to *override* an already-selected engine-synthesized material kind — demoting a correctly-classified effect-shader mesh to `MATERIAL_KIND_GLASS` and stamping fixed metalness/roughness/IOR over its authored PBR. Same corpus #2297 separately flags as `MATERIAL_KIND_FIRE_REFRACTION` content.
- **Suggested Fix**: Drop `|| bgem.base.refraction` from the short circuit, or fold it into the v<21 bundle as one more conjunct. Add a regression fixture: `refraction=true, effect_lighting_enabled=false`, no envmap stack → must NOT classify glass.

#### SF-D9-2026-08-07-02 — MEDIUM — BGSM `inner_layer_texture` is parsed and has a live populated `MaterialTextureSet::inner_layer` role, but `merge_external_material` never connects them
- **Severity**: MEDIUM
- **Location**: `crates/bgsm/src/bgsm.rs:42,200`, `crates/nif/src/import/material/mod.rs:1108` (NIF path fills the role), `byroredux/src/asset_provider/material.rs:881-975` (BGSM fill block — no `inner_layer` entry)
- **Status**: NEW
- **Description**: The BGSM v≤2 legacy texture list reads `envmap, glow, inner_layer, wrinkles, displacement`; the merge forwards `envmap`, `glow`, `wrinkles`, `displacement` and silently drops `inner_layer`. Unlike the documented #2109 glass-overlay deferral, the sink here already exists (populated by the NIF `BSLightingShaderProperty` multi-layer-parallax path, resolved to a real texture handle downstream) — only the BGSM arm fails to wire it.
- **Impact**: A BGSM authoring its inner layer externally (Skyrim SE ice/glass, FO4 layered panes — the multi-layer-parallax slot this dimension's glass/transmissive coverage targets) renders with the layer absent.
- **Suggested Fix**: One more `fill(&mut material.textures.inner_layer, &bgsm.inner_layer_texture, ...)` adjacent to the existing `displacement → height` fill.

#### SF-D9-2026-08-07-03 — LOW — BGSM `distance_field_alpha_texture` (v≥17) is parsed but has no `MaterialTextureSet` role and no sink, undocumented unlike its BGEM sibling deferral
- **Severity**: LOW
- **Location**: `crates/bgsm/src/bgsm.rs:38,194-196`
- **Status**: NEW
- **Description**: No role exists in `MaterialTextureSet` for this field (genuinely a deferred-consumer gap, not a wiring bug) — but unlike the #2109 BGEM glass-overlay deferral, there is no explanatory comment at the BGSM fill block. v≥17 is exactly the FO76/Starfield-era range this dimension targets; distance-field alpha drives crisp signage/decal cutouts that currently fall back to plain alpha test.
- **Suggested Fix**: Add the role + fill, or at minimum a one-line deferral comment mirroring #2109's precedent.

#### SF-D9-2026-08-07-04 — LOW — Two small BGEM asymmetries: authoring both palette-enable bits loses the color variant; envmap texture fill ignores `env_mapping_enabled()`
- **Severity**: LOW
- **Location**: `byroredux/src/asset_provider/material.rs:1173-1200`, `byroredux/src/cell_loader.rs:272-278`
- **Status**: NEW
- **Description**: (1) `bgsm_greyscale_lut_is_alpha`/`_enabled` derivation makes `PALETTE_ALPHA`/`PALETTE_COLOR` mutually exclusive; a BGEM authoring both independent bits (the format permits it) yields `PALETTE_ALPHA` only, dropping color — the inline NIF effect-shader path (`pack_effect_shader_flags`) derives the two bits independently and can set both, so the "documented mirror" paths diverge on this input. (2) `textures.environment`/`environment_mask` fill unconditionally from `bgem.envmap_texture`/`envmap_mask_texture` without consulting `BgemFile::env_mapping_enabled()` (the #2358 accessor for the authoritative v10+ subclass copy) — `bgem_uses_glass_behavior` *does* consult it, so the same authored bit is honoured for classification and ignored for texture binding within one file.
- **Impact**: Both narrow, neither known to fire on vanilla content.
- **Suggested Fix**: For (1), preserve both bits or pick a documented precedence; for (2), gate the environment fill on `env_mapping_enabled()`.

---

## CRC32 Flag Table

`crates/nif/src/shader_flags.rs::bs_shader_crc32` — the complete CRC32
hash-to-flag-name table for the FO76+/Starfield shader-flag arrays
(`sf1_crcs`/`sf2_crcs`, gated BSVER ≥ 132 / ≥ 152 respectively). Verified
this run by direct source read against `docs/legacy/nif.xml` lines
6520-6553: all **32** `BSShaderCRC32` entries are present, named, and pinned
by `bs_shader_crc32_matches_nif_xml_literals` — nothing here is an opaque
raw hash. (The Dimension 6 corpus sweep independently corroborates one
entry: word `w006` of the misaligned SF-D6-01 block decodes to
`348504749` = `VERTEX_COLORS` exactly.)

| Flag name | CRC32 (decimal) |
|---|---|
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
| `FACE` | 314919375 |
| `GRAYSCALE_TO_PALETTE_COLOR` | 442246519 |
| `HAIRTINT` | 1264105798 |
| `SKIN_TINT` | 1483897208 |
| `EMIT_ENABLED` | 2262553490 |
| `GLOWMAP` | 2399422528 |
| `REFRACTION` | 1957349758 |
| `REFRACTION_FALLOFF` | 902349195 |
| `NOFADE` | 2994043788 |
| `INVERTED_FADE_PATTERN` | 3030867718 |
| `RGB_FALLOFF` | 3448946507 |
| `EXTERNAL_EMITTANCE` | 2150459555 |
| `MODELSPACENORMALS` | 2548465567 |
| `TRANSFORM_CHANGED` | 3196772338 |
| `EFFECT_LIGHTING` | 3473438218 |
| `FALLOFF` | 3980660124 |
| `SOFT_EFFECT` | 3503164976 |
| `GRAYSCALE_TO_PALETTE_ALPHA` | 2901038324 |
| `WEAPON_BLOOD` | 2078326675 |
| `LOD_OBJECTS` | 2896726515 |
| `NO_EXPOSURE` | 3707406987 |

The derivation algorithm (flag-name string → CRC32) remains unknown/opaque
per `shader_flags.rs:460-467` (probing standard CRC-32/IEEE 802.3 over
name-string permutations produces no match) — irrelevant to correctness,
since what matters is matching Bethesda's wire literals, which nif.xml
authoritatively documents and the pin test above enforces.

## Remaining-Work Chain

Per `starfield-esm-roadmap.md`, ESM Phases 0+1 are done; Phases 2-4 are
invalidated by the 99.9%-parity measurement. Genuine remaining work, in
order:

1. **Per-field CDB material extraction (#2359 / #1289 Phase 2 follow-up)** —
   `.mat`-resolved materials currently reach the Disney BSDF lobe with
   NIF-derived defaults, not CDB-authored values. Still the single
   highest-value remaining item for Starfield visual fidelity, and still
   tracked under #2359 (confirmed open this pass). **This finalized report's
   own SF-D3-01/SF-D3-02 (CDB allocation safety) are hard prerequisites**:
   they are the first findings on the code path Phase 2 will actually
   exercise (`ComponentDatabaseFile::parse` on untrusted archive bytes), so
   fix them in the same patch that starts Phase 2, not after.
2. **Exterior worldspace tiles** — not yet in scope (Cydonia is an
   interior).
3. **Space-cell / planet / GBFM records** — GBFM/PNDT/STDT/BIOM are
   conscious stubs, a deliberate Phase-3 deferral, not a defect.
4. **#2105 residual truncation tail** — 6/29,849 `MeshesPatch.ba2` files
   still lose exactly 1 `BSWeakReferenceNode` block each; unchanged this
   pass, root cause still unexplained but not growing.

This is **not** "BGSM parser first, ESM very far" — both have shipped; the
per-field CDB extraction is a rendering-fidelity gap on top of an
already-working load/spawn/parse pipeline, and this audit's own three HIGH
shader/skinning findings are now a materially bigger fidelity gap than the
CDB Phase 2 item, on already-shipped content paths.

## Deduplication Summary

All 9 dimensions checked findings against `/tmp/audit/issues.json` (200 most
recent GitHub issues) before filing, per protocol. Confirmed still-live and
correctly not re-filed: #1827 (CLOSED, stale premise re: skin weights —
noted as needing correction alongside the SF2D2-D2-01 fix), #2097 (LZ4-01),
#2360 (v3 log stream position), #1761 (`start_mip` dead-code allow), #1571
(CLOSED — CDB path-matching half; SF-D3-04 is the archive-selection half),
#2353 (the BSLSP guard SF-D8-2026-08-07-01 mirrors), #2359, #2354, #2355
(fixed same-day, verified by Dimension 5), #2364, #1576, #1568, #1567,
#2109, #2108, #2297, #2358, #727, #1570, #594.

## Finding Count Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 5 |
| MEDIUM | 10 |
| LOW | 16 |
| **Total** | **31** |

**HIGH**: SF2D2-D2-01 (merged D2+D7, bind-pose skinning), SF-D3-01
(`index_chunks` unvalidated capacity), SF-D3-03 (`Archive::open` full-file
read), SF-D6-01 (`BSLightingShaderProperty` word misalignment),
SF-D8-2026-08-07-01 (`BSEffectShaderProperty` stub-guard gap, invisible
surfaces)

**MEDIUM**: SF-D1-01, SF-D1-03, SF2D2-D2-02, SF-D3-02, SF-D3-04, SF-D6-02,
SF-D6-03, SF-D6-04, SF-D9-2026-08-07-01, SF-D9-2026-08-07-02

**LOW**: SF-D1-02, SF-D1-04, SF-D1-05, SF2D2-D2-03, SF2D2-D2-04, SF-D3-05,
SF-D3-06, SF-D3-Value-Ref-doc, SF-D4-05, SF-D4-06, SF-D5-2026-08-07-01,
SF-D6-05, SF-D6-06, SF-D8-2026-08-07-02, SF-D9-2026-08-07-03,
SF-D9-2026-08-07-04

Per-dimension breakdown: Dim1 0H/2M/3L · Dim2 1H(merged)/1M/2L · Dim3
2H/2M/3L · Dim4 0/0/2L · Dim5 0/0/1L · Dim6 1H/3M/2L · Dim7 0 new (1
corroborated into Dim2) · Dim8 1H/0/1L · Dim9 0/2M/2L.

---

Suggested next step: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-08-07.md`
