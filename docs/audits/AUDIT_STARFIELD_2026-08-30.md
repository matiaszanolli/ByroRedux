# Starfield Compatibility Audit — 2026-08-30

**Scope**: depth/correctness audit of ByroRedux's Starfield support — BA2 v2/v3 +
LZ4 block, CDB materials, `BSGeometry` `.mesh` resolution, and the walkable
Cydonia interior. Nine dimensions, all processed in-process against real game
data at `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` (186 entries,
129 BA2 archives).

**Findings: 1 HIGH, 5 LOW.** No CRITICAL, no MEDIUM.

| Severity | Count | IDs |
|---|---:|---|
| CRITICAL | 0 | — |
| HIGH | 1 | SF-D2-01 |
| MEDIUM | 0 | — |
| LOW | 5 | SF-D1-01, SF-D3-01, SF-D3-02, SF-D4-01, SF-D9-01 |

Dimensions producing **no findings**: **5, 6, 7, 8** (all verified holding
against real data).

---

## Executive Summary

Starfield's bring-up surface is in **good shape and has not regressed**. Every
headline number in `ROADMAP.md` was independently re-measured and matched
exactly:

- **NIF parse rate: 120,524 / 120,543 = 99.98% clean, 100% recoverable** across
  all 13 mesh-bearing archives — byte-for-byte identical to the compat matrix,
  including the per-archive rows and the three correctly-excluded zero-NIF
  archives. The 19-file truncation tail has **not grown**.
- **BA2 extraction: 100%.** 104,818 / 104,818 v3 LZ4 textures and 137,214
  total sampled/brute-forced extractions, zero errors, zero archive-open
  failures, peak RSS 155 MB.
- **ESM: both baselines hold.** Top-level GRUP byte coverage is 86.1% (exactly
  the Phase-1 figure); Cydonia's per-cell resolve rate is 91.16%
  (25,433 / 27,898), with `LIGH = 656` confirming the #1567 DAT2 fix.
- Every named regression guard in dimensions 5, 6, 8 and 9 is present and
  correct. `NiUnknown` is **6 blocks out of ~1.86 M** (0.0003%).

Against that, one defect of real consequence surfaced, in a place no existing
gate can see.

### The headline: SF-D2-01 — every Starfield NPC face is silently dropped

**100% of Starfield facegen geometry fails to load.** All 4,832 `.mesh`
companion bodies in `Starfield - FaceMeshes.ba2` end immediately after the LOD
array, carrying no meshlet/cull-data trailer; `BSGeometryMeshData::parse` reads
that trailer unconditionally and hits EOF. End-to-end, **1,282 / 1,282 FaceMeshes
NIFs import zero meshes** — every NPC would render headless.

It is invisible to every existing gate because the gates measure `.nif` block
parsing, and the `.nif` files parse **perfectly** (`nif_parse_fail = 0`; ROADMAP
records FaceMeshes at 100.00%). The failure is one layer down, in the external
companion bodies the gates never open. The fix is small and does not require
inventing any field semantics.

### The other theme: measurement changed three scoping decisions

Three results should change how work is prioritised, none of them defects:

1. **Cydonia's geometry resolve rate is effectively 99.4%, not 91.2%.**
   Decomposing all 2,465 unresolved REFRs for the first time: 92.9% are
   non-geometry *by design* (PDCL decals 74.9%, audio, actors, consciously
   skipped types). Only 175 of 27,898 REFRs (0.63%) are genuinely missing
   placeable geometry. Anyone treating the 8.8% as a geometry gap is chasing the
   wrong 93% of it.
2. **Phase 2's memory problem is ~2× the documented size.** There are **13**
   CDBs totalling 3,077,172 chunks — and **two** of them are full-size (~105 MB,
   ~1.458 M chunks each), not one. The 9.19 GB peak was measured on one.
3. **The CDB is Starfield's *only* material source.** Zero `.bgsm`/`.bgem` files
   exist in any Starfield archive, yet 283 meshes reference them — so those
   references are guaranteed misses. Combined with the 68,441 `.mat` references,
   **100% of Starfield materials reach the Disney lobe with zero authored
   texture roles.** There is no partial BGSM fallback; Phase 2 is a step
   function.

---

## Findings by Severity

### HIGH

#### SF-D2-01 — every Starfield facegen `.mesh` body fails to parse; 1,282/1,282 FaceMeshes NIFs import ZERO geometry

**Where**: `crates/nif/src/blocks/bs_geometry.rs:521-541`
(`BSGeometryMeshData::parse`), cascading through
`crates/nif/src/import/mesh/bs_geometry.rs:175-190`.

**Labels**: `game:starfield`, `legacy-compat`, `nif`

The parser reads a meshlet + cull-data trailer unconditionally after the LOD
array:

