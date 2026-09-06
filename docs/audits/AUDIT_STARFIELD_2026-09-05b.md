# Starfield Compatibility Audit — 2026-09-05 (second pass, `b`)

**Command**: `/audit-starfield` (full, all 9 dimensions), run as part of the
`per-game-all` preset — last of six per-game audits.
**HEAD**: `6fba2b0a`
**Game data**: present — `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/`
(129 BA2 archives, 55 plugins, ~144 GB)
**Dedup baseline**: `gh issue list` re-pulled mid-audit at max issue `#3912`
(121 open, `/tmp/audit/sf_issues2.json`), plus a scan of all 20 prior
`docs/audits/AUDIT_STARFIELD_*.md` reports and `SF_CDB_PHASE2_SPIKE_2026-08-29.md`.

> **Filename note.** `docs/audits/AUDIT_STARFIELD_2026-09-05.md` already exists
> and is git-tracked — a concurrent session ran its own Starfield audit today
> (HEAD `fa5c4191`, `texture-roles-deep` preset) and committed it. This report
> uses the repo's established same-day `b` suffix (precedent:
> `AUDIT_STARFIELD_2026-08-27b.md`) rather than clobbering that work. Section
> *Relationship to the first 2026-09-05 pass* below cross-references it.

**Findings: 5 — CRITICAL 0 · HIGH 1 · MEDIUM 1 · LOW 3.**

---

## Executive Summary

Starfield's bring-up surface is healthy and this pass re-measured it rather
than re-reading it. Every headline number the compat matrix claims was
**re-derived from the real corpus, not assumed**:

| Measure | Result | Scope |
|---|---|---|
| NIF parse rate (recoverable) | **100.00%** | swept — all 13 mesh archives, 120,543 NIFs |
| NIF parse rate (clean) | **99.984%** (19 truncated) | swept — same |
| BA2 open + extract | **0 failures / 5,924 sampled entries** | sampled — all 129 archives |
| DDS reconstruction | **1,920 / 1,920 valid magic** | sampled — all DX10 archives |
| Per-block parser drift | **0 bytes, every type, every archive** | swept — 6 mesh archives |
| HEDR → `GameKind::Starfield` | **55 / 55 plugins in band** | swept — every `.esm` on disk |
| `NiSkinPartition` blocks | **0** | swept — all 13 archives |

