---
description: "Per-game audit of Fallout New Vegas compatibility — reference title, ESM + cells + RT lighting + ragdoll"
argument-hint: "--focus <dimensions>"
---

# Fallout New Vegas Compatibility Audit

Deep audit of ByroRedux readiness for **Fallout: New Vegas** content. FNV is the **reference title** — the most-validated end-to-end path in the engine and the *reference realization* for the canonical translation layers (NIFAL material/physics, PHYSAL ragdoll). Audits here hunt regressions and unshipped polish, not missing foundations: a foundation that broke on FNV is the single highest-severity finding this command can produce.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for the master project-layout map, key reference docs, game-data locations, methodology, dedup rules, and finding format. See `.claude/commands/_audit-severity.md` for severity.

## Game Context

| Aspect         | State                                                                                  |
|----------------|----------------------------------------------------------------------------------------|
| NIF format     | v20.2.0.7 · `bsver` 34 (`bsver::FO3_FNV` in `crates/nif/src/version.rs`)                |
| BSA format     | v104 — `crates/bsa/src/archive/`                                                        |
| ESM parser     | Long-tail dispatch closed; `unknown_records` catch-all removed                          |
| Ragdoll        | PHYSAL slice 1 *reference* (classic bhk chain) — `byroredux/src/ragdoll.rs`             |
| Reference data | `/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/`                       |

**Authoritative status** — do NOT hardcode counts here (they rot). Pull live from:
- `ROADMAP.md` — per-game compat matrix (FNV parse rate, the Prospector bench-of-record entity/FPS/fence/draw numbers + the commit they were taken at), Known Issues.
- `docs/feature-matrix.md` — what works at runtime on FNV per subsystem.

The Prospector Saloon bench is the FNV bench-of-record; treat any drop below the ROADMAP-recorded numbers (at the recorded commit) as the regression baseline. The full pre-collider FNV baseline has not been recovered — see ROADMAP Known Issues before flagging fence/FPS as a fresh regression.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 8.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/fnv`.
3. Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Fallout New Vegas/Data/` exists (required — FNV is the baseline).
5. Read the FNV row of `ROADMAP.md`'s compat matrix + `docs/feature-matrix.md` to capture the *current* baseline numbers and commit. Every "regression" claim is judged against those, not against numbers written into this skill.

## Phase 2: Launch Dimension Agents (parallel)

Dimensions are ordered by current FNV risk: the layers most likely to silently break FNV first (cell load + canonical translation + RT), regression guards last.

### Dimension 1: Cell Loading End-to-End (highest blast radius)
**Subagent**: `general-purpose`
**Entry points**: `byroredux/src/cell_loader/` (`cell_loader.rs` is a thin dispatcher), `byroredux/src/scene/world_setup.rs`, `byroredux/src/streaming.rs`
**Checklist**:
- Interior load — Prospector Saloon entity count + XCLL lighting + `NiAlphaProperty` decal routing.
- Exterior 7×7 (radius 3) WastelandNV grid — LAND terrain (`byroredux/src/cell_loader/terrain.rs`), LTEX/TXST splat, WTHR→CLMT→WTHR resolution, cloud texture resolution through the asset provider's `TextureProvider`.
- `NifImportRegistry` Arc cache (`byroredux/src/cell_loader/nif_import_registry.rs::CachedNifImport`) prevents duplicate parsing across cells.
- **Cell unload hygiene (regression guard)**: `byroredux/src/cell_loader/unload.rs` must drop BLAS per freed mesh handle and release physics bodies. **#1520 (`34c7a218`): Rapier bodies/colliders are released on unload** — verify the unload path frees them (covered by `byroredux/src/cell_loader/rapier_release_tests.rs`); a leak here compounds per cell-streaming cycle. Also check the `inventory_release_tests.rs` / `unload_skin_cleanup_tests.rs` siblings.
- M38 water — `byroredux/src/cell_loader/water.rs` spawns `WaterPlane` per cell; `byroredux/src/systems/water.rs::submersion_system` writes camera submersion state on entry.
**Output**: `/tmp/audit/fnv/dim_1.md`

