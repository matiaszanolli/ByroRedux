# ByroRedux — Development Roadmap

A clean Rust + C++ rebuild of the Gamebryo/Creation engine lineage with Vulkan rendering.
This document tracks completed milestones, current capabilities, planned work, and known gaps.

Last updated: 2026-04-02

---

## What Works Today

| Command | Description |
|---------|-------------|
| `cargo run -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior --bsa Meshes.bsa --textures-bsa Textures.bsa --textures-bsa Textures2.bsa` | Load and render a full FNV interior cell with real DDS textures |
| `cargo run -- --bsa "Skyrim - Meshes0.bsa" --mesh meshes\clutter\ingredients\sweetroll01.nif --textures-bsa "Skyrim - Textures3.bsa"` | Load and render a Skyrim SE mesh with textures from BSA v105 |
| `cargo run -- path/to/mesh.nif` | Load and render a loose NIF file |
| `cargo run -- --cmd help` | Run a console command at startup |
| `cargo run -- --swf path/to/menu.swf` | Load and render a Skyrim SE SWF menu overlay |
| `cargo run -- path/to/mesh.nif --kf path/to/anim.kf` | Play a .kf animation on a loaded NIF mesh |
| `cargo test` | 269 passing tests across all crates |

**Fallout New Vegas:** Interior cells load from ESM with placed objects (REFR → STAT), real DDS textures
from BSA v104 archives, correct coordinate transforms (Gamebryo CW rotation convention),
directional lighting, alpha blending, fly camera (WASD + mouse),
and per-frame debug stats. 781 entities at 334 FPS on RTX 4070 Ti.

**Skyrim SE:** Individual meshes load from BSA v105 (LZ4 decompression), BSTriShape geometry
with packed vertex data, BSLightingShaderProperty/BSEffectShaderProperty shaders,
DDS textures. Sweetroll renders at 1615 FPS.

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
| M9 | NIF Parser | Header parsing, 25+ block types, NifVariant enum (8 games), nif.xml reference, version-aware parsing | 76 |
| M10 | NIF-to-ECS Import | Scene graph flattening, Z-up→Y-up conversion, geometry/material/normal extraction, strip-to-triangle | — |
| M11 | Real Asset Loading | BSA v104/v105 reader (list, extract, zlib + LZ4), CLI (loose files + BSA + textures-bsa) | 2 |

**NIF block types supported:** NiNode, BSFadeNode, BSLeafAnimNode, BSTreeNode, NiTriShape,
NiTriStrips, NiTriShapeData, NiTriStripsData, BSTriShape,
BSShaderPPLightingProperty, BSShaderNoLightingProperty, BSLightingShaderProperty,
BSEffectShaderProperty, BSShaderTextureSet,
NiMaterialProperty, NiAlphaProperty, NiTexturingProperty, NiSourceTexture, NiExtraData,
NiControllerSequence, NiControllerManager, NiMultiTargetTransformController,
NiMaterialColorController, NiTransformController, NiVisController, NiTextureTransformController,
NiTimeController (base/fallback),
NiTransformInterpolator, BSRotAccumTransfInterpolator, NiTransformData, NiKeyframeData,
NiFloatInterpolator, NiFloatData, NiPoint3Interpolator, NiPosData,
NiBoolInterpolator, NiBoolData, NiTextKeyExtraData

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

### Phase 7 — Geometry & Multi-Game (M17–M18)

| # | Milestone | Scope | Tests |
|---|-----------|-------|-------|
| M17 | Coordinate System Fix | Gamebryo CW rotation convention, SVD degenerate matrix repair, editor marker filtering, coordinate system docs | 8 |
| M18 | Skyrim SE NIF Support | BSTriShape parser, BSLightingShaderProperty, BSEffectShaderProperty, BSA v105 (LZ4), NiAVObject conditionals | — |

---

## Next Milestones

### M19: Full Cell Loading — DONE
**Status:** Complete
**Scope:** All renderable record types (STAT, MSTT, FURN, DOOR, ACTI, CONT, LIGH, ACHR/NPC_),
WRLD exterior cell parsing with grid loading, LightSource ECS component, refactored cell loader.
**Result:** FNV Prospector Saloon: 809 entities. WastelandNV exterior 3x3 grid: 720 entities.
14 worldspaces, 30096 exterior cells, 17129 base objects parsed from FalloutNV.esm.

