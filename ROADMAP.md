# ByroRedux — Development Roadmap

A clean Rust + C++ rebuild of the Gamebryo/Creation engine lineage with Vulkan rendering.
This document tracks completed milestones, current capabilities, planned work, and known gaps.

Last updated: 2026-04-09 (session 7 — Starfield BA2 v3 DX10 LZ4 texture support, FO4 BA2 verification)

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
| `cargo run -- --bsa Meshes.bsa --mesh meshes\foo.nif --kf meshes\anim.kf` | Load KF from BSA (extracts automatically) |
| `cargo test` | 475 passing tests across all crates |

**Fallout New Vegas:** Interior cells load from ESM with placed objects (REFR → STAT), real DDS textures
from BSA v104 archives, correct coordinate transforms (Gamebryo CW rotation convention),
RT multi-light with ray query shadows, cell XCLL interior lighting (ambient + directional),
alpha blending with NIF decal detection, fly camera (WASD + mouse),
and per-frame debug stats. 789 entities at 85 FPS (RT) on RTX 4070 Ti.

**Fallout 3:** Interior cells load with zero NIF parse failures. Megaton Player House: 1609 entities,
199 textures at 42 FPS. Same BSA v104 + ESM pipeline as FNV.

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

**NIF block types supported (186 type names → 156 parsed + 30 Havok skip):**
Nodes: NiNode, BSFadeNode, BSLeafAnimNode, BSTreeNode, BSMultiBoundNode, RootCollisionNode,
BSOrderedNode, BSValueNode.
Geometry: NiTriShape, NiTriStrips, BSSegmentedTriShape, BSTriShape, BSMeshLODTriShape, BSSubIndexTriShape.
Geometry Data: NiTriShapeData, NiTriStripsData.
Shaders: BSShaderPPLightingProperty (with refraction/parallax), BSShaderNoLightingProperty,
BSLightingShaderProperty (8 shader-type variants), BSEffectShaderProperty, BSShaderTextureSet.
Properties: NiMaterialProperty, NiAlphaProperty, NiTexturingProperty (with bump map/parallax fields),
NiStencilProperty (version-aware), NiZBufferProperty, NiVertexColorProperty,
NiSpecularProperty, NiWireframeProperty, NiDitherProperty, NiShadeProperty.
Textures: NiSourceTexture, NiPixelData, NiPersistentSrcTextureRendererData.
Extra Data: NiStringExtraData, NiBinaryExtraData, NiIntegerExtraData, BSXFlags, NiBooleanExtraData,
BSBound, BSDecalPlacementVectorExtraData, BSBehaviorGraphExtraData, BSInvMarker,
BSClothExtraData, BSConnectPoint::Parents, BSConnectPoint::Children.
Controllers: NiTimeController, NiSingleInterpController, NiMaterialColorController,
NiMultiTargetTransformController, NiControllerManager, NiControllerSequence,
NiTextureTransformController, NiTransformController, NiVisController, NiAlphaController,
BSEffectShaderProperty{Float,Color}Controller, BSLightingShaderProperty{Float,Color}Controller,
NiGeomMorpherController, NiMorphData.
Interpolators: NiTransformInterpolator, BSRotAccumTransfInterpolator, NiTransformData/NiKeyframeData,
NiFloatInterpolator, NiFloatData, NiPoint3Interpolator, NiPosData,
NiBoolInterpolator, NiBoolData, NiTextKeyExtraData,
NiBlendTransformInterpolator, NiBlendFloatInterpolator, NiBlendPoint3Interpolator, NiBlendBoolInterpolator.
Skinning: NiSkinInstance, NiSkinData, NiSkinPartition, BsDismemberSkinInstance, BSSkin::Instance, BSSkin::BoneData.
Palette: NiDefaultAVObjectPalette, NiStringPalette.
Spatial: BSMultiBound, BSMultiBoundAABB, BSMultiBoundOBB.
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

