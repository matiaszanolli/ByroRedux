# ByroRedux — Development Roadmap

A clean Rust + C++ rebuild of the Gamebryo/Creation engine lineage with Vulkan rendering.
This document tracks completed milestones, current capabilities, planned work, and known gaps.

Last updated: 2026-04-19 (session 12 — audit closeout bundle #306–#463, 37 follow-up fixes across NIF shader plumbing, Oblivion CREA/ACRE indexing, FO4-era ESM dispatch (SCPT/CREA/LVLC), CLMT TNAM weather hours, Skyrim XCLL directional cube + fresnel, shader slot 3/4/5 on PPLighting + parallax/env on BSShaderTextureSet, SPIR-V reflection vs descriptor layout cross-check, TLAS instance custom_index unified with SSBO)

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
| `cargo test` | 867 passing tests across all crates |

**Fallout New Vegas:** Interior cells load from ESM with placed objects (REFR → STAT), real DDS textures
from BSA v104 archives, correct coordinate transforms (Gamebryo CW rotation convention),
RT multi-light with ray query shadows, cell XCLL interior lighting (ambient + directional),
alpha blending with NIF decal detection, fly camera (WASD + mouse),
and per-frame debug stats. Prospector Saloon renders at ~48 FPS on
RTX 4070 Ti (RT enabled) — last measured pre-M31; a re-bench via
`--bench-frames` is tracked in #456 since M31+/M36/M37/M37.5 have
shifted the cost curve substantially. Exterior cells load 3×3 grids
from WastelandNV worldspace (placed objects only — no landscape yet).

**Fallout 3:** Interior cells load with zero NIF parse failures. Megaton Player House parses with
929 REFRs (validated 2026-04-19 via
`parse_real_fo3_megaton_cell_baseline`). After NIF expansion the
~1609 ECS entities / 199 textures claim from the N23.4 validation demo
covered the whole cell — that figure pre-dates M31/M36/M37/M37.5 and
needs a fresh GPU bench; track via #456. Same BSA v104 + ESM
pipeline as FNV.

**Skyrim SE:** Individual meshes load from BSA v105 (LZ4 decompression), BSTriShape geometry
with packed vertex data, BSLightingShaderProperty/BSEffectShaderProperty shaders,
DDS textures. Sweetroll single-mesh bench historically renders at
>1000 FPS on RTX 4070 Ti; exact figure tracked via `--bench-frames`
per #456 since it drifts with TAA / SVGF / BLAS cost.

**RT Performance (M31 → M36):** Batched BLAS builds (single GPU submission per cell load),
ALLOW_COMPACTION + query-based compact copy (M36: 20–50% BLAS memory reduction),
streaming weighted reservoir shadow sampling (M31.5: 8 independent reservoirs per fragment
proportional to luminance; every light has non-zero shadow probability), distance-based
shadow/GI ray fallback, TLAS frustum culling, BLAS LRU eviction with VRAM/3 budget,
deferred geometry SSBO rebuild (no device_wait_idle). Interior
frametime historical baseline ~48 FPS (Prospector Saloon, pre-M37.5);
current figure pending a re-bench per #456.
Exterior: loads and renders placed objects; landscape/sky/LOD pending (M32–M35).

**TAA (M37.5):** `taa.comp` compute pass — Halton(2,3) sub-pixel projection jitter in
the vertex shader, motion-vector reprojection with Catmull-Rom 9-tap history resample,
3×3 YCoCg neighborhood variance clamp (γ=1.25), mesh_id disocclusion detection,
luma-weighted α=0.1 blend. Per-FIF RGBA16F history images, ping-pong descriptor sets,
first-frame guard, resize hooks. Composite HDR binding rewired through TAA output.

**FO4 architecture:** Auto-detect BSA vs BA2 archives from file magic. ESM parser extended
with SCOL / MOVS / PKIN / TXST record types. `BSLightingShaderProperty.net.name` flows
through `ImportedMesh` → `Material` as `material_path` for BGSM / BGEM diagnostics.

**Debug CLI additions (session 10):** Console commands `tex.missing`, `tex.loaded`,
`mesh.info <entity_id>`. Evaluator functions `tex_missing()` / `tex_loaded()` exposed
over the TCP protocol. Output shows BGSM material reference when `texture_path` is
absent (correct FO4 behavior — the real material lives in the BGSM file).

**Session 11 closeout (72 commits, zero milestone churn):** audit follow-through
bundle `#341`–`#438`. Major themes:

- **NIF parser correctness** — Oblivion stream-drift detector (`#395`), runtime
  size cache after a bad block (`#324`), NiTexturingProperty normal/parallax
  slot version gates (`#429`), BSDynamicTriShape import (`#341`) restores every
  Skyrim NPC head/face mesh, BSTriShape reads BSEffectShaderProperty (`#346`),
  BSTreeNode bones + BSRangeNode discriminator surfaced on import (`#363`/`#364`),
  VMAD `has_script` flag via script attachments (`#369`), BSBehaviorGraphExtraData
  reads bool (`#106`), FO4 NiParticleSystem + BS*ShaderProperty controllers
  (`#407`), BSLightingShaderProperty wetness.unknown_1 for BSVER ≥ 130 (`#403`),
  FNV particle trailing fields (`#383`), file-driven `Vec::with_capacity` budget
  guard (`#388`).
- **NIF → import → render plumbing** — `material_kind` threaded end-to-end
  through the import/render path (`#344`), all 8 TXST texture slots extracted
  (`#357`, was TX00 only), `NiDynamicEffect.affected_nodes` surfaced on
  `ImportedLight` (`#335`), BGRA color byte order on LIGH DATA (`#389`),
  NIF import cache promoted to a process-lifetime resource (`#381`),
  end-to-end CPU particle system so torches/FX are visible (`#401`),
  BSEffectShaderProperty emissive → base_color field rename (`#166`),
  MapMarker + Skyrim+ EditorMarker flag bit in editor culling (`#165`),
  BsTriShape two_sided lookup checks BSEffectShaderProperty (`#128`).
- **ESM parser** — SCOL body parses all ONAM/DATA child placements (`#405`),
  CELL `XCLW` water plane height (`#397`), REFR `XESP` gating skips default-
  disabled refs at cell load (`#349`), Skyrim CELL extended sub-records (`#356`),
  variant-aware group end in the walker (`#391`), triple exterior ESM parse
  collapsed to single pass (`#374`), dead legacy ESM parser stubs deleted
  (`#390`).
- **Archive reader** — long-lived file handle across extracts (`#360`, was
  re-opening per call), BSA extract arithmetic underflow guard (`#352`).
- **Renderer — sync + resource management** — `VkPipelineCache` threaded
  through every pipeline create site (`#426`, 10–50 ms cold → <1 ms warm),
  per-(src, dst, two_sided) blend pipeline cache (`#392`), TLAS build barrier
  widened to cover COMPUTE_SHADER (`#415`), TRIANGLE_FACING_CULL_DISABLE on
  TLAS instances gated on two_sided (`#416`), gl_RayFlagsTerminateOnFirstHitEXT
  on reflection + glass rays (`#420`), SVGF history age as weighted average
  per Schied 2017 §4.2 (`#422`), `caustic_splat.comp` _pad1 → materialKind
  sync (`#417`), BLAS compaction phases 5-6 GPU memory leak on partial OOM
  (`#316`), multi-draw indirect path collapses per-batch `cmd_draw_indexed`
  (`#309`), empty-TLAS size=0 guard plus VUID-VkBufferCopy-size-01988 and
  VUID-VkBufferMemoryBarrier-size-01188 suppression (`#317`), opt-in global
  lock-order graph for cross-thread ABBA detection (`#313`), terrain BLAS
  builds batched across the whole exterior grid (`#382`).

Workspace test count: 623 → 770+. Net source growth: ~64K → ~75K lines of Rust.

**Session 12 closeout (37 commits, zero milestone churn):** audit follow-through
bundle `#306`–`#463`, focused on renderer validation hygiene, Oblivion/FO4-era
ESM coverage, and NIF shader plumbing completeness. Major themes:

- **NIF shader + texture plumbing** — BSShaderTextureSet parallax + env slots
  routed to `GpuInstance` with POM gating (`#453`), BSShaderPPLightingProperty
  and BSLightingShaderProperty now read slots 3/4/5 (`#452`), BGEM
  `material_path` captured on both `NiTriShape` and `BsTriShape` via
  BSEffectShaderProperty (`#434`), `ShaderTypeData` payload surfaced on
  `ImportedMesh` for both trishape variants (`#430`), dedicated
  `TileShaderProperty` parser + unified decal flags across properties
  (`#454`/`#455`), `SF_DOUBLE_SIDED` no longer propagates through FO3/FNV
  BSShader* paths (`#441`), `BSGeometryDataFlags` decoded on Bethesda
  NiGeometryData (`#440`), `BSShader*Controller` preserves the controlled-
  variable enum (`#350`), `NiExtraData` version gating (`#329` + `#330`),
  `NiZBufferProperty` z_test/z_write/z_function plumbed through extended
  dynamic state (`#398`), NiTexturingProperty glow/detail/gloss slots wired to
  the fragment shader (`#399`), FO76 BSTriShape Bound Min Max AABB consumed
  (`#342`), `NiBlend*Interpolator` indirection resolved in animation import
  (`#334`), Shepperd quaternion fast-path renormalised (`#333`), `BSAnimNote` /
  `BSAnimNotes` parsed and IK hints surfaced on `AnimationClip` (`#432`),
  Oblivion KF import + decal slot off-by-one (`#400` + `#402`), stream-derived
  `Vec::with_capacity` sweep through `allocate_vec` (`#408`).
- **ESM parser** — `SCPT` pre-Papyrus bytecode records parsed (`#443`),
  `CREA` + `LVLC` groups dispatched in `parse_esm` (`#442` + `#448`),
  Oblivion CREA indexed and `ACRE` placements recognised (`#396`), FO4 NIF
  `HEDR` → `GameKind` bands corrected for FO3 and FO4 (`#439`), worldspace
  auto-pick + FormID mod-index remap when loading cells by editor ID
  (`#444` + `#445`), `CLMT` `TNAM` sunrise/sunset/volatility hours threaded
  through `weather_system` (`#463`), Skyrim `XCLL` directional-ambient cube +
  specular + fresnel extracted (`#367`), FNV `LAND` parse failure demoted
  warn → debug with error context forwarding (`#385`).
- **Renderer — validation + correctness** — SPIR-V reflection cross-checks
  every descriptor-set layout against shader declarations at pipeline create
  time (`#427`), bindless texture array sized from device limit with an `Err`
  return on overflow (`#425`), `R32_UINT` causticTex sampler switched to
  NEAREST (VUID-vkCmdDraw-magFilter-04553), window portal ray now fires
  along `-N` instead of `-V` (`#421`), TLAS `instance_custom_index` unified
  with SSBO position via a shared map (`#419`), fog moved from
  `triangle.frag` to `composite.frag` — kills SVGF ghosting on heavy fog
  (`#428`), grow-only scratch pool applied to the TLAS full-rebuild path
  (`#424` SIBLING), draw-command depth sort key switched to IEEE 754
  total-ordering (`#306`).

Workspace test count: 770+ → 867. Net source growth: ~75K → ~81K lines of
Rust across 188 source files.

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

### M22: RT-First Multi-Light System — DONE
**Status:** All phases complete. Full RT pipeline operational.
**Scope:** SSBO multi-light rendering (Phase A), RT shadow rays via VK_KHR_ray_query (Phase B),
contact-hardening soft shadows, RT reflections with barycentric UV lookup via global vertex/index
SSBOs, 1-bounce RT ambient GI with cosine-weighted hemisphere sampling, window light portals,
G-buffer expansion (normal, motion vector, mesh ID, raw indirect, albedo — 6 render targets),
SVGF temporal denoiser for indirect lighting with motion vector reprojection and mesh ID
disocclusion detection, composite pipeline (direct + denoised indirect reassembly, ACES tone
mapping), TLAS refit (UPDATE mode when BLAS layout unchanged between frames), clustered lighting.
Cell interior XCLL lighting (ambient + directional), windowed inverse-square attenuation.
BLAS per mesh, TLAS rebuilt/refitted per frame, dynamic depth bias for NIF-flagged decals.
**Result:** Prospector Saloon: 25 point lights + directional + RT shadows at ~85 FPS
on RTX 4070 Ti when this milestone landed. Current figure tracked
via `--bench-frames` per #456 (subsequent M31+/M36/M37/M37.5 work
has shifted the curve).

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

