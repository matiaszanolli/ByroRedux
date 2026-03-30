# ByroRedux — Development Roadmap

A clean Rust + C++ rebuild of the Gamebryo/Creation engine lineage with Vulkan rendering.
This document tracks completed milestones, current capabilities, planned work, and known gaps.

Last updated: 2026-03-29

---

## What Works Today

| Command | Description |
|---------|-------------|
| `cargo run` | Spinning cube with checkerboard texture, depth testing, directional lighting |
| `cargo run -- path/to/mesh.nif` | Load and render a loose NIF file (Fallout New Vegas) |
| `cargo run -- --bsa path.bsa --mesh meshes\foo.nif` | Extract mesh from BSA archive and render |
| `cargo run -- --bsa meshes.bsa --mesh meshes\foo.nif --textures-bsa textures.bsa` | Render with real DDS textures |
| `cargo test` | 182 passing tests across all crates |

Fallout New Vegas meshes load from loose files or BSA archives, parse through the NIF pipeline,
flatten into ECS entities, and render with vertex normals, directional lighting, and per-mesh
DDS textures (BC1/BC3 compressed, uploaded directly to Vulkan with mipmaps).

---

## Completed Milestones

### Phase 1 — Graphics Foundation (M1–M4)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M1 | Graphics Pipeline | Full Vulkan init chain (13 steps), hardcoded triangle rendering | — |
| M2 | GPU Geometry | Vertex/index buffers via gpu-allocator, geometry from Rust data | — |
| M3 | ECS Foundation | World, Component (SparseSet + Packed storage), Query, Scheduler, Resources, string interning | 81 |
| M4 | ECS-Driven Rendering | Spinning cube, perspective camera, push constants, Transform/Camera/MeshHandle components | — |

### Phase 2 — Data Architecture (M5–M6)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M5 | Plugin System | Stable Form IDs (content-addressed), FormIdPool, plugin manifests (TOML), DataStore, DAG-based conflict resolution | 41 |
| M6 | Legacy Bridge | ESM/ESP/ESL/ESH Form ID conversion, LegacyLoadOrder, per-game parser stubs (Morrowind, Oblivion, Skyrim, FO4) | — |

### Phase 3 — Visual Pipeline (M7–M8, M13)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M7 | Depth Buffer | D32_SFLOAT depth attachment, correct multi-object occlusion | — |
| M8 | Texturing | Staging buffer upload, descriptor sets, UV-mapped geometry, checkerboard test texture | — |
| M13 | Directional Lighting | Vertex normals in vertex format, directional light in fragment shader | — |

### Phase 4 — Asset Pipeline (M9–M11)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M9 | NIF Parser | Header parsing, 15 block types, scene graph walking, version-aware parsing | 37 |
| M10 | NIF-to-ECS Import | Scene graph flattening, world-space transform composition, geometry/material extraction | — |
| M11 | Real Asset Loading | Fixed parser for real FNV meshes, BSA v104 reader (list, extract, zlib), CLI (loose files + BSA) | — |

**NIF block types supported:** NiNode, NiTriShape, NiTriStrips, NiTriShapeData, NiTriStripsData,
BSShaderPPLightingProperty, BSShaderTextureSet, NiMaterialProperty, NiAlphaProperty,
NiTexturingProperty, NiSourceTexture, NiTimeController, NiExtraData, BSFadeNode, BSLeafAnimNode, BSTreeNode

### Phase 5 — Scripting Foundation (M12)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M12 | Scripting Foundation | ECS-native events (ActivateEvent, HitEvent, TimerExpired), timer system, event cleanup | 8 |

---

### Phase 6 — Texture Pipeline (M14)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M14 | DDS Texture Loading | DDS parser (BC1/BC3/BC5 + uncompressed), TextureRegistry with per-mesh descriptor sets, BSA texture extraction, `--textures-bsa` CLI | 13 |

---

### Phase 7 — Debug Infrastructure (M15)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M15 | Debug Logging & Diagnostics | DebugStats resource (128-frame FPS window), ConsoleCommand trait + CommandRegistry, built-in commands (help/stats/entities/systems), `--debug` and `--cmd` CLI flags, structured log targets | 11 |

---

## Next Milestones

### M16: ESM Parser & Demo Scene
**Status:** Next
**Scope:** Minimal ESM/ESP binary parser (TES4, GRUP, CELL, REFR, STAT records). Load a Fallout New Vegas interior cell (Prospector Saloon) with all placed objects at correct positions, rotations, and scales. Foundation for full cell loading (M22).
**Depends on:** M9 (NIF parser), M11 (BSA reader), M14 (DDS textures)
**Acceptance:** `cargo run -- --esm FalloutNV.esm --cell GoodspringsSaloon --bsa ... --textures-bsa ...` renders a recognizable bar interior.

### M17: RT-First Multi-Light System
**Status:** Planned
**Scope:** Vulkan ray tracing pipeline (VK_KHR_ray_tracing_pipeline), acceleration structures (BLAS/TLAS), light ECS components, NIF light parsing, shadow rays. Rasterized multi-light fallback for non-RT GPUs.
**Depends on:** M16 (demo scene for evaluation), M13 (directional lighting)
**Acceptance:** RT-lit demo scene with multiple light types and shadows. Rasterized fallback without shadows.

### M18: Skyrim SE NIF Support
**Status:** Planned
**Scope:** BSLightingShaderProperty, BSEffectShaderProperty, BSFadeNode field differences for Skyrim SE NIF version (uv=12, uv2=83–100). Extend version-aware parsing.
**Depends on:** M9 (NIF parser)
**Acceptance:** Parse and render a Skyrim SE mesh (e.g., iron sword).