### N23.3: Oblivion Support — DONE (block types)
**Status:** Block types complete, Oblivion cell loading deferred to BSA v103 decompression fix
**Scope:** NIF v20.0.0.5 (no block sizes, inline strings). +15 block types all landed:
NiStencilProperty, NiVertexColorProperty, NiZBufferProperty, NiGeomMorpherController,
NiMorphData, NiSkinInstance, NiSkinData, NiSkinPartition, NiSpecularProperty,
NiWireframeProperty, NiDitherProperty, NiShadeProperty, NiPixelData, RootCollisionNode,
NiStringPalette. NiFlagProperty shared struct for 4 flag-only properties.
**Block count:** +15 (all done) | **Games:** Oblivion

### N23.4: Fallout 3/NV Validation — DONE
**Status:** Complete. FO3 Megaton Player House loads with zero parse failures (1609 entities).
FNV Prospector Saloon loads with zero warnings. NiTexturingProperty decal slot off-by-one fixed.
**Scope:** +7 block types: BSMultiBound, BSMultiBoundAABB, BSMultiBoundOBB,
BSOrderedNode, BSValueNode, BSDecalPlacementVectorExtraData, BSBound.
Real-file validation: FO3 Megaton, FNV Prospector Saloon — zero parse failures.
**Block count:** +7 (total 119) | **Games:** FO3, FNV

### N23.5: Skinning and Dismemberment — DONE (parsers)
**Status:** All 6 skinning parsers landed. GPU skinning deferred to M29.
**Scope:** NiSkinInstance, NiSkinData (per-bone transforms + vertex weights),
NiSkinPartition, BsDismemberSkinInstance, BSSkin::Instance, BSSkin::BoneData.
Remaining for M29: HasSkinning trait, bone_weights/indices in ImportedMesh, GPU skinning.
**Block count:** 6 done | **Games:** All (characters)

### N23.6: Collision (Havok) — SKIP DONE, FULL PARSE DEFERRED
**Status:** 30 Havok types registered for clean block_size skip (no parse failures).
Full parsing deferred to M28 (physics).
**Scope:** 30 bhk/hk types skip cleanly via block_size on FO3+ (v20.2.0.7).
Oblivion NIFs (no block_size) need dedicated parsers — deferred.
HasCollision trait deferred to M28.
**Block count:** 30 registered (skip) | **Games:** FO3+ (Oblivion deferred)

### N23.7: Fallout 4 Support — DONE
**Status:** Complete. All FO4 block types parsed.
BSTriShape half-float vertices (VF_FULL_PRECISION bit), FO4 shader flags (u32 pair),
BSLightingShaderProperty FO4 trailing fields (subsurface, rimlight, backlight, fresnel,
wetness params), FO4 shader-type extras (SSR bools, skin tint alpha).
BSSubIndexTriShape, BSClothExtraData, BSConnectPoint::Parents/Children,
BSBehaviorGraphExtraData, BSInvMarker, BSSkin::Instance/BoneData.
BA2 archive reader deferred (separate milestone).
**Block count:** +8 (total ~119) | **Games:** Fallout 4

### N23.8: Particle Systems — DONE
**Status:** Complete. ~48 particle block types parsed.
NiParticles, NiParticleSystem, NiMeshParticleSystem, BSStripParticleSystem,
BSMasterParticleSystem. Data: NiParticlesData, NiPSysData, NiMeshPSysData,
BSStripPSysData, NiPSysEmitterCtlrData. 18 modifiers, 5 emitters, 2 colliders,
6 field modifiers, 21 controllers via shared base parsers.
**Block count:** +48 (total ~167) | **Games:** All (effects)

