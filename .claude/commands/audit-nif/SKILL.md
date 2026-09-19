---
description: "Deep audit of NIF parser — stream position, version gating, block dispatch coverage, geometry handoff"
argument-hint: "--focus <dimensions> --game <fnv|fo3|skyrim|oblivion|fo4|fo76|starfield> --corpus <path>"
---

# NIF Parser Audit

Audit the NIF binary-format parser (`crates/nif/src/`) for byte-accurate correctness
across the Oblivion → Starfield version span. Real game data is used when available.

The recurring failure mode is **stream-position drift**: a block over/under-reads its
payload, the consumed-byte count diverges from the header `block_sizes` entry, and either the
reconciliation masks it (parse "succeeds" with silent corruption) or — on Oblivion-era files
with *no* `block_sizes` table — every later block is misaligned and the scene truncates.
Dimensions are ordered by that risk.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` for
shared protocol (layout, game-data locations, dedup, finding format). Do not duplicate them.
`docs/engine/nif-parser.md` is the code-verified reference (module map, version handling,
per-game coverage matrix, block coverage) — prefer it over re-deriving. The authoritative
block spec is `/mnt/data/src/reference/nifxml/nif.xml` (*reference_nifxml*); Gamebryo 2.3
source is the tiebreaker.

**Not in scope**: BSA/BA2/CSG archive-reader discipline and the FO4 previs header
(`/audit-parsers`); per-game *material* translation and the canonical tier (`/audit-nifal`);
SpeedTree `.spt` (`/audit-speedtree`); Havok→Rapier behaviour (`/audit-physics` — only the
constraint CInfo decode is a NIF seam).

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated numbers (e.g. `1,2`). Default: all 6.
- `--game <name>`: `fnv`, `fo3`, `skyrim`, `oblivion`, `fo4`, `fo76`, `starfield`. Default: all detected.
- `--corpus <path>`: directory of extracted `.nif` files for bulk testing.

## Extra Per-Finding Fields

- **Dimension**: Stream Position | Version Gating | Block Dispatch Coverage | Geometry Handoff | Collision/Shader Parsing | Allocation Hygiene
- **Game Affected**: which `NifVariant`(s), citing the `bsver` band.

## Phase 1: Setup

1. Parse `$ARGUMENTS`; `mkdir -p /tmp/audit/nif`; fetch the dedup baseline (`_audit-common.md`).
2. Note which game-data directories exist. Skim `docs/engine/nif-parser.md` § "Per-game NIF
   coverage" so findings are measured against current clean/recoverable rates.
3. `cargo test -p byroredux-nif` (default lane); record the pass count.
4. **Corpus gates are opt-in** (`#[ignore]`, need installed data): `parse_real_nifs`,
   `per_block_baselines`, `block_coverage_baselines`, `oblivion_stream_drift_corpus`,
   `constraint_drift_corpus`, `normal_synthesis_corpus` under `crates/nif/tests/`. They run
   nightly per title in `.github/workflows/real-data-gates.yml` (`BYROREDUX_REQUIRE_GAME_DATA=1`,
   so an absent corpus is a failed job). Locally: one title at a time,
   `BYROREDUX_<GAME>_DATA=<Data dir> cargo test -p byroredux-nif --test per_block_baselines <game> -- --ignored --nocapture`.
   Check the latest nightly result before trusting a green default `cargo test` for parse rates.

## Phase 2: Dimensions