## Session 10 — Shadow pipeline overhaul + TAA + BLAS compaction + FO4 architecture

Renderer-quality push that retired the largest remaining visual regressions
and shipped three renderer milestones (M31.5 streaming RIS, M36 BLAS
compaction, M37.5 TAA). Audit bundle `#314`–`#340` produced alongside.

**Streaming RIS (M31.5)**
- Replaced the deterministic top-K shadow pipeline with 8 independent
  weighted reservoirs per fragment, each sampled from the full light
  cluster proportional to luminance. Every light now has non-zero
  shadow probability — fixes the "large occluder never shadows large
  receiver" pathology the top-K pipeline hit on big overhead lamps.
- Unbiased weight `W = resWSum / (K · w_sel)`, clamped at 64× to tame
  fireflies. Directional sun angular radius tightened 0.05 → 0.0047 rad
  (physically correct).

**TAA (M37.5) — `taa.comp` + `TaaPipeline`**
- Halton(2,3) sub-pixel projection jitter applied in the vertex shader;
  motion vectors stay un-jittered for correct reprojection.
- Motion-vector reprojection with Catmull-Rom 9-tap history resample.
- 3×3 YCoCg neighborhood variance clamp (γ = 1.25).
- mesh_id disocclusion detection reuses the GBuffer mesh_id attachment.
- Luma-weighted α = 0.1 history blend.
- Per-FIF RGBA16F history images, ping-pong descriptor sets,
  first-frame guard, resize hooks. Camera UBO extended with
  `vec4 jitter` (all 4 shader UBO layouts updated in lockstep).