### N23.9: Fallout 76 and Starfield — DONE (shader blocks)
**Status:** Shader blocks complete. BSGeometrySegmentData deferred — current
block_size skip is correct; full parsing only needed when we surface segment
metadata to rendering (not yet).
**Scope:** BSLightingShaderProperty and BSEffectShaderProperty extended for
BSVER >= 132 (CRC32-hashed shader flag arrays replacing the u32 flag pair) and
BSVER >= 152 (SF2 array). BSVER == 155 (FO76) adds BSShaderType155 dispatch
with distinct skin/hair tint layouts, BSSPLuminanceParams, BSSPTranslucencyParams,
BSTextureArray lists, and refraction power (effect shader). WetnessParams
extended with Unknown 1 (BSVER > 130) and Unknown 2 (BSVER == 155). Stopcond
short-circuit: when BSVER >= 155 and Name is a non-empty BGSM/BGEM file path,
return a material-reference stub — the real material lives in the BGSM file
(out of scope for NIF parsing). BSEffectShaderProperty adds Reflectance,
Lighting, Emittance, and Emit Gradient textures for FO76.
**Result:** Both shader blocks now track correct stream positions through
BSVER 132–170+, preserving block size integrity on Starfield NIFs (where
material references via Name are the norm). 6 new unit tests exercise the
FO76 flag-array, trailing, skin-tint, and stopcond paths.
**Block count:** 0 new (extends 2 existing) | **Games:** FO76, Starfield

### N23.10: Test Infrastructure — DONE
**Status:** Complete
**Scope:** Per-game integration tests with env-based game data paths
(`BYROREDUX_{OBLIVION,FO3,FNV,SKYRIMSE}_DATA` with Steam-install fallbacks),
an `nif_stats` example binary for manual archive sweeps with block histogram
and error grouping, and graceful per-block parse recovery in the top-level
parser: when a block parse errors out but `block_size` is known, the stream
is advanced past the broken block, a `NiUnknown` placeholder is recorded,
and parsing continues. This turns single-block parser bugs from NIF-killing
errors into measurable telemetry.
**Result:** All four supported games comfortably exceed the 95% acceptance
threshold on full-archive sweeps:

| Game | NIFs parsed | Rate |
|------|-------------|------|
| Fallout New Vegas | 14881 / 14881 | 100.00% |
| Fallout 3         | 10989 / 10989 | 100.00% |
| Skyrim SE         | 18862 / 18862 | 100.00% |
| Oblivion          | 7963 / 8032   | 99.14%  |

The Oblivion residual is a handful of no-block-size NIFs where an early
parse error stops the walk (Oblivion NIFs can't use the per-block recovery
path because they have no size table). Integration tests live in
`crates/nif/tests/parse_real_nifs.rs` and are `#[ignore]`d so they don't
require game data on CI; run with `cargo test -p byroredux-nif --test
parse_real_nifs -- --ignored`.
**Block count:** 0 new (infrastructure + robustness) | **Games:** all

### N23 Summary

| # | Milestone | Blocks | Total | Status |
|---|-----------|--------|-------|--------|
| N23.1 | Trait hierarchy + FNV audit | 0 | ~49 | **DONE** |
| N23.2 | Shader completeness | 0 | ~49 | **DONE** |
| N23.3 | Oblivion block types | +15 | ~64 | **DONE** |
| N23.4 | FO3/FNV validation | +7 | ~71 | **DONE** |
| N23.5 | Skinning | +6 | ~77 | **DONE** |
| N23.6 | Collision (full parse) | +30 | ~107 | **DONE** (compressed mesh + shapes) |
| N23.7 | Fallout 4 | +12 | ~119 | **DONE** |
| N23.8 | Particles | +48 | ~167 | **DONE** |
| N23.9 | FO76/Starfield | 0 | ~167 | **DONE** (shader blocks) |
| N23.10 | Test infra | 0 | ~167 | **DONE** (95%+ all games) |

**Current registered type names: 186** (156 parsed + 30 Havok skip)

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

## M26: BA2 Archive Support — DONE

**Status:** Complete for FO4 / FO76 / Starfield meshes and FO4 textures.
Starfield DX10 textures (BA2 v3) deferred — chunk layout differs.

**Scope:**
- New `Ba2Archive` reader covering BTDX versions 1, 2, 3, 7, and 8 with the
  `GNRL` (general) and `DX10` (texture) variants. The version numbering is
  non-monotonic across games — v1 is the original FO4/FO76 layout, v2/v3
  are Starfield (with an 8-byte header extension), and v7/v8 are FO4 Next
  Gen patches that revert to the v1 24-byte header.
- DX10 texture extraction reconstructs a complete `.dds` byte stream
  (148-byte DDS+DX10 header + assembled mip chunks) since BA2 does not
  store the DDS header itself — pixel data is keyed only by `dxgi_format`,
  width, height, and mip count on the record.