Every named regression guard in the skill was re-verified against current code
and all are intact: the BA2 v3 36-byte header and hard-erroring
`compression_method` dispatch, the `packed_size == 0` raw/LZ4 chunk selector,
`normalize_mesh_path`'s `geometries\` passthrough (#1292), the sentinel-slot
skip on both BSGeometry stages (#1828/#1829), the `remaining() == 0` meshlet
trailer gate (#3777), `read_starfield_tail`'s capture-to-`block_size` (#1606),
the PDCL named skip (#1568), `XCLL_SIZES_STARFIELD` (#1291), the LIGH `DAT2`
decode (#1567), and the `base_layer` collision gate (#1294). **No regression of
any of them was found.**

**The one serious finding is that a closed fix solved the right problem with
the wrong instrument.** #3549 (RT-3, closed 2026-08-30) addressed "every
Starfield skinned mesh has 100% unresolved bones" by adding
`solve_bone_names` — a geometric bind-pose fit against an externally-resolved
skeleton that *declines rather than guesses*, recovering ~21%. Its premise,
stated in the code comment, is that "the identity is not in the file at all".
Measured over 21,222 skinned `BSGeometry` shapes: **every single one of the
18,990 shapes with all-null `bone_refs` carries a `SkinAttach` extra-data block
in its own `extra_data_refs`, holding a bone-name list whose length matches its
`BSSkin::BoneData` bone count exactly — 18,990 / 18,990, zero mismatches.** The
NIF parser already decodes those names into `NiExtraData::skin_attach_bones`.
No code anywhere reads the field. The remaining ~79% of Starfield NPCs and
apparel sit in bind pose while the authored answer is parsed and discarded one
struct away.

### Coverage / method limits — what was sampled vs swept

- **Swept** (full corpus, no sampling): NIF header walk and block-type
  histogram over all 120,543 NIF entries in all 13 mesh archives; the
  `parse_rate_starfield_all_meshes` gate over the same; the per-block drift
  histogram over 6 archives; the opaque-tail length histogram over 5 archives
  (90,922 files); the `SkinAttach` correlation over 4 archives (21,222 skins);
  `.cdb`/`.mat`/`.bgsm`/`.bgem` enumeration over all 129 archives; HEDR reads
  over all 55 plugins.
- **Sampled**: BA2 entry extraction — a stride-sample of ~50 entries per
  archive across all 129 (5,924 entries). A full extract of all 129 archives
  was not attempted under the session's memory constraint.
- **Not exercised** — deliberately, and this is the same limit the first
  2026-09-05 pass recorded:
  - **Dim 4 (`--sf-smoke` per-cell resolve rate)** was **not run**. It parses the
    1.36 GB `Starfield.esm` in-process, which is the known 20+ GB spike class
    (`plugin_ignored_tests_oom`); the session ran with ~4 GB free RAM and 28 of
    39 GB swap already committed. Dim 4's guards were verified statically and
    the HEDR classifier was measured on real data; **the per-cell resolve rate
    is not re-measured this pass** and no claim is made about it.
  - **No engine launch.** Static analysis and offline parsing only, per the
    standing rule and #3540 (Cydonia stalls at `M28.5 frame 0`).
  - `byroredux-plugin --ignored` corpus tests were not run.
- **Not audited** (skill says vanilla Starfield ships zero of these, and the
  block histogram below confirms it): the NIFAL particle slice
  (`NiPSysEmitter`/`NiPSysEmitterCtlr`) and the per-shape collision slice
  (`BhkMultiSphereShape`/`BhkConvexListShape`). A test against either would be
  vacuous.

---

## The requested `NiSkinPartition` measurement (clean negative)

The cross-game lead — `NiSkinPartition::parse` pre-sizing its de-stripped
triangle buffer with `allocate_vec_sized::<[u16; 3]>(num_triangles)` inside the
`num_strips > 0` branch, demanding 6 B/triangle where strip-derived triangles
cost ~2 B — **does not reach Starfield at all.**

The premise is confirmed live in code (`crates/nif/src/blocks/skin.rs`,
`NiSkinPartition::parse`): `allocate_vec_sized` delegates to
`allocate_vec_min_bytes(count, size_of::<T>())` = `count * 6`, bounded against
`remaining`, while the strip payload that follows is only
`sum(strip_lengths) * 2` bytes. On strip-authored partitions that is roughly a
3× over-demand. **Starfield is immune because it authors no such partitions.**

```
Full sweep — 13 mesh-bearing archives, 120,543 NIF entries, header string
table walked for every block index (0 header-parse failures):

  NiSkinPartition blocks   = 0        (0 files)
  BSSkin::Instance files   = 6,019
  BSGeometry files         = 101,107
```

Zero, swept not sampled. Starfield's skin path is `BSSkin::Instance` +
`BSSkin::BoneData` + `mesh_data.skin_weights` end-to-end, exactly as the skill
states; the classic `NiSkinPartition` path is never constructed. This bounds
the suite's most severe finding: whatever its blast radius on FO3 / FNV /
Oblivion, **Starfield contributes nothing to it.**

For completeness, the full Starfield block-type histogram (30 distinct types,
swept) — every one of the 30 has a dispatch arm in
`crates/nif/src/blocks/mod.rs`, and the parse-rate sweep below confirms only
one of them ever lands on the recovery path:

```
484,820 BSGeometry            484,820 NiIntegerExtraData   483,837 BSLightingShaderProperty
175,165 NiNode                 99,906 BSXFlags              62,705 bhkNPCollisionObject
 61,701 NiStringExtraData      42,411 bhkPhysicsSystem      23,209 BSSkin::BoneData
 23,209 BSSkin::Instance       22,378 SkinAttach            19,154 BSWeakReferenceNode
 15,672 NiIntegersExtraData     1,552 BSFaceGenNiNode        1,309 BSConnectPoint::Children
  1,196 BSClothExtraData        1,077 NiAlphaProperty          983 BSEffectShaderProperty
    662 BSConnectPoint::Parents   605 bhkRagdollSystem         416 NiFloatExtraData
    281 BoneTranslations          155 BSBound                   76 NiBillboardNode
     69 NiSwitchNode               61 BSBlastNode               49 NiCamera
     34 NiPointLight               12 BSOrderedNode             10 BSValueNode
```

No `BSShaderTextureSet` appears anywhere in vanilla Starfield — relevant to
#3900, and noted again under Dimension 8/9 below.

---

## Findings

### SF2-2026-09-05-D2-01: `SkinAttach` carries the authored bone names for 100% of the skins #3549 has to solve geometrically — parsed into `skin_attach_bones`, read by nothing

- **Severity**: HIGH
- **Dimension**: 2 — BSGeometry mesh extraction / skin chain
- **Location**: `crates/nif/src/blocks/extra_data.rs` (`NiExtraData::skin_attach_bones`),
  `crates/nif/src/import/mesh/skin.rs` (the `external_names` block),
  `crates/nif/src/import/mesh/skeleton.rs` (`solve_bone_names`,
  `resolve_external_bone_names`)
- **Status**: NEW — supersedes the premise of **#3549** (CLOSED 2026-08-30)
- **Description**:
  #3549 fixed "every Starfield skinned mesh has 100% unresolved bones — all SF
  actors and apparel render in bind pose" by adding a geometric solver: for a
  skin whose `BSSkin::Instance.bone_refs` are all NULL, fit the per-file bind
  offset against an externally-resolved skeleton and accept the names only on a
  unique fit, otherwise decline to the `Bone{i}` placeholder. Measured recovery
  was ~21% of clothes skins (~3,900 of ~19,500 bones); the rest correctly
  decline. The reasoning is recorded verbatim in `skin.rs`:

  > `// The identity is not in the file at all (its header string table holds`
  > `// only ExportScene, BSX, the mesh name and material paths), so`
  > `// resolve_node_name returned None …`

  That premise is false. The identity is in the file — not in the *header
  string table* (the comment is literally true about that), but in a
  `SkinAttach` extra-data block hanging off the very same `BSGeometry`, whose
  bone names are stored as **inline length-prefixed `NiString`s**, which is why
  they never appear in the header table. `crates/nif/src/blocks/mod.rs`
  dispatches `SkinAttach` to `NiExtraData::parse`, which decodes the list into
  `NiExtraData::skin_attach_bones: Option<Vec<String>>`. Nothing consumes it.
- **Evidence**:
  Per-shape structural walk (not scene-level co-occurrence): for each
  `BSGeometry`, follow its own `av.net.extra_data_refs` to a `SkinAttach`, and
  its own `skin_instance_ref` → `bone_data_ref` to the authoritative bone
  count. Swept over `Meshes01` + `MeshesPatch` + `FaceMeshes` +
  `ShatteredSpace - Main01`:

  ```
  BSGeometry shapes with a skin instance      = 21,222
    skin has ALL-NULL bone_refs               = 18,990   (89.5%)
      shape's OWN extra_data has SkinAttach   = 18,990   (100.0%)
        names.len() == BoneData bone count    = 18,990   (100.0%)
        count mismatch                        =      0
    skin bone_refs resolve (control group)    =  2,232
      OWN SkinAttach agrees in ORDER + name   =      0
      OWN SkinAttach disagrees                =  1,640
  ```

  The control group is what makes this conclusive rather than coincidental.
  Where `bone_refs` *do* resolve, the shape's `SkinAttach` entries are **empty
  strings** — the names live in the node refs instead. The two mechanisms are
  complementary alternatives, exactly as a "0 → use the sibling channel"
  encoding would be:

  ```
  ORDER_DISAGREE meshes\actors\minibota\mesh\minibota_security\minibota_security.nif
      attach=["", "COM", "", ""]  resolved=["C_Chassis","C_Body","C_Axle","C_Base"]
  ```

  And the recovered names on the all-null population are unambiguous Starfield
  skeleton bones, i.e. precisely what the solver is trying to reconstruct:

  ```
  meshes\clothes\spacesuit_ucpilot_01\spacesuit_ucpilot_lowerbody_01_f.nif   n=21
      ["R_Foot","R_Calf","R_Toe","C_Hips","C_Spine","C_Spine1","R_Butt","L_Butt", …]
  meshes\clothes\spacesuit_starborn_01\spacesuit_starborn_hunter_plates_f.nif n=43
      ["R_Clavicle","R_Deltoid","R_Biceps_Twist1","C_Chest","R_Biceps","R_Elbow", …]
  meshes\actors\human_crowd\mesh\female\hairs\messy_business_f_crowd.nif      n=1
      ["C_Head"]
  ```

  Repo-wide consumer check — `skin_attach_bones` is written by the parser and
  read only by a dispatch test:

  ```
  crates/nif/src/blocks/extra_data.rs        (declaration + 4 assignment sites)
  crates/nif/src/blocks/dispatch_tests/starfield.rs:281,283
  crates/nif/src/import/mesh/tangent_convention_tests.rs:517   (struct-literal `None`)
  ```

  No hit in `crates/nif/src/import/mesh/skin.rs`, `skeleton.rs`, or anywhere in
  `byroredux/src/`.
- **Impact**:
  ~79% of Starfield skinned content — the share `solve_bone_names` correctly
  declines on rather than guessing — stays in bind pose, when the authored bone
  names for 100% of it are already sitting parsed in memory. Blast radius is
  every NPC body, every head (all 1,282 `FaceMeshes` NIFs are in the affected
  population), and every apparel/spacesuit piece: 18,990 of 21,222 skinned
  shapes in the four sampled archives. This is the single largest remaining
  correctness gap on the Starfield content path, and unlike the geometric
  solver it needs no external skeleton resolution, no tolerance tuning, and no
  decline path — the data is exact and count-checked.
- **Related**: #3549 (CLOSED — the solver this supersedes as the primary
  source); #708 / NIF-D5-02 (added the `SkinAttach` parser); `SF2-…-D2-02`
  below (the `BoneTranslations` sibling, same class).
- **Suggested Fix**:
  In `crates/nif/src/import/mesh/skin.rs`, before the `external_names`
  geometric fallback, look up the owning shape's own `SkinAttach` via
  `extra_data_refs` and use its list as the primary name source. Resolve
  **per entry**, not wholesale — `minibota_security.nif` proves a single
  `SkinAttach` can mix authored and blank entries — giving the chain
  `SkinAttach[i]` (if non-empty) → `resolve_node_name(bone_refs[i])` →
  `solve_bone_names` → `Bone{i}`. Keep the geometric solver as the last
  resort it already is; this only moves it behind the authored data. Assert
  the count agreement (`names.len() == bone_data.bones.len()`) and decline the
  whole list on a mismatch, mirroring the existing decline discipline.

---

### SF2-2026-09-05-D2-02: `BoneTranslations` is decoded on every instance and consumed by nothing — the same drop as D2-01, one order of magnitude smaller

- **Severity**: MEDIUM
- **Dimension**: 2 — BSGeometry mesh extraction / skin chain
- **Location**: `crates/nif/src/blocks/extra_data.rs`
  (`NiExtraData::bone_translations`)
- **Status**: NEW
- **Description**:
  `BoneTranslations` is dispatched alongside `SkinAttach` to
  `NiExtraData::parse` and its payload — `(bone_name, [f32; 3])` pairs
  supplying per-bone offset deltas for the skeleton at a given LOD, sourced
  from `nifly::BoneTranslations::Sync` — is fully decoded into
  `NiExtraData::bone_translations`. As with `skin_attach_bones`, the field has
  no consumer outside the block parser and its own dispatch test. Every
  instance that ships in vanilla Starfield carries a non-empty payload, so this
  is not a dormant field waiting for content that does not exist.
- **Evidence**:
  Swept over the same four archives:
  ```
  BoneTranslations blocks           = 256
    with a decoded payload          = 256   (100%)
  ```
  Corpus-wide the histogram counts 281 instances across all 13 archives.
  Consumer grep over `crates/`, `byroredux/`, `tools/` returns only
  `crates/nif/src/blocks/extra_data.rs` (declaration + assignment),
  `crates/nif/src/blocks/dispatch_tests/starfield.rs:294,314,316`, and a
  struct-literal `None` in `tangent_convention_tests.rs:518`.
- **Impact**:
  Per-bone LOD offset deltas are dropped, so a skinned mesh at a reduced
  skeleton LOD is posed from the unadjusted bind data. Far narrower than
  D2-01 (281 instances vs 22,378 `SkinAttach`), and it only manifests at LOD
  boundaries rather than at LOD 0, which is why it is MEDIUM rather than HIGH:
  translatable data silently dropped, with a visible but bounded consequence.
  Worth fixing in the same change as D2-01, since both hang off the same
  `extra_data_refs` walk.
- **Related**: `SF2-…-D2-01`; #708 (added both parsers in one commit).
- **Suggested Fix**:
  Carry the pairs onto `ImportedSkin` alongside the bone list, keyed by name so
  they survive the `SkinAttach`/`bone_refs` name resolution above, and apply
  them when a non-zero bone-LOD level is selected. If no LOD selection exists
  yet, the honest interim is to record the field as deliberately deferred with
  a dated comment rather than leave it looking wired.

---

### SF2-2026-09-05-D7-01: #3524's title scopes the `BSWeakReferenceNode` residual to six MeshesPatch files; there are 19, and the localisation is the `BSWaterReferenceStruct` 80-byte entry skip

- **Severity**: LOW
- **Dimension**: 7 — real-data validation
- **Location**: `crates/nif/src/blocks/node.rs`
  (`BsWeakReferenceNode::parse`, the `num_water_refs` loop)
- **Status**: Existing: **#3524** — scope correction + added localisation, not a new defect
- **Description**:
  The residual truncation tail has **not grown** — it is exactly 19 files, matching
  the 2026-08-30 characterisation, and 100% of the corpus remains *recoverable*.
  But #3524's title reads "the six residual **MeshesPatch** truncations", and the
  sweep shows the same defect at more than twice that count in a second archive
  that the title does not mention. Anyone re-measuring against the title will
  conclude the count tripled.
- **Evidence**:
  Full gate run, all 13 archives, 120,543 NIFs — `recoverable 100.00%` on every
  archive; 11 of 13 clean at 100.00%:
  ```
  Starfield - MeshesPatch.ba2   99.98%   6 truncated   BSWeakReferenceNode 7,546 parsed / 6 unknown
  ShatteredSpace - Main01.ba2   99.86%  13 truncated   BSWeakReferenceNode 2,991 parsed / 13 unknown
  (all 11 other archives: 0 truncated, 0 unknown, 0 types with partial unknown)
  ```
  `nif_stats --unknown-only --all` confirms `BSWeakReferenceNode` is the **only**
  type with any unknown instance anywhere in the corpus — notably, the #1510
  regression guard holds: `BSLightingShaderProperty` NiUnknown count is 0 across
  483,837 blocks. All 19 files are `meshes\terrain\<world>\objects\<world>.X.Y.Z.nif`
  terrain-object composites. `trace_block` on one of them localises the failure:
  ```
  meshes\terrain\cydoniacity\objects\cydoniacity.2.-2.-2.nif
    version 20.2.0.7  bsver 175  num_blocks 1
    [0] @0 BSWeakReferenceNode size=35860
        [ERR at consumed 35850: skip(80) at position 35850 would exceed data length 35868]
  ```
  `skip(80)` is `64 + 12 + 4` — the per-entry transform + `BSResourceID` +
  `unkInt1` skip inside the `BSWaterReferenceStruct[]` loop. The failure is the
  loop's final iteration finding 18 bytes where it wants 80, so either
  `num_water_refs` is over-counted by one or an earlier entry's variable-length
  material string consumed the wrong amount and drifted the cursor forward.
- **Impact**:
  Bounded and already known: 19 terrain-object composite scenes drop one block
  each and lose their water references. No growth since the last measurement.
  The value here is the localisation (the water-ref loop, not the
  `SF_WEAK_REF_GAP` 2-byte field) and the corrected scope.
- **Related**: #2105 (the `SF_WEAK_REF_GAP` gate, a *different* field — do not
  conflate); #3524.
- **Suggested Fix**:
  Re-title / re-scope #3524 to "19 files across MeshesPatch + ShatteredSpace -
  Main01" and record the water-ref-loop localisation on it. For the defect
  itself, hand-decode `cydoniacity.2.-2.-2.nif` backwards from the trailing
  `materials/water/*.mat` string to establish whether the per-entry stride is
  80 or the count is off by one.

---

### SF2-2026-09-05-D6-01: the Starfield opaque tails are measurable and quantified — 3,017 × 30 B lit and 831 × 32 B effect — but the drift metric that should report them is structurally zero

- **Severity**: LOW
- **Dimension**: 6 — NIF shader blocks, BSVER 155+
- **Location**: `crates/nif/src/blocks/shader.rs` (`read_starfield_tail`)
- **Status**: Existing: **#2625** (SF-D6-04) — this supplies the measurement that
  issue says is missing; also standing evidence for #1606 / #1881 (both CLOSED)
- **Description**:
  Running `nif_stats --drift-histogram` across all six Starfield mesh archives
  reports **"No drift detected"** on every one. That result is correct but must
  not be read as "the shader parsers consume exactly the right bytes":
  `read_starfield_tail` swallows `block_size - consumed` into `starfield_tail`
  for `BSLightingShaderProperty` and `BSEffectShaderProperty` whenever
  `bsver >= STARFIELD (172)`, which is all Starfield content, so positive drift
  for those two types is zero *by construction*. This is exactly #2625.

  One useful precision this pass adds: **the masking is one-sided.**
  `read_starfield_tail` computes `remaining` with `saturating_sub`, so an
  *over*-read would leave the tail empty and still surface as negative drift.
  The zero-drift result therefore does soundly rule out over-reads — it only
  fails to report under-reads. There are none of either outside the tail.
- **Evidence**:
  Tail-length histogram, swept over `Meshes01` + `MeshesPatch` + `FaceMeshes` +
  `ShatteredSpace - Main01` + `LODMeshes` (90,922 files):
  ```
  BSLightingShaderProperty tails: total = 422,416
      len=0    419,399   (99.29%)
      len=30     3,017   ( 0.71%)      ← #1606's undocumented tail, exactly 30 B
  BSEffectShaderProperty  tails: total =     927
      len=0         96   (10.36%)
      len=32       831   (89.64%)      ← #1881's tail, exactly 32 B
  ```
  Both distributions are single-bucket: no spread at all. That uniformity is
  itself information — each is one fixed-size field group, not variable-length
  data, which is the shape a future decode effort should assume.
- **Impact**:
  No data loss and no misalignment today (both tails are captured opaquely and
  the corpus parses at 100% recoverable). The cost is diagnostic: the parse-time
  instrument that would flag a *future* schema change on these two types is
  disarmed for them, and the 831 effect-shader tails carry undecoded semantics
  on 89.6% of all Starfield effect materials.
- **Related**: #2625 (open — the telemetry gap); #1606 (30 B lit tail, closed,
  re-measured here at 30 B and 3,017 instances, confirming the #3474 correction
  of the earlier 38 B figure); #1881 (32 B effect tail, closed).
- **Suggested Fix**:
  Attach the measurement to #2625 and give `nif_stats` a `--tail-histogram`
  companion so tail length is a first-class, assertable metric wherever
  `starfield_tail` disarms drift — a per-type tail-length pin would catch a
  schema change that the drift histogram now cannot.

---

### SF2-2026-09-05-D3-01: `crates/sfmaterial`'s module doc points the consumer-side mapping at `byroredux/src/asset_provider.rs`, a file deleted in the Session 34 split

- **Severity**: LOW
- **Dimension**: 3 — CDB material database
- **Location**: `crates/sfmaterial/src/lib.rs` (module doc, "Scope (Stage B per
  audit #762)" section)
- **Status**: NEW
- **Description**:
  The crate-level doc closes with: *"The consumer-side mapping (Starfield-specific
  material → `ImportedMesh` fields) happens in `byroredux/src/asset_provider.rs`
  and is a separate concern from the format parsing here."* That path does not
  exist — `asset_provider` became a directory during the Session 34 refactor and
  the CDB consumer specifically lives in `byroredux/src/asset_provider/material.rs`
  (`discover_starfield_cdbs`, `register_starfield_cdb`, `apply_cdb_pbr_fallback`).
  This is the only pointer from the CDB parser to its consumer, so it is the
  first place someone tracing the boundary looks.
- **Evidence**:
  ```
  $ ls byroredux/src/asset_provider.rs
  ls: cannot access 'byroredux/src/asset_provider.rs': No such file or directory
  ```
  The live consumer sites are `byroredux/src/asset_provider/material.rs:177`
  (`discover_starfield_cdbs`), `:599` (`register_starfield_cdb`), `:970`
  (`apply_cdb_pbr_fallback`).
- **Impact**:
  Doc-rot only, but on a cross-crate boundary pointer with no other signpost.
  The same statement also predates `MergeOutcome` — the mapping today produces
  `PresenceOnly`, i.e. one routing flag and no fields, which the sentence's
  "material → `ImportedMesh` fields" phrasing overstates.
- **Related**: #3889 (`register_starfield_cdb` is a test-only duplicate of the
  shipped registration path — same neighbourhood, already filed, not re-reported
  here); #3398 (the Phase-2 work this doc is describing).
