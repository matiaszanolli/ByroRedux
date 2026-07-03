---
description: "Per-game audit of Fallout 3 compatibility — NIF v20.2.0.7, BSA v104, ESM via FNV-shared parser"
argument-hint: "--focus <dimensions>"
---

# Fallout 3 Compatibility Audit

Deep audit of ByroRedux readiness for **Fallout 3** content.

FO3 rides the **FNV path** almost end-to-end: same NIF era (v20.2.0.7 / BSVER 34),
same BSA format (v104), same ESM parser, same cell loader, same RT lighting. So
this audit is NOT a re-run of `/audit-fnv` — it hunts the **divergences**: where
FO3 content exercises a path FNV doesn't, where the shared code carries an
FNV-only assumption, and where a feature is verified on FNV but only *assumed* on
FO3. Order below is by FO3 risk, highest first.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations,
methodology, deduplication, and finding format. See `.claude/commands/_audit-severity.md`
for the severity scale (including the NIFAL canonical-translation rows).

## Game Context

| Aspect          | State                                                                       |
|-----------------|-----------------------------------------------------------------------------|
| NIF format      | v20.2.0.7 (BSVER 34) — same as FNV                                          |
| BSA format      | v104 ✓                                                                       |
| ESM parser      | Shared with FNV (`crates/plugin/src/esm/`); no FO3-specific arm             |
| NIF parse rate  | 100.00% (10 989) — ROADMAP compat matrix                                    |
| ESM records     | 44 657 = 37 459 structured + 7 198 NAVMs (re-verified 2026-05-26)           |
| Interior        | ✓ — Megaton, **929 REFRs** (parse-side baseline; NOT the stale 1609 figure) |
| Exterior        | Wired (Capital Wasteland WRLD); **fresh GPU bench pending (R6a-stale-15)**   |
| Scripting       | 1 257 SCPT records parse; no runtime executes them (M47.0; tail M47.2)      |
| Reference data  | `/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/`              |

### FO3-vs-FNV divergence map (the actual audit surface)

- **Inline shader stack only.** FO3 ships `BSShaderPPLightingProperty` /
  `BSShaderNoLightingProperty` with the legacy single-u32 flag layout — never
  `BSLightingShaderProperty` (Skyrim+) and never BGSM external materials (FO4+).
  Disney-BSDF and BGSM paths must be provably unreachable.
- **Earlier authoring conventions** than FNV: pre-FNV record subforms (NPC_,
  DIAL/INFO), `BSSegmentedTriShape` biped parts, FO3-era particle stacks. These
  are the FNV-shared paths most likely to hit an untested edge on FO3 data.
- **Different worldspace.** Capital Wasteland is a distinct WRLD form ID with its
  own origin/CLMT curves — any FNV-hardcoded worldspace name or coord is a bug.
- **B-splines are reachable** (`NiBSplineCompTransformInterpolator`) — do not
  rule them out by game era (`feedback_bspline_not_skyrim_only.md`).
- **SpeedTree 5.x** (`crates/spt/`) is the FO3/FNV `.spt` generation — placeholder
  billboard fallback only.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,4`). Default: all 7.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/fo3`.
3. Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Fallout 3 goty/Data/` exists; if not, note which dimensions lose real-data validation.
5. Before reporting a status/count claim, reconcile against the ROADMAP per-game
   compat matrix and `docs/feature-matrix.md` — both carry live FO3 numbers.

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: FO3 Rendering Path — Inline Shaders (highest divergence)
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/import/material/mod.rs` (+ `walker.rs`, `shader_data.rs`), `byroredux/src/material_translate.rs` (`translate_material`), `crates/core/src/ecs/components/material.rs` (`resolve_pbr`, `EmissiveSource`, `classify_pbr_keyword`), `crates/nif/src/shader_flags.rs`, `crates/renderer/shaders/triangle.frag`
**Checklist**:
- `BSShaderPPLightingProperty` flag bits mapped correctly (decal, alpha-test, two-sided, glow, window). **Per-game flag bit positions differ across FO3/FNV/Skyrim** — confirm FO3 uses the legacy single-u32 layout via `crates/nif/src/shader_flags.rs`, not the FO4 u32-pair.
- Normal map handle resolved from the dedicated `BSShaderTextureSet` normal slot (FO3+), NOT the bump slot — Oblivion reads normals from the bump slot via `NiTexturingProperty`; FO3 must not regress into that path.
- `BSShaderNoLightingProperty` (UI / sky / glow / blood-splat) routes through the **fullbright** path (`c351e0b6`) and its decal flags are honored (`crates/nif/src/import/material/mod.rs` — the pre-#454 NoLighting branch had no flags2 check). Self-illumination must not dim with distance.
- **NIFAL single boundary (#1241 / #1244, `3ce98db8`)**: FO3 materials flow through the SINGLE `material_translate::translate_material` (`ImportedMesh` + `ResolvedPaths` → canonical `Material`). `Material.metalness` / `Material.roughness` are plain resolved `f32` (no `Option`, no per-draw classify). FO3 authors no BGSM, so they arrive as the NaN sentinel and `Material::resolve_pbr` fills them via `classify_pbr_keyword`. Confirm concrete `f32` scalars out of the one boundary; no second per-game material site.
- **`EmissiveSource` discriminator (#1280, `2e884741`)**: FO3 `BSShaderPPLighting`/`NoLighting` self-illumination maps to `EmissiveSource::Material` (legacy `NiMaterialProperty.emissive_mult` slot). All three variants currently share the `emissive_mult` slot — the discriminator is provenance, not yet a render branch. Skyrim+ `Lighting` / FO4+ `Effect` variants must not bleed into the FO3 ~1.0 scale.
- **Disney-BSDF gate (#1248–#1252)**: zero FO3 materials author BGSM, so `MAT_FLAG_PBR_BSDF` (`(1u << 5)`, defined in `crates/renderer/shaders/include/shader_constants.glsl`) must be 0 across the Fallout3.esm material universe — the Burley/anisotropic-GGX/per-material-IOR lobe (`crates/renderer/shaders/include/pbr.glsl`) is unreachable for FO3. If any FO3 scene activates it, the gate regressed.
- **`WaterShaderProperty` (#1243, `3509482a`)**: FO3 water materials route through `MaterialInfo` for distinct `GpuMaterial` entries (no dedup collapse with glass).
- See also `/audit-nifal`.
**Output**: `/tmp/audit/fo3/dim_1.md`

### Dimension 2: NIF v20.2.0.7 Parser — FO3 Block Subset
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/properties.rs`, `crates/nif/src/blocks/shader.rs`, `crates/nif/src/blocks/particle.rs`, `crates/nif/src/blocks/mod.rs` (dispatch), `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), `byroredux/src/systems/particle.rs` (`apply_emitter_params`)
**Checklist**:
- `BSShaderPPLightingProperty` field completeness (refraction strength/period, parallax passes/scale, bump-map tiling) and `BSShaderNoLightingProperty` decode.
- `BSSegmentedTriShape` (biped body parts) vertex-index handling.
- Stream-position audit: any block type that passes on FNV but trips on FO3-era authoring. The dispatch arm count is in `crates/nif/src/blocks/mod.rs` — a histogram shift from `nif_stats` flags a mis-dispatched block.
- **Typed particle emitters (NIFAL particle slice, `5708b5b9` / `9db60714` / `8f856d35`)**: `NiPSysEmitter` / `NiPSysEmitterCtlr` / `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` are TYPED blocks (`crates/nif/src/blocks/particle.rs` — `parse_box_emitter` / `parse_sphere_emitter` / `parse_grow_fade_modifier`), not the old opaque controller stack. `extract_emitter_params` / `extract_emitter_rate` decode authored base kinematics + birth rate + GrowFade `base_scale` into `apply_emitter_params`. Dispatch is version-agnostic so FO3 smoke/fire/dust emitters hit this path — **but the NIFAL decode doc-comments verify only against FNV + Oblivion, so FO3 is an UNVERIFIED gap**: confirm FO3 authored emitter params + rate + GrowFade scale extract correctly. See also `/audit-nifal`.
**Output**: `/tmp/audit/fo3/dim_2.md`

### Dimension 3: ESM Record Coverage (Fallout3.esm)
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/records/`, `crates/plugin/src/esm/cell/` (post-Session-34 split — `walkers.rs` / helpers / support / `wrld.rs`), `byroredux/src/cell_loader/refr_texture_overlay_tests.rs`
**Checklist**:
- Parse `Fallout3.esm` through the shared parser. Reconcile against the live baseline: **44 657 records = 37 459 structured + 7 198 NAVMs** (re-verified 2026-05-26). A drop here is the regression signal — do NOT use any older "13 684 structured" figure.
- FO3-unique authoring vs FNV: pre-FNV subforms for NPC_, DIAL, INFO. The parser deliberately keeps the soft/strict truncation read semantics (`crates/plugin/src/esm/sub_reader.rs` migration, R2 Phase B).
- **SCPT SCHR flags are a u16 (#1654, `590351c1`)**: shared with Oblivion/FNV — the SCHR is exactly 20 bytes with a `u16` flags tail (cursor @18). `crates/plugin/src/esm/records/script.rs` reads it via `u16_or_default`; the old "u32 tail on FO3+" comment was itself wrong. A regression to a `u32` read fails on every real FO3 script and pins `ScriptRecord.flags` to 0.
- CELL XCLL / RCLR layout identity vs FNV (FO3 interior lighting uses the same `CellLightingRes` path — confirm, don't assume).
- WATR (rivers/ponds) and NAVM differences. WTHR / CLMT pulled through the shared parser.
- **REFR per-instance texture overrides (XATO/XTNM/XTXR — #584)**: FO3 cell REFRs can carry per-instance texture-set overrides that resolve against `EsmCellIndex.texture_sets` and feed the `ResolvedPaths` consumed by `translate_material` (Dim 1). Confirm FO3 overlays produce distinct resolved paths, not a collapse to the base mesh material. Regression tests: `byroredux/src/cell_loader/refr_texture_overlay_tests.rs`.
- **TXST/XATO/XTNM/XTXR dispatch** lives in `crates/plugin/src/esm/cell/walkers.rs`; an `unreachable_patterns` warning there is a smell.
**Output**: `/tmp/audit/fo3/dim_3.md`

### Dimension 4: FO3 Cell Loading End-to-End (interior + exterior)
**Subagent**: `general-purpose`
**Entry points**: `byroredux/src/cell_loader/{load,unload,exterior,references,spawn}.rs`, `byroredux/src/scene/{nif_loader,world_setup}.rs`
**Checklist**:
- FO3 interior loads via the SAME `--esm Fallout3.esm --cell <id>` CLI as FNV. Megaton parse-side baseline: **929 REFRs** (down from 1609 pre-NIF-expand; #455+, cell-loader stale-comment cleanup #822 / `ca6be24`). Any audit citing 1609 references pre-expand stats — confirm against current `cell_loader/*.rs` comments first.
- **Exterior is WIRED** (ROADMAP: "Exterior wired; fresh GPU bench pending"). Capital Wasteland is a distinct WRLD form ID — audit `cell_loader/exterior.rs` + `crates/plugin/src/esm/cell/wrld.rs` for any FNV-hardcoded worldspace name, origin coord, or default grid that would mis-place FO3 exterior cells. The open item is a fresh GPU bench (R6a-stale-15), not a missing feature.
- No FNV-only branch in the shared cell loader. WTHR→CLMT→WTHR resolution and FO3 CLMT sun-position curves resolve through the shared weather path.
- `CachedNifImport` Arc cache prevents duplicate parsing; no leak across FO3 unload/load cycles.
- `_far.nif` distant-object LOD (#1726/#1745, Session 52) — verify the
  Oblivion/FO3/FNV placement scheme + real LOD textures resolve on FO3's
  Capital Wasteland exteriors; entry points `cell_loader/object_lod.rs`,
  `cell_loader/placement_lod.rs`.
**Output**: `/tmp/audit/fo3/dim_4.md`

### Dimension 5: FO3 Collision Import (Havok → CollisionShape)
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/import/collision.rs` (`extract_collision`, `examine_collision_kind`, `resolve_shape`, `CollisionAuthoring`)
**Checklist**:
- FO3 Havok content is no longer merely skipped via `block_size` — `extract_collision` walks `bhk*CollisionObject` → `BhkRigidBody` → shape into `CollisionShape` + `RigidBodyData`. `examine_collision_kind` must classify FO3 chains as `CollisionAuthoring::Classic` (BSVER 34, legacy side), not `NewPhysicsStub` / `Phantom` / `Unrecognised` — a misclassified discriminator silently drops the rigid body.
- **#1277 / `9c6096aa`**: `BhkMultiSphereShape` (→ sphere path) and `BhkConvexListShape` (→ `CollisionShape::Compound`, mirroring `BhkListShape`) now translate — they were dropped before. FO3 uses these in static/clutter collision; confirm FO3 meshes carrying them yield a non-`None` `extract_collision`, not a discarded shape.
- **bhk motion_type via the canonical Havok enum (#1652, `dc33ec7d`)**: `collision.rs::havok_motion_type` maps the raw `hkMotionType` byte per the full nif.xml enum (1–5/8 → Dynamic, 6 KEYFRAMED → Keyframed, 7 FIXED → Static, 9 CHARACTER → CharacterKinematic, 0/other → Static). The pre-fix `4 => Keyframed` / `_ => Static` collapse mis-typed BOX_INERTIA (4) clutter (crates/ammo boxes/debris) as kinematic-frozen instead of falling — shared with FNV/Oblivion, so confirm FO3 dynamic clutter still simulates.
- Cross-check the Dim 2 "skips via block_size" note — that is now only true for shape kinds WITHOUT a translator. See also `/audit-nifal` (collision is part of the canonical tier) and `/audit-nif` for raw block decode.
**Output**: `/tmp/audit/fo3/dim_5.md`

### Dimension 6: BSA v104 + Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/archive/`, `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**:
- `Fallout - Meshes.bsa` lists + extracts cleanly; current NIF parse rate **100% / 10 989** (`nif_stats`). `Fallout - Textures.bsa` DDS extraction yields valid BC1/BC3/BC5 headers. Folder-hash collisions across FO3's subdirectories. Format is identical to FNV — divergence here would be a v104 regression, not a format gap.
- Pick **Megaton** interior (validated baseline — should match 929 REFRs / current entity count; capture `/cmd stats` and compare to feature-matrix, NOT the stale 1609/199-tex/42-FPS numbers).
- Load a creature mesh (e.g. deathclaw): verify NiSkinData skinning extraction (`crates/nif/src/import/mesh/skin.rs`).
- Pick a UI/menu `BSShaderNoLightingProperty` element: verify the fullbright (non-Phong) route.
- Pick one FaceGen head mesh (`crates/facegen/`): parses; may not render fully — note the gap, don't fail the dimension.
- Load a `.spt` (SpeedTree 5.x, `crates/spt/`): confirm placeholder-billboard fallback, not a hard parse error.
**Output**: `/tmp/audit/fo3/dim_6.md`

### Dimension 7: FO3 Animation / NPC Spawn + Scripting Gap
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/anim/` (Session-35 split: `entry`, `sequence`, `controlled_block`, `transform`, `bspline`, `channel`, `keys`, `coord`), `crates/core/src/animation/`, `byroredux/src/anim_convert.rs`, `byroredux/src/npc_spawn.rs`
**Checklist** — M41.0 long-tail regression guards (shared with FNV, Session 29; verify they hold on FO3 data):
- B-spline pose-fallback (#772, `3c32a5e`): gated on the `FLT_MAX` sentinel. B-splines are reachable on FO3 (`feedback_bspline_not_skyrim_only.md`) — don't rule them out by era.
- `AnimationClipRegistry` dedup (#790, `da99d15`): case-insensitive interning by lowercased path; without it one keyframe set leaks per cell load (RAM growth on FO3 exterior streaming).
- NPC hand-mesh load (#793 / M41-HANDS, `da8d7e2`): `lefthand.nif` + `righthand.nif` loaded alongside `upperbody.nif` on kf-era NPCs (`byroredux/src/npc_spawn.rs`). Megaton dwellers depend on this — bodies with no hands = #793 regression. FO3 kf-era spawn works because its `skeleton.nif` resolves (unlike FO4).
- **Scripting gap (FO3-distinctive)**: 1 257 FO3 SCPT records parse but **no runtime executes them** (M47.0 event-hook + M47.1 condition-eval landed; the M47.2 compiled-Papyrus recognizer slice is in progress). This is the largest FO3-specific functional gap — note it as a known blocker for FO3 quest/world interactivity, not a bug to file. The scripting runtime itself is owned by `/audit-scripting` (crates/scripting, crates/pex, crates/papyrus) — do not deep-audit it here.
**Output**: `/tmp/audit/fo3/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fo3/dim_*.md`.
2. Combine into `docs/audits/AUDIT_FO3_<TODAY>.md`:
   - **Executive Summary** — Compatibility level + delta vs FNV (what's shared, what diverges).
   - **Dimension Findings** — Grouped by severity per dimension.
   - **FNV-Shared Surface** — Record types / block types / shader paths FO3 inherits from FNV coverage, plus any FO3-only gap inside them.
   - **FO3-Distinctive Gaps** — Inline-shader-only material universe, Capital Wasteland worldspace, the 1 257-SCPT scripting runtime gap.
   - **Validation Status** — Interior (Megaton 929 REFRs) + exterior (wired, bench pending) + creature/NPC.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FO3_<TODAY>.md`