```rust
let n_lods = stream.read_u32_le()?;
for _ in 0..n_lods { … }
let n_meshlets = stream.read_u32_le()?;          // <-- EOF here on facegen bodies
let meshlets = stream.read_pod_vec::<Meshlet>(n_meshlets as usize)?;
let n_cull_data = stream.read_u32_le()?;
let cull_data = stream.read_pod_vec::<CullData>(n_cull_data as usize)?;
```

Facegen bodies ship no such trailer. A field-by-field walk classifying where
each of the **680,239** vanilla `.mesh` bodies ends:

```
GLOBAL: {"ENDS_AFTER_LODS": 4832, "FULL_EXACT": 675407}
of which carry skin weights: {"ENDS_AFTER_LODS": 4832, "FULL_EXACT": 10185}
```

`FULL_EXACT` = byte-exact fit through `cull_data`. The split is **100% clean
along the FaceMeshes archive boundary** — all 4,832 facegen bodies, none of the
other 675,407. It is **not** "skinned bodies omit the trailer": 10,185
`FULL_EXACT` bodies also carry skin weights.

Worked example (`geometries\04849f0a968b16012bb3\88ae67fe92bc26895682.mesh`,
38,940 B) — every preceding field consumes exactly the right bytes:

```
version=2 @4 | n_tri_indices=4200 @8 | after_tris @8408 | scale=1.0 @8412
weights_per_vert=1 @8416 | n_vertices=1386 → @16736 | n_uv0=1386 → @22284
n_uv1=0 | n_colors=0 | n_normals=1386 → @27840 | n_tangents=1386 → @33388
n_total_weights=1386 → @38936 | n_lods=0 → cursor 38940 == file_len 38940
                                          ^ parser reads n_meshlets here → EOF
```

**Cascade**: `parse_from_bytes` → `Err("failed to fill whole buffer")` → Stage B's
`Err(e)` arm logs at **`debug!`** and `continue`s → all LOD slots exhaust →
`log::warn!` → `return None` → the shape never becomes an `ImportedMesh`.

**End-to-end** (real `Ba2Archive`-backed `MeshResolver`, `parse_nif` +
`import_nif_scene_with_resolver`, every NIF in each archive):

```
Starfield - FaceMeshes.ba2
  nifs=1282 imported=1282 nif_parse_fail=0 with_mesh=0 ZERO_MESH=1282 total_meshes=0
Starfield - Meshes01.ba2
  nifs=31058 imported=31058 nif_parse_fail=0 with_mesh=29213 ZERO_MESH=1845 total_meshes=172907
```

Re-confirmed under a realistic **4-archive** resolver (Dimension 7): FaceMeshes
stays at 1,282/1,282 zero. Not a resolver-scope artifact.

**Why no gate caught it**: `nif_parse_fail = 0` — the NIF *files* parse
perfectly. `crates/nif/tests/parse_real_nifs.rs:312` gates FaceMeshes at
`min_clean: 0.995` and it measures **100.00%**. That gate reads `.nif` blocks;
the external `.mesh` companion bodies are never opened by it. The compat matrix
is correct and simultaneously blind to a total content loss.

**Severity**: `_audit-severity.md` lists "NIF parse failures that prevent loading
game content" under HIGH, and its special-rules table sets "NIF parse failure
(hard error)" at HIGH minimum. Loss is 100% of a content class but recoverable
and non-corrupting → HIGH, not CRITICAL.

**Suggested fix** (bounded, no fabrication): treat EOF at the `n_meshlets` read
as "no trailer present" — empty `meshlets`/`cull_data` — exactly as the
`scale <= 0` sentinel arm already returns an all-empty body. Gate it on the
cursor being *exactly* at EOF so a genuinely truncated body still errors.
Everything before the trailer already decodes with an exact byte fit; no other
field changes. Add a FaceMeshes-shaped fixture and consider extending the
parse-rate gate to open `.mesh` companions, since the current gate structurally
cannot see this class of defect.

**Dedup**: no existing issue. #3526 (FaceMeshes *path* composition) and #3464
(`BSFaceGenNiNode` 2-byte under-read) touch the same archive but are different
defects; neither would fix this.

---

### LOW

#### SF-D1-01 — stale archive-corpus count in the `ba2.rs` module doc
`crates/bsa/src/ba2.rs:14` and `:220-222` both say "observed across 108 vanilla
archives". No installed corpus matches 108 (Starfield 129 / 50 vanilla; FO76
101; FO4 187). The substantive claim it supports — "no v3 GNRL observed" — is
independently **true** (0 v3 GNRL across all 129 Starfield archives). Doc rot,
not a correctness defect. **Labels**: `game:starfield`, `legacy-compat`,
`tech-debt`