### Dimension 2: NIFAL Canonical Translation — FNV Slice
**Subagent**: `legacy-specialist`
**Entry points**: `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`, `crates/nif/src/import/collision.rs`, `docs/engine/nifal.md`
**Checklist**: FNV is the reference content for this boundary, so it must be exercised here first.
- `material_translate.rs::translate_material` is the **single** `ImportedMesh → Material` boundary — no second per-game material path may exist.
- FNV materials land with `Material::metalness` / `roughness` as **plain resolved `f32`** (`material.rs`), not `Option`. `Material::resolve_pbr` (→ `classify_pbr_keyword`) runs **once** at translation — there must be no per-draw keyword scan in `byroredux/src/render/static_meshes.rs` (the old render-time `Material::classify_pbr` is deleted).
- **EmissiveSource guard**: FNV legacy emissive uses `EmissiveSource::Material` (the genuine `NiMaterialProperty.emissive_mult` scalar). The `EmissiveSource` enum (`material.rs`) carries `Material` / `Lighting` / `Effect` variants; Skyrim+ `Lighting` and FO4+ `Effect` must not bleed into the FNV `Material` path (~1.0 scale untouched).
- **Collision-shape no-drop guard (`9c6096aa`)**: `BhkMultiSphereShape` + `BhkConvexListShape` translate to `CollisionShape` via `collision.rs::resolve_shape` (Compound of `Ball` children / `ConvexHull`) — previously silently dropped. Any FNV mesh with a multi-sphere / convex-list Havok shape must surface a `CollisionShape`.
- **No-fabrication invariant**: translation may not invent PBR values FNV never authored; keyword-classified dielectric defaults are fine, fabricated metalness is not.
- See `/audit-nifal` for the dedicated single-boundary / no-fabrication / no-render-time-fallback audit.
**Output**: `/tmp/audit/fnv/dim_2.md`

