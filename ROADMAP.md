# ByroRedux — Development Roadmap

A clean Rust + C++ rebuild of the Gamebryo/Creation engine lineage with Vulkan rendering.
This document tracks completed milestones, current capabilities, planned work, and known gaps.

Last updated: 2026-03-30

---

## What Works Today

| Command | Description |
|---------|-------------|
| `cargo run -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior --bsa Meshes.bsa --textures-bsa Textures.bsa --textures-bsa Textures2.bsa` | Load and render a full FNV interior cell with real DDS textures |
| `cargo run -- path/to/mesh.nif` | Load and render a loose NIF file |
| `cargo run -- --bsa path.bsa --mesh meshes\foo.nif --textures-bsa textures.bsa` | Extract from BSA and render with textures |
| `cargo run -- --cmd help` | Run a console command at startup |
| `cargo test` | 233 passing tests across all crates |

Fallout New Vegas interior cells load from ESM with placed objects (REFR → STAT), real DDS textures
from BSA archives, vertex normals, directional lighting, alpha blending, fly camera (WASD + mouse),
and per-frame debug stats (FPS, entity/mesh/texture counts). 818 entities at 283 FPS on RTX 4070 Ti.

**Known rendering issues:** Degenerate NIF rotation matrices cause wall offsets in room shell meshes.
Editor markers and light ray effects render when they shouldn't. See M17 for the fix plan.

---

## Completed Milestones

### Phase 1 — Graphics Foundation (M1–M4)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M1 | Graphics Pipeline | Full Vulkan init chain (13 steps), hardcoded triangle rendering | — |
| M2 | GPU Geometry | Vertex/index buffers via gpu-allocator, geometry from Rust data | — |
| M3 | ECS Foundation | World, Component (SparseSet + Packed storage), Query, Scheduler, Resources, string interning | 92 |
| M4 | ECS-Driven Rendering | Spinning cube, perspective camera, push constants, Transform/Camera/MeshHandle components | — |

### Phase 2 — Data Architecture (M5–M6)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M5 | Plugin System | Stable Form IDs (content-addressed), FormIdPool, plugin manifests (TOML), DataStore, DAG-based conflict resolution | 50 |
| M6 | Legacy Bridge | ESM/ESP/ESL/ESH Form ID conversion, LegacyLoadOrder, per-game parser stubs (Morrowind through Starfield) | — |

### Phase 3 — Visual Pipeline (M7–M8, M13)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M7 | Depth Buffer | D32_SFLOAT depth attachment, correct multi-object occlusion | — |
| M8 | Texturing | Staging buffer upload, descriptor sets, UV-mapped geometry, checkerboard test texture | — |
| M13 | Directional Lighting | Vertex normals (4-attribute vertex format), Blinn-Phong directional light in fragment shader | — |

### Phase 4 — Asset Pipeline (M9–M11)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M9 | NIF Parser | Header parsing, 20+ block types, NifVariant enum (8 games), nif.xml reference, version-aware parsing | 71 |
| M10 | NIF-to-ECS Import | Scene graph flattening, Z-up→Y-up conversion, geometry/material/normal extraction, strip-to-triangle | — |
| M11 | Real Asset Loading | BSA v104 reader (list, extract, zlib), CLI (loose files + BSA + textures-bsa) | 2 |

**NIF block types supported:** NiNode, BSFadeNode, NiTriShape, NiTriStrips, NiTriShapeData,
NiTriStripsData, BSShaderPPLightingProperty, BSShaderNoLightingProperty, BSShaderTextureSet,
NiMaterialProperty, NiAlphaProperty, NiTexturingProperty, NiSourceTexture, NiExtraData,
NiControllerSequence, NiControllerManager, NiMultiTargetTransformController,
NiMaterialColorController, NiTransformController, NiVisController, NiTextureTransformController,
NiTimeController (base/fallback)

### Phase 5 — Scripting Foundation (M12)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M12 | Scripting Foundation | ECS-native events (ActivateEvent, HitEvent, TimerExpired), timer system, event cleanup | 8 |

### Phase 6 — Texture & Cell Loading (M14–M16)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M14 | DDS Texture Loading | DDS parser (BC1/BC3/BC5 + DX10), TextureRegistry with per-mesh descriptor sets, BSA texture extraction | 13 |
| M15 | Debug Logging & Diagnostics | DebugStats resource, ConsoleCommand trait, built-in commands, `--debug`/`--cmd` CLI | 11 |
| M16 | ESM Parser & Cell Loading | ESM binary parser (23 record types), CELL/REFR/STAT loading, Prospector Saloon demo, fly camera, alpha blending | — |

---

## Current: Geometry Correctness (M17)

### M17: Robust Transform Pipeline & Scene Correctness
**Status:** In progress — root cause identified
**Priority:** CRITICAL — blocks all visual work

**Problem:** FNV room shell NIF files contain degenerate rotation matrices (rank-deficient, determinant=0).
`glam::Quat::from_mat3()` produces garbage for these. Walls render offset/misaligned. Additionally,
editor markers and light ray effects render when they shouldn't.

