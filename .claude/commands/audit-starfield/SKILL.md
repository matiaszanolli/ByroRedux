---
description: "Per-game audit of Starfield compatibility — BA2 v2/v3 + LZ4 block, CDB materials, BSGeometry .mesh resolution, walkable Cydonia interior"
argument-hint: "--focus <dimensions>"
---

# Starfield Compatibility Audit

Depth/correctness audit of ByroRedux's **Starfield** support. Starfield is a
first-class `GameKind`: NIF + BA2 v2/v3, CDB + BGSM/BGEM materials, and a
**walkable Cydonia interior** all ship today. This is a regression-and-depth
audit of that bring-up surface, **not** a from-scratch gap inventory.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations,
methodology, deduplication rules, finding format, and the SF Material / SF Smoke
entries. See `.claude/commands/_audit-severity.md` for the severity scale (the
NIFAL rows gate the `translate_material` boundary at HIGH minimum).

## Game Context

Authoritative status lives in `ROADMAP.md` (Starfield compat-matrix row +
parse-rate breakdown), `docs/feature-matrix.md` (Starfield-Specific section),
and the two ESM specs `docs/engine/starfield-esm-roadmap.md` +
`docs/engine/starfield-esm-phase0-baseline.md`. Do not duplicate counts here —
read those at audit time. Snapshot of the shape, not the numbers:

| Aspect      | State (verify against ROADMAP) |
|-------------|--------------------------------|
| NIF format  | BSVER 155 (FO76 baseline) → Starfield retail extensions on top |
| BA2 format  | v2 + v3; v3 adds a 12-byte header extension carrying `compression_method` (0 = zlib, 3 = LZ4 block) |
| ESM parser  | **Live** — `GameKind::Starfield` via HEDR-0.96 classifier; existing dispatch captures Starfield content at ~99.9% record parity (per `starfield-esm-roadmap.md` plan revision) |
| Mesh path   | `BSGeometry` (inline geom data **or** external `geometries\<X>.mesh` companion) — NOT `BSTriShape` |
| Materials   | CDB (`crates/sfmaterial/`) for vanilla `materialsbeta.cdb` + external BGSM/BGEM (`crates/bgsm/`), both wired via `--materials-ba2` |
| Cell        | **Walkable Cydonia interior** (#1289/#1291/#1292/#1294/#1295) |
| Reference   | `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` |

### Known Specifics (where to look, not what to assume)

- **CRC32-hashed shader flag arrays** (BSVER ≥ `FO4_CRC_FLAGS` = 132) —
  `BSLightingShaderProperty` / `BSEffectShaderProperty` store shader flags as
  arrays of CRC32 hashes (`sf1_crcs` / `sf2_crcs`) instead of bit masks. Parsed in
  `parse_skyrim_shader_base` in `crates/nif/src/blocks/shader.rs`. SF2 array
  gated on BSVER ≥ `FO76_SF2_CRCS` = 152.
- **BSVER == `FO76` (155) baseline** — `BSShaderType155` dispatch + the
  luminance / translucency / texture-array tail; this is where #1510 lived.
- **BGSM / BGEM material references** — `is_material_reference` (`shader.rs`)
  short-circuits when `Name` is a non-empty `.bgsm`/`.bgem` path and returns a
  material-reference stub; the real material is the external file, parsed by
  `crates/bgsm/` and folded in by `merge_external_material`
  (`byroredux/src/asset_provider/material.rs`).
- **CDB material database** — vanilla Starfield ships all material data inside a
  single `materials\materialsbeta.cdb` Component Database (in
  `Starfield - Materials.ba2`), consumed by `crates/sfmaterial/` and extracted
  via `--materials-ba2`.
- **BA2 v3 compression** — header has a 12-byte extension (vs 8 for v2). GNRL +
  DX10 both dispatch through a unified decompress path selected by archive-level
  `compression_method` (`Ba2Compression` in `crates/bsa/src/ba2.rs`).
- **Live full-engine blocker (#3540, OPEN, filed from `docs/audits/AUDIT_RUNTIME_2026-08-30.md`
  RT-1, not yet fixed)** — a real `cargo run` on `citycydoniamainlevel` loads the
  cell (95,095 fixed colliders, `grounded=true`) then stalls dead at `M28.5
  frame 0`: single-core-pinned, RSS oscillating 12→20.6 GB over a 10-minute
  window. This dimension suite's own methodology deliberately avoids a full
  engine launch (bounded examples + `--ignored`-gated tests only) and is
  unaffected, but do not assume `--bench-hold` against Cydonia currently
  produces a bench line — it doesn't.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `2,9`). Default: all 9.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/starfield`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Starfield/Data/` exists; if not, note which dimensions lose real-data validation.

## Phase 2: Launch Dimension Agents (parallel)

Dimensions are ordered by Starfield-specific risk: the highest-risk seams
(BA2 v3/LZ4 decompression, CDB material correctness, BSGeometry `.mesh`
resolution, ESM resolve-rate) come first.

### Dimension 1: BA2 v2 / v3 — LZ4 Block Decompression
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/ba2.rs`
**Checklist**: v2 header (8-byte extension) vs v3 header (12-byte extension with
`compression_method` at the correct offset). Dispatch via the `Ba2Compression`
enum: `0` → zlib, `3` → LZ4 block, others → error (confirm the unsupported-method
branch is a hard error, not a silent fall-through). `lz4_flex` block decompress —
does it need an explicit `max_size`, and does BA2 supply it from the chunk's
uncompressed size? Per the module doc, v3 DX10 mips can **mix raw and
LZ4-compressed chunks within one texture** — the selector is a `packed_size == 0`
marker per chunk (not a compressed/uncompressed-size comparison); verify it picks
raw vs. LZ4-decompress correctly (measured 2026-08-30: 3.66% of v3 textures
genuinely mix both within one texture, and zero chunks carry a nonzero
`packed_size` equal to `unpacked_size`, so the sentinel is unambiguous).
GNRL + DX10 must both reach the unified decompress path. Regression guard:
DX10 chunk layout is unchanged from FO4 v1 — the v3 issue was the
`compression_method` offset, not a per-chunk-layout difference. Parse-rate sweep
across all v2 and v3 archives (extract rate is 100% per the compat matrix —
confirm it holds).
**Output**: `/tmp/audit/starfield/dim_1.md`

### Dimension 2: BSGeometry Mesh Extraction (Starfield's actual mesh path)
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/import/mesh/bs_geometry.rs` (geometry extraction),
`crates/nif/src/blocks/bs_geometry.rs` (block parse), with
`crates/nif/src/import/mesh/bs_tri_shape.rs` as the FO4/Skyrim contrast
(Starfield does NOT use `BSTriShape`)
**Checklist**: `extract_bs_geometry` — Stage A inline geometry
(`has_internal_geom_data()`) vs Stage B external `.mesh` companion.
**#1292** — external `.mesh` resolved via the canonical `geometries\<X>.mesh`
path; the importer must NOT prepend `meshes\` (implemented in
`byroredux/src/asset_provider/archive.rs`, regression-guarded by the
`normalize_mesh_path` tests in `byroredux/src/asset_provider/tests/material_path.rs` —
confirm the `geometries\` head is left untouched). Without this the Cydonia spawn rate
collapses. **#1209** — iterate every LOD slot, not `meshes.first()` (a `None`
short-circuit when LOD 0 was external despite later internal slots).
**#1828/#1829 (`ba728882`)** — both the Stage A `find_map` and the Stage B
external-`.mesh` loop must also skip a slot whose body is the `scale<=0`
sentinel (empty `vertices`/`triangles`) even when it parses `Ok`/matches
`Internal` first — accepting a sentinel-first slot silently drops the whole
BSGeometry. A regression re-accepting the first `Internal` match or first
`Ok(...)` parse without the emptiness check reintroduces this.
**#1203** — skin chain resolved via `BSSkin::Instance` + `BSSkin::BoneData` +
`mesh_data.skin_weights`. **#3549** — `BSSkin::Instance.bone_refs` are NULL on
73% of Starfield skin refs (all-null on 3,738/5,896 skins), so resolving by
in-file node name alone leaves every affected NPC/apparel piece in bind pose;
`crates/nif/src/import/mesh/skeleton.rs::solve_bone_names` recovers names by
fitting each skin's bind-pose offsets against an externally-resolved skeleton
(via the same `MeshResolver` precedent as Stage B's `.mesh` lookup), declining
rather than guessing on an ambiguous or non-unique fit. Confirm a decline still
falls back to the prior `Bone{i}` placeholder (never worse) and that a unique
fit is still required before any name is accepted — measured recovery is ~21%
of clothes skins / ~3,900 of ~19,500 bones, the remainder correctly declining.
**#3777** — `BSGeometryMeshData::parse` (`crates/nif/src/blocks/bs_geometry.rs`)
must treat EOF at the post-LOD meshlet/cull-data trailer as "no trailer
present" (Starfield facegen `.mesh` bodies end exactly at the LOD array and
ship none), not a hard parse error — pre-fix this silently zeroed all geometry
in `Starfield - FaceMeshes.ba2` (1,282/1,282 NIFs, every Starfield NPC head)
because Stage B's per-slot `Err` arm only `debug!`-logs and `continue`s. A
regression here is invisible to `.nif`-block parse-rate gates (the `.nif`
files themselves still parse at 100%) — confirm `NifStream::remaining() == 0`
gates the trailer read and a body truncated *mid*-trailer still errors.
**#1232** — empty/zero-length tangent blobs route through
`synthesize_tangents_yup` (Mikkelsen); verify the fallback is reached and
produces unit-length tangents (vanilla Starfield never actually exercises this
path — 0 of 675,407 `.mesh` bodies lack authored tangents — so don't expect a
live repro; confirm the fallback stays correct on a synthetic fixture instead).
PBR scalars `metalness_override` / `roughness_override` are forwarded from the
BGSM-resolved `legacy_pbr`. Watch for new vertex-attribute bits beyond FO4's
set and Starfield's far-higher vertex counts.
**Output**: `/tmp/audit/starfield/dim_2.md`

### Dimension 3: CDB Material Database Correctness
**Subagent**: `renderer-specialist`
**Entry points**: `crates/sfmaterial/src/reader.rs` (`ComponentDatabaseFile::parse`,
`index_chunks`), `crates/sfmaterial/src/chunk.rs`, `string_table.rs`, `types.rs`,
`value.rs`, `byroredux/src/asset_provider/material.rs` (`--materials-ba2` wiring)
**Checklist**: `ComponentDatabaseFile::parse` consumes `materials\materialsbeta.cdb`
extracted from `Starfield - Materials.ba2` via `--materials-ba2`. **#762** —
guard `index_chunks` against the chunk-index regression already referenced in
`byroredux/src/asset_provider/tests/starfield_mat.rs`. **DLC/Creation CDB discovery by scanning (#1571, `8c99c50d`)** —
`asset_provider/material.rs::discover_starfield_cdbs` scans each materials archive for
**every** `materials\materialsbeta.cdb` AND DLC/Creation-namespaced
`materials\creations\<plugin>\materialsbeta.cdb`, instead of extracting one
hardcoded base path; a regression that re-hardcodes the single base path silently
drops every DLC/Creation material database. Walk the parse path: header (`parse_header`) → chunk index
(`index_chunks`) → class parse (`parse_class`). Are unknown `ChunkType` /
`Value` variants handled (warn-and-skip) or do they bail/panic? Confirm
`peek_magic` correctly distinguishes a CDB from a loose BGSM. Correctness, not
just "it parses": does the per-`.mat` material resolution forward roughness /
metalness / texture-slot values into the `ImportedMesh`, or do `.mat`-resolved
materials currently reach the Disney lobe with NIF defaults? (Per-field CDB
extraction is the #1289/#3398 Phase 2 follow-up — confirm current state and
scope the gap, don't re-report it as new.)
**CDB Phase 2 is UNBLOCKED as of 2026-08-29 (#3398, `c5cd4e6f`) — audit text
calling the lookup key or the field vocabulary "unknown" is stale.** Measured
against the vanilla 105 MB CDB and 3 085 real NIF-named material paths, and
recorded in `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md`:
  * The key exists and is computable. `BSMaterial::Internal::CompiledDB.HashMap`
    is a 48 749-entry `BSResource::ID → u64` index; `DBFileIndex::ObjectInfo`
    carries the same `BSResource::ID` as *PersistentID*, joining path → DBID →
    components → edge graph.
  * The hash is **reflected CRC-32 (poly `0xEDB88320`), init 0, no final XOR**,
    over the lowercased backslash path, hashed as **directory and stem
    separately**. One of nine tried parameterisations matched, at 3 032/3 084
    (98.3%); the reversed column assignment matches 0/3 084.
  * `BSResource::ID`'s decoded field labels are **rotated** relative to their
    contents: `.Dir` holds the stem hash, `.Ext` holds the directory hash, and
    `.File` is the constant `0x0074616d` — the literal `"mat"` extension. The
    rotation is NOT explained by the `read_user_class` offset defect below
    (`BSResource::ID` declares ascending offsets); it stays unresolved and does
    not affect the empirical column semantics.
  * 61 `BSMaterial::*` classes are reached, ~20 relevant, already tabulated
    against their `ImportedMaterial` targets with real field names and value
    shapes. Enum fields are **strings** (`"Deferred"`, `"AlphaBlend"`,
    `"MATERIAL_LAYER_0"`) — each needs an explicit arm plus a documented
    default. The old "schema and counts only" framing was wrong.
  * **The real Phase-2 blocker is memory, not vocabulary**: `parse` peaks at
    **9.19 GB on the single 105 MB base CDB**, and `ParseLimits` is a pre-walk
    reject, not a streaming budget. That sizing is per-CDB, not corpus-wide —
    there are **13 CDBs totalling 3,077,172 chunks / ~232 MB** (re-measured
    2026-08-30, SF-D3-01), and **two** of them are full-size (~105 MB /
    ~1.46 M chunks each, not one), so a Phase-2 reader reusing today's `parse`
    across the discovered set would peak north of **~18 GB**, not 9.19 GB. An
    indexed reader is the project.
  * **Live defect, still open (#3398, `93095413`)**: `read_user_class` reads
    field values in *declaration* order and never consults `Field::offset`. For
    96 of 97 CDB classes the two orders agree; *XMCOLOR* is the exception —
    declared `r,g,b,a` at offsets 2,1,0,3 — so its channels bind to the wrong
    values today. The declaration-order-vs-offset-order check lives in the spike
    example.
Count unique CDB material handles vs loose BGSM/BGEM references in a Starfield
archive — but do NOT conclude "CDB supersedes loose-file BGSM" as a rule: the
key's extension column is the literal constant `"mat"`, so a lookup ignores the
reference's own suffix, and 17 of 57 `.bgsm`/`.bgem`-named paths in the sampled
corpus resolve to real CDB materials. Neither "always CDB" nor "always BGSM" is
correct — #3230 (`e3dd71e8`) made it **try-then-fall-through**: `.bgsm`/`.bgem`
fall through to `resolve_bgsm` / `resolve_bgem` first and reach the CDB PBR flip
(`apply_cdb_pbr_fallback`, `byroredux/src/asset_provider/material.rs`) only on a
resolver miss, while `.mat` keeps its early return. The short-circuit's premise
is narrower than the comment used to claim (#3782): **vanilla** Starfield ships
no `.mat`/`.bgsm`/`.bgem` sidecars, but an installed Creation/mod archive can (20
JSON `.mat` exports measured across 129 installed archives, 2026-08-30) — the
short-circuit is retained because no JSON `.mat` resolver exists yet, not because
the files structurally cannot exist. Separately, zero `.bgsm`/`.bgem` **files**
exist in any vanilla Starfield archive either (SF-D9-01), so every `.bgsm`/`.bgem`-
*named* reference is a guaranteed resolver miss that falls through to the CDB
flip — the CDB is Starfield's only real material source today, not one of
several. A re-added early `PresenceOnly` return above the resolvers is the
regression — it discarded every authored texture role, `glass_enabled` flag and
PBR scalar.
**Output**: `/tmp/audit/starfield/dim_3.md`

### Dimension 4: Starfield ESM Resolve-Rate Baseline
**Subagent**: `general-purpose`
**Entry points**: `byroredux/src/sf_smoke.rs` (`--sf-smoke <CELL_EDID>` resolve-rate
harness), `crates/plugin/examples/sf_smoke.rs` + `crates/plugin/examples/sf_parse_check.rs`
(top-level GRUP byte-coverage tools), `docs/engine/starfield-esm-phase0-baseline.md`
**Checklist**: The two tools answer different questions — keep them straight.
`crates/plugin/examples/sf_smoke.rs` measures **byte/FourCC coverage** of the
top-level GRUP walk vs `DISPATCH_HANDLED_FOURCCS`; `byroredux/src/sf_smoke.rs`
(`--sf-smoke`) measures the **per-cell base-form resolve rate** (of N REFRs in a
named interior cell, how many point at a base form actually decoded into
`EsmCellIndex.statics`). Run `--sf-smoke` against Cydonia and confirm the resolve
rate has not regressed below the Phase 0/1 baseline. A drop = the CELL handler
silently dropped REFRs (moved subrecord size, new XCLL field) or a base record
(STAT/MSTT/FURN/LIGH) failed to index — REFRs then spawn the 3D-unit-cube
placeholder. Cross-check the per-record-type breakdown for new Starfield-only
base types (GBFM/GBFT/PNDT/STDT/BIOM) showing up where a real parser is missing;
note frequency, don't re-report the known GBFM stub gap.
**#1567 (`0d9ee07f`)** — Starfield `LIGH` records carry no `MODL`/`DATA`, only a
component-block `DAT2` payload; `build_static_object_from_subs` must decode it
(test: `starfield_ligh_dat2_decodes_to_light_data`) or every REFR pointing at a
LIGH misses at the static lookup and drops silently (656 Cydonia lights
pre-fix). Regression guard for this dimension's resolve-rate baseline.
**Output**: `/tmp/audit/starfield/dim_4.md`

### Dimension 5: ESM + Cell Bring-up Regression Surface
**Scope split with `/audit-esm` (added 2026-08-13)**: `/audit-esm` owns the parser *as a parser* — GRUP walk, `SubReader` byte accounting, schema dispatch, FormID remap. This dimension owns **this game's data through it**: record counts, game-unique authoring, and the semantics that only show up on this title's masters. If the defect is in the shared mechanism, file it against `/audit-esm` instead of here.
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/reader.rs` (`GameKind::Starfield` HEDR-0.96
classifier), `crates/plugin/src/esm/records/mod.rs` (FourCC dispatch),
`crates/plugin/src/esm/cell/walkers.rs` (XCLL + per-cell NAVM),
`byroredux/src/cell_loader/spawn.rs` (REFR placement)
**Checklist**: HEDR-0.96 → `GameKind::Starfield` classification (`reader.rs`).
FourCC dispatch coverage in `records/mod.rs` — which record types are parsed vs
warned-skip; cross-check against the resolve-rate baseline from Dim 4.
**PDCL conscious skip (#1568, `b804c180`)** — the Starfield `PDCL`
(BGSProjectedDecal) GRUP is skipped *consciously* (named into
`index.skipped_unconsumed_groups` + a one-shot warn) rather than vanishing into
the anonymous catch-all; verify it stays a named skip (so coverage tooling counts
it) and does not silently regress into the catch-all.
**#1291** — `XCLL_SIZES_STARFIELD = [28, 108]` (`walkers.rs`), split off the
Fallout-era `[28, 40]` bucket. **Important correction to any stale doc**: the
108-byte Starfield XCLL is **NOT** "Skyrim's 92-byte body + a 16-byte tail" — per
the `walkers.rs` doc comment it shares only bytes 0-39 with Skyrim and is decoded
in full against xEdit SF1 `wbStruct(XCLL,'Lighting')` (the old #1293
"16-byte-tail follow-up" framing is resolved). Per-cell NAVM collection
(`walkers.rs`, #1272). Spawn-path regression guards in `cell_loader/spawn.rs`:
**#1294** static-trimesh fallback gated on `base_layer` not `final_layer`
(synthesized collider count was 0 before the fix); **#1235** `SceneFlags::from_nif`
(`crates/core/src/ecs/components/scene_flags.rs`) attached at spawn;
**#1295** `DoorTeleport` stamped from REFR XTEL; **#1212/#1213/#1214**
`FormIdComponent` / `LocalBound` / `BSXFlags` at spawn; **#1284** `SkinSlotPool`
ceiling raise (`crates/core/src/ecs/resources/skin_slot_pool.rs`) for Cydonia's skinned density.
Also confirm synthesized colliders stay out of the BLAS: *IsCollisionOnly* was
removed as dead code by #1570 (2026-06-15) — the real exclusion mechanism is
structural, not marker-based. `spawn_trimesh_collider_ghost` /
`spawn_packed_havok_proxy` (`byroredux/src/cell_loader/spawn.rs`) spawn
colliders without a `MeshHandle`, so they can never enter `blas_specs`
regardless of any marker component (R6a-stale-13/14 collider-cost fix, see
ROADMAP).
**Output**: `/tmp/audit/starfield/dim_5.md`

### Dimension 6: NIF Shader Blocks — BSVER 155+ (regression guard)
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/shader.rs` (`parse_skyrim_shader_base`,
`BSLightingShaderProperty`, `BSEffectShaderProperty`), `docs/legacy/nif.xml`
**Checklist**: CRC32 flag-array parsing for BSVER ≥ `FO4_CRC_FLAGS` (132) —
`num_sf1` + per-element u32 CRC into `sf1_crcs`; SF2 array for BSVER ≥
`FO76_SF2_CRCS` (152) into `sf2_crcs`. Is there a CRC32 hash → flag-name table,
or are the hashes opaque? **#1510 regression guard** — `BSShaderType155` dispatch
+ the luminance / translucency / texture-array tail in `shader.rs` previously
over-read by 4 B, truncating all ~1036 Starfield full-body
`BSLightingShaderProperty` blocks to `NiUnknown`; confirm the block-histogram
NiUnknown count for these stays at 0. WetnessParams extended fields, refraction
power on `BSEffectShaderProperty` (FO76-style), and the new BSEffectShaderProperty
textures (Reflectance / Lighting / Emittance / Emit Gradient) — verify byte
consumption against nif.xml.
**#1606 undocumented BSLightingShaderProperty tail (`497700e7`)** — the empty-name
full-body Starfield `BSLightingShaderProperty` carries a 30-byte trailing field
(7× f32 + 2 B) that nif.xml does NOT document (re-measured 2026-08-31 via #3474 —
an earlier 38-byte / 9× f32 recording was 8 bytes stale after #2622 moved a
leading float pair into the Starfield decode path proper); the dispatcher passes
the declared `block_size` to `BSLightingShaderProperty::parse_with_size`, which captures
`block_size - consumed` trailing bytes **opaquely** into `starfield_tail: Vec<u8>`
(gated `bsver >= STARFIELD`). The legacy `parse` (None size) path is unchanged and
yields an empty tail. Verify the tail is captured to-block_size (not a hardcoded
length, no over-read) and that LODMeshes drift stays at 0 — do NOT fabricate field
names/semantics. Tests: `parse_bs_lighting_starfield_captures_trailing_tail` +
`..._tail_empty_without_size_or_drift` in `crates/nif/src/blocks/shader_tests/starfield.rs` (split by era, #2056).
The sibling BSEffectShaderProperty +32 B under-read on the same archive is a known
follow-up (left scoped out) — note frequency, don't re-file as new.
**Output**: `/tmp/audit/starfield/dim_6.md`

### Dimension 7: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**: Parse rate holds at the compat-matrix figure (see ROADMAP
Starfield row + `docs/engine/game-compatibility.md`) via
`BYROREDUX_*_DATA=... cargo test -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes -- --ignored`
(walks all 13 mesh-bearing archives since the #3466 corpus widening;
`parse_rate_starfield` covers Meshes01 only).
The residual truncation tail in Meshes01/MeshesPatch is tracked at #2105/#3524
(`BSWeakReferenceNode`'s residual, characterised at 19 files at the
2026-08-30 measurement) — **not #746/#747**, both CLOSED and unrelated
(they were the version-gating `bsver == 155` defects whose fix *reduced*
the tail, not truncation trackers themselves; already flagged once as
stale by #2365). Confirm the residual count has not grown. Verify
Starfield texture archives matching
`Starfield - *Textures*.ba2` extract cleanly (compat matrix records 100% extract
recover, post-#754). Pick 5 representative meshes — a clutter item, a ship hull,
a character body, a weapon, a landscape feature — and trace each through
`import_nif_scene` (`crates/nif/src/import/mod.rs`). Watch for `NiUnknown`
placeholders in the block histogram — these flag new block types introduced since
the FO76/Starfield baseline.
**Output**: `/tmp/audit/starfield/dim_7.md`

### Dimension 8: NIFAL Canonical Material Translation for Starfield
**Subagent**: `renderer-specialist`
**Entry points**: `byroredux/src/material_translate.rs` (`translate_material` — the
single boundary), `crates/core/src/ecs/components/material.rs`
(`Material::resolve_pbr`)
**Checklist**: `translate_material` is the **single** raw `ImportedMesh` → ECS
`Material` boundary — per-game / per-material classification happens here, never
per-draw in the shader (see also `/audit-nifal`). Verify BSGeometry/BGSM/CDB-
resolved Starfield meshes land with `Material.metalness` / `Material.roughness` as
**plain resolved `f32`** (`material.rs`), set once — no `Option<f32>` per-draw
`classify_pbr` plumbing (removed by the NIFAL refactor; `resolve_pbr` is the
resolve-once fill). Confirm `Material::resolve_pbr` and the `EmissiveSource`
discriminator (#1280, tagged in `crates/nif/src/import/material/dedicated_shader.rs`
and `legacy_properties.rs` since the #2059 `walker.rs` split) behave
for SF content. **NIFAL particle slice (`NiPSysEmitter`/`NiPSysEmitterCtlr`) and
the per-shape collision slice (`BhkMultiSphereShape`/`BhkConvexListShape`) are
types vanilla Starfield ships ZERO of** (confirmed 2026-08-30 over the full
block histogram across all six mesh archives, 24 distinct block types
observed) — do not audit them here; a regression test against either would be
vacuous. Starfield collision is entirely the `bhkNPCollisionObject` (59,761) /
`bhkPhysicsSystem` (40,724) / `bhkRagdollSystem` (571) `BhkSystemBinary` blob
path, not a per-shape translate path — Cydonia's colliders come from the
synthesized fallback in `byroredux/src/cell_loader/spawn.rs` (see Dimension 5),
not from `crates/nif/src/import/collision/shape.rs`.
**Output**: `/tmp/audit/starfield/dim_8.md`

### Dimension 9: BGSM/BGEM External Material Flow
**Subagent**: `renderer-specialist`
**Entry points**: `crates/bgsm/src/bgsm.rs` + `crates/bgsm/src/bgem.rs` (external
parser), `byroredux/src/asset_provider/material.rs` (`merge_external_material`),
`byroredux/src/cell_loader.rs` (`pack_imported_material_flags`)
**Checklist**: The material-reference stub from `shader.rs` resolves to the
external file — confirm the BGEM variant (`bgem.rs`) is handled distinctly from
BGSM (`bgsm.rs`): different texture-set conventions plus the BGEM `glass_enabled`
flag. `merge_external_material` folds the parsed result into `ImportedMesh.material`
(an `ImportedMaterial` — it takes `&mut ImportedMaterial`, so it cannot touch
geometry/skinning; a widened signature is a NIFAL boundary violation);
Starfield `.mat` texture paths must land in `MaterialTextureSet` roles, never
in a CDB-specific slot index;
`pack_imported_material_flags` packs `byroredux_renderer::vulkan::material::material_flag::{BGSM_AUTHORED, PBR_BSDF, TRANSLUCENCY, MODEL_SPACE_NORMALS, EFFECT_PALETTE_COLOR}`
(#1147 / #1077 / #1076 / #1280) — verify each flag derives from the right
`ImportedMaterial` field. BGEM `glass_enabled` (`bgem.rs`) is the authoritative glass
signal (#1280), consumed in `byroredux/src/helpers.rs` (and must NOT misclassify an
opaque architecture piece carrying a stuck flag — there's a regression test for
that). **Disney BSDF / PBR (#1248-#1252)** is the canonical lobe (GLSL-PathTracer
MIT + Burley 2012, attribution at top of `crates/renderer/shaders/triangle.frag`);
the classification feeding it happens at the single `translate_material` boundary
(Dim 8), not per-draw.
**Output**: `/tmp/audit/starfield/dim_9.md`

## Phase 3: Merge

1. Read all `/tmp/audit/starfield/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_STARFIELD_<TODAY>.md` with structure:
   - **Executive Summary** — Current state: Starfield is a first-class `GameKind`
     with NIF + BA2 at the compat-matrix rate, CDB + BGSM/BGEM materials, and a
     walkable Cydonia interior. This is a depth/correctness audit — focus on
     regressions in the bring-up surface (BA2 v3 decompress, CDB chunk index,
     BSGeometry `.mesh` resolution, spawn gates, NIFAL translation) and the
     remaining ESM phase work.
   - **Dimension Findings** — Grouped by severity per dimension.
   - **CRC32 Flag Table** — Known/unknown flag-name → CRC32 mappings for the
     shader flag arrays (anything derivable empirically from observed hashes).
   - **Remaining-Work Chain** (per `starfield-esm-roadmap.md` — Phases 0+1 done,
     2-4 invalidated by the 99.9%-parity measurement) — in order: per-field CDB
     extraction (#1289/#3398 Phase 2 follow-up — `.mat`-resolved materials
     currently reach the Disney lobe with NIF defaults; the lookup key and field
     vocabulary are **solved** as of 2026-08-29, so what is left is the indexed
     reader that avoids the corpus-wide ~18 GB parse peak (13 CDBs, two
     full-size — not the single-CDB 9.19 GB figure), plus the *XMCOLOR*
     field-offset fix), **PDCL ahead of GBFM** (SF-D4-01, 2026-08-30: the
     baseline doc's promote/defer rule fires "defer" for GBFM at 0.081% of
     unresolved Cydonia REFRs, while PDCL sits unranked at 74.9% — ~900×
     more impactful by the same metric), exterior worldspace tiles,
     space-cell / planet / GBFM records, and the #2105/#3524 NIF truncation tail.
     Do NOT frame this as a "BGSM parser first / ESM very far" chain — both have
     shipped.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_<TODAY>.md`
(label every finding `game:starfield` + `legacy-compat`, plus its own domain label.)