### Dimension 3: RT Lighting Pipeline — FNV Scenes
**Subagent**: `renderer-specialist`
**Entry points**: `crates/renderer/src/vulkan/acceleration/`, `crates/renderer/shaders/triangle.frag`, `crates/renderer/shaders/composite.frag`, `docs/engine/lighting-from-cells.md`
**Checklist**:
- TLAS frustum culling — no lights dropped for in-view fragments.
- ReSTIR-DI direct lighting in `triangle.frag` — `NUM_RESERVOIRS = 16` reservoirs/fragment, unbiased `W = resWSum / (K · w_sel)` estimator, shadow-ray budget caps, distance-based shadow/GI ray fallback.
- BLAS compaction + **LRU eviction at the dynamic VRAM-derived budget**: `predicates.rs::compute_blas_budget` = `device_local_bytes / 3` floored at `MIN_BLAS_BUDGET_BYTES` (~4 GB on a 12 GB-VRAM dev box — NOT any stale "1 GB" figure); the result is cached in the `blas_budget_bytes` field (`acceleration/mod.rs`).
- SVGF temporal accumulation uses motion vectors + `mesh_id` disocclusion; TAA Halton jitter + YCoCg variance clamp.
- M33 sky gradient + cloud layer blends correctly with tone-mapped geometry.
- **Disney BSDF gate guard (#1248–#1252)**: zero FNV materials author BGSM (FO4+), so `MAT_FLAG_PBR_BSDF` (`crates/renderer/shaders/include/shader_constants.glsl` = 32u) must be 0 across the FalloutNV.esm material universe — the Disney lobe at `triangle.frag` is unreachable for FNV. If any FNV scene activates Burley retro-reflection / anisotropic GGX / per-material-IOR Fresnel, the gate regressed.
- **#1125 skyTint interior gate** at `triangle.frag` reflection + refraction miss fallbacks (2 sites) — FNV interiors (Prospector, every Vault) must drop to cell ambient alone, not default zenith blue.
- Sun-sprite mip-0 force (`8b5d77c1`) at `composite.frag::compute_sky` — explicit `textureLod` 0.0 avoids pixelating the tiny screen-space sun disc.
**Output**: `/tmp/audit/fnv/dim_3.md`

### Dimension 4: ESM Record Parser — Coverage & Accuracy
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/records/`, `crates/plugin/src/esm/cell/` (post-split: `walkers.rs` / `helpers.rs` / `support.rs` / `wrld.rs`)
**Checklist**:
- Record counts on FalloutNV.esm match the ROADMAP / `feature-matrix` baseline (do not transcribe a fixed count into this skill — diff against the living doc).
- Spot-check semantics: Varmint Rifle stats, NCR faction relations, VATS AVIF entries (the FNV gameplay-record path in `crates/plugin/src/esm/records/index.rs` + `crates/plugin/src/esm/records/misc/effects.rs`).
- CELL `XCLL` `fog_far_color` optional-field handling.
- FO4 additions (SCOL/MOVS/PKIN/TXST) must not steal FNV dispatch — the TXST/`XATO`/`XTNM`/`XTXR` match arms live in `crates/plugin/src/esm/cell/walkers.rs`; an `unreachable_patterns` warning there is a code smell.
- LVLI leveled-list flattening — `crates/plugin/src/equip.rs::expand_leveled_form_id` resolves NPC default-outfit LVLI refs into base ARMO/WEAP; FNV NPCs whose outfits reference LVLI must spawn gear, not empty.
**Output**: `/tmp/audit/fnv/dim_4.md`

### Dimension 5: NIF Parser — FNV Regression Guard
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/`, `crates/nif/tests/parse_real_nifs.rs`, `crates/nif/examples/nif_stats.rs`
**Checklist**:
- Parse rate holds at the ROADMAP FNV figure; block histogram from `nif_stats` matches expected distribution (a meaningful shift = a block type being mis-dispatched).
- `NiTexturingProperty` decal-slot off-by-one; `BSMultiBound*`; `BSDecalPlacementVectorExtraData` all stay fixed (reference N23.4 FO3/FNV validation).
- **#1277 collision/version guards**:
  - `collision.rs::examine_collision_kind` classifies FNV chains as `CollisionAuthoring::Classic` (the bhk* path), not `NewPhysicsStub`/`Phantom`/`Unrecognised` — a misclassified discriminator silently drops the rigid body.
  - `version.rs` raw-`bsver`-compare migration: `bsver::FO3_FNV = 34`, `RIGID_BODY_FLAGS16 = 76`, `NI_BS_LTE_16 = 16` etc. must still place FNV (`bsver` 34, `> NI_BS_LTE_16`) on the post-Oblivion side of every gate — a flipped comparison shifts field layout and corrupts collision/anim reads.
- **#1269 walker guard**: `MAX_NIF_NODE_DEPTH = 128` in `crates/nif/src/import/walk/mod.rs` guards both hierarchical + flat walkers; a legit FNV scene must never trip the 128-depth bail (covered by `crates/nif/src/import/walk/tests.rs`).
**Output**: `/tmp/audit/fnv/dim_5.md`

### Dimension 6: Animation, Skinning & Particles (FNV)
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/anim/`, `crates/core/src/animation/`, `byroredux/src/anim_convert.rs`, `byroredux/src/npc_spawn.rs`, `byroredux/src/systems/particle.rs`
**Checklist**:
- `.kf` load from BSA; AnimationClipRegistry populated; `NiTransformInterpolator` + `NiFloatInterpolator` + `NiBoolInterpolator` channels sample correctly; NiTextKeyExtraData text events collected; Clamp/Loop/Reverse cycle types honored; FixedString interning at clip-load (#340) — no per-frame StringPool locks.
- Skinning regression (NOT a foundation check — GPU skinning M29 + #178 SkinnedMesh palette are live): NiSkinData sparse weights still parse; bone palette stays correct on the GPU path.
- **B-spline pose-fallback (#772)**: gated on a `FLT_MAX` sentinel; without it NPCs vanish under FNV `BSPSysSimpleColorModifier` particle stacks that share time-zero with the actor's player. `NiBSplineCompTransformInterpolator` IS reachable on FNV/FO3 — do not rule it out by game era.
- **AnimationClipRegistry dedup (#790)**: dedup by lowercased path so cell streaming doesn't grow it unboundedly (else one keyframe set leaks per cell load).
- **NPC hand-mesh load (#793)**: `lefthand.nif` + `righthand.nif` load alongside `upperbody.nif` on kf-era NPCs (`npc_spawn.rs`) — any body assembly loading only `upperbody` leaves Doc Mitchell / Sunny Smiles handless.
- **Typed-emitter particle pin (`5708b5b9` / `9db60714`)**: `NiPSysEmitter` / `NiPSysEmitterCtlr` / `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` are typed structs in `crates/nif/src/blocks/particle.rs`. `walk/mod.rs::extract_emitter_params` + `::extract_emitter_rate` feed `systems/particle.rs::apply_emitter_params` — FNV's heavy particle stacks must drive from the **authored** birth-rate / emitter size / `base_scale`, not preset kinematics. (Particle translation is part of the NIFAL tier — see `/audit-nifal`.)
**Output**: `/tmp/audit/fnv/dim_6.md`

### Dimension 7: PHYSAL Ragdoll — FNV Reference Slice
**Subagent**: `legacy-specialist`
**Entry points**: `byroredux/src/ragdoll.rs`, `crates/nif/src/import/collision.rs` (ragdoll + constraint decode), `crates/nif/src/blocks/collision/`, `docs/engine/physal.md`
**Checklist**: FNV is the *reference realization* for PHYSAL slice 1 (the classic bhk chain — `0a0bc3ce` / `2c21a470`, 2026-06-14). Newly shipped, so audit for correctness, not just regression.
- The importer hands `ImportedRagdoll` (bone *names* + `ImportedJointKind`); `ragdoll.rs::activate_ragdoll` resolves it against the skeleton's `GlobalTransform`, and `ragdoll_writeback_system` writes solver results back to bone transforms. Verify name resolution doesn't silently drop a joint on a real FNV creature/NPC skeleton.
- Per PHYSAL, the *only* per-game seam is the constraint CInfo decode — confirm no per-game branch leaked into `ragdoll.rs` or the solver bridge (`crates/physics/`).
- FNV's dominant constraint form is a `bhkMalleableConstraint` wrapping a Ragdoll (see `docs/engine/physal.md` §FO3/FNV) — confirm that decode path in `crates/nif/src/blocks/collision/constraints.rs` + `ragdoll.rs` survives and produces a jointed body, not a single rigid blob.
- Writeback must not corrupt the skinned bone palette feeding the GPU skin path (cross-check Dimension 6).
**Output**: `/tmp/audit/fnv/dim_7.md`

### Dimension 8: Real-Data Validation & Bench-of-Record
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, demo CLI invocations
**Checklist**:
- **CWD matters** (ROADMAP repro note): bare `--bsa` / `--textures-bsa` names resolve against CWD, not the `--esm` folder. Run with CWD = `Fallout New Vegas/Data/`, else archives silently fail and the scene loads near-empty (~36 entities / spurious FPS).
- Interior bench-of-record:
  `cargo run --release -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior --bsa Meshes.bsa --textures-bsa Textures.bsa --textures-bsa Textures2.bsa --bench-frames 300 --bench-hold`
  then attach `byro-dbg` (port 9876) and capture `stats`. Compare entity / draw / FPS / fence against the **ROADMAP FNV row** (not numbers in this skill).
- Exterior: `--grid <x>,<y> --radius 3` on WastelandNV.
- Validate `tex.missing` / `tex.loaded` return sensible output (FNV ships base textures split across `Fallout - Textures.bsa` + DLC archives — `tex.missing` first when surfaces look chrome/posterized).
**Output**: `/tmp/audit/fnv/dim_8.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fnv/dim_*.md`.
2. Combine into `docs/audits/AUDIT_FNV_<TODAY>.md`:
   - **Executive Summary** — FNV is the baseline; any regression against the ROADMAP-recorded numbers is at least HIGH (CRITICAL if it breaks a shipped foundation).
   - **Dimension Findings** — grouped by severity per dimension.
   - **Baseline Comparison Table** — ROADMAP number vs observed for entity count, draw count, FPS, fence, parse rate, record count (cite the ROADMAP commit you compared against).
   - **Regression Guard List** — previously-fixed issues this audit verified still correct.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FNV_<TODAY>.md`
