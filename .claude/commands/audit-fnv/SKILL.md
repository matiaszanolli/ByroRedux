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

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 9.

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
Companion docs: `docs/engine/pipeline-overview.md` (interior cell load trace) and
`docs/engine/exterior-grid-streaming.md` (exterior grid, background pre-parse,
cell-boundary + door-teleport swaps) — verified against the tree 2026-07-15 and
2026-07-27 respectively (each doc's own currency note has the exact date).
**Checklist**:
- Interior load — Prospector Saloon entity count + XCLL lighting + `NiAlphaProperty` decal routing.
- Exterior 7×7 (radius 3) WastelandNV grid — LAND terrain (`byroredux/src/cell_loader/terrain.rs`), LTEX/TXST splat, WTHR→CLMT→WTHR resolution, cloud texture resolution through the asset provider's `TextureProvider`.
- `NifImportRegistry` Arc cache (`byroredux/src/cell_loader/nif_import_registry.rs::CachedNifImport`) prevents duplicate parsing across cells.
- **Cell unload hygiene (regression guard)**: `byroredux/src/cell_loader/unload.rs` must drop BLAS per freed mesh handle and release physics bodies. **#1520 (`34c7a218`): Rapier bodies/colliders are released on unload** — verify the unload path frees them (covered by `byroredux/src/cell_loader/rapier_release_tests.rs`); a leak here compounds per cell-streaming cycle. Also check the `inventory_release_tests.rs` / `unload_skin_cleanup_tests.rs` siblings.
- M38 water — `byroredux/src/cell_loader/water.rs` spawns `WaterPlane` per cell; `byroredux/src/systems/water.rs::submersion_system` writes camera submersion state on entry.
- **Object LOD — `ObjectLodScheme::FalloutLegacyBlocks`** (#3321, `e23a9908`; the newest and least-reviewed FNV LOD code). Verify on the WastelandNV exterior grid:
  - quad path shape `meshes\landscape\lod\<world>\blocks\<world>.level<L>.x<qx>.y<qy>.nif` (`cell_loader/object_lod.rs::object_lod_archive_path`);
  - the shared atlas `textures\landscape\lod\<world>\blocks\<world>.buildings.dds` resolving out of `Fallout - Textures2.bsa` (`object_lod_atlas_path`);
  - the legacy-ladder arm of `LodBandLadder::for_object_game` (`cell_loader/lod_bands.rs`).
  Census pin (2026-08-27, all 20 FNV BSAs / 182 177 entries): **0 `_far.nif`, 0 `distantlod\` entries** — FNV ships neither, and `placement_lod_supported` is Oblivion-only by construction (`cell_loader/placement_lod.rs:313-315`, pinned by `placement_lod_supported_is_oblivion_only`). Do not re-derive this; the `_far.nif` route can only ever confirm a no-op here.
**Output**: `/tmp/audit/fnv/dim_1.md`

### Dimension 2: NIFAL Canonical Translation — FNV Slice
**Subagent**: `legacy-specialist`
**Entry points**: `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`, `crates/nif/src/import/collision/mod.rs`, `docs/engine/nifal.md`
**Checklist**: FNV is the reference content for this boundary, so it must be exercised here first.
- `material_translate.rs::translate_material` is the **single** `ImportedMesh → Material` boundary — no second per-game material path may exist.
- FNV materials land with `Material::metalness` / `roughness` as **plain resolved `f32`** (`material.rs`), not `Option`. `Material::resolve_pbr` (→ `classify_pbr_keyword`) runs **once** at translation — there must be no per-draw keyword scan in `byroredux/src/render/static_meshes.rs` (the old render-time `Material::classify_pbr` is deleted).
- **EmissiveSource guard**: FNV legacy emissive uses `EmissiveSource::Material` (the genuine `NiMaterialProperty.emissive_mult` scalar). The `EmissiveSource` enum (`material.rs`) carries `Material` / `Lighting` / `Effect` variants; Skyrim+ `Lighting` and FO4+ `Effect` must not bleed into the FNV `Material` path (~1.0 scale untouched).
- **Collision-shape no-drop guard (`9c6096aa`)**: `BhkMultiSphereShape` + `BhkConvexListShape` translate to `CollisionShape` via `collision/shape.rs::resolve_shape` (Compound of `Ball` children / `ConvexHull`) — previously silently dropped. Any FNV mesh with a multi-sphere / convex-list Havok shape must surface a `CollisionShape`.
- **No-fabrication invariant**: translation may not invent PBR values FNV never authored; keyword-classified dielectric defaults are fine, fabricated metalness is not.
- See `/audit-nifal` for the dedicated single-boundary / no-fabrication / no-render-time-fallback audit.
**Output**: `/tmp/audit/fnv/dim_2.md`

### Dimension 3: RT Lighting Pipeline — FNV Scenes
**Subagent**: `renderer-specialist`
**Entry points**: `crates/renderer/src/vulkan/acceleration/`, `crates/renderer/shaders/triangle.frag`, `crates/renderer/shaders/composite.frag`, `docs/engine/lighting-from-cells.md`
**Checklist**:
- TLAS frustum culling — no lights dropped for in-view fragments.
- ReSTIR-DI direct lighting in `crates/renderer/shaders/triangle.frag` (shared radiance helper `shadowableLightRadiance` in `crates/renderer/shaders/include/lighting.glsl`) — the default path is a SINGLE spatiotemporal reservoir. The old reservoir *G-buffer attachment* was retired #1583/#1590, but reservoir state did **not** stay register-local: `6b061120` reintroduced it as `reservoirsCurr`/`reservoirsPrev` SSBOs (set 1, bindings 16/17 in `crates/renderer/shaders/include/bindings.glsl`), which carry temporal reuse plus an in-pass spatial disk of previous-frame neighbours; estimator `W = wSum / (M · pHat)`, with `RESTIR_M_CAP` bounding history. The legacy per-frame 16-slot WRS arm (`NUM_RESERVOIRS = 16`, `W = resWSum / (K · w_sel)`) is preprocessed OUT under `ENABLE_LEGACY_WRS = 0` (#1799) — flip that constant to A/B, don't assume it's live. Also: shadow-ray budget caps, distance-based shadow/GI ray fallback.
- BLAS compaction + **LRU eviction at the dynamic VRAM-derived budget**: `predicates.rs::blas_budget_for_heap` = `(heap_bytes - reserved_bytes) / 3` floored at `MIN_BLAS_BUDGET_BYTES` (~4 GB on a 12 GB-VRAM dev box — NOT any stale "1 GB" figure). The `reserved_bytes` subtraction is #3839: the resolution-scaled reservation is taken off the top before the third is computed, so the old `heap / 3` is the pre-#3839 formula. The raw heap comes from the sibling `probe_blas_heap_bytes`; the split (`fa5c4191`) lets a resize re-derive the budget without re-probing the device. The result is still cached in the `blas_budget_bytes` field (`acceleration/mod.rs`), now alongside `blas_heap_bytes`.
- SVGF temporal accumulation uses motion vectors + `mesh_id` disocclusion; TAA Halton jitter + YCoCg variance clamp.
- M33 sky gradient + cloud layer blends correctly with tone-mapped geometry.
- **Disney BSDF gate guard (#1248–#1252)**: zero FNV materials author BGSM (FO4+), so `MAT_FLAG_PBR_BSDF` (`crates/renderer/shaders/include/shader_constants.glsl` = 32u) must be 0 across the FalloutNV.esm material universe — the Disney lobe (now in `crates/renderer/shaders/include/pbr.glsl`) is unreachable for FNV. If any FNV scene activates Burley retro-reflection / anisotropic GGX / per-material-IOR Fresnel, the gate regressed.
- **#1125 skyTint interior gate** at both glass miss fallbacks — the reflection miss in `traceReflection` (`crates/renderer/shaders/include/raytrace.glsl`) and the refraction miss in `crates/renderer/shaders/triangle.frag` — FNV interiors (Prospector, every Vault) must drop to cell ambient alone, not default zenith blue.
- Sun-sprite mip-0 force (`8b5d77c1`) at `composite.frag::compute_sky` — explicit `textureLod` 0.0 avoids pixelating the tiny screen-space sun disc.
**Output**: `/tmp/audit/fnv/dim_3.md`

### Dimension 4: ESM Record Parser — Coverage & Accuracy
**Scope split with `/audit-esm` (added 2026-08-13)**: `/audit-esm` owns the parser *as a parser* — GRUP walk, `SubReader` byte accounting, schema dispatch, FormID remap. This dimension owns **this game's data through it**: record counts, game-unique authoring, and the semantics that only show up on this title's masters. If the defect is in the shared mechanism, file it against `/audit-esm` instead of here.
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/records/`, `crates/plugin/src/esm/cell/` (post-split: `walkers.rs` / `helpers.rs` / `support.rs` / `wrld.rs`)
**Checklist**:
- Record counts on FalloutNV.esm match the ROADMAP / `feature-matrix` baseline (do not transcribe a fixed count into this skill — diff against the living doc).
- Spot-check semantics: Varmint Rifle stats, NCR faction relations, VATS AVIF entries (the FNV gameplay-record path in `crates/plugin/src/esm/records/index.rs` + `crates/plugin/src/esm/records/misc/effects.rs`).
- CELL `XCLL` `fog_far_color` optional-field handling.
- **SCOL is FNV-era, not an FO4 addition** (#1538): FalloutNV.esm carries **98 SCOL bases referenced by 1084 REFRs** (road segments, guardrails, debris LOD) — the `is_scol_era = is_fo4_plus || Fallout3NV` gate in `crates/plugin/src/esm/records/mod.rs` MUST keep dispatching `parse_scol_group` for FNV/FO3; re-narrowing it to FO4-only is the regression that silently drops those 1084 placements. The genuinely FO4+-only records are **MOVS / PKIN / MSWP** (byte-scan-confirmed absent from FalloutNV.esm) — those must not steal FNV dispatch. TXST/`XATO`/`XTNM`/`XTXR` cell-subrecord arms live in `crates/plugin/src/esm/cell/walkers.rs`; an `unreachable_patterns` warning there is a code smell. **FNV's 219 `XATO` are NOT texture overlays** (#3511/#1887): on FO3/FNV `XATO` is the *Activation Prompt* string sub-record, grouped with the SCRV/SCVR/SLSD script vars. FalloutNV.esm ships 0 `XTNM` and 0 `XTXR`, so the per-instance texture-override path has no FNV corpus either — it is an FO4 path (42 `XTNM` on Fallout4.esm).
- LVLI leveled-list flattening — `crates/plugin/src/equip.rs::expand_leveled_form_id` resolves NPC default-outfit LVLI refs into base ARMO/WEAP; FNV NPCs whose outfits reference LVLI must spawn gear, not empty.
- **SCPT SCHR flags are a u16 (#1654, `590351c1`)**: the Oblivion/FO3/FNV SCHR is exactly 20 bytes with a `u16` flags tail after the `script_type` u16 (cursor @18, 2 bytes left). `crates/plugin/src/esm/records/script.rs` reads it via `u16_or_default` into `ScriptRecord.flags`; a `u32` read fails on every real script and `unwrap_or(0)` pins flags to 0. The field is a u16 on every game — a regression back to u32 silently zeroes all script flags.
**Output**: `/tmp/audit/fnv/dim_4.md`

### Dimension 5: NIF Parser — FNV Regression Guard
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/`, `crates/nif/tests/parse_real_nifs.rs`, `crates/nif/examples/nif_stats.rs`
**Checklist**:
- Parse rate holds at the ROADMAP FNV figure; block histogram from `nif_stats` matches expected distribution (a meaningful shift = a block type being mis-dispatched).
- `NiTexturingProperty` decal-slot off-by-one; `BSMultiBound*`; `BSDecalPlacementVectorExtraData` all stay fixed (reference N23.4 FO3/FNV validation).
- **#1277 collision/version guards**:
  - `collision/mod.rs::examine_collision_kind` classifies FNV chains as `CollisionAuthoring::Classic` (the bhk* path), not `NewPhysicsStub`/`Phantom`/`Unrecognised` — a misclassified discriminator silently drops the rigid body.
  - **bhk motion_type via the canonical Havok enum (#1652, `dc33ec7d`)**: `collision/mod.rs::havok_motion_type` maps the raw `hkMotionType` byte per the full nif.xml enum (1–5/8 → Dynamic, 6 KEYFRAMED → Keyframed, 7 FIXED → Static, 9 CHARACTER → CharacterKinematic, 0/other → Static). The pre-fix `4 => Keyframed` / `_ => Static` collapse mis-typed BOX_INERTIA (4) clutter (crates/ammo boxes) as kinematic-frozen instead of falling — re-introducing the collapse is the regression.
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
**Entry points**: `byroredux/src/ragdoll.rs`, `crates/nif/src/import/collision/mod.rs` (ragdoll + constraint decode), `crates/nif/src/blocks/collision/`, `docs/engine/physal.md`
**Checklist**: FNV is the *reference realization* for PHYSAL slice 1 (the classic bhk chain — `0a0bc3ce` / `2c21a470`, 2026-06-14). Newly shipped, so audit for correctness, not just regression.
- The importer hands `ImportedRagdoll` (bone *names* + `ImportedJointKind`); `ragdoll.rs::activate_ragdoll` resolves it against the skeleton's `GlobalTransform`, and `ragdoll_writeback_system` writes solver results back to bone transforms.
- **Silent-drop regression guards (#1718/#1539/#1540/#1772, the D7 audit-guard family)**: `template_from_imported` (`ragdoll.rs`) warns on dropped bodies/constraints by bone-name miss (#1718, `ffe9a816`); `extract_ragdoll` (`import/collision/ragdoll.rs`) warns on dropped constraint kinds (#1539) — note #3330 then #3792 narrowed what reaches that arm: `bhkHingeConstraint` and `bhkPrismaticConstraint` are now both decoded into canonical `LimitedHinge`/`Prismatic` joints (both eras, bare **and** `BhkBreakableConstraint`-wrapped), leaving only `bhkBallAndSocketConstraint` / `bhkStiffSpringConstraint` undecoded (still `Other`), so the two Sentry Turret skeletons no longer fragment. `creatures\protectron\skeleton.nif` — the live example this bullet used to cite (2 × `bhkPrismaticConstraint` + 1 breakable-wrapped edge) — is fixed as of #3792 (13 bodies, 12/12 joints, 1 connected component; real-data-gated `fnv_protectron_skeleton_is_one_connected_component`, `crates/nif/tests/ragdoll_import.rs`). No FNV occupancy census has been run for BallAndSocket/StiffSpring (#3792 left it open), so there is no confirmed live example still hitting that drop path — look for one rather than assuming none exists. Trimesh bone inertia no longer degenerate (#1540); keyframed bone-follower bodies are torn down on ragdoll activation, not left double-simulating (#1772, `da4a849d`). Confirm all still hold on a real FNV skeleton with divergent bone naming.
- Per PHYSAL, the *only* per-game seam is the constraint CInfo decode — confirm no per-game branch leaked into `ragdoll.rs` or the solver bridge (`crates/physics/`).
- FNV's dominant constraint form is a `bhkMalleableConstraint` wrapping a Ragdoll (see `docs/engine/physal.md` §FO3/FNV) — confirm that decode path in `crates/nif/src/blocks/collision/constraints.rs` + `ragdoll.rs` survives and produces a jointed body, not a single rigid blob.
- Writeback must not corrupt the skinned bone palette feeding the GPU skin path (cross-check Dimension 6).
- **Scope split with `/audit-physics` (added 2026-08-13)**: the solver end — collider translation, the fixed-step accumulator, `build_ragdoll`/`remove_ragdoll` completeness, the character controller — is owned there. Keep this dimension on the FNV *source axis*: does FNV's authored bhk chain reach the canonical spec intact on real skeletons.
**Output**: `/tmp/audit/fnv/dim_7.md`

### Dimension 8: Real-Data Validation & Bench-of-Record
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, demo CLI invocations
**Checklist**:
- **CWD matters** (ROADMAP repro note): bare `--bsa` / `--textures-bsa` names resolve against CWD, not the `--esm` folder. Run with CWD = `Fallout New Vegas/Data/`, else archives silently fail and the scene loads near-empty (~36 entities / spurious FPS).
- **Prefer `--game fnv` for anything that isn't the bench-of-record (#3346).** It expands to *absolute* `--esm` / `--bsa` / `--textures-bsa` paths from `assets/debug_profiles.toml`, so it is CWD-independent and cannot mistype an archive name — neither failure mode above can occur:
  `cargo run --release -- --game fnv --cell GSProspectorSaloonInterior --bench-frames 300 --bench-hold`
  Since #3331 the bench-of-record below uses this same profile form. The
  bare-name + `cd` shape survives only in `ROADMAP.md`'s repro column, where it
  is annotated; do not copy it from there without adding `--upscaler taa`.
- Interior bench-of-record. **Use `scripts/fsr-bench-matrix.sh 3 300` as the
  authoritative form** — it already encodes the archive names, the CWD, and the
  upscaler sweep. Only hand-run the command when you need a single config:
  `cargo run --release -- --game fnv --cell GSProspectorSaloonInterior --upscaler taa --bench-frames 300 --bench-hold`
  then attach `byro-dbg` (port 9876) and capture `stats`. Compare entity / draw / FPS / fence against the **ROADMAP FNV row** (not numbers in this skill).
  Three things this command gets right that the pre-#3331 one did not:
  - **The archive names.** A vanilla FNV `Data/` has no `Meshes.bsa` /
    `Textures.bsa` — they are `Fallout - Meshes.bsa` / `Fallout - Textures.bsa`.
    `Archive::open` takes the **literal** path with no stem matching, so the old
    bare names opened nothing and produced exactly the ~36-entity near-empty
    scene the CWD bullet above warns about. `--game fnv` sidesteps the question
    entirely by expanding absolute paths from `assets/debug_profiles.toml`.
  - **`--upscaler taa`.** The flag defaults to `fsr3` (`cli_args.rs`), so the
    bare command measures **FSR 3.1 Quality (~254 FPS)** while the ROADMAP FNV
    row's headline figure is **TAA native (~145 FPS)**. Comparing them reads as
    a 75% "improvement" that is purely a config difference — this is #2560 /
    FNV-D8-01, annotated in ROADMAP but never propagated here until #3331.
  - **No third `--textures-bsa`.** `Fallout - Textures2.bsa` auto-loads as a
    `<stem>N.bsa` sibling of `Fallout - Textures.bsa` (`asset_provider/archive.rs`);
    naming it explicitly is redundant.
- Exterior: `--grid <x>,<y> --radius 3` on WastelandNV.
- Validate `tex.missing` / `tex.loaded` return sensible output (FNV ships base textures split across `Fallout - Textures.bsa` + DLC archives — `tex.missing` first when surfaces look chrome/posterized).
**Output**: `/tmp/audit/fnv/dim_8.md`

### Dimension 9: AI Packages & Procedure Runtimes (M41.5/M42–M42.8)
**Subagent**: `general-purpose`
**Entry points**: `byroredux/src/npc_spawn.rs` (`spawn_npc_entity`), `byroredux/src/npc_spawn/ai_package.rs` (`apply_ai_package_behavior` — the package-selection tail — + `package_conditions_pass`; split out of `npc_spawn.rs` under #2198), `byroredux/src/systems/{sandbox,wander,travel,follow,escort,guard,patrol,locomotion}.rs`, `crates/core/src/ecs/components/{sandbox,furniture,wander,travel,follow,escort,guard,patrol}.rs`, `crates/plugin/src/esm/records/misc/pack.rs` (PACK/PKDT/PSDT/PLDT/PTDT decode + the seven `active_package_is_*` selectors), `docs/engine/npc-spawn-ai-packages.md`
**Checklist**: Seven of ~17 FO3/FNV package procedures execute (Sandbox, Wander M42.3, Travel M42.4, Follow M42.5, Escort M42.6, Guard M42.7, Patrol M42.8) — audit for correctness of what's implemented, not for missing scope. **Do not** flag the absence of a Find/Eat/Sleep/Accompany/UseItemAt/Ambush/FleeNotCombat/CastMagic/Dialogue/UseWeapon runtime as a bug — each needs a subsystem (item/furniture use beyond seat-snap, combat, magic, dialogue) that doesn't exist in this engine at all yet, not just a missing dispatch arm. Do also not flag the seven's documented v0 approximations (no animation-clip swap, `PTD2` unparsed, single-tile-only pathing) as bugs — see `docs/engine/npc-spawn-ai-packages.md` for the authoritative v0-scope list per procedure. **Two former entries on that list have since shipped and were removed from it by #3351** — NAVM pathing (2026-08-23, single-tile; Phase 2 cross-tile is *blocked*, not unscheduled) and package re-evaluation (`ambient_ai_package_system`, once per in-game minute per actor, M42.9 / #2652). Do not suppress a finding about either on the grounds that it is "documented v0 scope".
- **CTDA fail-open is intentional, not a bug (M42.2)**: `package_conditions_pass` treats a package's whole condition list as passing if ANY referenced function is outside the ~15-function M47.1 catalog (`ConditionFunction::Unknown`) — this preserves M42.1 behavior (every scheduled package eligible) rather than silently dropping packages the evaluator can't reason about. Only lists whose every function is implemented are gated for real. Applies to all seven procedures' selection, not just Sandbox's. Verify a regression doesn't flip this to fail-closed.
- **Schedule gating, all seven procedures**: `active_package` (`crates/plugin/src/esm/records/misc/pack.rs`) picks the first package scheduled-active at `GameTimeRes.hour` whose CTDA conditions pass — verify an NPC with a non-matching package active at the current hour (e.g. an `AtBar` schedule) does NOT get that procedure's Behavior marker for that hour. Since an NPC's active package is always a single winning `PackRecord`, `apply_ai_package_behavior` resolves it ONCE and matches its procedure type through an `is_sandbox()`/`is_wander()`/… `else if` chain (#2031 collapsed the former 14 `active_package_is_*`/`active_*_location` calls into that single resolve) — mutual exclusion is by construction, so verify the chain still inserts at most one Behavior component per actor.
- **PLDT search radius/destination, six of seven**: Sandbox/Wander/Travel/Escort/Guard/Patrol all read a PLDT radius (Follow instead reads PTDT's `count_or_distance` as a stand-off distance) — verify radius-0 / no-PLDT packages fall back to each system's own default rather than a degenerate radius-0 search.
- **NearReference resolution differs by procedure — do not flag the difference as inconsistency**: Sandbox's search *center* is deliberately never resolved (investigated 2026-07-14 against real FalloutNV.esm: 1822 NearReference Sandbox packages, ~12% theoretically resolvable, not worth it). Travel/Escort/Guard DO attempt `NearReference` FormID resolution via `resolve_entity_by_global_form_id` on their own first tick (a materially different, later vantage point than Sandbox's spawn-time investigation — the whole cell has finished loading by then). Guard's *fallback* on a resolution miss is deliberately the actor's own position (not Travel's hash-picked point — reusing Travel's fallback was tried and reverted, since it trivially satisfies Guard's own leash check on tick one and the actor never walks anywhere). Follow has no fallback at all — an unresolved PTDT target means the actor never moves, by design (a Follow package with nothing to follow has nothing meaningful to do).
- **Seat reservation correctness (`0a21d5f9`)**: seats are keyed `(furniture entity, marker index)`, not just `furniture entity` — verify a multi-marker furniture (bench, long table) seats one actor per marker independently rather than treating the whole furniture as one seat. Sandbox-specific; the other six procedures don't touch furniture.
- **Legacy marker over-match (known v0 limitation, Sandbox-specific)**: FNV/FO3/Oblivion `BSFurnitureMarker`s carry no `AnimationType`, so the translate-boundary discriminant (`furniture_component`, `byroredux/src/cell_loader/references/attach.rs`, #2010) defaults every legacy marker to `FurnitureMarkerKind::Sit` and `is_sit_marker` (which now just reads that resolved `kind`) treats them all as sit-eligible — sleep/lean markers on FNV furniture will be over-matched as sit targets. Confirm this is still documented as a known gap, not silently "fixed" by a heuristic that could misfire.
- **Live-tracking vs. frozen destination (Follow/Escort vs. Travel/Guard)**: `follow_system` and `escort_system`'s collect phase re-read the target's `GlobalTransform` fresh every tick; `travel_system`, `escort_system`'s lead phase, and `guard_system` resolve/pick a position exactly once and freeze it. A `NearReference` target that moves after Travel/Guard resolution is NOT re-tracked — that's Follow's job, and conflating the two is a finding.
- **Patrol shares Wander's algorithm on purpose, not by accident**: no patrol-route/waypoint data is decoded anywhere in this codebase (Bethesda's real routes come from linked patrol-idle markers, outside `PACK`'s own sub-records) — v0 Patrol calls `wander_system`'s shared `step_oscillating_wander` core directly. Do not flag "Patrol is identical to Wander" as a bug; do flag if `patrol_system` silently diverges from `wander_system`'s core without an equivalent update on both sides, or if it duplicates the state machine instead of calling the shared function.
- **All seven are opt-in, none in the default scheduler**: `BYRO_SANDBOX_SIT`/`BYRO_WANDER`/`BYRO_TRAVEL`/`BYRO_FOLLOW`/`BYRO_ESCORT`/`BYRO_GUARD`/`BYRO_PATROL` (`boot.rs`) — a regression that registers any of these seven systems unconditionally silently changes FNV NPC behavior for every cell load, not just test scenarios.
**Output**: `/tmp/audit/fnv/dim_9.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fnv/dim_*.md`.
2. Combine into `docs/audits/AUDIT_FNV_<TODAY>.md`:
   - **Executive Summary** — FNV is the baseline; any regression against the ROADMAP-recorded numbers is at least HIGH (CRITICAL if it breaks a shipped foundation).
   - **Dimension Findings** — grouped by severity per dimension.
   - **Baseline Comparison Table** — ROADMAP number vs observed for entity count, draw count, FPS, fence, parse rate, record count (cite the ROADMAP commit you compared against).
   - **Regression Guard List** — previously-fixed issues this audit verified still correct.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FNV_<TODAY>.md`
(label every finding `game:fnv` + `legacy-compat`, plus its own domain label.)