- Side-fix: the NIF header parser's `BSStreamHeader` reading was wrong for
  FO4 and FO76 — it always read three short strings (author, process,
  export) regardless of `BS Version`. Per `nif.xml`, BSVER > 130 has an
  extra `Unknown Int u32` after Author and **drops** the Process Script,
  and BSVER ≥ 103 adds a `Max Filepath` short string. Without the fix the
  string-table cursor desyncs and every FO4/FO76 NIF fails to parse.
- `tests/common` grows a unified `MeshArchive` enum so integration tests
  do not branch on BSA vs BA2, plus FO4 / FO76 / Starfield entries with
  BA2 paths and the `BYROREDUX_FO4_DATA` / `BYROREDUX_FO76_DATA` /
  `BYROREDUX_STARFIELD_DATA` env-var overrides.

**Result:** Full-archive parse rates across **seven** Bethesda games:

| Game              | NIFs parsed       | Rate    |
|-------------------|-------------------|---------|
| Fallout New Vegas | 14881 / 14881     | 100.00% |
| Fallout 3         | 10989 / 10989     | 100.00% |
| Skyrim SE         | 18862 / 18862     | 100.00% |
| Oblivion          | 7963 / 8032       | 99.14%  |
| **Fallout 4**     | **34995 / 34995** | **100.00%** |
| **Fallout 76**    | **58469 / 58469** | **100.00%** |
| **Starfield**     | **31058 / 31058** | **100.00%** |

Combined: **177,217 NIFs** parse cleanly across the entire Bethesda lineage.

### M26 follow-up (Oblivion → 100%)

The post-M26 follow-up rooted out three more bugs in the NIF header parser
that were holding Oblivion below 100%:

- `user_version` was being read for any file ≥ 10.0.1.0, but per nif.xml
  it only exists from 10.0.1.8 onward. Older NetImmerse files (a chunk
  of Oblivion's content like `meshes/creatures/minotaur/horn*.nif`) had
  their `num_blocks` field shifted by 4 bytes and blew up downstream.
- `BSStreamHeader` was gated on `user_version >= 10`, but Oblivion's
  10.0.1.2 content (the original Bethesda Gamebryo era) has the
  metadata struct unconditionally and uses a completely different
  `BSVER` value (3, not 11). The condition is now `version == 10.0.1.2
  || user_version >= 3`, matching nif.xml's `#BSSTREAMHEADER#` macro.
- The remaining six failures were `meshes/marker_*.nif` debug placeholders
  in NIF v3.3.0.13 (pre-Gamebryo NetImmerse). These files inline each
  block's type name as a sized string instead of using a global block-type
  table, which we don't currently parse. They are filtered out by the
  M17 marker name filter at render time anyway, so we now return an
  empty `NifScene` with a debug log when the type table is empty —
  matching N23.10's "soft fail and keep going" philosophy.

| Game     | Before  | After      |
|----------|---------|------------|
| Oblivion | 99.13%  | **100.00%** |

All seven supported games now sit at 100% on the full mesh archive sweep.

**Resolved:** Starfield BA2 v3 DX10 textures now supported. The v3 header
has a 12-byte extension (vs. 8 for v2) containing a `compression_method`
field (0 = zlib, 3 = LZ4 block). The DX10 base record and chunk record
layouts are unchanged from FO4 v1; the original "different per-chunk
layout" diagnosis was incorrect — the real issue was the missing 4-byte
compression method field shifting the reader past the header, plus zlib
being used for LZ4-compressed chunks. Both GNRL and DX10 extraction now
dispatch through a unified `decompress_chunk()` that selects zlib or
LZ4 block based on the archive-level compression method.

---

## M24 Phase 1: Full ESM/ESP Record Parser — DONE

**Status:** Phase 1 complete. Item, container, leveled-list, actor, and small
records (GLOB/GMST) parse cleanly across the full FNV.esm. Quest / dialogue /
perk / magic-effect semantic structures stay deferred until the systems that
consume them come online.

**Scope:**
- New `crates/plugin/src/esm/records/` module organised by category:
  - `common.rs` — shared sub-record helpers (`read_zstring`, `find_sub`,
    `read_u32_at`, `CommonItemFields`)
  - `items.rs` — `ItemRecord` + `ItemKind` enum covering WEAP, ARMO, AMMO,
    MISC, KEYM, ALCH, INGR, BOOK, NOTE. Type-specific stats are in the enum
    variant; common name/model/value/weight live on the parent struct.
  - `container.rs` — `ContainerRecord` (CONT) and shared `LeveledList` for
    LVLI / LVLN with `InventoryEntry` / `LeveledEntry` rows.
  - `actor.rs` — `NpcRecord` (NPC_) plus supporting `RaceRecord` (RACE),
    `ClassRecord` (CLAS), `FactionRecord` (FACT) with `FactionRelation`
    cross-links.
  - `global.rs` — `GlobalRecord` (GLOB) and `GameSetting` (GMST) with a
    typed `SettingValue` enum (Int / Float / Short / String).
- New `EsmIndex` aggregator that combines the existing `EsmCellIndex` with
  per-category HashMaps. Top-level `parse_esm()` walks the GRUP tree once
  per category, dispatching by 4-char record type code, and reuses the
  existing `parse_esm_cells()` walker for the cell side. Existing callers
  that only need cells continue to use `parse_esm_cells` unchanged.
- Side-fix: `EsmCellIndex` now `derives(Default)` so the aggregator can
  start from `EsmIndex::default()`.

**Result on the real FalloutNV.esm (release build, single ~190 MB pass):**

| Category          | Count |
|-------------------|------:|
| Items (W/A/etc.)  | 2 643 |
| Containers        | 2 478 |
| Leveled items     | 2 738 |
| Leveled NPCs      |   365 |
| NPCs              | 3 816 |
| Races             |    22 |
| Classes           |    74 |
| Factions          |   682 |
| Globals           |   218 |
| Game settings     |   648 |

13,684 structured records on top of the existing cell + static extraction,
parsed in 0.19s release. 14 new unit tests in the records module plus an
`#[ignore]`d FNV.esm integration test that verifies record counts and
spot-checks Varmint Rifle / NCR faction.

**Deferred to Phase 2 (when the consuming systems land):**
- QUST / DIAL / INFO semantic parsing (quest stages, conditions, dialog trees)
- PERK entry points (~120 types from the Perk Entry Points memory)
- MGEF / SPEL / ENCH magic effects
- AVIF actor value definitions (currently referenced as raw form IDs)
- Dynamic weapon DNAM fields beyond the basic stats block

---

## M28 Phase 1: Physics Foundation — DONE

Rapier3D integration on top of the `CollisionShape` / `RigidBodyData`
components the NIF importer has been populating since N23.6. New
`byroredux-physics` crate keeps `rapier3d` / `nalgebra` confined so
`core` stays physics-agnostic and the loose-NIF viewer can opt out.

- `PhysicsWorld` resource owns the Rapier sets + pipeline + fixed
  60 Hz accumulator (max 5 substeps/frame)
- `physics_sync_system` runs four phases per tick: register newcomers,
  push kinematic transforms, step, pull dynamic transforms back
- `PlayerBody::HUMAN` marker on the camera entity spawns a dynamic
  capsule with rotations locked; `fly_camera_system` drives it via
  `set_linear_velocity` instead of mutating `Transform`
- Gravity is −686.7 BU/s² (−9.81 m/s² × 70 BU/m); the NIF importer
  already strips Havok's 7.0 scale factor so Rapier sees Bethesda
  units throughout
- 14 unit tests: glam↔nalgebra round-trips, shape mapping for every
  `CollisionShape` variant, dynamic ball falling under gravity, static
  floor blocking a dropped ball to rest, accumulator substep cap

Deferred to M28.5: kinematic character controller with step-up and
slope limiting. Deferred to M29: constraints and joints (ragdolls).
See [docs/engine/physics.md](docs/engine/physics.md) for full details.

## N26: Oblivion Coverage Sweep — DONE

Post-N23 audit (`nif.xml` vs. dispatch table) that closed 9
CRITICAL / HIGH severity parser gaps. Oblivion's v20.0.0.5 header has
no `block_sizes` fallback, so a single missing dispatch arm takes
down the entire mesh. Every fix ships with a `dispatch_tests`
regression test that asserts exact stream consumption on a minimal
Oblivion-shaped payload.

| # | Block types | Severity | Game impact |
|---|-------------|----------|-------------|
| #145 | 11 specialized `BSShader*Property` aliases | CRITICAL | Oblivion exteriors (sky / water / grass / distant LOD) |
| #144 | `NiKeyframeController` + `NiSequenceStreamHelper` | CRITICAL | All Oblivion NPC animation |
| #164 | `NiStringsExtraData` + `NiIntegersExtraData` | MEDIUM | Oblivion bone LOD / material overrides |
| #142 | `NiBillboardNode` + 12 NiNode subtypes | CRITICAL | Foliage, magic FX, LOD architecture, furniture markers |
| #156 | Full `NiLight` hierarchy + ECS wiring | HIGH | Torches / candles / magic light everywhere |
| #154 | `NiUVController` + `NiUVData` | HIGH | Oblivion water, fire, banners |
| #153 | Embedded `NiCamera` | HIGH | Cinematic / cutscene cameras |
| #163 | `NiTextureEffect` | MEDIUM | Oblivion magic FX projectors |
| #143 | Legacy particle stack (13 types) | CRITICAL | Every Oblivion magic FX / fire / dust / blood mesh |

Total: ~50 new block types parsed, 10 dispatch regression tests
landed, workspace test count rose from 372 → 396.

## Session 6 — N26 closeout + skinning end-to-end + Oblivion parser fix

A long bug-bash session that closed out 26 GitHub issues and tracked
down a long-standing Oblivion parser regression. The 35 commits split
into four buckets:

**Skeletal skinning, end-to-end (#178)**
- Part A (`923d11b`): new `SkinnedMesh` ECS component with
  `compute_palette()` pure function. Scene assembly resolves
  `ImportedSkin.bones[].name` → `EntityId` via a name map built
  during NIF node spawn. 8 unit tests cover the palette math.
- Part B (`4c97a36`): GPU side. Vertex format extended with
  `bone_indices: [u32; 4]` + `bone_weights: [f32; 4]` (44 → 76 B,
  6 attribute descriptions). New 4096-slot bone-palette SSBO on
  scene set 1 binding 3. Push constants 128 → 132 B (`uint
  bone_offset`). Single unified vertex shader — rigid vertices tag
  themselves with `sum(weights) ≈ 0` and route through `pc.model`,
  skinned vertices blend 4 palette entries via `bone_offset +
  inBoneIndices[i]`. `build_render_data` walks `(GlobalTransform,
  SkinnedMesh)` and stamps each draw with its bone offset.

**N26 dispatch closeout — every "block silently dropped" issue closed**
- `#157` BSDynamicTriShape + BSLODTriShape (Skyrim facegen + FO4 LOD)
- `#147` BSMeshLODTriShape + BSSubIndexTriShape (Skyrim DLC + FO4 actors)
- `#146` BSSegmentedTriShape (FO3/FNV/Skyrim LE biped body parts)
- `#148` BSMultiBoundNode (interior cell culling volumes)
- `#159` BSTreeNode (Skyrim SpeedTree wind-bone lists)
- `#158` BSPackedCombined[Shared]GeomDataExtra (FO4 distant LOD batches)
- `#150` `as_ni_node` walker helper that unwraps every NiNode subclass
  (BsOrderedNode, BsValueNode, BsRangeNode, NiBillboardNode, NiSwitchNode,
  NiSortAdjustNode, NiLODNode, BsMultiBoundNode, BsTreeNode) — every
  subclass with a `base: NiNode` field now descends correctly during
  scene-graph walks.
- `#160` `NiAVObject` properties list + `NiNode` effects list now use
  the raw `bsver()` instead of variant-based helpers, fixing
  non-Bethesda Gamebryo (`Unknown` variant) misalignment.
- `#175` `NifScene.truncated` field surfaces Oblivion's no-block-size
  early-bailout state to consumers.

**Critical Oblivion parser regression (`afab3e7`)**
- Wrote a new `crates/nif/examples/trace_block.rs` debug tool that
  dumps per-block start positions + 64-byte hex peeks. Used it to
  bisect the runtime `NiSourceTexture: failed to fill whole buffer`
  spam on Oblivion cell loads (consumed counts 8K–250K bytes per
  block on real files like `Quarto03.NIF`).
- Root cause: an earlier fix (#149) had added a `Has Shader Textures:
  bool` gate on `NiTexturingProperty`'s shader-map trailer based on
  `nif.xml`. The authoritative Gamebryo 2.3 source reads the count
  as a `uint` directly — no leading bool. The bool gate consumed
  the first byte of the u32 count, leaving the parser **3 bytes
  short** on every NiTexturingProperty. On Oblivion (no per-block
  size to recover) this misaligned the following NiSourceTexture's
  filename length field, which then read garbage as a u32 ≈ 33 M
  and bled through the rest of the file.
- Reverted the bool gate. All ~80 unique Oblivion clutter / book /
  furniture meshes that were previously truncating now parse to
  completion. Visual confirmation: Anvil Heinrich Oaken Halls
  interior renders fully populated (chandeliers, paintings,
  bookshelves, table settings).

**Quality + correctness fixes**
- `#137` lock_tracker RAII scope guards (no stale state on poison panics)
- `#136` 16× anisotropic filtering on the shared sampler
- `#134` frame-counter-based deferred texture destruction (was call-count)
- `#152` NiAlphaProperty alpha-test bit + threshold extracted (cutout
  foliage / hair / fences no longer mis-routed through alpha-blend)
- `#131` NiTexturingProperty `bump_texture` slot extracted as the
  Oblivion normal map (the dedicated `normal_texture` slot only landed
  in FO3, so Oblivion architecture was rendering completely
  flat-shaded before this)
- `#155` NiBSpline* compressed animation family (parse + De Boor
  evaluator at 30 Hz)
- `#151` + `#177` NIF skinning data extraction (NiSkinData sparse
  weights + BSTriShape VF_SKINNED packed vertex bones)
- `#79` binary KFM (KeyFrame Metadata) animation state-machine
  parser, Gamebryo 1.2.0.0 → 2.2.0.0
- `#108` BSConnectPoint::Children skinned flag is `byte`, not `uint`
- `#127` bhkRigidBody body_flags threshold 76 → 83 per nif.xml
- `#172` NIF string-table version threshold aligned to 20.1.0.1
- `#149` (now superseded by `afab3e7` revert)
- `#50` per-draw vertex/index buffer rebind dedup via mesh_handle
  sort key + `last_mesh_handle` cache
- `#36` `World::spawn` now panics on EntityId overflow instead of
  silent wrap
- Cell loader (`65d34dd`): each unique NIF parses + imports exactly
  once per cell load via a new `CachedNifImport` Arc cache. Cuts
  the parser warning volume from O(N placements) to O(M unique
  meshes) — typically 10-40× reduction on dense interior cells.
- `lock_tracker.rs`: silenced release-build dead-code warnings on
  the no-op stubs (`69c4f7a`).

Workspace test count: 396 → 472. Zero new warnings.

## Deferred Roadmap (post-N23)

| # | Milestone | Scope |
|---|-----------|-------|
| M22+ | RT Lighting Polish | Soft shadows, emissive bypass, lighting tuning (resumes after NIF correctness) |
| M24 | Full ESM/ESP Parser | **DONE (Phase 1)** — see below |
| M25 | Vulkan Compute | Batch transforms, coordinate conversion, GPU skinning |
| M26 | BA2 Archive Support | **DONE** — see below |
| M27 | Parallel System Dispatch | Rayon-based parallel ECS execution |
| M28 | Physics Foundation | **DONE (Phase 1)** — Rapier3D bridge, dynamic capsule player body; kinematic controller deferred to M28.5 |
| M29 | Skeletal Animation | GPU skinning via compute shaders (uses N23.5 skin data); ragdolls follow via Havok constraint parsing |

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
- [x] ~~BSLightingShaderProperty trailing fields per shader type~~ → 8 ShaderTypeData variants (N23.2)
- [x] ~~No skinning blocks~~ → 6 skinning parsers (NiSkinInstance/Data/Partition, BsDismemberSkinInstance, BSSkin::Instance/BoneData) (N23.5)
- [x] ~~No collision blocks~~ → 30 Havok types registered for block_size skip (N23.6, full parse → M28)
- [x] ~~No BA2 reader for FO4/FO76/Starfield~~ → BA2 v1/v2/v3/v7/v8, GNRL + DX10, zlib + LZ4 (M26)

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
| 1 — Working | Fallout: New Vegas | 89 parsed + 30 skip, RT shadows, XCLL | BSA v104 ✓ | 23 record types + XCLL | Interior + exterior ✓ |
| 1 — Working | Fallout 3 | Validated: Megaton 1609 entities, 0 parse failures | BSA v104 ✓ | Same as FNV ✓ | Interior ✓ |
| 2 — Partial | Skyrim SE | BSTriShape + BSLightingShader (8 variants) | BSA v105 ✓ (LZ4) | Stub | Individual meshes ✓ |
| 3 — Planned | Oblivion | All block types landed, needs BSA v103 decompression | BSA v103 (opens, decompression WIP) | Stub | — |
| 4 — Partial | Fallout 4 | 8 block types landed, half-float vertex WIP | BA2 v1/v7/v8 ✓ (GNRL + DX10, zlib) | Stub | — |
| 5 — Future | Fallout 76 | stopcond needed | BA2 v1 ✓ (GNRL + DX10, zlib) | — | — |
| 6 — Future | Starfield | No spec | BA2 v2/v3 ✓ (GNRL + DX10, zlib + LZ4) | — | — |

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
| Passing tests | 475 |
| Workspace crates | 11 |
| Completed milestones | 23 (M1–M22 + M24 Phase 1 + M26 + M28 Phase 1) + N23 + N26 + #178 skinning |
| NIF block types | ~215 distinct type names, ~185 parsed + 30 Havok skip |
| NifVariant games | 8 (Morrowind → Starfield) |
| Per-game NIF parse rate | 100% across 177,286 NIFs (7 games) |
| Supported archive formats | BSA v103 / v104 / v105, BA2 v1 / v2 / v3 / v7 / v8 |
| Primary language | Rust (2021 edition) |
| Renderer | Vulkan 1.3 via ash, RT extensions (VK_KHR_ray_query) |
| Physics | Rapier3D 0.22 (simd-stable), fixed 60 Hz substep |
| Target platform | Linux-first (Wayland + X11) |
| Reference GPU | NVIDIA GeForce RTX 4070 Ti |
| Reference CPU | AMD Ryzen 9 7950X (16-core) |

---

## Crate Map

| Crate | Milestones | Tests |
|-------|------------|-------|
| `byroredux-core` | M3 (ECS), M5 (Form IDs), M21 (Animation), #178A (SkinnedMesh), #137 (lock guards) | 162 |
| `byroredux-renderer` | M1, M2, M4, M7, M8, M13, M14, M22, #178B (bone palette), #136 (16× AF) | 25 |
| `byroredux-platform` | M1 (windowing) | — |
| `byroredux-plugin` | M5, M6, M19, M24 Phase 1 | 71 |
| `byroredux-nif` | M9, M10, M17, M18, M21, N23.1–N23.10, N26 audit, #79 KFM, session 6 closeout | 178 |
| `byroredux-bsa` | M11, M18, M26 (BA2), session 7 (v3 LZ4) | 11 |
| `byroredux-physics` | M28 Phase 1 (Rapier3D bridge) | 17 |
| `byroredux-scripting` | M12 | 8 |
| `byroredux-ui` | M20 (Ruffle/SWF) | — |
| `byroredux-cxx-bridge` | Cross-cutting | — |
| `byroredux` (binary) | M4, M11, M14, M15, M16, M17, M19, M28 integration, parse-once cell cache | — |

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