### M19: BSA v105 Support
**Status:** Planned
**Scope:** Extend BSA reader for v105 format (Skyrim SE). 24-byte folder records, LZ4 compression option.
**Depends on:** M11 (BSA v104 reader)
**Acceptance:** Extract and render meshes from Skyrim SE BSA archives.

### M20: Animation Playback
**Status:** Planned
**Scope:** Parse .kf files (NiControllerSequence, NiTransformInterpolator, NiTransformData). Keyframe interpolation systems. Animation component + AnimationPlayer system.
**Depends on:** M9 (NIF parser — controllers already parsed), M4 (Transform component)
**Acceptance:** FNV mesh plays idle animation from .kf file.

---

## Medium-Term Roadmap (M21–M26)

| # | Milestone | Scope |
|---|-----------|-------|
| M21 | Full ESM/ESP Parser | Extend M16's minimal parser: NPC_, WEAP, ARMO, FURN, DOOR + more record types. Wire to DataStore. |
| M22 | Full Cell Loading | Lighting templates, cell transitions, exterior cell grids, LOD stubs. |
| M23 | BA2 Archive Support | Fallout 4 archive format (General + DX10 variants, LZ4 compression). |
| M24 | Oblivion NIF Support | Older NIF version (v20.0.0.5), NiTexturingProperty-based materials, different block field layout. |
| M25 | Parallel System Dispatch | Rayon-based parallel execution in Scheduler. Dependency graph from system read/write declarations. |
| M26 | Shadow Maps (Rasterized) | Depth-only pass from light perspective, shadow sampling in fragment shader. For non-RT fallback. |

---

## Long-Term Vision (M27+)

| Area | Scope |
|------|-------|
| World Loading | WRLD records, exterior cell grids, LOD terrain, streaming |
| Physics | Collision detection, rigid bodies, character controller |
| AI | AI packages, patrol paths, combat behavior, navmesh pathfinding |
| Quests & Dialogue | Quest stages, conditions, dialogue trees, script fragments |
| Save/Load | Serialize world state, change forms, cosave format |
| Audio | Sound sources, 3D spatial audio, music system |
| UI | Menu system, HUD, mod configuration (replaces SWF/Scaleform) |
| Modding | Full plugin loading pipeline: discover, sort, merge, resolve conflicts |

---

## Known Issues and Gaps

### Parser Gaps
- [ ] Legacy ESM/ESP parsers are stubs (`todo!()` for Morrowind, Oblivion, Skyrim, FO4)
- [x] ~~NIF texture paths extracted but DDS textures not loaded~~ (M14: DDS loading done)
- [ ] NIF material properties beyond diffuse not wired to renderer (no normal maps, no PBR)
- [ ] Animation controllers parsed but not executed (.kf files not supported)
- [ ] Only BSA v104 supported (not v105 Skyrim SE, not BA2 Fallout 4)

### Renderer Gaps
- [ ] No shadow maps
- [ ] No multi-light system (single hardcoded directional light)
- [ ] No alpha blending / transparency sorting
- [ ] No skinned mesh rendering (skeletal animation)
- [ ] No LOD system or frustum culling

### Engine Gaps
- [x] ~~No structured diagnostics or debug console~~ (M15: DebugStats, ConsoleCommand, --debug/--cmd)
- [ ] Scheduler is single-threaded (parallel dispatch designed but not implemented)
- [ ] No physics or collision
- [ ] No save/load system
- [ ] No audio subsystem
- [ ] No UI/menu system
- [ ] No navmesh or AI

---

## Game Compatibility

| Tier | Games | Status | Notes |
|------|-------|--------|-------|
| 1 — Working | Fallout: New Vegas | Meshes load and render | BSA v104, NIF with NiTriStrips + BSShaderPPLighting |
| 2 — Next | Skyrim SE, Fallout 3 | Planned (M16, M17) | BSLightingShaderProperty, BSA v105. FO3 shares FNV's engine. |
| 3 — Medium | Oblivion | Planned (M22) | Older NIF version, NiTexturingProperty materials |
| 4 — Long-term | Fallout 4, Starfield | Research phase | BA2 archives, new NIF blocks, different record formats |

### Per-Game Parser Status

| Game | NIF | Archive | ESM/ESP | Cell Loading |
|------|-----|---------|---------|--------------|
| Fallout: New Vegas | Working | BSA v104 Working | Stub | — |
| Fallout 3 | Untested (likely works) | BSA v104 (likely works) | Stub | — |
| Skyrim SE | Not yet | Not yet (v105) | Stub | — |
| Oblivion | Not yet | Not yet (v104 variant) | Stub | — |
| Fallout 4 | Not yet | Not yet (BA2) | Stub | — |

---

## Project Stats

| Metric | Value |
|--------|-------|
| Passing tests | 215 |
| Workspace crates | 8 |
| Completed milestones | 15 |
| NIF block types | 15 |
| Supported archive formats | BSA v104 |
| Primary language | Rust (2021 edition) |
| Renderer | Vulkan via ash |
| Target platform | Linux-first |

---

## Crate Map

| Crate | Milestones |
|-------|------------|
| `byroredux-core` | M3 (ECS), M5 (Form IDs, plugin types) |
| `byroredux-renderer` | M1, M2, M4, M7, M8, M13, M14 |
| `byroredux-platform` | M1 (windowing) |
| `byroredux-plugin` | M5, M6 |
| `byroredux-nif` | M9, M10 |
| `byroredux-bsa` | M11 |
| `byroredux-scripting` | M12 |
| `byroredux-cxx-bridge` | Cross-cutting (C++ interop) |
| `byroredux` (binary) | M4, M11 (CLI integration) |
