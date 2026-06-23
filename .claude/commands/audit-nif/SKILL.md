---
description: "Deep audit of NIF parser — stream position, version gating, block dispatch coverage, geometry handoff"
argument-hint: "--focus <dimensions> --game <fnv|fo3|skyrim|oblivion|fo4|fo76|starfield> --corpus <path>"
---

# NIF Parser Audit

Deep audit of the NIF binary-format parser (`crates/nif/src/`) for byte-accurate
correctness across the Oblivion → Starfield version span. Tests against real game
data when a corpus is available.

The recurring NIF failure mode is **stream-position drift**: a block over-reads
or under-reads its payload, the consumed-byte count diverges from the header
`block_sizes` entry, and either the `block_size` reconciliation masks it (parse
"succeeds" with silent corruption) or — on Oblivion-era files that ship *no*
`block_sizes` table — every following block is misaligned and the scene
truncates. Dimensions below are ordered by that risk.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout (its NIF Blocks / NIF
Import / NIF Animation lines map the full `crates/nif/src/` tree), game-data
locations, methodology, deduplication, severity, context rules, path-reference
convention, and the base finding format. Do not duplicate any of that here.

`docs/engine/nif-parser.md` is the code-verified reference for this domain
(module map, parse pipeline, version-handling thresholds, per-game coverage
matrix, per-block recovery). Prefer it over re-deriving facts from source.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g. `1,2`). Default: all 6.
- `--game <name>`: Restrict to one variant: `fnv`, `fo3`, `skyrim`, `oblivion`, `fo4`, `fo76`, `starfield`. Default: all detected.
- `--corpus <path>`: Directory of extracted `.nif` files for bulk testing.

## Extra Per-Finding Fields

- **Dimension**: Stream Position | Version Gating | Block Dispatch Coverage | Geometry Handoff | Collision/Shader Parsing | Allocation Hygiene
- **Game Affected**: Which `NifVariant`(s) the finding applies to (cite the `bsver` band)

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/nif`.
3. Fetch the dedup baseline (see `_audit-common.md` → Deduplication).
4. Check which game-data directories exist (`_audit-common.md` → Game Data Locations).
5. Skim `docs/engine/nif-parser.md` § "Per-game NIF coverage" so you measure
   findings against the current clean/recoverable rates rather than re-counting.

## Phase 2: Launch Dimension Agents

### Dimension 1: Stream Position Integrity (PRIMARY)
**Entry points**: `crates/nif/src/lib.rs` (`parse_nif` → `parse_nif_with_options` block loop, `parse_block_with_name_arc` calls), `crates/nif/src/stream.rs` (`position`, `skip`, the `read_*` cursor primitives), every block parser's `parse()`.
**Checklist**:
- Each block consumes exactly its `block_sizes` entry when one exists. The loop in `lib.rs` already diffs consumed-vs-`block_size` and logs both the over-read (recovers by seeking to expected end) and the under-read ("parsed Ok but consumed != block_size") paths — a finding here is a *block whose drift the reconciliation is silently absorbing*, not the reconciliation itself.
- Oblivion-era files (NIF v20.0.0.4/5 and the v10.x NetImmerse family) ship **no** `block_sizes` table (`header.block_sizes.is_empty()` → `no_block_sizes` branch). There is no per-block recovery anchor — one wrong field cascades. Audit these parsers for unconditional reads that assume a later-game layout.
- No read may exceed the block boundary unconditionally; version-gated trailing fields (see Dim 2) must be guarded, not always-read.
- Boolean width correctness: `read_bool` (version-dependent 1-or-4-byte, see `stream.rs:203` + `read_bool_version_dependent` test) vs `read_byte_bool` (always 1 byte) must match the nif.xml type annotation per field. A phantom or missing bool is the classic 1-/3-byte drift (cf. the resolved `NiTriStripsData`/`NiTriShapeData` v10.0.1.x phantom-bool and `NiGeomMorpherController` phantom-weight cases — keep them from regressing).
- `BSShaderPropertyData::parse_fo3` (`crates/nif/src/blocks/base.rs`) is the shared FO3-era shader prelude — confirm only FO3/FNV-era shaders call it and that it returns the `(Self, texture_clamp_mode)` byte-for-byte.
**If corpus available**: parse every NIF and report consumed-vs-`block_size` mismatches grouped by block type with frequency counts; flag any non-zero count on a *known* block type (drift), and separately any `no_block_sizes` truncation.
**Regression guards** (resolved drift — verify still fixed, do not re-report):
- **Oblivion v10.x truncation family**: `#1509` (`NiGeomMorpherController` bsver=9 over-read on v10.2 morph rigs), the v10.1.0.x `NiBlendInterpolator` bands + `ControlledBlock` blend fields, v10.x `NiPSysData` + emitter trailing fields, `#1337` (full v10.0.1.x format), `#1329` (v10.0.1.0 Havok chain), `#1310`/`#1301`/`#1302` (`NiTriStripsData`/`NiTriShapeData`/`NiGeomMorpherController` phantom bool/weight). Method of record: extract → `trace_block` → byte-decode the stride drift (see memory "NIF v10.x stride drift resolved").
- **Starfield `BSLightingShaderProperty` over-read** (`#1510`): drove 1036 `NiUnknown` → 0. Regression = a Starfield (`bsver >= FO76 = 155`) shader reading the FO4 field tail.
- **Starfield `BSLightingShaderProperty` trailing tail** (`#1606`): a `bsver >= STARFIELD` form carries an undocumented 38-B tail (9× f32 + 2 B, nif.xml does not document it). Captured opaque as `starfield_tail: Vec<u8>` (`crates/nif/src/blocks/shader.rs`, `read_starfield_tail`) consumed **to `block_size`** (not a hardcoded 38, so it survives a future tail length). Guard tests: `parse_bs_lighting_starfield_captures_trailing_tail` + the empty-without-size sibling. Regression = dropping the tail capture (re-opens the consumed != block_size drift) or hardcoding 38.
- **Root-NiNode / inline-type-name truncation** (`#688` / `#698`): truncate on inline type-name read failure (`#698`) rather than hard-`Err`; `#688` refuted the "v=20.0.0.5" framing — keep the recovery semantics.
**Output**: `/tmp/audit/nif/dim_1.md`