### Dimension 1: Stream Position Integrity (PRIMARY)
Paths: `crates/nif/src/{lib,stream,header,scene}.rs`, every `blocks/**/*.rs` `parse()`
First step: `git log --since=<last report> --format='%h %s' -- crates/nif/src/blocks crates/nif/src/lib.rs`
Flow: `parse_nif` → `parse_nif_with_options` → `dispatch_blocks` (block loop, `no_block_sizes`
branch) with `parse_header` / `finalize_scene` as sibling phases.
**Guards**: the `NifScene::recovered_by_guess` zero-assert in the corpus walk
(`tests/common`, surfaced by `parse_real_nifs`: a size-cache/`oblivion_skip_sizes` skip is
*inferred*, not file-declared — absolute zero, not a rate; a size-cache skip that lands
inside the failed block's own consumed bytes is refused, #3926);
`every_tail_capturing_block_reports_it_and_parse_nif_records_it` (`blocks/shader_tests/starfield.rs`;
source-shape scan of `blocks/shader/*.rs` + `blocks/node.rs`: every struct declaring a
`starfield_tail` field overrides `opaque_tail_len`; a tail-capturing block in a file not in
its list is invisible to it); `blocks/controller/sequence_pre_10_1_0_106_tests.rs`;
`tests/oblivion_stream_drift_corpus.rs` (no-`block_sizes` detector has zero false positives).
**Checklist** (what the guards cannot see):
- Each block consumes exactly its `block_sizes` entry. A finding is a block whose drift the
  reconciliation is *silently absorbing*, not the reconciliation itself.
- Oblivion-era files (v20.0.0.4/5 and the v10.x NetImmerse family) ship **no** `block_sizes`:
  one wrong field cascades. Audit those parsers for unconditional reads that assume a
  later-game layout; version-gated trailing fields must be guarded, never always-read.
- Bool width: `read_bool` (version-dependent 1 or 4 bytes) vs `read_byte_bool` (always 1) must
  match the nif.xml type per field — a phantom/missing bool is the classic 1-/3-byte drift.
- **An opaque tail captured to `block_size` is invisible to `drift_histogram`** (consumed ==
  declared by construction). It needs its own telemetry: `NiObject::opaque_tail_len` feeding
  `NifScene::opaque_tail_histogram` (#2625). Regression = a new tail-capturing block that
  doesn't wire it. Also confirm capture-to-`block_size` is used, never a hardcoded length
  (Starfield `BSLightingShaderProperty` tail is 38 B on most content but bimodal `{38, 42}`).
- Pre-10.1.0.106 `NiSequence` / `ControlledBlock` / `NiControllerSequence`: exactly one of
  each `until="10.1.0.103"` / `since="10.1.0.106"` pair is present for any version (a read
  that fires both, or drops the `until` half, loses text keys and binds no channel);
  below-band `cycle_type` default is `CYCLE_CLAMP = 2`, and `import_sequence`
  (`crates/nif/src/anim/sequence.rs`) collapses a non-finite/non-positive duration to `0.0`.
- `NiDynamicEffect` carries two affected-nodes groups (`until="4.0.0.2"` and
  `since="10.1.0.0"`); both must be read (`NiLight`/`NiTextureEffect`, no anchor on files
  that old).
- Starfield `BSFaceGenNiNode` has its own type with an opaque `starfield_tail` (aliasing it
  to plain `NiNode::parse` under-reads 2 B on every instance); `BSWeakReferenceNode` declines
  an unfittable payload into `starfield_tail` rather than overrunning.
- Regression pins (verify still fixed, do not re-report): Oblivion v10.x family (#1509,
  #1337, #1329, #1310/#1301/#1302 — method: extract → `trace_block` → byte-decode; memory
  *nif_v10x_stride_drift_resolved*); Starfield shader over-read (#1510: gate FO76-tail fields
  on `bsver < STARFIELD`, since `#BS_F76#` is stream 155 only); root-NiNode / inline-type-name
  truncation recovers rather than hard-`Err` (#688/#698).
- If a corpus is available: report consumed-vs-`block_size` mismatches by block type with
  counts; any non-zero on a *known* type is drift; separately any `no_block_sizes` truncation.
**Output**: `/tmp/audit/nif/dim_1.md`

### Dimension 2: Version Gating (highest report yield)
Paths: `crates/nif/src/{version,shader_flags}.rs`, all `stream.bsver()` / `stream.version()` sites in `blocks/`
First step: `git log --since=<last report> --format='%h %s' -- crates/nif/src/version.rs crates/nif/src/shader_flags.rs`, then for every parser changed since, compare each `since=`/`until=`/`vercond` in nif.xml with the gate written; `grep -rn 'V10_1_0_106\|V10_1_0_103' crates/nif/src/blocks` shows raw uses of constants a `NifVersion` helper already encodes.
**Guard**: the `detect_*` tests in `version.rs`. Nothing guards helper-vs-literal use.
**Checklist**:
- `NifVariant::detect` covers every `(version, user_version, user_version_2)` combination of
  the seven titles + the FO3-dev edge case. `impl NifVariant` is minimal (`detect`, `bsver`);
  the live named-helper surface is on **`NifVersion`** (`has_mopp_offset`,
  `has_havok_strips_scale`, `has_object_group_id`, `has_skin_data_partition_ref`,
  `has_skin_data_vertex_weights_flag`, `has_keyframe_controller_data`,
  `has_quat_transform_trs_valid`, `has_interp_controller_manager_controlled`,
  `uses_old_rigid_body_layout`, `has_ni_sequence_prologue`, `has_controller_sequence_fields`) —
  verify against `version.rs` before citing (it has been pruned and grown). **A new parser
  that hardcodes a raw version literal for a feature a helper covers is the regression**; a
  new helper needs a consumer in the same change (dead helpers were pruned). Doctrine:
  `docs/engine/nif-parser.md` "Version handling".
- `bsver` bands are named constants (`OBLIVION = 11`, `FO3_FNV = 34`, `RIGID_BODY_FLAGS16 = 76`,
  `SKYRIM_LE = 83`, `SKYRIM_SE = 100`, `FALLOUT4 = 130`, `FO4_DLC_UPPER = 140`, `FO76 = 155`,
  `STARFIELD = 172`, …): check operator and constant (off-by-one band membership silently
  corrupts a neighbouring game). FO76 (155) and Starfield (172+) differ — keep apart.
- `shader_flags.rs`: FO3/FNV `BSShaderFlags` + `BSShaderFlags2` u32 *pair* (`fo3nv_f1/f2`) vs
  Skyrim+ single-word storage (`skyrim_slsf1/2`, `fo4_slsf1/2`) and the FO76/Starfield
  `bs_shader_crc32` arrays; each game reads its own storage shape. Import reads them via
  `is_decal_from_legacy_shader_flags` / `is_decal_from_modern_shader_flags` /
  `is_two_sided_from_modern_shader_flags` (`crates/nif/src/import/material/mod.rs`).
- Oblivion specifics: u16 flags below `FLAGS_U32_THRESHOLD = 26`; inline strings below
  `STRING_TABLE_THRESHOLD`; `NiPSysEmitter` routed through the nif.xml gate (`bsver < 26`);
  `NiTextureEffect`'s embedded `NiDynamicEffect` base gated `bsver < FALLOUT4`.
**Output**: `/tmp/audit/nif/dim_2.md`

### Dimension 3: Block Dispatch Coverage
Paths: `crates/nif/src/blocks/mod.rs`, `crates/nif/tests/{per_block,block_coverage}_baselines.rs`, `crates/nif/tests/data/`, `crates/nif/src/corpus.rs`, `crates/nif/src/kfm.rs`
First step: `cargo run -p byroredux-nif --release --example nif_stats -- <archive-or-dir> --tsv` and diff against `tests/data/per_block_baselines/<game>.tsv` (`--unknown-only` for the short view)
**Checklist**:
- Dispatch is a hand-written `match type_name` in `parse_block_inner` (the `impl_ni_object!`
  macro only generates trait impls). Count top-level arms fresh — nested `match`es
  (`type_name_static` RTTI arms, `type_name_arc_hint`) inflate a naive `grep -c '=>'`; never
  quote a stale number or copy one from `docs/engine/nif-parser.md` § "Block coverage".
- From a corpus or archive listing, enumerate block types that fall through to `NiUnknown`;
  count per game; flag any that cascade (a missing block on Oblivion truncates the scene → Dim 1).
- A block that parses but is dropped downstream is a Dim 4/5 finding, not coverage.
- **Baseline harness invariants**: baselines key on **wire RTTI** via
  `header.block_type_indices` (`record_scene_blocks`, `tests/common/mod.rs`), not the parsed
  struct name (several arms parse multiple wire types into one struct — `BhkRigidBody.is_t`,
  `NiPSysBlock.original_type`); the corpus definition is shared in `corpus.rs`
  (`NIF_ENTRY_EXTENSIONS` includes `.bto`/`.btr` — renamed NIFs; a second private copy of
  the rule is the regression); the gates walk **every** mesh-bearing archive
  (`open_all_mesh_archives` all-or-nothing, `open_optional_mesh_archives` present-only,
  `run_all_meshes_gate` with a *per-archive* `limit`); `parse_real_nifs` asserts a per-archive
  clean-rate floor (`min_clean`, 0.995) plus recoverable 100% — truncation counts as
  recoverable, so the floor is what catches silent clean-rate collapse. New coverage
  findings should land as a baseline-test extension; `BYROREDUX_REGEN_BASELINES=1`
  regenerates after an intentional change.
- `kfm.rs` (KFM binary catalog, v1.2.0.0–2.2.0.0, transcribed from `NiKFMTool::ReadBinary`)
  has no engine consumer today — audit for version-gate fidelity and `allocate_vec` bounds
  only; "unused" is not a finding.
**Output**: `/tmp/audit/nif/dim_3.md`

### Dimension 4: Geometry Extraction & Import Handoff
Paths: `crates/nif/src/import/{mod,types,transform,coord,precombine}.rs`, `import/mesh/`, `import/walk/`, `import/material/{walker,dedicated_shader}.rs`
First step: `git log --since=<last report> --format='%h %s' -- crates/nif/src/import/mesh crates/nif/src/import/walk`
This is the *parse → ECS* handoff; per-game material classification is `/audit-nifal`.
**Guards**: `import/mesh/sse_skin_index_space_tests.rs` (`#[ignore]`, needs Skyrim SE data),
`bs_tri_shape_partition_remap_tests.rs`, `sse_skin_geometry_reconstruction_tests.rs`,
`tangent_convention_tests.rs`, `tests/normal_synthesis_corpus.rs` (ignored), `import/walk/tests.rs`.
**Checklist**:
- All `NiAVObject` fields via the `.av.*` sub-struct. Coordinate conversion (Z-up → Y-up,
  `coord.rs`) applied consistently to positions, normals, rotations.
- Per-game geometry path: classic `NiTriShape` (Oblivion/FO3/FNV) vs Skyrim SE+ packed-half
  `BSTriShape` vs Starfield `BSGeometry` (bulk-read via `read_u16_array` + unpack; import
  extractor in `import/mesh/bs_geometry.rs`). Each decodes its own stride and index format.
- **Tangents**: FO4+ `BSTriShape` inline when `VF_TANGENTS | VF_NORMALS` set;
  Oblivion/FO3/FNV `NiBinaryExtraData` "Tangent space (binormal & tangent vectors)" — must
  honor the `[tangents…, bitangents…]` swap (the field holds ∂P/∂V, #786); Starfield UDEC3
  `tangents_raw` with W normalized to ±1 at import (#2246); otherwise
  `tangent::synthesize_tangents` (Mikkelsen). Distinct paths — don't cross-wire.
- **Skyrim SE skinning index spaces** (#3355/#3360, then the 2026-09-17 correction): the
  `NiSkinPartition::Triangles` are already **global** vertex indices on the `Stream() == 100`
  band (do not push them through `vertex_map`); the packed `BSTriShape` / SSE global-buffer
  bone indices already address the skin's bone list and are only *widened*
  (`widen_packed_bone_indices`); only the separate `NiSkinPartition.bone_indices` channel is
  partition-local. Regression = re-applying a partition palette to the packed channel (the
  deleted *remap_bs_tri_shape_bone_indices* behaviour) or `vertex_map` to triangles.
  `sse_recon::try_reconstruct_sse_geometry` and `skin::*` consume the right counts.
- **Particle emitters**: `NiPSysEmitter`/`NiPSysEmitterCtlr`/`NiPSysEmitterCtlrData`/
  `NiPSysGrowFadeModifier` are typed (`blocks/particle.rs`); params flow
  `extract_emitter_params` / `extract_emitter_rate` (`import/walk/emitter.rs`) →
  `apply_emitter_params` (`byroredux/src/systems/particle.rs`, overrides kinematics + size,
  not colour). The rate walk is scoped to the system's own controller chain; when a sibling
  `NiPSys*` controller parses to the opaque `NiPSysBlock` marker (which discards
  `next_controller_ref`) the fallback resolves by `base.target_ref` to the ctlr targeting the
  system whose `controller_ref` equals the chain head — per-instance exact, never a
  whole-scene first-match, and gated on a non-NULL chain head (#4467; opt-in gate
  `real_archive_torch_meshes_surface_particle_emitters`). Regression = a hardcoded preset
  replacing an authored birth rate, or the fallback claiming another system's ctlr.
- FO4 precombined geometry (`import/precombine.rs`, M49) reuses `decode_bs_vertex_stream`
  with `full_precision = false` (PSG positions are half even when the descriptor sets
  full-precision); container read is `byroredux_bsa::CsgArchive` (`/audit-parsers`); spec
  `docs/engine/fo4-csg-format.md`.
- FO4 model-space normals + alpha-test: `dedicated_shader.rs` ORs `Model_Space_Normals`
  (F4SF1 bit 12, or the `MODELSPACENORMALS` CRC for `bsver >= 132`) and `Alpha_Test`
  (F4SF2 bit 25) into `MaterialInfo`; parsed-but-dropped bits render object-space normals as
  tangent-space. The NIF flag ranks below the later BGSM merge.
**Output**: `/tmp/audit/nif/dim_4.md`

### Dimension 5: Collision & Shader Block Parsing
Paths: `crates/nif/src/blocks/collision/`, `crates/nif/src/blocks/shader/`, `crates/nif/src/import/collision/`
First step: `cargo test -p byroredux-nif -- dispatch_coverage_tests bhk_ hk_packed`
**Guards**: `import::collision::dispatch_coverage_tests::every_dispatched_bhk_shape_has_resolve_arm`
(a new `bhk*Shape` dispatch arm needs a `resolve_shape_inner` `downcast_ref` arm — else it
parses then silently drops collision; *nif_shape_dispatch_resolve_parity*);
`import::collision::dispatch_tests::havok_motion_type_maps_full_enum`;
`blocks/collision/hk_packed_ni_tri_strips_data_tests.rs`;
`tests/constraint_drift_corpus.rs` (opt-in; drift must be a known motor-tail value).
**Checklist**:
- **`bhk*` field-for-field**: rigid-body flag width (`uses_old_rigid_body_layout`,
  `RIGID_BODY_FLAGS16 = 76`, `RIGID_BODY_EXTRA_FLOATS = 9`); MOPP offset / strips scale gated
  (`has_mopp_offset`, `has_havok_strips_scale`); FO3+ `hkSubPartData` is *decoded* (filter +
  material), never `skip(12)`. The PHYSAL per-game seam is only the constraint CInfo decode.
- **Constraint CInfo**: typed decoders exist for hinge (into `LimitedHingeCInfo`), limited
  hinge, prismatic, ragdoll, ball-and-socket, stiff-spring and the ball-socket chain
  (`BhkConstraintData`; `BhkBreakableConstraint` and malleable wrappers decode the inner
  CInfo). Only `bhkGenericConstraint` remains a name-only stub
  (`is_havok_constraint_stub` in `lib.rs` — its drift is suppressed; anything else on that
  list hides real drift, the mechanism that hid `bhkHingeConstraint`'s +128). By-design
  residuals are pinned by `corpus::is_known_constraint_motor_tail_drift` (1/18/19/26;
  malleable +4; the three no-motor types exactly 0). *Decoded is not imported*: the ragdoll
  importer (`import/collision/ragdoll.rs`) still declines ball-and-socket / spring / chain
  until a canonical joint kind exists (`docs/engine/physal.md`).
- **`BSLightingShaderProperty::parse`** is a thin `bsver` dispatcher → `parse_skyrim`
  (83–129) / `parse_fo4` (130–154) / `parse_fo76_plus` (≥155); each reads only its own field
  set; per-`shader_type` trailing count (0–7) matches nif.xml. Starfield captures
  `starfield_tail` to `block_size`. The material-reference stub gate is `!name.is_empty()`
  for `bsver >= STARFIELD` (hash-path refs carry no `.bgem` suffix) but the suffix-aware
  `is_material_reference` for FO76 (152..171); `BSEffectShaderProperty` and
  `BSLightingShaderProperty::parse_fo76_plus` must stay in lockstep (`parse_bs_effect_starfield_hashpath_name_stubs`).
- Starfield shader-type translation is keyed at the **parser** boundary
  (`parse_with_size` routes every `bsver >= 155` through `parse_fo76_plus`), not the
  slot-table layout tag; `normalize_shader_type` masks types 4/5 — a Starfield FaceTint (3)
  must not reach the slot table as Skyrim Parallax (#3364).
- **`BhkNPCollisionObject` is "approximated", not "decoded"**: the FO4/FO76/Starfield
  `BhkSystemBinary` blob does not resolve to a `CollisionShape`.
  `blocks::collision::havok_packfile::parse_havok_packfile` decodes the container (header,
  sections, three fixup tables → typed object graph); what stays opaque is
  `hknpCompressedMeshShapeData`'s bit-packed layout (`docs/engine/physal.md`). Do not report
  it as decoded. `summarize_collision_authoring` scans every collision block and feeds
  `CachedNifImport.collision_authoring` (`byroredux/src/cell_loader/nif_import_registry.rs`);
  `examine_collision_kind` keeps `None / Classic / NewPhysicsStub / Phantom / Unrecognised`
  distinct (a Skyrim+ phantom must not be force-translated as a rigid body). A per-game
  detail (raw `bsver`, block-type string) escaping `import/collision/` is a finding →
  cross-link `/audit-nifal`.
- `hkMotionType` byte → canonical `MotionType` (`havok_motion_type`): `1..=5 | 8` Dynamic,
  `6` Keyframed, `7` Static, `9` CharacterKinematic, else Static; the zero-mass "Dynamic"
  reclassification lives beside it — see `/audit-physics`, do not re-derive.
**Output**: `/tmp/audit/nif/dim_5.md`

### Dimension 6: Allocation Hygiene (PERF)
Paths: `crates/nif/src/stream.rs`, `blocks/**/*.rs` callers, `crates/nif/tests/heap_allocation_bounds*.rs`, `byroredux/src/streaming.rs` (`pre_parse_cell`)
First step: `cargo test -p byroredux-nif --features dhat-heap --test heap_allocation_bounds` (CI job `nif-heap-allocation-bounds` runs the three heap files, each its own process)
**Guards**: the dhat-gated `heap_allocation_bounds.rs` (single node, FO4 packed vertices,
SSE geometry+particle, skin blocks), `heap_allocation_bounds_geometry.rs`,
`heap_allocation_bounds_import.rs`; `stream.rs` unit tests
(`allocate_vec_sized_*`, `allocate_vec_min_bytes_uses_the_supplied_minimum_not_size_of`).
**Checklist**:
- `allocate_vec` / `allocate_vec_sized` / `allocate_vec_min_bytes` are `#[must_use]`; a call
  that only bound-checks and drops the Vec is a no-op. Bulk arrays go through `read_pod_vec<T>`
  (single allocation); allocate-then-loop-fill is the regression. `read_pod_vec` is bounded by
  the crate-local `unsafe trait AnyBitPattern` with an explicit per-type impl list
  (`stream.rs`; `bytemuck` is *not* a dependency — check before reporting it) and a
  big-endian `compile_error!` gate.
- **`allocate_vec_min_bytes`'s minimum must be the smallest legitimate on-disk encoding of one
  *emitted* element, not `size_of` of the in-memory type** (#3918): a generated element (e.g.
  de-stripped triangles, ≥ 2 B each on disk, 6 B in memory) with a `size_of`-derived bound
  rejects valid data near EOF and fails silently downstream (a demoted `NiUnknown`
  `NiSkinPartition` un-hides skin partitions → NPCs render bare skin through armor). Known
  siblings over-demand by 12–18% (`InterpBlendItem` 20 vs 17, `NiAgdDataStream` 28 vs 25).
- Per-block loop counters use the `entry().get_mut() / insert` split, not
  `or_insert(name.to_string())`; block names are interned `Arc<str>`; `ragdoll.rs` uses
  `allocate_vec` (not the old `check_alloc`).
- `pre_parse_cell` is two-phase — serial header extract → rayon-parallel body parse — with a
  serial fast path for small models; collapsing either path is the regression.
- `NifStream` caps any single file-driven allocation (256 MB); confirm new readers route
  through the capped helpers, not raw `vec![0; n]`.
**Output**: `/tmp/audit/nif/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/nif/dim_*.md`; combine into `docs/audits/AUDIT_NIF_<TODAY>.md`:
   - **Executive Summary** — clean/recoverable rate per game (vs the `nif-parser.md` matrix),
     stream-position mismatches, critical coverage gaps.
   - **Block Type Coverage Matrix** — block types × games (parsed / skipped / NiUnknown).
   - **Findings** — by severity (`_audit-severity.md` NIF rows: hard parse failure = HIGH;
     a mismatch the `block_size` reconciliation covers = MEDIUM).
   - **Prioritized Fix Order** — rendering-blocking blocks, then animation, then collision.
2. Remove cross-dimension duplicates; material translation → `/audit-nifal`.

Suggest: `/audit-publish docs/audits/AUDIT_NIF_<TODAY>.md`