### M20: Scaleform/SWF UI System (Ruffle Integration) — DONE
**Status:** Complete
**Scope:** Ruffle (Rust Flash player) integrated as a library for Bethesda Scaleform GFx menu
rendering. New `crates/ui/` crate wrapping Ruffle's Player with offscreen wgpu rendering and
RGBA pixel readback. CPU-bridge architecture: Ruffle wgpu → pixel buffer → Vulkan texture upload
→ fullscreen quad overlay with UI-specific pipeline (no depth, alpha blend, passthrough shaders).
**Result:** Skyrim SE SWF menus (fadermenu, loadingmenu, messagebox) load and render via
`--swf <path>` CLI. All are AS2/Flash v15, parsed and executed by Ruffle with zero GFx stubs needed.
Dynamic texture update pipeline with device-wait-idle sync. Clean shutdown.
**Future:** Scaleform GFx stubs (`_global.gfx`), Papyrus↔UI bridge, input routing, font loading.

### M21: Animation Playback — DONE
**Status:** Complete
**Scope:** Full keyframe animation pipeline: NiTransformInterpolator/NiTransformData/NiFloatInterpolator/
NiFloatData/NiPoint3Interpolator/NiPosData/NiBoolInterpolator/NiBoolData/NiTextKeyExtraData block
parsers. KeyGroup parsing with Linear/Quadratic/TBC/XyzRotation key types. AnimationClip import
from .kf files via `import_kf()`. Interpolation engine with linear lerp, SLERP (quaternion),
cubic Hermite (quadratic tangents), and Kochanek-Bartels (TBC) splines. AnimationClipRegistry
resource, AnimationPlayer ECS component, animation_system with per-frame time advance and
name-based entity targeting. Cycle types: Clamp, Loop, Reverse (ping-pong). Z-up to Y-up
coordinate conversion for keyframe data. StringPool-based Name components on imported meshes.
**Result:** `--kf <path>` CLI loads .kf animation and plays it on named mesh entities.
269 tests passing (25 new). 10 new NIF block types parsed.
**Future:** XYZ euler rotation keys (#1), scene graph hierarchy (#2), non-transform channels (#3),
animation blending (#4), BSA KF loading (#5), NiControllerManager (#6), text key events (#7),
root motion (#8), name collision fix (#9), name lookup caching (#10). Skeletal animation in M29.

### M22: RT-First Multi-Light System
**Status:** Planned
**Scope:** Vulkan ray tracing pipeline (VK_KHR_ray_tracing_pipeline), acceleration structures
(BLAS/TLAS), light ECS components (point/spot/directional/ambient), NIF light extraction,
shadow rays. Rasterized multi-light fallback for non-RT GPUs.
**Depends on:** M17 (correct geometry), M13 (directional lighting), M19 (LIGH placement)
**Acceptance:** RT-lit Prospector Saloon with multiple light types and shadows.

---

## Medium-Term Roadmap (M23–M29)

| # | Milestone | Scope |
|---|-----------|-------|
| M23 | Full ESM/ESP Parser | NPC_, WEAP, ARMO, LVLI, QUST, DIAL + all record types. Wire to DataStore + conflict resolution. |
| M24 | Vulkan Compute BLAS | Compute shader infrastructure for batch transforms, coordinate conversion, skinning. Replace nalgebra hot paths. |
| M25 | Oblivion Support | Older NIF version (v20.0.0.5), NiTexturingProperty materials, BSA v104 variant. |
| M26 | Fallout 4 / BA2 Support | BA2 archive format (General + DX10 variants, LZ4), NIF uv2=130 changes. |
| M27 | Parallel System Dispatch | Rayon-based parallel execution in Scheduler. Dependency graph from read/write declarations. |
| M28 | Physics Foundation | Collision shapes from NIF bhk* blocks, Rapier/custom physics, character controller. |
| M29 | Skeletal Animation | NiSkinInstance/NiSkinData parsing, bone transforms, GPU skinning via compute shaders. |

---

## Long-Term Vision (M29+)

| Area | Scope |
|------|-------|
| World Loading | WRLD records, exterior cell grids, LOD terrain, streaming, navmesh |
| AI | AI packages (30 procedures), patrol paths, combat behavior, Sandbox |
| Quests & Dialogue | Quest stages, conditions (~300 functions), dialogue trees, Story Manager |
| Save/Load | Serialize world state, change forms, cosave format |
| Audio | Sound descriptors, 3D spatial audio, music system |
| UI | Scaleform GFx stubs, Papyrus↔UI bridge, input routing, font loading, all 34 menus |
| Modding | Full plugin loading: discover, sort, merge, resolve conflicts |
| Scripting | Full ECS-native scripting: 136 event types, condition system, perk entry points |

---

## Known Issues and Gaps

### Geometry
- [x] ~~Degenerate NIF rotation matrices~~ → SVD decomposition (M17)
- [x] ~~Gamebryo CW rotation convention~~ → Euler angle sign fix (M17)
- [x] ~~Editor markers render~~ → filtered by name prefix (M17)
- [x] ~~Light ray effect meshes render~~ → FX mesh filtering (M17)
- [ ] 43 remaining NiTexturingProperty byte-count warnings
- [ ] Backface culling disabled pending winding verification (confirmed CCW from Skyrim data)

### Parser Gaps
- [ ] Legacy ESM/ESP parsers are stubs for Morrowind, Oblivion, Skyrim, FO4
- [x] ~~NIF parser warnings: 274~~ → 43 remaining (84% fixed)
- [ ] NIF material properties beyond diffuse not wired to renderer
- [x] ~~Animation controllers parsed but not executed~~ → full .kf playback pipeline (M21)
- [x] ~~Only BSA v104 supported~~ → v105 with LZ4 added (M18)
- [x] ~~Cell loader only handles STAT~~ → all renderable types (M19)

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
- [x] ~~No UI/menu system~~ → Ruffle SWF integration (M20)
- [ ] No navmesh or AI

---

## Game Compatibility

| Tier | Games | NIF | Archive | ESM | Cell Loading |
|------|-------|-----|---------|-----|-------------|
| 1 — Working | Fallout: New Vegas | Working (25+ blocks) | BSA v104 ✓ | 23 record types | Interior cells ✓ |
| 1 — Working | Fallout 3 | Untested (likely works) | BSA v104 (likely) | Likely works | Likely works |
| 2 — Working | Skyrim SE | BSTriShape + BSLightingShader ✓ | BSA v105 ✓ (LZ4) | Stub | Individual meshes ✓ |
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
| Coordinate system | Z-up→Y-up with CW angle negation | Documented in docs/engine/coordinate-system.md |

---

## Project Stats

| Metric | Value |
|--------|-------|
| Passing tests | 269 |
| Workspace crates | 9 |
| Completed milestones | 21 (M1–M21) |
| NIF block types | 37 |
| NifVariant games | 8 (Morrowind → Starfield) |
| Supported archive formats | BSA v104, BSA v105 |
| Primary language | Rust (2021 edition) |
| Renderer | Vulkan 1.3 via ash |
| Target platform | Linux-first (Wayland + X11) |
| Reference GPU | NVIDIA GeForce RTX 4070 Ti |
| Reference CPU | AMD Ryzen 9 7950X (16-core) |

---

## Crate Map

| Crate | Milestones | Tests |
|-------|------------|-------|
| `byroredux-core` | M3 (ECS), M5 (Form IDs), M21 (Animation) | 106 |
| `byroredux-renderer` | M1, M2, M4, M7, M8, M13, M14 | 13 |
| `byroredux-platform` | M1 (windowing) | — |
| `byroredux-plugin` | M5, M6 | 50 |
| `byroredux-nif` | M9, M10, M17, M18, M21 | 87 |
| `byroredux-bsa` | M11, M18 | 2 |
| `byroredux-scripting` | M12 | 8 |
| `byroredux-ui` | M20 (Ruffle/SWF) | — |
| `byroredux-cxx-bridge` | Cross-cutting | — |
| `byroredux` (binary) | M4, M11, M14, M15, M16, M17 | — |

---

## Reference Materials

| Resource | Location | Purpose |
|----------|----------|---------|
| nif.xml (niftools) | `docs/legacy/nif.xml` | Authoritative NIF format spec (8563 lines) |
| Gamebryo 2.3 source | External drive | Byte-exact serialization reference |
| FNV game data | Steam library | Primary test content |
| Skyrim SE game data | Steam library | Secondary test content |
| Creation Kit wiki | uesp.net | Record type documentation |
| Coordinate system docs | `docs/engine/coordinate-system.md` | Transform pipeline, CW convention, winding chain |
| Memory system | `.claude/projects/.../memory/` | 38 documented engine systems |