#### SF-D3-01 — Phase 2's memory problem is ~2× the documented size: there are **two** full-size CDBs
`docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md` scopes Phase 2 against "a 105 MB
CDB" peaking at 9.19 GB. Measured: `SFBGS007 - Main.ba2` carries a second
`materialsbeta.cdb` of **104,868,172 B / 1,458,383 chunks / 97 classes** — within
0.2% of the base on every axis. Corpus total is **3,077,172 chunks across
~232 MB in 13 CDBs**, versus the 1,457,575 the 9.19 GB figure was measured on. A
Phase-2 reader reusing the current `parse` across the discovered set would peak
north of **18 GB** on a 29 GB machine. Planning-accuracy correction; nothing
calls `parse` today. **Suggested action**: amend the spike doc's sizing paragraph
with the per-CDB table (Dimension 3). **Labels**: `game:starfield`,
`legacy-compat`, `materials`

#### SF-D3-02 — "Starfield ships no `.mat` sidecar files" is true for vanilla, false for installed Creation/mod content
`byroredux/src/asset_provider/material.rs:1127` states it as fact. Measured
across all 129 archives: **20 `.mat` files exist**, all from third-party
Creation/mod archives, all JSON material-editor exports. Zero `.bgsm` and zero
`.bgem` anywhere — that half of the premise is fully confirmed. The
*conclusion* still holds (there is no JSON `.mat` resolver, so running the
dispatch would gain nothing), but the premise is what would justify never
writing one. **Fix the comment, not the code.** **Labels**: `game:starfield`,
`legacy-compat`, `materials`

#### SF-D4-01 — the baseline doc's open GBFM question is now answerable: **defer, confirmed**
`docs/engine/starfield-esm-phase0-baseline.md:134` sets a decision rule that has
never been evaluated: promote GBFM if GBFM-targeted refs dominate the missing
count, defer if they are "<10% of skipped refs". Measured for the first time:
**GBFM-targeted REFRs are 2 of 2,465 unresolved in Cydonia (0.081%)**, 1 of 895
in CydoniaMainLevel02, and 0 of 4,218 in Nishina01. **The rule fires "defer".**
The same measurement identifies what should take its place: **PDCL, at 74.9% of
unresolved REFRs**, ~900× more impactful by the identical metric and not
currently ranked anywhere. **Labels**: `game:starfield`, `legacy-compat`, `esm`

#### SF-D9-01 — the BGSM/BGEM external-file path is entirely unreachable on Starfield; 100% of its materials reach the renderer with zero authored texture roles
Zero `.bgsm`/`.bgem` **files** exist in any Starfield archive, yet **283 meshes
reference them** (`{"mat": 68441, "bgsm": 255, "bgem": 28}` over 69,170 sampled
meshes) — so every such reference is a guaranteed archive miss that falls through
to `apply_cdb_pbr_fallback`, joining the 68,441 `.mat` references at
`MergeOutcome::PresenceOnly`. Confirmed at the boundary: `meshes with >=1 texture
role filled: 0 (0.00%)`, `is_pbr=0`, `from_bgsm=0`. Consequences:
`BGSM_AUTHORED`, `TRANSLUCENCY(+2 variants)` and `MODEL_SPACE_NORMALS` can never
be set on Starfield content, and **Starfield has no glass-classification signal
at all** (BGEM `glass_enabled` is unreachable). The missing extraction itself is
#1289/#3398 and is **not re-filed**; what is new is the **exclusivity** — the CDB
is not one of several sources, it is the only one, so Phase 2 is a step function
with no partial-coverage state. **Suggested action**: record on #3398 so the
design does not assume a BGSM fallback exists. **Labels**: `game:starfield`,
`legacy-compat`, `materials`, `nifal`

---

## Confirmed Still-Live (already filed — deliberately NOT re-filed)

| Issue | Confirmation this run |
|---|---|
| **#3398** *XMCOLOR* field-offset | `read_user_class` (`reader.rs:520-590`) still ignores `Field::offset`. Independently reproduced across all 13 CDBs: **821 classes, `decl != offset` for exactly 2 — both `XMCOLOR`.** Sharper characterisation: fields declare `r,g,b,a` at offsets **2,1,0,3**, i.e. a straight **R↔B transposition** (green and alpha survive). |
| **#3474** `starfield_tail` doc says 38 B | Measured **30 B, uniformly**, over `{0: 451014, 30: 2564}` — extends the evidence base from 1,879 blocks / 2 archives to **2,564 / all 6**. The `9× f32 + 2 B = 38` arithmetic is wrong; this audit's own skill file repeats it. |
| **#3524** MeshesPatch residual truncations | **Sharpened: the tail is 19 files across 2 archives, not 6 in 1.** All 19 declare `BSWeakReferenceNode`, all are `bsver == 175` (exactly `SF_WEAK_REF_GAP`, so the #2105/#2201 gate is *active* and the residual is a variation within the 175 layout), and all are distant-terrain LOD at `meshes\terrain\<world>\objects\*.nif`. The 13 `ShatteredSpace - Main01` files should be folded in. |
| **#3396** FO76-only shader arms unreachable | `shader_type histogram: {0: 453578}` — every Starfield `BSLightingShaderProperty` in all six archives has `shader_type == 0`. |
| **#2625** opaque-tail capture suppresses drift telemetry | `read_starfield_tail`'s `saturating_sub` (`shader.rs:787`) still yields an empty tail rather than a negative-drift signal on an over-read. |
| **#1576** model-less STAT/BNDS/ACTI/ARMO drop | **Now quantified**: its exact record list accounts for all 144 genuinely-missing-geometry REFRs in Cydonia (BNDS 60 + STAT 44 + ACTI 33 + ARMO 7) — and for nothing else. |
| **#1289 / #3398** per-field CDB extraction | Current state confirmed: `apply_cdb_pbr_fallback` sets `is_pbr = true` and returns `PresenceOnly`; `register_starfield_cdb_probe` discards its `CdbHeaderInfo`. Accurately documented in place. |