- **Suggested Fix**:
  Repoint at `byroredux/src/asset_provider/material.rs` and name
  `merge_external_material` / `apply_cdb_pbr_fallback` as the actual entry
  points, noting that the mapping is currently `PresenceOnly` pending #3398.

---

## Verified clean (measured, not assumed)

### Dimension 1 — BA2 v2 / v3 + LZ4 block decompression

Version and compression-method census over **all 129 installed archives**, read
from the raw header bytes and cross-checked against `Ba2Archive::open`:

```
  92  v2 GNRL  (no compression_method field)
  22  v2 DX10  (no compression_method field)
  15  v3 DX10  compression_method = 3  (LZ4 block)
   0  v3 GNRL
   0  v3 with compression_method = 0 (zlib)
```

Extraction sample (~50 stride-sampled entries per archive, 5,924 total):
**0 failures**, and every one of the 1,920 sampled `.dds` entries came back with
a valid `DDS ` magic from the reconstructed header. Independently, the mesh-side
sweep extracted **120,543 GNRL entries with 0 failures and 0 header-parse
failures**, so the GNRL path is swept rather than sampled.

Code re-verified: the v3 arm reads the 8-byte extra **then** the 4-byte
`compression_method` (36-byte header), logs the boundary *after* that read
(#2360), maps `0 → Zlib` / `3 → Lz4Block`, and **hard-errors** on anything else
rather than falling through — confirmed, not a silent default. Unknown BA2
majors hit an exhaustive-match error arm (#811). The `packed_size == 0` raw/LZ4
chunk selector is present on both the GNRL (`ba2.rs:856`) and DX10
(`ba2.rs:890`) paths, and both dispatch into the single `decompress_chunk`.
`lz4_flex`'s `safe-decode` is pinned on, making the documented undersized-hint
panic structurally impossible, with `catch_unwind` retained as defence in depth.

Two honest coverage notes: **the v3 + zlib (`compression_method = 0`)
combination has zero coverage in this install** — no archive exercises it — and
one mod archive (`starfieldresourcerevival - textures.ba2`) ships `.dds` files
inside a **GNRL** archive rather than DX10, which extracts correctly (97/97).

### Dimension 5 — ESM classifier, on real data

Every `.esm` in the Starfield Data directory, header-only read (no GRUP walk,
no memory risk):

```
  55 plugins, HEDR version histogram: {0.96: 55}
  plugins outside the Starfield band 0.955..=0.97: 0 of 55
  record_version range observed: 552 … 582
```

All 55 — base game, ShatteredSpace, every SFBGS Creation, and installed
community plugins — classify as `GameKind::Starfield`. Worth recording that the
`from_header` bands are `(0.945..=0.955)` with `record_version >= 100` → FO4
*before* `(0.955..=0.97)` → Starfield, so `0.955` exactly is in both and the
first arm would win. Not reachable on real data (everything is 0.96), so this is
a note, not a finding.

### Dimension 3 / 9 — CDB and sidecar reality, swept over all 129 archives

```
  TOTAL archives=129  cdbs=13  mat_files=20  bgsm_files=0  bgem_files=0
```

This confirms three claims the skill makes and grounds a fourth:

- **13 CDBs**, matching the 2026-08-30 count, and **two are full-size**:
  `materials\materialsbeta.cdb` (105,037,616 B) and
  `materials\creations\sfbgs007\materialsbeta.cdb` (104,868,172 B). The
  ~18 GB corpus-wide parse-peak estimate for a Phase-2 reader reusing today's
  `parse` is well-founded, not the single-CDB 9.19 GB figure.
- **Exactly one CDB per archive**, and 12 of the 13 live in *content* archives
  (`ShatteredSpace - Main01.ba2`, `SFBGS0xx - Main.ba2`, `kgcdoom - main.ba2`, …)
  at `materials\creations\<plugin>\materialsbeta.cdb`, not in
  `Starfield - Materials.ba2`. This is precisely why #1571's scan-don't-hardcode
  discovery and #2621's numeric-sibling extension are load-bearing: a hardcoded
  base path would find 1 of 13. Both are intact, and `--bsa` archives are
  scanned for CDBs too, so a session that loads DLC meshes also gets that DLC's
  material database. **Not a finding** — the default `starfield` profile
  discovers only the base CDB, but it also loads none of the content the other
  twelve describe.
- **Zero `.bgsm` and zero `.bgem` files exist in any Starfield archive** (SF-D9-01
  re-confirmed at full sweep), so every `.bgsm`/`.bgem`-*named* reference is a
  guaranteed resolver miss falling through to the CDB flip. The #3230
  try-then-fall-through order is intact and no early `PresenceOnly` return has
  been re-added above the resolvers.
- **20 `.mat` files exist**, all in installed mod archives
  (`qog-pawnshop` 12, `sp2_factionrequisitionkiosks` 5,
  `starfieldresourcerevival` 2, `avontechshipyards` 1) — confirming #3782's
  correction that the `.mat` short-circuit is retained for want of a JSON `.mat`
  resolver, not because such files structurally cannot exist.

`read_user_class` still reads field values in **declaration order and never
consults `Field::offset`** — the *XMCOLOR* channel-binding defect is live and
unchanged. That is #3398's known open scope, not re-reported. `ChunkType`
decoding returns `Error::UnknownChunkType` rather than panicking, and the
allocation sites are bounded (`chunk_count.min(bytes.len() / 8)`,
`field_count.min(payload.len() / 12)`).

### Dimension 8 — today's palette-remap change is a Starfield no-op

`79194306` (#3897/#3898, landed today, after the first 2026-09-05 pass) made
`BSLightingShaderProperty` a producer of the greyscale→palette enable bits via
`is_palette_color_from_modern_shader_flags`, which unions the typed flag word
with the FO76/Starfield CRC32 arrays — so it is reachable on Starfield content
by construction. Traced to the end: it cannot misfire here. The pack site
(`byroredux/src/cell_loader.rs`, `pack_imported_material_flags`) gates on
`material.textures.greyscale_lut.is_some() && material.bgsm_greyscale_lut_enabled`,
and both shader branches in `triangle.frag` gate again on
`mat.greyscaleLutIndex != 0u`. Vanilla Starfield authors **no `BSShaderTextureSet`
at all** (see the histogram), so the `greyscale_lut` role is never filled and
the flag is never packed. Double-gated, verified clean — recorded because it is
new, Starfield-reachable code that no prior Starfield audit has seen.

### Other guards re-read and intact

`normalize_mesh_path` leaves a `geometries\` head untouched (#1292);
`extract_bs_geometry` skips `scale<=0` sentinel slots on **both** the Stage-A
`find_map` and the Stage-B external-`.mesh` loop (#1828/#1829) and iterates every
LOD slot rather than `meshes.first()` (#1209); `BSGeometryMeshData::parse` gates
the meshlet/cull trailer on `stream.remaining() == 0` so a body ending at the LOD
array is "no trailer" while one truncated *mid*-trailer still errors (#3777) —
`FaceMeshes` parses 1,282/1,282 clean, which is the population that regression
protects; `XCLL_SIZES_STARFIELD = [28, 108]` (#1291); the Starfield LIGH `DAT2`
component-block decode with its `starfield_ligh_dat2_decodes_to_light_data` test
(#1567); the PDCL named skip with its "recorded in skip telemetry, not lost to
the catch-all" test (#1568); the `base_layer` (not `final_layer`) static-trimesh
collision gate (#1294); `5bca381a`'s patch-archive ordering fix is applied to the
`starfield` profile (`MeshesPatch`/`LODMeshesPatch`/`TexturesPatch01,02` all
listed last), and all 21 archives the profile names exist on disk.

---

## Relationship to the first 2026-09-05 pass

`AUDIT_STARFIELD_2026-09-05.md` (HEAD `fa5c4191`, `texture-roles-deep` preset)
filed 6 findings. Status at this pass's HEAD `6fba2b0a`:

| Its finding | Status now |
|---|---|
| D7-01 archive precedence inverted by #3637 | **FIXED** by `5bca381a` (#3896, CLOSED). Re-verified against the live profile. |
| D8-01 `slot_to_role` vs `canonical_shader_type` disagree on Starfield | Open as **#3900**. My histogram adds evidence: vanilla Starfield ships **zero** `BSShaderTextureSet`, so the inconsistency is latent-only today, exactly as filed. |
| D9-01 `record_external_texture_sources` has no exhaustiveness guard | Open as **#3903**. Not re-examined. |
| D3-01 `Mat` texture provenance unreachable | Open as **#3906**. Not re-examined. |
| D8-02 `nifal.md` BGEM glass roles | Open as **#3907**; doc fixed by `2853464f`. |
| D8-03 `_audit-common.md` says 18 roles, live struct has 22 | **FIXED** by `2853464f`. |

**No finding in this report duplicates one of those.** The first pass covered
the material/texture-role axis; this pass covers the skin chain, the archive
and parse-rate surface, and the ESM classifier — all of which it explicitly
listed as not re-measured.

Where this pass **supersedes** prior conclusions:

- **#3549's premise** ("the identity is not in the file at all") is
  contradicted by direct measurement — see SF2-…-D2-01.
- **#3524's scope** ("six residual MeshesPatch truncations") understates the
  population by 13 identical cases in a second archive — see SF2-…-D7-01.
- The skill's *"the residual truncation tail in Meshes01/MeshesPatch"* is stale
  on the Meshes01 half: **Meshes01 is 100.00% clean, 0 truncated**. The tail is
  MeshesPatch + ShatteredSpace - Main01.

---

## CRC32 Flag Table

No new flag-name → CRC32 mappings were derived this pass. The constants in use
on the Starfield path are unchanged and are consumed via
`modern_effect_shader_bit`'s typed-word ∪ SF1-CRC ∪ SF2-CRC union:

| Flag | CRC32 | Constant |
|---|---|---|
| `SLSF1::Greyscale_To_PaletteColor` | `442246519` | `bs_shader_crc32::GRAYSCALE_TO_PALETTE_COLOR` |
| `SLSF1::Greyscale_To_PaletteAlpha` | `2901038324` | `bs_shader_crc32::GRAYSCALE_TO_PALETTE_ALPHA` |
| `SLSF1::Soft_Effect` | see `shader_flags.rs` | `bs_shader_crc32::SOFT_EFFECT` |
| `SLSF2::Effect_Lighting` | see `shader_flags.rs` | `bs_shader_crc32::EFFECT_LIGHTING` |

The hashes remain opaque in the sense that there is no reverse table from an
*observed* CRC to a flag name — only forward constants for the four bits the
importer needs. Deriving the full inverse table would require the flag-name
vocabulary, which is not in nif.xml for BSVER ≥ 152.

## Remaining-Work Chain

Unchanged in ordering from the skill; the measurements above only sharpen the
sizing:

1. **Per-field CDB extraction** (#3398 Phase 2). Key and field vocabulary are
   **solved**; the blocker is the indexed reader that avoids the corpus-wide
   parse peak — now confirmed to be **13 CDBs, two of them full-size at
   ~105 MB each**, so the ~18 GB figure stands. Plus the *XMCOLOR*
   declaration-order-vs-`Field::offset` fix.
2. **`SkinAttach` bone-name consumption** — new to this chain, and cheaper than
   everything else on it: the data is parsed, exact, and count-checked on
   100% of the affected population (SF2-…-D2-01).
3. **PDCL ahead of GBFM** (SF-D4-01) — 74.9% of unresolved Cydonia REFRs vs
   0.081%, ~900× more impactful by the baseline doc's own promote/defer metric.
4. Exterior worldspace tiles; space-cell / planet / GBFM records.
5. The #2105/#3524 NIF truncation tail — now localised to the
   `BSWaterReferenceStruct` per-entry skip, 19 files, not growing.

## Dimension Summary

| Dim | Area | Result |
|---|---|---|
| 1 | BA2 v2/v3 + LZ4 | **Clean** — 129 archives censused, 5,924 extracts + 120,543 GNRL extracts, 0 failures |
| 2 | BSGeometry mesh extraction | **1 HIGH + 1 MEDIUM** — `SkinAttach` / `BoneTranslations` decoded and unconsumed; all named guards intact |
| 3 | CDB material database | **Clean + 1 LOW** (doc-rot); #3398 / #3889 known-open, not re-reported |
| 4 | ESM resolve-rate baseline | **Not measured** (memory constraint) — guards verified statically |
| 5 | ESM + cell bring-up | **Clean** — 55/55 plugins classify correctly; XCLL / PDCL / LIGH-DAT2 / base_layer guards intact |
| 6 | NIF shader blocks BSVER 155+ | **Clean + 1 LOW** — #1510 guard holds (0 NiUnknown on 483,837 blocks); tails quantified |
| 7 | Real-data validation | **Clean + 1 LOW** — 100% recoverable, 19 truncated (no growth), scope correction on #3524 |
| 8 | NIFAL canonical translation | **Clean** — today's palette change is double-gated and inert on Starfield |
| 9 | BGSM/BGEM external flow | **Clean** — 0 `.bgsm`/`.bgem` files corpus-wide; #3230 fall-through intact |

## Deduplication

Dedup list re-pulled mid-audit (max issue `#3912`, 121 open). Checked against
all 20 prior `AUDIT_STARFIELD_*` reports. Known-open items deliberately **not**
re-filed: #3398 (CDB Phase 2 + *XMCOLOR* offsets), #3889 (test-only
`register_starfield_cdb`), #3900 / #3903 / #3906 / #3907 (the first pass's open
findings), #2625 (drift telemetry — extended with measurement instead), #3524
(truncation tail — extended with scope + localisation instead), #2637, #1576.
Findings sourced from pre-2026-06-07 reports were verified against code rather
than against GitHub, per the shared protocol.

---

*Report generated by `/audit-starfield`. Suggested next step:*
`/audit-publish docs/audits/AUDIT_STARFIELD_2026-09-05b.md`
*(label every finding `game:starfield` + `legacy-compat`, plus its domain label:
D2-01/D2-02 → `nif` `import-pipeline`; D7-01 → `nif-parser`; D6-01 →
`nif-parser` `test-gap`; D3-01 → `doc-rot`.)*
