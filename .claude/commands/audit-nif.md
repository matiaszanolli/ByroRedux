---
description: "Deep audit of NIF parser — block correctness, version handling, stream position, coverage"
argument-hint: "--focus <dimensions> --game <fnv|skyrim|oblivion|fo4> --corpus <path>"
---

# NIF Parser Audit

Deep audit of the NIF binary format parser for correctness across all game versions. Tests against real game data when available.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 7.
- `--game <name>`: Focus on specific game variant: `fnv`, `fo3`, `skyrim`, `oblivion`, `fo4`, `fo76`, `starfield`. Default: all detected.
- `--corpus <path>`: Path to a directory of extracted NIF files for bulk testing.

## Extra Per-Finding Fields

- **Dimension**: Block Parsing | Version Handling | Stream Position | Import Pipeline | Coverage | Stream Allocation | NIFAL Translation
- **Game Affected**: Which NifVariant(s) are affected

## Phase 1: Setup

1. Parse `$ARGUMENTS`
2. `mkdir -p /tmp/audit/nif`
3. Fetch dedup baseline
4. Check which game data directories exist (from `_audit-common.md` game data locations)

## Phase 2: Launch Dimension Agents

### Dimension 1: Block Parsing Correctness
**Entry points**: `crates/nif/src/blocks/*.rs` (all block parsers)
**Checklist**: Every field read matches nif.xml spec (compare struct fields vs nif.xml `<add>` elements), NiObjectNETData.parse() called correctly by all blocks, NiAVObjectData.parse() vs parse_no_properties() used correctly, BSShaderPropertyData.parse_fo3() used by FO3-era shaders only, block_size adjustment warnings (compile list from real NIF files if corpus available), boolean type correctness (read_bool vs read_byte_bool per nif.xml type annotation).
**2026-05-04/05 architectural pins (regression guards)**:
- **`NiLodTriShape`** (#838 SK-D5-NEW-07): inherits from `NiTriBasedGeom`, NOT from `BSTriShape`. nif.xml is authoritative. Routing it through `BSTriShape` produces a 23-byte over-read on every Skyrim tree LOD. The wrapper layout is `NiTriShape + 3 LOD-size u32s`. If the dispatch in `blocks/mod.rs` reverts to BSTriShape, Skyrim Meshes0 sweep loses its `100.00% / 0 truncated / 0 recovered / 0 realignment WARN` baseline
- **`BsLagBoneController`** + **`BsProceduralLightningController`** (#837 SK-D5-NEW-03): both have dedicated parsers. Without them, ~120 by-design `block_size` WARN events fire per Skyrim Meshes0 sweep
- **BSTriShape `data_size` warning** (#836 SK-D5-NEW-02): gated on `num_vertices != 0`. Removing the gate fires 67 false-positive WARNs/parse on the SSE skinned-body reconstruction path
- **`DecalData`** (#813 / #814): FO4 TXST `DODT` sub-record + `DNAM` flags must be parsed; without them, 207/382 (DODT) and 382/382 (DNAM) vanilla TXSTs silently drop their authoring
- **FO4+ BSTriShape inline tangents** (#795 / #796, b63ab0c): when `VF_TANGENTS | VF_NORMALS` are both set, tangents ship inline in the packed-vertex blob (NOT in a separate `NiBinaryExtraData`). Distinct from the Skyrim path; FO4 inline decode lives in `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` (Session 35 split — was `tri_shape.rs`), in the `BSTriShape` packed-vertex loop (`for _ in 0..num_vertices`, ~line 435). The Bethesda authored-blob path (Oblivion / FO3 / FNV) reads from `NiBinaryExtraData` named `"Tangent space (binormal & tangent vectors)"` and MUST honor the `[tangents..., bitangents...]` swap (`tangents` field actually holds ∂P/∂V — see #786 / 5dde345)
**2026-05-28 typed-particle pin (regression guard)**:
- **Typed particle decode** (5708b5b9 / 9db60714 / 8f856d35): `NiPSysEmitter`, `NiPSysEmitterCtlr`, `NiPSysEmitterCtlrData`, `NiPSysGrowFadeModifier` are now TYPED structs in `crates/nif/src/blocks/particle.rs` (was opaque NiPSysBlock). Their authored params flow `extract_emitter_params` / `extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs`) → `apply_emitter_params` (`byroredux/src/systems/particle.rs`, re-exported as `crate::systems::apply_emitter_params`). Regression patterns: reverting to an opaque block, or dropping the authored birth-rate (`NiPSysEmitterCtlr`) / base-scale (`NiPSysGrowFadeModifier`) so the runtime falls back to a hardcoded preset. Verify the typed structs still carry the on-disk fields and that `apply_emitter_params` overrides the preset kinematics + size (not color). See `/audit-nifal` for the canonical-translation side of authored-param resolution.
**Output**: `/tmp/audit/nif/dim_1.md`

### Dimension 2: Version Handling
**Entry points**: `crates/nif/src/version.rs`, all `stream.variant()` and `stream.version()` calls in block parsers
**Checklist**: NifVariant::detect() covers all known user_version/user_version_2 combinations, feature flags match nif.xml version conditions (has_properties_list, has_shader_alpha_refs, etc.), bsver() return values are correct, version comparisons use correct operators (>= vs >, < vs <=), Oblivion v20.0.0.5 handling (no block sizes, u16 flags, inline strings).
**2026-05-28 NifVariant-helper pin (regression guard)**:
- **NifVariant feature-flag helper canonicalization** (#1277 Task 5, 2bd447d5): variant-aligned raw `bsver` literal comparisons were migrated to named `NifVariant` helpers in `crates/nif/src/version.rs` — `has_dedicated_shader_refs`, `uses_bs_lighting_shader`, `has_shader_alpha_refs`, `uses_fo4_shader_flags`, `has_dynamic_effect_fields` (verify exact set against `version.rs`). These helpers are now the canonical version-band surface; a new block parser that hardcodes a raw `bsver < N` literal for a feature already covered by a helper is the regression. Cross-link the Dim 6 #1239/#1240 version-gate pins (NiPSysEmitter / NiTextureEffect) — those gate on the same BSVER bands and should route through the helper surface too. Note also the **`ShaderFlags` typed variant view** (#1277 Task 6, 525a2262, `crates/nif/src/shader_flags.rs`) that wraps the FO3/FNV `BSShaderFlags`+`BSShaderFlags2` u32 pair vs the Skyrim+ single-word storage.
**Output**: `/tmp/audit/nif/dim_2.md`

### Dimension 3: Stream Position Integrity
**Entry points**: `crates/nif/src/lib.rs` (parse_nif block loop), all block parsers
**Checklist**: Every parsed block consumes exactly block_size bytes (when known), no unconditional reads that may exceed block boundaries, skip logic for unknown blocks works correctly, SVD decomposition doesn't read extra bytes, NiTexturingProperty consistent 1-byte shortfall (known issue — diagnose root cause).
**If corpus available**: Parse all NIFs in corpus and report stream position mismatches by block type with frequency counts.
**Output**: `/tmp/audit/nif/dim_3.md`

### Dimension 4: Import Pipeline Correctness
**Entry points**: `crates/nif/src/import/mod.rs` (thin dispatch — `import_nif`, `import_nif_scene`), `crates/nif/src/import/types.rs` (ImportedNode / ImportedMesh / ImportedScene types, post-Session-34 split), `crates/nif/src/import/tests.rs`, `crates/nif/src/import/walk/` (walk_node_hierarchical, walk_node_flat), `crates/nif/src/import/mesh/` (Session 35 split: `ni_tri_shape::extract_mesh`, `bs_tri_shape::extract_bs_tri_shape`, `bs_geometry::extract_bs_geometry`, `tangent::synthesize_tangents`, `sse_recon::try_reconstruct_sse_geometry`, `material_path::material_path_from_name`, `decode::*`, `skin::extract_skin_*`; in-directory test siblings: `bs_tri_shape_shader_flag_tests`, `bs_tri_shape_partition_remap_tests`, `material_path_capture_tests`, `shader_type_fields_tests`, `skin_tests`, `sse_skin_geometry_reconstruction_tests`, `tangent_convention_tests`), `crates/nif/src/import/material/` (walker.rs `extract_material_info` / `extract_material_info_from_refs` — the texture/material resolution entry; mod.rs flag helpers `is_decal_from_legacy_shader_flags` / `is_decal_from_modern_shader_flags` / `is_two_sided_from_modern_shader_flags` / `apply_alpha_flags`; shader_data.rs; *_tests.rs siblings), `crates/nif/src/import/transform.rs`, `crates/nif/src/import/coord.rs`, `crates/nif/src/import/collision.rs`
**Checklist**: All NiAVObject fields accessed via `.av.*` (no stale field access), shader property lookup covers all shader types for each game variant, texture path resolution works for NiTexturingProperty (Oblivion), BSShaderPPLightingProperty (FO3/FNV), BSLightingShaderProperty (Skyrim), BSEffectShaderProperty (Skyrim+), coordinate conversion (Z-up to Y-up) applied consistently, decal flag detection covers all shader flag bit positions per game.
**2026-05-28 collision-translation pins (regression guards)**:
- **Collision-shape translation completeness** (9c6096aa): `crates/nif/src/import/collision.rs::extract_collision` MUST translate `BhkMultiSphereShape` + `BhkConvexListShape` to `CollisionShape` (they were silently dropped before this fix — verify the `downcast_ref::<BhkMultiSphereShape>()` / `downcast_ref::<BhkConvexListShape>()` arms are still present). Regression = a future collision shape that parses but never reaches `extract_collision`.
- **Per-variant collision dispatch** (#1277 Task 1, 8d3a6861): `examine_collision_kind(scene, collision_ref) -> CollisionAuthoring` classifies the authoring before `extract_collision` runs. The `CollisionAuthoring` enum (in `collision.rs`) must distinguish `None` / `Classic` / `NewPhysicsStub` / `Phantom` / `Unrecognised` (note British spelling). Regression = collapsing the discriminator so Skyrim+ phantoms (`BhkPCollisionObject` wrapping `bhkPhantom`) get force-translated as rigid bodies instead of routed to a future TriggerVolume path. See `/audit-nifal` for the canonical-Material side of per-variant translation dispatch.
**Output**: `/tmp/audit/nif/dim_4.md`

### Dimension 5: Coverage Gaps
**Entry points**: `crates/nif/src/blocks/mod.rs` (parse_block dispatch), `docs/legacy/nif.xml`
**Checklist**: List all block type names that appear in real game NIFs (from corpus or BSA listing) but are not in the parse_block dispatch table, count NiUnknown fallbacks per game, identify which missing block types cause cascading failures (blocks without block_size in Oblivion format), estimate coverage percentage per game.
**2026-05-28 translation-coverage pin (test-infra signal)**:
- **Per-game translation-completeness harness** (#1277 epic): `crates/nif/tests/translation_completeness.rs::cross_game_translation_completeness` is the per-game material-translation coverage regression surface — parallel to how `crates/nif/tests/heap_allocation_bounds.rs` (#1247) guards allocation count for Dim 6. Parse-block coverage (this dimension) measures whether a block *parses*; the completeness harness measures whether a parsed block *translates* to a canonical `Material`. Future translation-coverage findings should extend this test rather than add an ad-hoc check. See `/audit-nifal` for the translation-tier audit this harness anchors.
**Output**: `/tmp/audit/nif/dim_5.md`

### Dimension 6: Stream Allocation Hygiene (PERF — 2026-05-04 batch + 2026-05-23 follow-up)
**Entry points**: `crates/nif/src/stream.rs` (allocate_vec, read_pod_vec), all `blocks/*.rs` callers, `byroredux/src/streaming.rs::pre_parse_cell`
**Checklist**:
- `stream.allocate_vec::<T>(n)?;` carries `#[must_use]`. Bound-check-only call sites that discard the empty Vec are a leak/no-op pattern fixed at 9 sites by #831 NIF-PERF-03; the attribute prevents recurrence — verify it's still on. **#1246 extended the `#[must_use]` discipline** (`a56c9e71`) to the `read_pod_vec` wrappers AND the KFM `allocate_vec` site
- 6 NIF bulk-array readers go through `read_pod_vec<T>` to collapse double allocation (#833 NIF-PERF-02). Direct allocate-then-loop-and-fill is the regression. The helper has a top-of-module compile-error gate for big-endian hosts; bytemuck is NOT a workspace dep despite some audits claiming it
- Per-block parse-loop counters use `entry().get_mut() / insert` split, NOT `entry().or_insert(name.to_string())` (#832 NIF-PERF-01) — the to_string path leaks ~150 KB/cell of throwaway short-string allocations on Oblivion
- #408 blanket `allocate_vec` sweep (60+ sites across 12 NIF files, Session 12): any new bulk-read site MUST use `allocate_vec` or `read_pod_vec`, NOT `Vec::with_capacity` + per-element read in a loop
- **#1245 `ragdoll.rs` `allocate_vec` adoption** (`8490d829`): the bone-pose / ragdoll-template parse path now uses `allocate_vec` instead of the `check_alloc` idiom. Verify no other bhk*/ragdoll parser in `crates/nif/src/blocks/collision/` regressed back to the old idiom
- **BSGeometry bulk-read fast path** (#1263 + #1265, `dd02ad3f`, 2026-05-24): `BSGeometry` (Starfield 155-bsver mesh) extracts vertex/index data via the `read_pod_vec` bulk-read fast path. NiTriShape tangent extraction uses `std::mem::take` on the `Vec<f32>` to avoid a per-mesh clone. Regression pattern: a future BSGeometry change that re-introduces `Vec::with_capacity` + per-vertex loop, OR a clone instead of `mem::take` on tangents. Affects parse perf at Starfield-cell scale
- **NIF dispatch `Arc<str>` regression + rayon serial fast path** (#1261 + #1262, `6368b077`, 2026-05-24): per-block dispatch routes through `Arc<str>` for block names (interned). The rayon-parallel parse path has a serial fast-path for small models (`pre_parse_cell` in `byroredux/src/streaming.rs`). Verify both paths are wired — serial fast path is the regression target when small cells go through the parallel overhead path
- **`pre_parse_cell` serial extract → parallel parse split** (#877, `ba646f8b`, 2026-05-23): the cell-streaming `pre_parse_cell` worker is now a two-phase pipeline (serial header extraction → rayon-parallel body parse). The serial extract phase is what the #1262 fast path skips on small models. Verify the phase split is intact — collapsing them back into a monolithic parallel pass is the regression
- **`#[must_use]` extension on `read_pod_vec` wrappers + KFM `allocate_vec`** (#1246, `a56c9e71`, 2026-05-23): six read_pod_vec helper functions and the KFM allocate_vec call now carry `#[must_use]`. A future helper that doesn't carry the attribute is the regression pattern
- **dhat-gated allocation-bound regression test landed** (#1247, `88cd8792`, 2026-05-23): NIF parser allocation count is now pinned by a `cfg(feature = "dhat")`-gated test. The infrastructure gap flagged across earlier audits is partially closed — verify the test is in `crates/nif/tests/` and asserts an upper-bound allocation count on a canonical input. Future alloc-reduction findings can NOW be guarded by extending this test (was the open dhat-infra gap noted in 2026-05-04+)
- **Per-game NiPSysEmitter / NiTextureEffect version gating** (#1239 + #1240, `97524667` + `9cb93b5b`, 2026-05-23):
  - `NiPSysEmitter` now routes through nif.xml's version gate so Oblivion (`bsver < 26`) parses correctly — was previously assuming Skyrim+ field layout
  - `NiTextureEffect.NiDynamicEffect`-base parsing is gated on `bsver < FALLOUT4` (the FO4+ split removed the embedded NiDynamicEffect)
  - Regression pattern: a future block parser that hardcodes a single layout without checking BSVER bands
**Output**: `/tmp/audit/nif/dim_6.md`

### Dimension 7: NIFAL Canonical Translation (NEW — 2026-05-28)
**See also `/audit-nifal`** — the dedicated NIFAL (NIF Abstraction Layer) audit owns the full canonical-translation tier; this dimension is the NIF-parser-side boundary check. Spec: `docs/engine/nifal.md`.
**Entry points**: `byroredux/src/material_translate.rs::translate_material` (the SINGLE per-game `ImportedMesh` → ECS `Material` boundary — per-game classification happens here, never at render time), `crates/core/src/ecs/components/material.rs` (`Material::resolve_pbr`, `EmissiveSource`, `classify_pbr_keyword`), `crates/nif/src/import/types.rs` (`ImportedMesh` carrying `metalness_override` / `roughness_override` / `bgem_glass` — the BGEM authoritative-glass flag), `byroredux/src/helpers.rs::classify_glass_into_material` (glass classified once, after the PBR resolve)
**Checklist**:
- **Single boundary**: `translate_material` is the only place a per-game `ImportedMesh` becomes a `Material`. The project invariant is that per-game material classification stays at this parser→Material boundary and NEVER leaks into the renderer/shader. A new per-game material decision added downstream of this fn is the regression.
- **PBR resolve-once** (3ce98db8): `Material.metalness` / `Material.roughness` are plain `f32` (NOT `Option`, NOT a per-draw classifier). `translate_material` seeds the `f32::NAN` sentinel for any slot the upstream parser left unauthored (`mesh.metalness_override.unwrap_or(f32::NAN)`), then calls `material.resolve_pbr()` exactly once. `resolve_pbr` fills NaN slots via the shared `classify_pbr_keyword` and is idempotent + only fills the missing slot (does not clobber authored values). Verify the deleted render-time `Material::classify_pbr` method has NOT been reintroduced.
- **EmissiveSource discriminator** (2e884741): the `EmissiveSource` enum (`None` / `Material` legacy scalar / `Lighting` Skyrim+ / `Effect` FO4+ effect-shader diffuse-tint) records *where* the emissive scalar came from. Verify the discriminator is set at translate time, not inferred per-draw.
- **BGEM glass authoritative signal** (4a96d50e / #1280): a referenced `.bgem` authoring `glass_enabled = true` sets `ImportedMesh.bgem_glass`, an authoritative glass trigger independent of path/name keywords. `translate_material` runs `classify_glass_into_material` AFTER `resolve_pbr` so the forced glass roughness wins. Regression = an opaque architecture piece getting force-classed as glass off a stuck flag, or the keyword path overriding an authored BGEM glass signal.
- **Per-variant `BSLightingShaderProperty::parse` dispatch** (#1279, 4e587c3d): `crates/nif/src/blocks/shader.rs` splits the monolithic parse into a thin `bsver` dispatcher → `parse_skyrim` (83–129) / `parse_fo4` (130–154) / `parse_fo76_plus` (≥155). Verify each variant reads only its own field set (e.g. Skyrim-only `lighting_effect_1/2`, no FO4 subsurface / FO76 trailing).
- **Translation completeness**: `crates/nif/tests/translation_completeness.rs::cross_game_translation_completeness` is the regression surface for per-game material-translation coverage — extend it for any new translation-coverage finding (see Dim 5 pin).
**Output**: `/tmp/audit/nif/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/nif/dim_*.md` files
2. Combine into `docs/audits/AUDIT_NIF_<TODAY>.md` with structure:
   - **Executive Summary** — Coverage per game, total mismatches, critical gaps
   - **Block Type Coverage Matrix** — Table of block types × games (parsed/skipped/unknown)
   - **Findings** — Grouped by severity
   - **Prioritized Fix Order** — Blocks needed for rendering first, then animation, then collision
3. Remove cross-dimension duplicates

Suggest: `/audit-publish docs/audits/AUDIT_NIF_<TODAY>.md`