**Noted, not re-filed** — `BSEffectShaderProperty` +32 B under-read, frequency as
requested: **879 of 983 (89.4%)** carry a 32-byte opaque tail. Note the tails
differ (lighting 30 B, effect 32 B) — **one fix will not cover both**.
`BSEffectShaderProperty` is 0.22% of Starfield shader blocks, which bounds the
impact.

**Bearing on #748** ("BSShaderCRC32 table covers ~32 of nif.xml's ~120"): for
vanilla Starfield the 32-entry table has **100% coverage of what ships** — the
gap is not reachable from this game's content.

---

## CRC32 Flag Table

Only **10 distinct CRC32 values** appear in the entire vanilla Starfield mesh
corpus (108,816 NIFs). `crates/nif/src/shader_flags.rs::bs_shader_crc32` names
**all 10 of 10** — the hashes are **not** opaque.

| CRC32 | occurrences | Name (`bs_shader_crc32`) |
|---|---:|---|
| `0x14C5C2AD` | 1,396 | `VERTEX_COLORS` |
| `0x67B70934` | 74 | `ZBUFFER_TEST` |
| `0xBCBAC5F3` | 74 | `ZBUFFER_WRITE` |
| `0x5DF93B67` | 10 | `DYNAMIC_DECAL` |
| `0xB2757B8C` | 10 | `NOFADE` |
| `0xE56D16E0` | 10 | `DECAL` |
| `0xDF3182B0` | 3 | `SKINNED` |
| `0x1A5C2577` | 1 | `GRAYSCALE_TO_PALETTE_COLOR` |
| `0x2D45EC6E` | 1 | `TWO_SIDED` |
| `0x74AAC97E` | 1 | `REFRACTION` |

Total 1,486 occurrences — the arrays are extremely sparse
(`sf1_crcs len: {0: 452106, 1: 1398, 2: 62, 3: 1, 4: 11}`; `sf2_crcs len:
{0: 453567, 1: 11}`), so 99.7% of blocks carry none. The arrays are actively
**consumed** by the importer (`contains_any` in `import/material/mod.rs`,
`dedicated_shader.rs`), not merely parsed.

**22 of the table's 32 entries are never observed** in vanilla Starfield:
`FACE`, `PBR`, `REFRACTION_FALLOFF`, `HAIRTINT`, `SKIN_TINT`, `CAST_SHADOWS`,
`WEAPON_BLOOD`, `EXTERNAL_EMITTANCE`, `EMIT_ENABLED`, `VERTEX_ALPHA`, `GLOWMAP`,
`MODELSPACENORMALS`, `ENVMAP`, `LOD_OBJECTS`, `GRAYSCALE_TO_PALETTE_ALPHA`,
`INVERTED_FADE_PATTERN`, `TRANSFORM_CHANGED`, `RGB_FALLOFF`, `EFFECT_LIGHTING`,
`SOFT_EFFECT`, `NO_EXPOSURE`, `FALLOFF`.

**Negative result, recorded so the search is not repeated**: the 10 hashes do
*not* resolve against either obvious external vocabulary. Hashed with reflected
CRC-32 (poly `0xEDB88320`, init 0, no final XOR — the same function as the CDB
key) in original/upper/lower/underscore/space variants: all **1,368**
`<bitflags>`/`<option>` names from `nif.xml` → **0 matches**; all **459** strings
from the 13 CDB string tables → **0 matches**. The in-repo table is the
authority.

---

## Cross-Cutting Results

### CROSS-CHECK: the `synthesize_normals` gap — **Starfield is NOT affected**

Measured elsewhere at 20.4% of Skyrim meshes, 100/100 Oblivion distant-terrain
LOD, and 14,054/15,614 (90.01%) FO4 distant-LOD shapes. The gap *exists
structurally* here — `bs_geometry.rs:238-252` falls back to
`vec![[0.0, 1.0, 0.0]; positions.len()]` with no `synthesize_normals` sibling to
`synthesize_tangents_yup` — but it is never taken:

```
TOTAL parsed=675407  no_normals=0  no_tangents=0  no_uv0=0
live(non-sentinel)=675407  no_normals=0.00%  no_tangents=0.00%  no_uv0=0.00%
```

**0 of 675,407** external `.mesh` bodies ship without authored UDEC3 normals —
including LOD slots, where the other games' gap concentrates. Starfield also has
no distant-LOD `.mesh` corpus at all: `LODMeshes(.Patch).ba2` hold
`.nif`/`.lod`/`.bgsdb` and **zero** `.mesh` files, so the FO4/Oblivion failure
mode has no Starfield analogue. **Do not file a Starfield
`synthesize_normals` finding**; a regression test would be vacuous. Note the
asymmetry for whoever fixes the shared gap.

Side effect of the same census: the #1232 `synthesize_tangents_yup` fallback is
correct but **unexercised** by vanilla Starfield (0 of 675,407 lack authored
tangents).

### CROSS-FILE to `/audit-esm`: placed records dropped for lack of walker arms

Starfield is exposed, at a scale between FO3 and FO4, with an **inverted**
dominant class. The cell walker dispatches `REFR`/`ACHR`/`LAND`/`NAVM`.
Population inside CELL/WRLD sub-GRUPs in `Starfield.esm`:

| FourCC | Count | Arm? |
|---|---:|---|
| REFR | 3,291,860 | ✅ |
| NAVM | 56,576 | ✅ |
| ACHR | 9,530 | ✅ |
| **PGRE** | **1,268** | ❌ dropped |
| **PHZD** | **375** | ❌ dropped |

**1,643 placed records dropped.** FO3: 350 (PGRE). Starfield: 1,643 (PGRE 77.2%).
FO4: 2,928 (PHZD 82.1%). Starfield ships **zero**
`PMIS`/`PARW`/`PBAR`/`PBEA`/`PCON`/`PFLA`, so PGRE + PHZD is the complete
exposure. **Defect is in the shared walker, not Starfield handling — cross-filed
to `/audit-esm`, deliberately not filed here** per this dimension's scope split.

---

## Dimension Findings

### Dimension 1 — BA2 v2/v3 LZ4 block decompression — **CLEAN** (1 LOW)

