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
  `crates/bgsm/` and folded in by `merge_bgsm_into_mesh` (`asset_provider.rs`).
- **CDB material database** — vanilla Starfield ships all material data inside a
  single `materials\materialsbeta.cdb` Component Database (in
  `Starfield - Materials.ba2`), consumed by `crates/sfmaterial/` and extracted
  via `--materials-ba2`.
- **BA2 v3 compression** — header has a 12-byte extension (vs 8 for v2). GNRL +
  DX10 both dispatch through a unified decompress path selected by archive-level
  `compression_method` (`Ba2Compression` in `crates/bsa/src/ba2.rs`).

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
LZ4-compressed chunks within one texture** — verify the per-chunk
compressed/uncompressed-size comparison selects raw-vs-decompress correctly.
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
path; the importer must NOT prepend `meshes\` (regression-guarded by
`normalize_mesh_path` tests in `asset_provider.rs` — confirm
`head == "geometries\\"` is left untouched). Without this the Cydonia spawn rate
collapses. **#1209** — iterate every LOD slot, not `meshes.first()` (a `None`
short-circuit when LOD 0 was external despite later internal slots).
**#1203** — skin chain resolved via `BSSkin::Instance` + `BSSkin::BoneData`.
**#1232** — empty/zero-length tangent blobs route through
`synthesize_tangents_yup` (Mikkelsen); verify the fallback is reached and
produces unit-length tangents. PBR scalars `metalness_override` /
`roughness_override` are forwarded from the BGSM-resolved `legacy_pbr`. Watch for
new vertex-attribute bits beyond FO4's set and Starfield's far-higher vertex
counts.
**Output**: `/tmp/audit/starfield/dim_2.md`

### Dimension 3: CDB Material Database Correctness
**Subagent**: `renderer-specialist`
**Entry points**: `crates/sfmaterial/src/reader.rs` (`ComponentDatabaseFile::parse`,
`index_chunks`), `crates/sfmaterial/src/chunk.rs`, `string_table.rs`, `types.rs`,
`value.rs`, `byroredux/src/asset_provider.rs` (`--materials-ba2` wiring)
**Checklist**: `ComponentDatabaseFile::parse` consumes `materials\materialsbeta.cdb`
extracted from `Starfield - Materials.ba2` via `--materials-ba2`. **#762** —
guard `index_chunks` against the chunk-index regression already referenced in
`asset_provider.rs`. Walk the parse path: header (`parse_header`) → chunk index
(`index_chunks`) → class parse (`parse_class`). Are unknown `ChunkType` /
`Value` variants handled (warn-and-skip) or do they bail/panic? Confirm
`peek_magic` correctly distinguishes a CDB from a loose BGSM. Correctness, not
just "it parses": does the per-`.mat` material resolution forward roughness /
metalness / texture-slot values into the `ImportedMesh`, or do `.mat`-resolved
materials currently reach the Disney lobe with NIF defaults? (Per the ROADMAP
forward-blocker chain, per-field CDB extraction is the #1289 Phase 2 follow-up —
confirm current state and scope the gap, don't re-report it as new.) Count
unique CDB material handles vs loose BGSM/BGEM references in a Starfield archive
to confirm CDB supersedes loose-file BGSM for vanilla content.
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
**Output**: `/tmp/audit/starfield/dim_4.md`

### Dimension 5: ESM + Cell Bring-up Regression Surface
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/reader.rs` (`GameKind::Starfield` HEDR-0.96
classifier), `crates/plugin/src/esm/records/mod.rs` (FourCC dispatch),
`crates/plugin/src/esm/cell/walkers.rs` (XCLL + per-cell NAVM),
`byroredux/src/cell_loader/spawn.rs` (REFR placement)
**Checklist**: HEDR-0.96 → `GameKind::Starfield` classification (`reader.rs`).
FourCC dispatch coverage in `records/mod.rs` — which record types are parsed vs
warned-skip; cross-check against the resolve-rate baseline from Dim 4.
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
ceiling raise (`crates/core/src/ecs/resources.rs`) for Cydonia's skinned density.
Also confirm synthesized colliders carry `IsCollisionOnly` (`components.rs`) so
they stay out of the BLAS (R6a-stale-13/14 collider-cost fix, see ROADMAP).
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
**Output**: `/tmp/audit/starfield/dim_6.md`

### Dimension 7: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**: Parse rate holds at the compat-matrix figure (see ROADMAP
Starfield row + `docs/engine/game-compatibility.md`) via
`BYROREDUX_*_DATA=... cargo test -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes -- --ignored`
(walks all 5 vanilla mesh archives; `parse_rate_starfield` covers Meshes01 only).
The residual truncation tail in Meshes01/MeshesPatch is tracked at #746/#747 —
confirm it has not grown. Verify Starfield texture archives matching
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
discriminator (#1280, tagged in `crates/nif/src/import/material/walker.rs`) behave
for SF content. NIFAL particle slice reaching SF NIFs: typed `NiPSysEmitter` /
`NiPSysEmitterCtlr` (`crates/nif/src/blocks/particle.rs`) →
`extract_emitter_params` / `extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs`)
→ `apply_emitter_params` (`byroredux/src/systems/particle.rs`). NIFAL collision
slice (Cydonia's synthesized + bhk colliders): `BhkMultiSphereShape` +
`BhkConvexListShape` translate to `CollisionShape` in
`crates/nif/src/import/collision.rs`.
**Output**: `/tmp/audit/starfield/dim_8.md`

### Dimension 9: BGSM/BGEM External Material Flow
**Subagent**: `renderer-specialist`
**Entry points**: `crates/bgsm/src/bgsm.rs` + `crates/bgsm/src/bgem.rs` (external
parser), `byroredux/src/asset_provider.rs` (`merge_bgsm_into_mesh`),
`byroredux/src/cell_loader.rs` (`pack_bgsm_material_flags`)
**Checklist**: The material-reference stub from `shader.rs` resolves to the
external file — confirm the BGEM variant (`bgem.rs`) is handled distinctly from
BGSM (`bgsm.rs`): different texture-set conventions plus the BGEM `glass_enabled`
flag. `merge_bgsm_into_mesh` folds the parsed result into `ImportedMesh`;
`pack_bgsm_material_flags` packs `byroredux_renderer::vulkan::material::material_flag::{BGSM_AUTHORED, PBR_BSDF, TRANSLUCENCY, MODEL_SPACE_NORMALS, EFFECT_PALETTE_COLOR}`
(#1147 / #1077 / #1076 / #1280) — verify each flag derives from the right
`ImportedMesh` field. BGEM `glass_enabled` (`bgem.rs`) is the authoritative glass
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
     extraction (#1289 Phase 2 follow-up — `.mat`-resolved materials currently
     reach the Disney lobe with NIF defaults), exterior worldspace tiles,
     space-cell / planet / GBFM records, and the #746/#747 NIF truncation tail.
     Do NOT frame this as a "BGSM parser first / ESM very far" chain — both have
     shipped.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_<TODAY>.md`