- Composite's HDR binding rewired to the TAA output via
  `rebind_hdr_views()`.

**BLAS compaction (M36)**
- `ALLOW_COMPACTION` flag on BLAS build, async occupancy query, compact
  copy allocated at exact size, original BLAS destroyed via
  `deferred_destroy`. 20–50% BLAS memory reduction on typical cells.

**FO4 architecture**
- `asset_provider` auto-detects BSA vs BA2 from file magic at open time
  so the cell loader no longer hard-codes the archive type.
- ESM parser extended with `SCOL`, `MOVS`, `PKIN`, `TXST` record types —
  the building blocks of FO4's prefab architecture.
- `BSLightingShaderProperty.net.name` now flows through `ImportedMesh`
  → `Material.material_path` so BGSM / BGEM material references
  surface in diagnostics even though the external material file itself
  isn't parsed yet.

**Debug CLI**
- Console commands `tex.missing`, `tex.loaded`, `mesh.info <entity_id>`.
- Evaluator functions `tex_missing()` / `tex_loaded()` over TCP.
- `mesh.info` shows BGSM material reference when `texture_path` is
  absent (correct FO4 behavior).

**NIF parser fixes (`#322`–`#325`, `#340`)**
- `#322`: NiPSysData over-reads — respect BS202 zero-array rule.
- `#323`: NiMaterialProperty variant mapping was wrong for cross-game
  files. Check file `BSVER` directly, not the `NifVariant`.