Measured census of all 129 archives: 114 v2 (92 GNRL + 22 DX10, all zlib), 15 v3
(all DX10, all `compression_method == 3`). **Zero v3 GNRL** and **zero v3
method-0** — the module-doc claim confirmed. Header extension offsets correct (v2
+8, v3 +8+4 = 36 B). The unsupported-method branch is a **hard error**, and
unknown BTDX majors hit an exhaustive-match error arm (#811).

Per-chunk raw-vs-decompress selection is the `packed_size == 0` sentinel, and the
mixing the module doc describes is **real**: **3,833 of 104,818 v3 textures
(3.66%) genuinely mix raw and LZ4 chunks within one texture**. The sentinel is
unambiguous — across the full **1,914,947-chunk** corpus, **zero** chunks carry a
nonzero `packed_size` equal to `unpacked_size`.

`safe-decode` verified **live**, not just documented: `cargo tree -p byroredux-bsa
-i lz4_flex -e features` resolves `std, safe-encode, safe-decode, frame,
checked-decode` on 0.11.6, matching the #3392 pin. Brute-force extraction of
every v3 file: **104,818 / 104,818 (100.00%)**, 0 errors, peak RSS 155 MB.
`cargo test -p byroredux-bsa`: 80 passed, 0 failed.

### Dimension 2 — BSGeometry mesh extraction — **1 HIGH** (SF-D2-01, above)

All other checklist items verified correct: #1292 canonical path composition
(head/tail tested independently on byte slices, #3391 UTF-8 fix present;
`normalize_mesh_path` leaves the `geometries\` head untouched at both cases, with
two pinning tests); #1209 every-LOD-slot iteration in **both** stages; #1828/#1829
sentinel-skip in **both** stages (the described regression is absent); #1203 skin
chain via `BSSkin::Instance` + `BSSkin::BoneData` + `mesh_data.skin_weights`;
#1232 tangent fallback reachable and correct (with #2246 `clamp_sign` on the
2-bit UDEC3 W); PBR scalar forwarding gated on `!no_pbr_signal`.

Informational: 354,874 of 675,407 (52.5%) `.mesh` bodies ship no vertex colors.

### Dimension 3 — CDB material database — **2 LOW** (SF-D3-01, SF-D3-02)

13 CDBs discovered by `discover_starfield_cdbs`'s own predicate, 12 of them
DLC/Creation-namespaced — a re-hardcoded base path would drop 12 of 13 (#1571
**not** regressed). All 13 pass `peek_magic` (BETH, cannot collide with BGSM) and
`probe_header`. #762/#2614 `index_chunks` reservation cap present and correct.

Presence path is cheap and correct — **162 ms wall, 140 MB peak for all 13**
(base 22.2 ms, SFBGS007 21.7 ms), memoised in `sf_cdb_cache`. The 9.19 GB `parse`
is genuinely not on the live path.

**Strictness observation** (no live defect): unknown `ChunkType`, `BuiltinType`
and class-flag bits all **bail**, aborting the entire database. Measured safe
today — across all **821 classes in all 13 CDBs** the flag histogram is
`{0: 747, 8: 42, 4: 24, 12: 8}` with **zero** bits outside `KNOWN = 0b1100`. Worth
knowing for Phase 2: one new reflection bit in a future Creation CDB takes the
whole material database down rather than degrading.

**#3230 try-then-fall-through INTACT**: only `.mat` short-circuits;
`.bgsm`/`.bgem` carry `cdb_pbr_fallback` to the resolvers and reach the CDB flip
at four resolve-miss sites. The re-added-early-return regression is absent.

`cargo test -p byroredux-sfmaterial`: 23 passed, 0 failed.

### Dimension 4 — ESM resolve-rate baseline — **1 LOW** (SF-D4-01)

`parse_esm` on the 1.39 GB master was run under a hard `ulimit -v` cap so it
could not OOM the machine; measured peak **3.36 GB / 5.6 s**. (The 20 GB spike in
the brief came from the `--ignored` test binary, not `parse_esm`.) All work in
`crates/plugin` examples, never the engine binary.

Both baselines hold: GRUP byte coverage **86.1%** (176 GRUPs, 78 handled / 98
skipped, zero walk errors, peak 1.42 GB / 0.67 s) — exactly the Phase-1 figure;
per-cell resolve rate **91.16%** for Cydonia (27,898 REFRs, matching #1292),
90.75% / 84.76% / 95.02% for three comparison interiors. `interior cells 11,985`,
`38 cydonia matches`, `statics 42,185` (grown from the doc's 41,620).
**#1567 holds**: `LIGH = 656`, byte-identical to the documented figure.

**New**: the 8.84% unresolved decomposed exhaustively (0 forms absent from the
ESM, so none of it is unloaded-master) — see Executive Summary. Starfield-only
record frequency: GBFM 36.1 MB/3,141, PNDT 25.8 MB/1,765, STDT 12.0 MB/123, BIOM
5.3 MB/431, SFBK 1.3 MB/1,753, PDCL 107 KB/706, SUNP 41.7 KB/52, GBFT 1.7 KB/8.
**Note the inversion: PDCL is the smallest by byte size yet by far the largest by
REFR impact** — byte-coverage ranking (which is what the 86.1% figure measures)
actively mis-ranks it.

### Dimension 5 — ESM + cell bring-up regression surface — **NO FINDINGS**

HEDR-0.96 → `GameKind::Starfield` verified on the real master (`TES4`, record
version 581, HEDR 0.9599999785423279). `XCLL_SIZES_STARFIELD = [28, 108]` present
and measured: decompressing every CELL record yields **`{108: 11985}`** — all
11,985 XCLL are exactly 108 bytes, no outliers, **zero sub-record read errors
across all 30,717 CELLs**. (The `28` entry is a defensive allowance, unexercised
by vanilla. Starfield CELLs are zlib-compressed — a naive raw scan finds zero
XCLL; worth knowing for future probes.)

All spawn guards present: **#1294** (`missing_collision_fallback` takes
`base_layer`; no `final_layer` exists), **#1235**, **#1295**, **#1212/13/14**,
**#1272**, **#1284** (now *derived*: `(196608/144)-1 = 1364`, so it cannot drift
from the SSBO bound), **#1570** (`IsCollisionOnly` has **zero** references
anywhere), and colliders stay out of the BLAS **structurally** — neither ghost
spawner inserts a `MeshHandle`. **#1568 PDCL named skip intact**, pinned by
`records/tests.rs:988-1019`. `cargo test -p byroredux-plugin`: **860 passed, 0
failed** (no `--ignored`).

### Dimension 6 — NIF shader blocks, BSVER 155+ — **NO FINDINGS**

**#1510 holds**: across 108,816 NIFs / ~1.86 M blocks, `truncated = 0`
everywhere and **zero** `BSLightingShaderProperty` degrade to `NiUnknown`. The
~1,036 blocks #1510 was truncating all parse. LODMeshes + LODMeshesPatch (39,075
NIFs) drift stays at **0**. The only 6 corpus-wide `NiUnknown` are *recovered*
(`recovered_blocks = 6`, `truncated = 0`) and belong to #3524.

CRC array parse correct (`shader.rs:417-447`), gates at `FO4_CRC_FLAGS = 132` and
`FO76_SF2_CRCS = 152`. **#1606 mechanism correct** — `read_starfield_tail`
captures `block_size - consumed`, no hardcoded 38, no over-read; all 451,014
material-reference stubs correctly yield an empty tail. See the flag table and
the confirmed-still-live rows above for the measured 30 B and the 89.4% BSEffect
frequency.

### Dimension 7 — real-data validation — **NO FINDINGS**

Parse-rate gate replicated as a bounded example (the real one is `#[ignore]`):
**every row matches `ROADMAP.md:595` exactly**, aggregate 120,524/120,543 =
99.98%, recoverable 100%, tail unchanged at 19. `ShatteredSpace - Main01` sits
0.09 pp **below** its 0.995 floor — the pre-existing 2026-08-29 state, not a new
regression, flagged for whoever next touches the floors.

**Methodology caution**: a first pass keying "clean" on `scene.truncated` alone
reported **100.00% — a false improvement**. The harness also counts
`recovered_blocks > 0` (`common/mod.rs:596`). Caught before it reached this
report; any future re-measurement must use both conditions.

Block histogram: `NiUnknown` = 6 of 1.86 M (0.0003%); no unrecognised block type
since the FO76/Starfield baseline. Four representative meshes traced end-to-end
(clutter / ship furniture / skinned character / weapon) all import correct
geometry, `bsx_flags` and skinning.

**Zero-mesh NIFs — mostly a probe artifact worth recording**: with a
single-archive resolver Meshes01 showed 1,596 NIFs carrying `BSGeometry` yet
importing nothing; with a realistic 4-archive resolver that collapses to **406**.
**1,190 were cross-archive `.mesh` references** — any future audit measuring
Starfield spawn rate must use a multi-archive resolver or over-report mesh loss
by ~3×. Residual outside FaceMeshes: 435 of 60,907 (0.71%), consistent with
legitimate sentinel-only LOD stubs. Also: `Starfield - Meshes02.ba2`'s 7,552 NIFs
carry **no** `BSGeometry` and **no** `bhk*` blocks at all — it is not a geometry
archive despite the name.

### Dimension 8 — NIFAL canonical material translation — **NO FINDINGS**

Single boundary holds: three production `translate_material` sites, and **no
per-draw `classify_pbr`** in the renderer (one doc-comment hit in
`triangle.frag`, no call). `Material.metalness`/`.roughness` are plain `f32` with
a resolve-once NaN-sentinel fill and clamps — no `Option<f32>` plumbing. #2707's
fix visible and correct for the dominant stub case. `EmissiveSource` tagged in
both post-#2059 locations.

### Dimension 9 — BGSM/BGEM external material flow — **1 LOW** (SF-D9-01)

`merge_external_material` takes `&mut ImportedMaterial` — it structurally cannot
touch geometry/skinning; the NIFAL boundary is intact, and `MergeOutcome` (#2709)
correctly distinguishes "resolved to nothing" from "resolved with data". BGEM
handled distinctly from BGSM with its own `glass_enabled` field and no
inheritance walk; #1280's authoritative-glass behaviour and its
opaque-architecture counter-guard both present with tests. All five
`pack_imported_material_flags` flags derive from the right `ImportedMaterial`
fields.

**Forward-looking note for #3398** (not a present defect): the brief's concern
that "`.mat` texture paths must land in `MaterialTextureSet` roles, never a
CDB-specific slot index" has no current instance — no CDB texture data is
extracted at all. Recording it as a Phase-2 acceptance criterion.
`MaterialTextureSet<T>` is the correct named-role target (18 roles), and since
the CDB's enum fields are **strings**, the mapping needs an explicit arm plus a
documented default per role — precisely where a silent per-game divergence would
be introduced.

---

## Skill-File Drift (reported explicitly, as instructed)

The audit brief warned that per-game skills systematically send auditors after
types their game ships **zero** of (2 in FNV, 1 in FO3, 10 in Oblivion, 3 in
FO4). **Starfield adds 2**, plus 4 further stale premises. All were investigated
on real data before being dropped.

**Types Starfield ships ZERO of** (Dimension 8 checklist):

1. **NIFAL particle slice** — `NiPSysEmitter` / `NiPSysEmitterCtlr` →
   `extract_emitter_params` → `apply_emitter_params`. The full block histogram
   over all six mesh archives yields **24 distinct block types** and the particle
   family is **empty**: no `NiPSysEmitter`, no `NiPSysEmitterCtlr`, no
   `NiParticleSystem`, no `NiPSysBlock`. Unreachable.
2. **NIFAL collision slice** — `BhkMultiSphereShape` + `BhkConvexListShape` →
   `CollisionShape`. **Neither occurs once.** Starfield collision is entirely
   `bhkNPCollisionObject` (59,761) + `bhkPhysicsSystem` (40,724) +
   `bhkRagdollSystem` (571) — the `BhkSystemBinary` blob path, not a per-shape
   translate path. Cydonia's colliders come from the synthesized fallback.

**Recommendation**: delete both from the Dimension 8 checklist and replace with
the `bhkNPCollisionObject` / `bhkPhysicsSystem` blob path. The complete
24-type Starfield vocabulary is recorded in `dim_8.md` — anything not on it
cannot be exercised by vanilla content.

**Other stale premises dropped** (4):

3. **"38-byte trailing field (9× f32 + 2 B)"** (Dim 6) — measured **30 B**
   uniformly. The skill repeats the identical error #3474 already filed against
   the code comment.
4. **"#746/#747 track the residual truncation tail"** (Dim 7) — ROADMAP records
   that tail as *mis-attributed* to those closed issues, fixed by #2105, with the
   residual now owned by #3524.
5. **"walks all 5 vanilla mesh archives"** (Dim 7) — the gate now covers **13**
   (#3466).
6. **"v3 raw-vs-compressed chosen by a per-chunk compressed/uncompressed-size
   comparison"** (Dim 1) — the actual selector is the `packed_size == 0`
   sentinel. Code is right; the checklist prose describes a different algorithm.

Additionally, **skill text calling the CDB lookup key or field vocabulary
"unknown" is stale** (correctly pre-flagged in the brief) — confirmed solved as
of #3398 (2026-08-29) and not re-investigated as a gap.

**Stale-candidate accounting**: 6 stale checklist premises dropped, plus 1
pre-flagged by the brief. Zero findings were filed on a stale premise. One
finding of my own (a hypothesis that the 19 truncations were the
`BSDistantObjectExtraData` gap #3461, by analogy with FO76's distant-LOD tail)
was **falsified before reporting** by reading the header-declared block types —
all 19 are `BSWeakReferenceNode`, so #3524's premise stands.

---

## Remaining-Work Chain

Phases 0+1 are done; Phases 2-4 were invalidated by the 99.9%-parity
measurement. In order:

1. **SF-D2-01 — facegen `.mesh` trailer** *(new, HIGH)*. Small and bounded; it
   is the only item here that is a live content-loss defect rather than a
   feature gap. Recovers 100% of Starfield NPC head geometry.
2. **Per-field CDB extraction (#1289 / #3398 Phase 2).** The lookup key and
   field vocabulary are **solved** (2026-08-29). What remains is (a) the indexed
   reader that avoids the parse peak — now sized against **3,077,172 chunks
   across 13 CDBs, two of them full-size**, not 1.46 M in one (SF-D3-01) — and
   (b) the *XMCOLOR* field-offset fix, whose effect is a straight R↔B swap. Note
   from SF-D9-01 that the CDB is Starfield's **only** material source: there is
   no BGSM fallback and no partial-coverage state.
3. **PDCL, ahead of GBFM.** SF-D4-01 resolves the baseline doc's open GBFM
   question as **defer** (0.081% of unresolved REFRs) and identifies PDCL as
   ~900× more impactful by the same metric (74.9%). PDCL is currently a
   conscious, correctly-telemetered skip (#1568) and is unranked.
4. **#1576 model-less STAT/BNDS/ACTI/ARMO** (geometry in a BFCB component
   block) — now quantified at exactly 144 REFRs in Cydonia, the *entire*
   genuinely-missing-geometry population outside PDCL.
5. **Exterior worldspace tiles; space-cell / planet / GBFM records.**
6. **The 19-file NIF truncation tail (#3524)** — narrowed this run to
   `BSWeakReferenceNode` at `bsver == 175` on distant-terrain LOD objects across
   two archives.

Do **not** frame this as "BGSM parser first / ESM very far" — both have shipped.

---

## Reproduction

All probes are throwaway examples prefixed `_tmp_sfaudit_*` in
`crates/bsa/examples/`, `crates/nif/examples/`, `crates/plugin/examples/`,
`crates/sfmaterial/examples/`, plus Python scripts in `/tmp/audit/starfield/`.
Per-dimension detail in `/tmp/audit/starfield/dim_1.md` … `dim_9.md`.

**Memory discipline observed throughout**: `CARGO_BUILD_JOBS=4`, package-scoped
builds/tests only, one cargo command at a time, **no `--ignored` / `--include-ignored`
ever**, no engine launch, and every `parse_esm` run under a hard `ulimit -v` cap.
Peak RSS by dimension: D1 155 MB, D2 180 MB, D3 140 MB, D4 3.36 GB (capped at
12-14 GB), D5 1.43 GB, D6 78 MB, D7 79-303 MB. No OOM, no swap pressure.

**Next**: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-08-30.md`
(label every finding `game:starfield` + `legacy-compat`, plus its own domain
label.)