**Scope:**
1. **nalgebra integration** — Add `nalgebra` to workspace for robust SVD decomposition of NIF matrices.
   Extract nearest valid rotation from rank-deficient matrices. Replace `Quat::from_mat3()` in import
   and cell loader with SVD-based decomposition.
2. **Mesh filtering** — Skip editor markers (`EditorMarker`, `MarkerX`, `Marker_Audio`), idle markers,
   and audio markers during NIF import. Filter by node name prefix or NIF extra data flags.
3. **Effect mesh handling** — Identify and skip/defer volumetric light effects (`IndFXLightRays*.NIF`)
   that require additive blending (not yet supported).
4. **NiTexturingProperty final fixes** — Resolve remaining 43 byte-count warnings (1-byte off for most,
   3 meshes with texture transforms).
5. **Vulkan compute foundation** — Design the compute shader infrastructure for future batch transform
   processing. nalgebra is the CPU reference; Vulkan compute is the long-term target.

**Depends on:** M16 (demo scene to evaluate against)
**Acceptance:** Prospector Saloon renders with correct wall/floor/ceiling alignment, no editor markers,
no light ray artifacts. All NIF parser warnings resolved.

---

## Next Milestones

### M18: Skyrim SE NIF Support
**Status:** Planned — nif.xml feature flags ready
**Scope:** Wire `NifVariant` feature flags into NiAVObject (no properties list for Skyrim+),
NiGeometry (shader/alpha refs), NiNode (no effects for FO4+). Add BSLightingShaderProperty parser
and BSEffectShaderProperty parser. Extend BSA reader for v105 (24-byte folder records).
**Depends on:** M17 (correct transforms), M9 (NIF parser)
**Acceptance:** Parse and render Skyrim SE sweetroll and iron sword from BSA.

### M19: RT-First Multi-Light System
**Status:** Planned
**Scope:** Vulkan ray tracing pipeline (VK_KHR_ray_tracing_pipeline), acceleration structures
(BLAS/TLAS), light ECS components (point/spot/directional/ambient), NIF light extraction,
shadow rays. Rasterized multi-light fallback for non-RT GPUs.
**Depends on:** M17 (correct geometry), M13 (directional lighting)
**Acceptance:** RT-lit Prospector Saloon with multiple light types and shadows.

### M20: Animation Playback
**Status:** Planned — controller parsers ready
**Scope:** Parse .kf files (NiControllerSequence fields already parsed). NiTransformData keyframe
extraction (linear/bezier/TCB interpolation). AnimationPlayer system. Cycle types (clamp/loop/reverse).
**Depends on:** M9 (controller parsers), M17 (correct transforms)
**Acceptance:** FNV mesh plays idle animation from .kf file.

### M21: Full Cell Loading
**Status:** Planned
**Scope:** Load all renderable record types (MSTT, DOOR, FURN, CONT, ACTI with models, LIGH placement).
Lighting templates from CELL record. Exterior cell grids. Cell transition support.
**Depends on:** M16 (basic cell loading), M19 (lighting)
**Acceptance:** Multiple FNV interior cells load with all visible objects and proper lighting.

---

## Medium-Term Roadmap (M22–M28)

| # | Milestone | Scope |
|---|-----------|-------|
| M22 | Full ESM/ESP Parser | NPC_, WEAP, ARMO, LVLI, QUST, DIAL + all record types. Wire to DataStore + conflict resolution. |
| M23 | Vulkan Compute BLAS | Compute shader infrastructure for batch transforms, coordinate conversion, skinning. Replace nalgebra hot paths. |
| M24 | Oblivion Support | Older NIF version (v20.0.0.5), NiTexturingProperty materials, BSA v104 variant. |
| M25 | Fallout 4 / BA2 Support | BA2 archive format (General + DX10 variants, LZ4), NIF uv2=130 changes. |
| M26 | Parallel System Dispatch | Rayon-based parallel execution in Scheduler. Dependency graph from read/write declarations. |
| M27 | Physics Foundation | Collision shapes from NIF bhk* blocks, Rapier/custom physics, character controller. |
| M28 | Skeletal Animation | NiSkinInstance/NiSkinData parsing, bone transforms, GPU skinning via compute shaders. |

---

## Long-Term Vision (M29+)

| Area | Scope |
|------|-------|
| World Loading | WRLD records, exterior cell grids, LOD terrain, streaming, navmesh |
| AI | AI packages (30 procedures), patrol paths, combat behavior, Sandbox |
| Quests & Dialogue | Quest stages, conditions (~300 functions), dialogue trees, Story Manager |
| Save/Load | Serialize world state, change forms, cosave format |
| Audio | Sound descriptors, 3D spatial audio, music system |
| UI | Menu system, HUD, mod configuration (Ruffle for SWF compat or custom) |
| Modding | Full plugin loading: discover, sort, merge, resolve conflicts |
| Scripting | Full ECS-native scripting: 136 event types, condition system, perk entry points |

---

## Known Issues and Gaps

