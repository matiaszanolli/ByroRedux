# ByroRedux — Development Roadmap

A clean Rust + C++ rebuild of the Gamebryo/Creation engine lineage with Vulkan rendering.
This document tracks completed milestones, current capabilities, planned work, and known gaps.

Last updated: 2026-04-05 (session 2)

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
| `cargo test` | 312 passing tests across all crates |

**Fallout New Vegas:** Interior cells load from ESM with placed objects (REFR → STAT), real DDS textures
from BSA v104 archives, correct coordinate transforms (Gamebryo CW rotation convention),
RT multi-light with ray query shadows, cell XCLL interior lighting (ambient + directional),
alpha blending with NIF decal detection, fly camera (WASD + mouse),
and per-frame debug stats. 789 entities at 85 FPS (RT) on RTX 4070 Ti.

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

**NIF block types supported (94 type names → 37 parser structs + 30 Havok skip):**
Nodes: NiNode, BSFadeNode, BSLeafAnimNode, BSTreeNode, BSMultiBoundNode.
Geometry: NiTriShape, NiTriStrips, BSSegmentedTriShape, BSTriShape, BSMeshLODTriShape.
Geometry Data: NiTriShapeData, NiTriStripsData.
Shaders: BSShaderPPLightingProperty (with refraction/parallax), BSShaderNoLightingProperty,
BSLightingShaderProperty, BSEffectShaderProperty, BSShaderTextureSet.
Properties: NiMaterialProperty, NiAlphaProperty, NiTexturingProperty (with bump map/parallax fields),
NiStencilProperty (version-aware), NiZBufferProperty, NiVertexColorProperty.
Textures: NiSourceTexture.
Extra Data: NiStringExtraData, NiBinaryExtraData, NiIntegerExtraData, BSXFlags, NiBooleanExtraData.
Controllers: NiTimeController, NiSingleInterpController, NiMaterialColorController,
NiMultiTargetTransformController, NiControllerManager, NiControllerSequence,
NiTextureTransformController, NiTransformController, NiVisController, NiAlphaController,
BSEffectShaderProperty{Float,Color}Controller, BSLightingShaderProperty{Float,Color}Controller,
NiGeomMorpherController, NiMorphData.
Interpolators: NiTransformInterpolator, BSRotAccumTransfInterpolator, NiTransformData/NiKeyframeData,
NiFloatInterpolator, NiFloatData, NiPoint3Interpolator, NiPosData,
NiBoolInterpolator, NiBoolData, NiTextKeyExtraData,
NiBlendTransformInterpolator, NiBlendFloatInterpolator, NiBlendPoint3Interpolator, NiBlendBoolInterpolator.
Skinning: NiSkinInstance, NiSkinData, NiSkinPartition, BsDismemberSkinInstance.
Palette: NiDefaultAVObjectPalette.
Collision (skip via block_size): 30 Havok types (bhkCollisionObject, bhkRigidBody, bhkMoppBvTreeShape, etc.).

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

## NIF Parser Overhaul (N23 — Priority 0, Active)

The NIF binary format is the foundation of all visual content. Correct parsing across all
games (Oblivion through Starfield) must come before renderer features.

### N23.1: Trait Hierarchy and Base Class Extraction — DONE
**Status:** Complete
**Scope:** Refactored flat block structs into composable base class hierarchy:
NiObjectNETData, NiAVObjectData (with parse_no_properties() for BSTriShape),
BSShaderPropertyData. Consumer traits: HasObjectNET, HasAVObject, HasShaderRefs.
Also fixed via `/audit-nif --game fnv` (7 bugs): NiBoolInterpolator bool size,
KeyType::Constant, NiBooleanExtraData, BSShaderPPLightingProperty refraction/parallax,
NiTexturingProperty bump map fields (root cause of stream position warnings),
parallax offset, BSMultiBoundNode dispatch, version thresholds.
**Result:** 11 blocks migrated. Base class parsing deduplicated (11→1, 3→1, 4→1).
Net -211 lines. 95 NIF tests, 290 workspace tests. 11 audit commands.
Post-N23.1 additions: NiVertexColorProperty, NiStencilProperty, NiZBufferProperty,
NiGeomMorpherController, NiMorphData, NiBlend*Interpolator family, NiSkinInstance/Data/Partition,
BsDismemberSkinInstance, NiDefaultAVObjectPalette, 30 Havok collision types, Material ECS component.