- `#324`: Oblivion runtime size cache prevents cascading parse failure
  after a single bad block (Oblivion has no per-block size table).
- `#325`: NiGeometryData `Has UV` should only be read until 4.0.0.2.
- `#340`: Pre-intern animation channel names as `FixedString` at clip
  load time so the per-frame sampler hot path never touches the
  `StringPool` lock.

**Reflection + metal quality (`#315`, `#320`)**
- `#315`: Route metal reflection into the direct path to avoid
  albedo double-modulation (post-#268 demodulation invariant).
- `#320`: Exponential distance falloff on reflection rays plus
  roughness-driven angular jitter. Prevents the "infinite gold mirror"
  look on distant glossy surfaces.

**Quality fixes (`#301`, `#302`, `#314`)**
- `#301` + `#302`: Narrow the HOST_VISIBLE flush range to the dirty
  region and reuse a single one-time-submit fence across transfer
  submissions.
- `#314`: Refresh stale `lock_tracker` doc comments on
  `Query`/`Resource` constructors.

**Audit bundle (session 10)**
- Renderer audit 2026-04-14 (3 parallel specialists, 10 dimensions,
  5 findings — 1 MEDIUM, 4 LOW). NIF, legacy-compat, ECS audits dated
  2026-04-14 / 2026-04-15. 31 issue reports (#314–#340 plus
  consolidation dirs for #266–#292, #301–#302, #320–#321) staged for
  GitHub sync.

Workspace test count: 472 → 623. Zero new warnings.

## Previously Completed (M22–M30)

| # | Milestone | Status |
|---|-----------|--------|
| M24 | ESM/ESP Record Parser | **Phase 1 DONE** — 13,684 structured records (items, actors, factions, etc.) from FNV.esm |
| M25 | Vulkan Compute | Partial — clustered lighting compute, SSAO compute, SVGF temporal compute |
| M26 | BA2 Archive Support | **DONE** — BTDX v1/v2/v3/v7/v8, GNRL + DX10, zlib + LZ4. All 7 games 100% |
| M28 | Physics Foundation | **Phase 1 DONE** — Rapier3D, dynamic capsule player body |
| M30 | Papyrus Parser | **Phase 1 DONE** — logos lexer + Pratt expression parser + full AST. 45 tests |
| M31 | RT Performance at Scale | **DONE** — batched BLAS builds, TLAS culling, importance-sorted shadow budget, distance-based ray fallback, GI hit simplification, BLAS LRU eviction, deferred SSBO rebuild |
| M31.5 | Streaming RIS Direct Lighting | **DONE** — weighted reservoir shadow sampling (8 independent reservoirs / fragment, luminance-proportional), unbiased W = resWSum / (K·w_sel) clamped at 64× to tame fireflies. Replaces deterministic top-K. |
| M36 | BLAS Compaction | **DONE** — ALLOW_COMPACTION flag, async occupancy query, compacted copy with original destroyed, 20–50% BLAS memory reduction |
| M37.5 | TAA / Antialiasing | **DONE** — Halton(2,3) jitter, motion-vector reprojection, YCoCg variance clamp (γ=1.25), mesh-id disocclusion, luma-weighted blend. Kills stair/column/tapestry edge shimmer. |

---

## Active Roadmap

Priority: **robust renderer first** — make exterior scenes look correct before
expanding gameplay systems. Each milestone produces a visible improvement.

### Tier 1 — Exterior Rendering (immediate priority)

| # | Milestone | Scope | Depends on |
|---|-----------|-------|------------|
| M32 | Landscape Mesh | Parse LAND records (33×33 heightmap grid per cell), generate terrain mesh, LTEX/TXST texture layers with alpha-blended splatting, vertex colors. The missing ground plane for all exterior cells. | ESM parser |
| M33 | Sky & Atmosphere | Parse WTHR (Weather) records. Sky gradient dome, sun disc with position from game-time, cloud layers (scrolling textures), horizon fog. Procedural fallback when no WTHR is set. Replace hardcoded clear color. | ESM parser |
| M34 | Exterior Lighting | Proper directional sun derived from WTHR/climate sun position. Time-of-day ambient color interpolation. Exterior fog from WTHR fog data (distance + color). Interior/exterior light path split in the shader. | M33 |
| M35 | Terrain LOD | Parse `.btr` terrain LOD meshes from BSA. Distance-based LOD selection (full LAND → LOD4 → LOD8 → LOD16). LOD terrain texture atlas. Object LOD (`.bto` files) for distant statics. | M32 |

### Tier 2 — Renderer Robustness

| # | Milestone | Scope | Depends on |
|---|-----------|-------|------------|
| M37 | SVGF Spatial Filter | A-trous wavelet filter using existing moments data. 3 iterations, edge-stopping on normal/depth/variance. Major GI noise reduction (1-SPP → ~8-SPP visual quality). | — |
| M37.3 | ReSTIR-DI | Full spatiotemporal reservoir reuse for direct lighting. New GBuffer reservoir attachment (light index, wSum, M, selected target pdf). Temporal pass: motion-vector reprojection + reservoir combine. Spatial pass: k-neighborhood resample with normal/depth rejection. Drops shadow rays to 1/pixel while sampling from hundreds of lights. Streaming-RIS already shipped as M31.5. | M31.5, M37 |
| M29 | GPU Skinning | Compute shader bone palette evaluation. SkinnedMesh component → bone SSBO → unified vertex shader. Characters and creatures animate. | M25 |
| M38 | Transparency & Water | Proper OIT or depth-peeled transparency. Water plane mesh with reflection/refraction (screen-space or planar). NIF alpha sort correctness. | — |
| M39 | Texture Streaming | Mip-chain-aware loading: upload low mips immediately, stream high mips on demand. Distance-based texture detail. Memory budget with LRU eviction. | — |
| M37.6 | DLSS2 integration (optional) | NVIDIA DLSS2 as an upscale pass after TAA. 4070 Ti target, proprietary. Nice-to-have on top of the TAA baseline. | M37.5 |

### Tier 3 — Engine Infrastructure

| # | Milestone | Scope | Depends on |
|---|-----------|-------|------------|
| M27 | Parallel System Dispatch | Rayon-based parallel ECS system execution. Type-sorted lock acquisition already in place. | — |
| M28.5 | Character Controller | Kinematic capsule with step-up, slope limiting, ground snapping. Replaces the current dynamic body fly camera for on-foot movement. | M28, M32 |
| M24.2 | ESM Phase 2 | QUST/DIAL/PERK/MGEF semantic parsing. Quest stages, dialogue trees, perk entry points. | M24 |
| M30.2 | Papyrus Phase 2–4 | Statement parser, script declarations, FO4 extensions. Full `.psc` → AST for the entire Skyrim/FO4 corpus. | M30 |

### Tier 4 — Gameplay Systems

| # | Milestone | Scope | Depends on |
|---|-----------|-------|------------|
| M40 | World Streaming | Cell load/unload based on player position. Multi-cell exterior grid with async loading. BLAS streaming (evict/reload) ties into M31's LRU eviction. | M32, M35 |
| M41 | NPC Spawning | Resolve NPC_ records → ECS entities with race/class/equipment. Spawn ACHR references. Visual appearance from head parts + body mesh + equipped items. | M24, M29 |
| M42 | AI Packages | 30 composable procedures, package stack, Sandbox. Patrol paths from NAVM. Basic wander/follow/travel. | M28.5, M41 |
| M43 | Quests & Dialogue | Quest stages, conditions (~300 functions), dialogue trees, Story Manager event triggers. | M24.2, M41 |
| M44 | Audio | Sound descriptors, 3D spatial audio (OpenAL or miniaudio), music system, ambient sounds. | — |
| M45 | Save/Load | Serialize world state, change forms, cosave format. | M40 |
| M46 | Full Plugin Loading | Discover, sort, merge, resolve conflicts across full load order. | M24.2 |
| M47 | Scripting Runtime | ECS-native scripting: 136 event types, condition evaluation, perk entry points. Papyrus transpiler (M30 AST → ECS components). | M30.2, M43 |
| M48 | UI Integration | Scaleform GFx stubs (`_global.gfx`), Papyrus↔UI bridge, input routing, font loading, all 34 menus. | M20, M47 |

---

## Known Issues

### Open
- [ ] No sky, sun, clouds, or atmosphere — exterior uses hardcoded clear color (M33)
- [ ] No terrain or object LOD — no distant rendering (M35)
- [ ] BSA v103 (Oblivion) decompression not working
- [ ] Legacy ESM/ESP parsers are stubs for Morrowind, Oblivion, Skyrim (FO4 has SCOL/MOVS/PKIN/TXST + architecture)
- [ ] NIF material properties beyond diffuse/normal/alpha not fully wired (M38 wetness/subsurface)
- [ ] No skinned mesh rendering (GPU skinning deferred to M29)
- [ ] Scheduler is single-threaded (M27)
- [ ] parry3d panics on nested compound collision shapes (catch_unwind guard in place)

### Resolved
- [x] Pipeline cache bypassed on some create sites (cold shader compile on every run) → VkPipelineCache threaded through every create site with disk persistence (#426)
- [x] BLAS compaction leaked GPU memory when the occupancy query failed partway through → fenced teardown across all five phases (#316)
- [x] Multi-draw indirect path used per-batch `cmd_draw_indexed` → collapsed to a single `cmd_draw_indexed_indirect` (#309)
- [x] Skyrim NPC heads/faces invisible (BSDynamicTriShape import path missing) → full BSDynamicTriShape mesh extraction (#341)
- [x] Torches + magic FX invisible (CPU particle data present but unrendered) → end-to-end CPU particle system (#401)
- [x] NIF import recomputed per cell load → promoted to process-lifetime resource (#381)
- [x] BSA/BA2 file handle re-opened per extract on exterior cell loads → long-lived handle (#360)
- [x] Exterior cell ESM parse ran three times per load → collapsed to a single pass (#374)
- [x] TXST record surfaced only the diffuse slot (TX00) → extract all 8 texture slots (#357)
- [x] SCOL placements on FO4 cells invisible → parse ONAM/DATA child placements (#405)
- [x] CELL XCLW water plane height not parsed → surfaced on the cell descriptor (#397)
- [x] REFR default-disabled references rendered anyway → XESP gating at cell load (#349)
- [x] Oblivion parser cascade-failed after one bad block (no per-block size table) → runtime size cache + stream-drift detector (#324, #395)
- [x] No antialiasing — stair/column/tapestry edge shimmer → TAA compute pass with Halton jitter, Catmull-Rom history, YCoCg neighborhood clamp (M37.5)
- [x] Top-K shadow rays dropped lights outside the top cliff → streaming weighted reservoir sampling (M31.5)
- [x] BLAS memory pressure → ALLOW_COMPACTION + query + compact copy, 20–50% reduction (M36)
- [x] FO4 architecture invisible — no SCOL/MOVS/PKIN/TXST records → parser extended (session 10)
- [x] BA2 vs BSA routing fragile → auto-detect from file magic in asset_provider (session 10)
- [x] RT shadow budget was FIFO, not importance-sorted → top-K by contribution (M31), then streaming RIS (M31.5)
- [x] Exterior lighting had no proper sun direction → default exterior directional sun (M34)
- [x] No landscape mesh — exterior cells had no ground → LAND heightmap + LTEX/TXST splatting (M32)
- [x] No distance fallback for shadow/GI rays → smooth fade at 600–800/1200–1500 units (M31)
- [x] BLAS builds blocked per-mesh with fence stall → batched single-submission (M31)
- [x] No BLAS eviction → LRU eviction with VRAM/3 memory budget (M31, refined #387)
- [x] Geometry SSBO rebuild called device_wait_idle → deferred destroy (M31)
- [x] TLAS instance Vec allocated fresh each frame → amortized scratch (M31)
- [x] MAX_INSTANCES 4096 too small for exteriors → 8192 (M31)
- [x] Full RT pipeline: shadows, reflections, GI, SVGF denoiser, composite (M22)
- [x] SSBO multi-light + clustered lighting + cell XCLL (M22)
- [x] Degenerate NIF rotation matrices → SVD decomposition (M17)
- [x] Gamebryo CW rotation convention → Euler angle sign fix (M17)
- [x] Animation controllers → full .kf playback pipeline (M21)
- [x] BA2 support → v1/v2/v3/v7/v8, all 7 games 100% (M26)
- [x] UI/menu system → Ruffle SWF integration (M20)

---

## Game Compatibility

| Tier | Games | NIF | Archive | ESM | Cell Loading |
|------|-------|-----|---------|-----|-------------|
| 1 — Working | Fallout: New Vegas | 89 parsed + 30 skip, RT shadows, XCLL | BSA v104 ✓ | 23 record types + XCLL | Interior + exterior ✓ |
| 1 — Working | Fallout 3 | Validated: Megaton 929 REFRs, 0 parse failures | BSA v104 ✓ | Same as FNV ✓ | Interior ✓ · Exterior wired (needs fresh GPU bench — #457) |
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
| Passing tests | 867 (336 nif, 206 core, 123 plugin, 62 renderer, 45 papyrus, 32 binary, 19 bsa, 17 physics, 9 nif-fixtures, 8 scripting, 4 debug-protocol, + integration) |
| Workspace members | 15 (13 engine crates + `byroredux` binary + `byro-dbg` CLI) |
| Completed milestones | 30+ (M1–M22, M24 Phase 1, M26, M28 Phase 1, M30 Phase 1, M31, M31.5, M32 Phase 1+2, M34 Phase 1, M36, M37.5) + N23 + N26 + #178 + session-11 #316/#381/#401/#405/#426 + session-12 #306–#463 closeout bundle |
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
| `byroredux-core` | M3 (ECS), M5 (Form IDs), M21 (Animation), #178A (SkinnedMesh), #137 (lock guards), #340 (interned channel names), #313 (lock-order graph), #333 (Shepperd renorm), #334 (NiBlend*Interpolator) | 206 |
| `byroredux-renderer` | M1, M2, M4, M7, M8, M13, M14, M22, M31, M31.5 (streaming RIS), M36 (BLAS compaction), M37.5 (TAA), #178B (bone palette), #136 (16× AF), #309/#316/#317/#392/#415/#416/#420/#422/#426 (session 11), #306/#419/#421/#424/#425/#427/#428 + VUID-04553 (session 12) | 62 |
| `byroredux-platform` | M1 (windowing) | — |
| `byroredux-plugin` | M5, M6, M19, M24 Phase 1, FO4 SCOL/MOVS/PKIN/TXST, #349 XESP, #356 Skyrim CELL, #374 single-pass exterior, #389 BGRA LIGH, #391 variant group end, #397 XCLW, #405 SCOL body, #357 all 8 TXST slots, #367 Skyrim XCLL cube/fresnel, #385 LAND error context, #396 Oblivion CREA/ACRE, #442/#448 CREA/LVLC dispatch, #443 SCPT, #444/#445 worldspace auto-pick + FormID remap, #463 CLMT TNAM | 123 |
| `byroredux-nif` | M9, M10, M17, M18, M21, N23.1–N23.10, N26 audit, #79 KFM, #322/#323/#324/#325/#395 Oblivion robustness, #106/#128/#165/#166/#335/#341/#344/#346/#363/#364/#369/#381/#401/#403/#407/#429 session 11, BGSM material_path, session 12: #306/#329/#330/#342/#350/#398/#399/#400/#402/#408/#430/#432/#434/#439/#440/#441/#452/#453/#454/#455 | 336 |
| `byroredux-bsa` | M11, M18, M26 (BA2), session 7 (v3 LZ4), session 10 (archive auto-detect), #352/#360 extract robustness | 19 |
| `byroredux-physics` | M28 Phase 1 (Rapier3D bridge) | 17 |
| `byroredux-scripting` | M12 | 8 |
| `byroredux-papyrus` | M30 Phase 1 (Papyrus parser) | 45 |
| `byroredux-ui` | M20 (Ruffle/SWF) | — |
| `byroredux-debug-protocol` | Wire protocol + component registry | 4 |
| `byroredux-debug-server` | TCP server + Papyrus evaluator (tex_missing, tex_loaded, mesh.info) | — |
| `byroredux-cxx-bridge` | Cross-cutting | — |
| `byroredux` (binary) | M4, M11, M14–M17, M19, M28, M32, M34, cell cache, terrain, FO4 architecture, #401 CPU particle system, #463 weather_system TNAM hours | 32 |
| `tools/byro-dbg` | Standalone debug CLI (TCP client, REPL) | — |

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