### Geometry (blocking — M17)
- [ ] Degenerate NIF rotation matrices → SVD decomposition needed (nalgebra)
- [ ] Editor markers render (EditorMarker, MarkerX, Marker_Audio)
- [ ] Light ray effect meshes render as opaque white (IndFXLightRays)
- [ ] 43 remaining NiTexturingProperty byte-count warnings
- [ ] Back-face culling incorrect for some room shell meshes

### Parser Gaps
- [ ] Legacy ESM/ESP parsers are stubs for Morrowind, Oblivion, Skyrim, FO4
- [x] ~~NIF parser warnings: 274~~ → 43 remaining (84% fixed)
- [ ] NIF material properties beyond diffuse not wired to renderer
- [ ] Animation controllers parsed but not executed (.kf files)
- [ ] Only BSA v104 supported (not v105 Skyrim SE, not BA2 FO4)
- [ ] Cell loader only handles STAT record type (not MSTT, DOOR, FURN, etc.)

### Renderer Gaps
- [ ] No shadow maps or ray tracing
- [ ] No multi-light system (single hardcoded directional light)
- [ ] No transparency sorting for alpha-blended meshes
- [ ] No skinned mesh rendering (skeletal animation)
- [ ] No LOD system or frustum culling
- [ ] No Vulkan compute pipeline (planned for M23)

### Engine Gaps
- [x] ~~No structured diagnostics or debug console~~ (M15)
- [ ] Scheduler is single-threaded
- [ ] No physics or collision
- [ ] No save/load system
- [ ] No audio subsystem
- [ ] No UI/menu system
- [ ] No navmesh or AI

---

## Game Compatibility

| Tier | Games | NIF | Archive | ESM | Cell Loading |
|------|-------|-----|---------|-----|-------------|
| 1 — Working | Fallout: New Vegas | Working (20+ blocks) | BSA v104 ✓ | 23 record types | Interior cells ✓ |
| 1 — Working | Fallout 3 | Untested (likely works) | BSA v104 (likely) | Likely works | Likely works |
| 2 — Next | Skyrim SE | Feature flags ready | v105 extraction works | Stub | — |
| 3 — Planned | Oblivion | Variant defined | v104 variant | Stub | — |
| 4 — Future | Fallout 4 | Variant defined | BA2 needed | Stub | — |
| 5 — Future | Fallout 76 | Variant defined | BA2 needed | — | — |
| 6 — Future | Starfield | Variant defined | BA2 needed | — | — |

**NifVariant enum covers all 8 game variants** with semantic feature flags (has_properties_list,
has_shader_alpha_refs, has_material_crc, has_effects_list, uses_bs_lighting_shader, uses_bs_tri_shape).

---

## Architecture Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| GPU BLAS | Vulkan compute (target), nalgebra (bridge) | Portable, no proprietary deps, reuses existing Vulkan infra |
| Rendering | RT-first with rasterized fallback | RTX 4070 Ti available, future-proof |
| Format parsing | GameVariant trait abstraction | Per-game impls, not scattered version checks |
| Scripting | ECS-native (no VM) | Eliminates Papyrus queue latency, stack serialization, orphaned stacks |
| Plugin identity | Content-addressed Form IDs | Eliminates load order dependency, slot limits |
| Legacy compat | Parse data, don't emulate engine | Better results, clean room, no copyright issues |

---

## Project Stats

| Metric | Value |
|--------|-------|
| Passing tests | 233 |
| Workspace crates | 8 |
| Completed milestones | 16 (M1–M16) |
| NIF block types | 22 |
| NifVariant games | 8 (Morrowind → Starfield) |
| Supported archive formats | BSA v104 |
| Primary language | Rust (2021 edition) |
| Renderer | Vulkan 1.3 via ash |
| Target platform | Linux-first (Wayland + X11) |
| Reference GPU | NVIDIA GeForce RTX 4070 Ti |
| Reference CPU | AMD Ryzen 9 7950X (16-core) |

---

## Crate Map

| Crate | Milestones | Tests |
|-------|------------|-------|
| `byroredux-core` | M3 (ECS), M5 (Form IDs) | 92 |
| `byroredux-renderer` | M1, M2, M4, M7, M8, M13, M14 | 13 |
| `byroredux-platform` | M1 (windowing) | — |
| `byroredux-plugin` | M5, M6 | 50 |
| `byroredux-nif` | M9, M10 | 71 |
| `byroredux-bsa` | M11 | 2 |
| `byroredux-scripting` | M12 | 8 |
| `byroredux-cxx-bridge` | Cross-cutting | — |
| `byroredux` (binary) | M4, M11, M14, M15, M16 | — |

---

## Reference Materials

| Resource | Location | Purpose |
|----------|----------|---------|
| nif.xml (niftools) | `docs/legacy/nif.xml` | Authoritative NIF format spec (8563 lines) |
| Gamebryo 2.3 source | External drive | Byte-exact serialization reference |
| FNV game data | Steam library | Primary test content |
| Skyrim SE game data | Steam library | Secondary test content |
| Creation Kit wiki | uesp.net | Record type documentation |
| Memory system | `.claude/projects/.../memory/` | 27 documented engine systems |