### N23.2: BSLightingShaderProperty Completeness — DONE
**Status:** Complete
**Scope:** ShaderTypeData enum with 8 variants (EnvironmentMap, SkinTint, HairTint,
ParallaxOcc, MultiLayerParallax, SparkleSnow, EyeEnvmap, None). Skyrim LE/SE trailing
fields fully parsed. BSEffectShaderProperty: soft_falloff_depth, greyscale_texture,
lighting_influence, env_map_min_lod, FO4+ textures (env/normal/mask + scale).
**Block count:** 0 new (fixes 2 existing types) | **Games:** Skyrim LE/SE, FO4

### N23.3: Oblivion Support
**Status:** Partially done
**Scope:** NIF v20.0.0.5 (no block sizes, inline strings). Originally +15 block types.
Already landed: NiStencilProperty, NiVertexColorProperty, NiZBufferProperty,
NiGeomMorpherController, NiMorphData, NiSkinInstance, NiSkinData, NiSkinPartition.
Remaining: NiSpecularProperty, NiWireframeProperty, NiDitherProperty, NiShadeProperty,
NiPixelData, RootCollisionNode, NiStringPalette.
NiTexturingProperty → NiSourceTexture materials. Oblivion variant detection (#83).
**Block count:** +7 remaining (8 already done) | **Games:** Oblivion

### N23.4: Fallout 3/NV Validation — DONE (block types)
**Status:** Block types complete, real-file validation deferred to test assets
**Scope:** +7 block types: BSMultiBound, BSMultiBoundAABB, BSMultiBoundOBB,
BSOrderedNode, BSValueNode, BSDecalPlacementVectorExtraData, BSBound.
**Block count:** +7 (total 107) | **Games:** FO3, FNV

### N23.5: Skinning and Dismemberment — PARTIALLY DONE
**Status:** Parsers landed, GPU skinning deferred
**Scope:** NiSkinInstance, NiSkinData (per-bone transforms + vertex weights),
NiSkinPartition, BsDismemberSkinInstance — all fully parsed.
Remaining: BSSkin::Instance + BSSkin::BoneData (FO4+), HasSkinning trait,
bone_weights/indices in ImportedMesh, GPU skinning (M29).
**Block count:** 4 done, +2 remaining | **Games:** All (characters)

### N23.6: Collision (Havok) — SKIP DONE, FULL PARSE DEFERRED
**Status:** 30 Havok types registered for clean block_size skip (no parse failures).
Full parsing deferred to M28 (physics).
**Scope:** 30 bhk/hk types skip cleanly via block_size on FO3+ (v20.2.0.7).
Oblivion NIFs (no block_size) need dedicated parsers — deferred.
HasCollision trait deferred to M28.
**Block count:** 30 registered (skip) | **Games:** FO3+ (Oblivion deferred)

### N23.7: Fallout 4 Support
**Status:** Planned
**Scope:** BSTriShape half-float positions, BSSubIndexTriShape, BSClothExtraData,
BSConnectPoint, BSBehaviorGraphExtraData, BSInvMarker, .bgsm material paths.
**Block count:** +8 (total ~105) | **Games:** Fallout 4

### N23.8: Particle Systems
**Status:** Planned
**Scope:** NiParticleSystem, NiPSysData, ~15 modifier/emitter subclasses.
Parse-only — no rendering.
**Block count:** +18 (total ~123) | **Games:** All (effects)

### N23.9: Fallout 76 and Starfield
**Status:** Planned
**Scope:** FO76 shader stopcond, BSGeometry, BSMaterial, extended flags.
Starfield: BSGeometrySegmentData, material paths.
**Block count:** +7-10 (total ~130+) | **Games:** FO76, Starfield

### N23.10: Test Infrastructure
**Status:** Planned (parallel)
**Scope:** Per-game integration tests, `nif-stats` binary, configurable asset paths.
**Acceptance:** 95%+ parse success rate per game.

### N23 Summary

| # | Milestone | Blocks | Total | Status |
|---|-----------|--------|-------|--------|
| N23.1 | Trait hierarchy + FNV audit | 0 | ~49 | **DONE** |
| N23.2 | Shader completeness | 0 | ~49 | **DONE** |
| N23.3 | Oblivion | +7 remaining | ~71 | **Partial** (8/15 done) |
| N23.4 | FO3/FNV validation | +7 | ~78 | **DONE** (block types) |
| N23.5 | Skinning | +2 remaining | ~80 | **Partial** (4/6 done) |
| N23.6 | Collision (skip) | 30 skip | ~107 | **DONE** (skip; full parse → M28) |
| N23.7 | Fallout 4 | +8 | ~115 | Planned |
| N23.8 | Particles | +18 | ~133 | Planned |
| N23.9 | FO76/Starfield | +7 | ~140 | Planned |
| N23.10 | Test infra | 0 | ~140 | Planned |

**Current registered type names: 107** (77 parsed + 30 Havok skip)

---

## Completed Milestones (M1–M22)

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

### M22: RT-First Multi-Light System — DONE (polish deferred)
**Status:** Phase A+B landed. Polish deferred for NIF correctness priority.
**Scope:** SSBO multi-light rendering (Phase A), RT shadow rays via VK_KHR_ray_query (Phase B).
Cell interior XCLL lighting (ambient + directional), windowed inverse-square attenuation.
BLAS per mesh, TLAS rebuilt per frame, dynamic depth bias for NIF-flagged decals.
**Result:** Prospector Saloon: 25 point lights + directional + RT shadows at 85 FPS.
**Deferred:** Soft shadows, emissive mesh bypass, lighting tuning (resumes after N23).

---

## Deferred Roadmap (post-N23)

| # | Milestone | Scope |
|---|-----------|-------|
| M22+ | RT Lighting Polish | Soft shadows, emissive bypass, lighting tuning (resumes after NIF correctness) |
| M24 | Full ESM/ESP Parser | NPC_, WEAP, ARMO, LVLI, QUST, DIAL + all record types |
| M25 | Vulkan Compute | Batch transforms, coordinate conversion, GPU skinning |
| M26 | BA2 Archive Support | Fallout 4/76 BA2 format (General + DX10 variants, LZ4) |
| M27 | Parallel System Dispatch | Rayon-based parallel ECS execution |
| M28 | Physics Foundation | Rapier/custom physics, character controller (uses N23.6 collision data) |
| M29 | Skeletal Animation | GPU skinning via compute shaders (uses N23.5 skin data) |

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
- [x] ~~43 NiTexturingProperty byte-count warnings~~ → bump map fields + parallax offset fixed (N23.1)
- [x] ~~Backface culling disabled~~ → enabled with confirmed CW winding convention

### Parser Gaps
- [ ] Legacy ESM/ESP parsers are stubs for Morrowind, Oblivion, Skyrim, FO4
- [x] ~~NIF parser warnings: 274~~ → NiBoolInterpolator and KeyType::Constant fixed (N23.1)
- [ ] NIF material properties beyond diffuse not wired to renderer
- [x] ~~Animation controllers parsed but not executed~~ → full .kf playback pipeline (M21)
- [x] ~~Only BSA v104 supported~~ → v103/v104/v105 (M18, Oblivion BSA open)
- [x] ~~Cell loader only handles STAT~~ → all renderable types (M19)
- [ ] BSA v103 (Oblivion) decompression not yet working
- [ ] BSLightingShaderProperty trailing fields per shader type not parsed (N23.2)
- [x] ~~No skinning blocks~~ → NiSkinInstance/NiSkinData/NiSkinPartition/BsDismemberSkinInstance parsed (N23.5 partial)
- [x] ~~No collision blocks~~ → 30 Havok types registered for block_size skip (N23.6, full parse → M28)
- [ ] No BA2 reader for FO4/FO76/Starfield — N23.7+

### Renderer Gaps
- [x] ~~No shadow maps or ray tracing~~ → RT ray query shadows (M22)
- [x] ~~No multi-light system~~ → SSBO multi-light + cell XCLL lighting (M22)
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
| 1 — Working | Fallout: New Vegas | 77 parsed + 30 Havok skip, RT shadows, XCLL | BSA v104 ✓ | 23 record types + XCLL | Interior + exterior ✓ |
| 1 — Working | Fallout 3 | Untested (same as FNV) | BSA v104 ✓ | Likely works | Likely works |
| 2 — Partial | Skyrim SE | BSTriShape + BSLightingShader | BSA v105 ✓ (LZ4) | Stub | Individual meshes ✓ |
| 3 — Planned | Oblivion | Variant defined | BSA v103 (opens, decompression WIP) | Stub | — |
| 4 — Future | Fallout 4 | Shader flags WIP | BA2 (BTDX v1) needed | Stub | — |
| 5 — Future | Fallout 76 | stopcond needed | BA2 (BTDX v1) needed | — | — |
| 6 — Future | Starfield | No spec | BA2 (BTDX v2) needed | — | — |

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
| Passing tests | 312 |
| Workspace crates | 10 |
| Completed milestones | 22 (M1–M22 Phase A+B) |
| NIF block types | 107 (77 parsed + 30 Havok skip) |
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
| `byroredux-core` | M3 (ECS), M5 (Form IDs), M21 (Animation) | 111 |
| `byroredux-renderer` | M1, M2, M4, M7, M8, M13, M14 | 13 |
| `byroredux-platform` | M1 (windowing) | — |
| `byroredux-plugin` | M5, M6 | 50 |
| `byroredux-nif` | M9, M10, M17, M18, M21, N23.1 | 95 |
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