### Dimension 2: Version Gating
**Entry points**: `crates/nif/src/version.rs` (`NifVersion` constants, `NifVariant::detect`, `bsver()`, the feature-flag helper methods), `crates/nif/src/shader_flags.rs` (`ShaderFlags` typed view), all `stream.variant()` / `stream.version()` call sites in block parsers.
**Checklist**:
- `NifVariant::detect` covers every known `(version, user_version, user_version_2)` combination (cross-check the `detect_*` unit tests in `version.rs` against the seven shipping games + the FO3-dev edge case).
- Feature presence routes through the **named helper surface** on `NifVariant`, not raw `bsver < N` literals. The canonical helpers (verify the live set in `version.rs` — it is deliberately pruned: #938/#1511 deleted nine call-site-less helpers, so do **not** re-cite a name without grepping for it) currently include `has_properties_list`, `has_effects_list`, `has_culling_mode`, `has_shader_alpha_refs`, `has_shader_property_fo3_fields`, `uses_bs_tri_shape`, `has_material_crc`, the collision-band `has_mopp_offset` / `has_havok_strips_scale` / `has_object_group_id` / `has_skin_data_partition_ref`, plus the v10.x-era `has_keyframe_controller_data` / `has_quat_transform_trs_valid` / `has_interp_controller_manager_controlled` / `uses_old_rigid_body_layout`. **A new parser that hardcodes a raw `bsver` literal for a feature a helper already covers is the regression.** New gates should add a helper, not a literal — but a helper with no call site is itself dead code (the #1511 lesson): add it *with* its consumer.
- `bsver` band thresholds are named constants in `version.rs` (`OBLIVION = 11`, `FO3_FNV = 34`, `RIGID_BODY_FLAGS16 = 76`, `SKYRIM_LE = 83`, `SKYRIM_SE = 100`, `FALLOUT4 = 130`, `FO4_DLC_UPPER = 140`, `FO76 = 155`, `STARFIELD = 172`, …). Confirm comparisons use the right operator and the right constant (off-by-one band membership is a silent cross-game corruptor).
- `ShaderFlags` (`shader_flags.rs`) wraps the FO3/FNV `BSShaderFlags`+`BSShaderFlags2` u32 *pair* vs the Skyrim+ single-word storage — verify each game reads its own storage shape.
- Oblivion v20.0.0.5 specifics: no block sizes (→ Dim 1), u16 flags below `FLAGS_U32_THRESHOLD = 26`, inline strings (no string table below `STRING_TABLE_THRESHOLD`).
**Regression guards**:
- **Per-game `NiPSysEmitter` / `NiTextureEffect` version gating** (`#1239` / `#1240`): `NiPSysEmitter` routes through the nif.xml version gate so Oblivion (`bsver < 26`) parses correctly; `NiTextureEffect`'s embedded `NiDynamicEffect` base is gated `bsver < FALLOUT4` (FO4+ removed it). Regression = a parser hardcoding one layout without a band check.
**Output**: `/tmp/audit/nif/dim_2.md`

### Dimension 3: Block Dispatch Coverage
**Entry points**: `crates/nif/src/blocks/mod.rs` (`parse_block` / `parse_block_with_name_arc` + the `impl_ni_object!` macro that *generates* the dispatch arms), the test-infra baselines.
**Checklist**:
- The dispatch is macro-generated — count live arms by counting `impl_ni_object!` registrations in `blocks/mod.rs` (do **not** quote a stale number; if you need a figure, count it fresh or cite `docs/engine/nif-parser.md` § "Block coverage").
- From a corpus or BSA/BA2 listing, enumerate block-type names that appear in real NIFs but fall through to the `NiUnknown` placeholder. Count `NiUnknown` fallbacks per game and flag any that cascade (a missing block with no `block_sizes` anchor in Oblivion truncates the rest of the scene — link Dim 1).
- A block that *parses but is silently dropped downstream* is a Dim 4/5 finding, not a coverage gap — keep the boundary clean.
**Test-infra signal** (extend, don't reinvent):
- `crates/nif/tests/per_block_baselines.rs` (opt-in `--ignored`, needs `nif_stats --tsv` + game data) compares per-type `parsed` vs `unknown` against checked-in 7-game TSV baselines and fails on `unknown` growth / `parsed` shrinkage. `crates/nif/tests/block_coverage_baselines.rs` is the sibling coverage surface. `BYROREDUX_REGEN_BASELINES=1` regenerates after an intentional change. New coverage findings should land as a baseline-test extension. Note from memory: FO76 can sit silently RED on `NiPSysBlock`, and `#BS_F76# == 155` while Starfield ≠ FO76 (different shader tail) — keep the two apart.
**Output**: `/tmp/audit/nif/dim_3.md`

### Dimension 4: Geometry Extraction & Import Handoff
**Entry points**: `crates/nif/src/import/mod.rs` (thin dispatch — `import_nif`, `import_nif_scene`), `crates/nif/src/import/types.rs` (`ImportedNode` / `ImportedMesh` / `ImportedScene`), `crates/nif/src/import/walk/mod.rs` (`walk_node_hierarchical` / `walk_node_flat`; also `extract_emitter_params` / `extract_emitter_rate`), `crates/nif/src/import/mesh/` (`ni_tri_shape`, `bs_tri_shape`, `bs_geometry`, `tangent`, `sse_recon`, `skin`, `material_path`, `decode`), `crates/nif/src/import/material/walker.rs` (`extract_material_info` / `extract_material_info_from_refs`) + `import/material/mod.rs` flag helpers, `crates/nif/src/import/transform.rs`, `crates/nif/src/import/coord.rs`.
**Checklist** (this is the *parse → ECS* handoff; per-game **material** classification lives at the `material_translate.rs` boundary — audit that under `/audit-nifal`, not here):
- All `NiAVObject` fields accessed via the `.av.*` sub-struct (no stale flat-field access after the split).
- Per-game geometry path selected correctly: classic `NiTriShape` (Oblivion/FO3/FNV) vs Skyrim SE+ packed-half `BSTriShape` vs Starfield `BSGeometry` (`bsver 155`). Each decodes its own vertex stride and index format.
- Tangent handoff: FO4+ `BSTriShape` ships tangents **inline** in the packed-vertex blob when `VF_TANGENTS | VF_NORMALS` are both set (decoded in `import/mesh/bs_tri_shape.rs`); the Bethesda authored-blob path (Oblivion/FO3/FNV) reads `NiBinaryExtraData` named `"Tangent space (binormal & tangent vectors)"` and MUST honor the `[tangents…, bitangents…]` swap (the `tangents` field actually holds ∂P/∂V — `#786`); content with neither uses `tangent::synthesize_tangents` (Mikkelsen). Distinct paths — don't cross-wire them.
- SSE skinned-geometry reconstruction (`sse_recon::try_reconstruct_sse_geometry`) and skin extraction (`skin::*`, partition-local → global bone remap) consume the right counts.
- Coordinate conversion (Z-up Gamebryo → Y-up renderer, `coord.rs`) applied consistently to positions, normals, and rotations.
**Regression guards**:
- **Typed particle decode**: `NiPSysEmitter` / `NiPSysEmitterCtlr` / `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` are TYPED structs in `crates/nif/src/blocks/particle.rs` (formerly opaque `NiPSysBlock`). Their params flow `extract_emitter_params` / `extract_emitter_rate` (`import/walk/mod.rs`) → `apply_emitter_params` (`byroredux/src/systems/particle.rs`). Regression = reverting to an opaque block, or dropping the authored birth-rate / base-scale so the runtime falls back to a hardcoded preset. `apply_emitter_params` overrides kinematics + size, *not* color (see its unit tests).
- **Geometry bulk-read fast paths** (perf, but also a correctness handoff): `BSGeometry` extracts via the `read_pod_vec` fast path; `NiTriShape` tangent extraction uses `std::mem::take` on the `Vec<f32>` to avoid a per-mesh clone (`#1263`/`#1265`). Regression = a re-introduced `Vec::with_capacity` + per-vertex loop, or a clone instead of `mem::take`.
**Output**: `/tmp/audit/nif/dim_4.md`

### Dimension 5: Collision & Shader Block Parsing
**Entry points**: `crates/nif/src/blocks/collision/` (`collision_object`, `rigid_body`, `ragdoll`, `shape_primitive`, `shape_compound`, `shape_mesh`, `compressed_mesh`, `constraints`, `phantom_action`), `crates/nif/src/blocks/shader.rs`, `crates/nif/src/import/collision.rs` (`extract_collision`, `examine_collision_kind`).
**Checklist**:
- **`bhk*` field-for-field**: rigid-body flag width changes across the band (`uses_old_rigid_body_layout`, `RIGID_BODY_FLAGS16 = 76`, `RIGID_BODY_EXTRA_FLOATS = 9`); MOPP offset / Havok strips scale presence is version-gated (`has_mopp_offset`, `has_havok_strips_scale`); constraint `CInfo` decode is per-game (`constraints.rs` carries `parse_fo3` arms). The PHYSAL per-game seam is *only* the constraint CInfo decode — keep it confined there (memory "PHYSAL").
- **`BSLightingShaderProperty::parse`** (`shader.rs`) is a thin `bsver` dispatcher → `parse_skyrim` (83–129) / `parse_fo4` (130–154) / `parse_fo76_plus` (≥155). Each variant must read **only** its own field set (Skyrim-only lighting-effect fields, no FO4 subsurface / FO76 trailing). A variant reading another's tail is the over-read in `#1510`; the Starfield path additionally captures `starfield_tail` to `block_size` (`#1606`, Dim 1).
- Shader-type trailing data: `BSLightingShaderProperty` has 0–7 type-specific trailing fields — confirm the per-`shader_type` field count matches nif.xml.
**Regression guards**:
- **Collision-shape translation completeness** (`import/collision.rs::extract_collision`): MUST translate `BhkMultiSphereShape` + `BhkConvexListShape` to `CollisionShape` (the `downcast_ref::<…>()` arms must stay). Regression = a shape that parses but never reaches `extract_collision`.
- **Per-variant collision dispatch** (`examine_collision_kind` → `CollisionAuthoring`): the enum must keep distinguishing `None` / `Classic` / `NewPhysicsStub` / `Phantom` / `Unrecognised` (British spelling). Regression = collapsing the discriminator so a Skyrim+ phantom (`BhkPCollisionObject` wrapping `bhkPhantom`) gets force-translated as a rigid body instead of routed to a future TriggerVolume path.
- **`hkMotionType` byte → canonical `MotionType`** (`#1652`, `extract_from_classic` in `crates/nif/src/import/collision.rs`): the raw Havok byte maps via the full canonical enum — `1..=5 | 8 => Dynamic`, `6 => Keyframed`, `7 => Static`, `9 => CharacterKinematic`, `0`/other `=> Static`. Regression = the old `4 => Keyframed / _ => Static` collapse (mislabels keyframed/fixed/character bodies, wrong solver behaviour). This decode is the canonical-tier boundary — the `MotionType` enum lives in `crates/core/src/ecs/components/collision.rs`; see `/audit-nifal` Dim 6.
- **FO4 model-space-normals + alpha-test consumption** (`#1592`, `crates/nif/src/import/material/walker.rs`): for an FO4 `BSLightingShaderProperty` the parser reads the full F4SF1/F4SF2 pair, but the walker must OR `Model_Space_Normals` (F4SF1 bit 12) + `Alpha_Test` (F4SF2 bit 25) into `MaterialInfo` (plus the FO76+ `MODELSPACENORMALS` CRC for `bsver >= 132`). Parsed-but-dropped bits render an object-space normal map as tangent-space or a cutout as opaque on inline/loose/modded FO4 NIFs. The NIF flag is strictly lower priority than the later BGSM merge. Regression = the walker dropping these bits again.
**Output**: `/tmp/audit/nif/dim_5.md`

### Dimension 6: Allocation Hygiene (PERF)
**Entry points**: `crates/nif/src/stream.rs` (`allocate_vec`, `read_pod_vec`), all `blocks/*.rs` callers, `byroredux/src/streaming.rs` (`pre_parse_cell`).
**Checklist**:
- `allocate_vec::<T>(count)` and the `read_pod_vec` wrappers carry `#[must_use]` (the message names the fix-up: bind it or `stream.skip()`). A bound-check-only call site that discards the Vec is a no-op/leak pattern. Verify the attribute is present on the helpers and the KFM `allocate_vec` site (`#831`, extended by `#1246`).
- Bulk arrays go through `read_pod_vec<T>` (collapses the double allocation, `#833`); direct allocate-then-loop-and-fill is the regression. `read_pod_vec` has a top-of-module big-endian compile-error gate. **`bytemuck` is NOT a workspace `Cargo.toml` dep** despite some audits claiming it — `read_pod_vec` bounds `T: AnyBitPattern` via the re-export; confirm before reporting.
- Per-block parse-loop counters use the `entry().get_mut() / insert` split, not `entry().or_insert(name.to_string())` (`#832`; the `to_string` path leaks throwaway short strings per Oblivion cell).
- `ragdoll.rs` bone-pose / template parse uses `allocate_vec` (not the old `check_alloc` idiom, `#1245`); verify no other `bhk*`/`ragdoll` parser regressed back.
- Per-block dispatch interns block names as `Arc<str>` (`#1261`); `pre_parse_cell` (`byroredux/src/streaming.rs`) is a two-phase pipeline — serial header extract → rayon-parallel body parse (`#877`) — with a serial fast path for small models (`#1262`). Verify the phase split is intact and both paths are wired (collapsing them, or routing small cells through the parallel overhead, is the regression).
**Test-infra signal**: `crates/nif/tests/heap_allocation_bounds.rs` (`parse_skyrim_se_single_node_stays_within_heap_budget`, `cfg(feature = "dhat")`-gated, `#1247`) + `heap_allocation_bounds_geometry.rs` pin an upper-bound allocation count on a canonical input. The bare-NiNode gate was extended (`#a3216671`) to also cover the **geometry + particle** parsers — a synthetic Skyrim SE NIF with a `BSTriShape` + `NiPSysSphereEmitter` (the blocks that do the bulk per-element allocation the #832/#833/#408 discipline guards), bounded at ~8× the measured ~1.5 KB / 15-block parse. New alloc-reduction findings should extend these rather than add ad-hoc checks.
**Output**: `/tmp/audit/nif/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/nif/dim_*.md`.
2. Combine into `docs/audits/AUDIT_NIF_<TODAY>.md` (`YYYY-MM-DD`) with:
   - **Executive Summary** — clean/recoverable rate per game (vs the `nif-parser.md` matrix), total stream-position mismatches, critical coverage gaps.
   - **Block Type Coverage Matrix** — block types × games (parsed / skipped / NiUnknown).
   - **Findings** — grouped by severity (per `_audit-severity.md`; note the NIF rows: hard parse failure = HIGH, stream-position mismatch the `block_size` reconciliation covers = MEDIUM).
   - **Prioritized Fix Order** — rendering-blocking blocks first, then animation, then collision.
3. Remove cross-dimension duplicates. Per-game **material** translation belongs to `/audit-nifal`; cross-link instead of duplicating.

Suggest: `/audit-publish docs/audits/AUDIT_NIF_<TODAY>.md`
