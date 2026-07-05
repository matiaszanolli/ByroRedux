# NIF Parser

NIF is the binary mesh format used by every Bethesda Gamebryo and
Creation engine game. The `byroredux-nif` crate parses every supported
game's mesh archive **recoverably at 100%** (184,886 NIFs across the
seven-game sweep — every file links end-to-end, counting `NiUnknown`
placeholders and truncated trailers as recoverable). The stricter
*clean* rate (no `NiUnknown`, no truncated trailer) is 100% on
FO3 / FNV / Skyrim SE and in the 96–99% band on FO4 / FO76 / Starfield /
Oblivion — see [Game Compatibility](game-compatibility.md) for the live
per-game matrix. This document explains how the parser is organised and
what each piece does.

Source: [`crates/nif/src/`](../../crates/nif/src/)

## At a glance

| | |
|---|---|
| Dispatch table       | hand-written `match type_name` in [`blocks/mod.rs`](../../crates/nif/src/blocks/mod.rs) `parse_block_inner` — **the live arm count is the source of truth; count fresh, this figure drifts.** ~260 arms covering ~309 distinct block-type-name literals (2026-07-05). |
| Game variants supported | 8 (Morrowind → Starfield) via the [`NifVariant`](../../crates/nif/src/version.rs) enum |
| Tests (unit)         | ~738 in-crate `#[test]`s with synthetic byte streams (per-parser regressions + the 67-test `dispatch_tests` suite + per-category `*_tests.rs` siblings) |
| Integration sweeps   | 7 games, 100% recoverable each ([`tests/parse_real_nifs.rs`](../../crates/nif/tests/parse_real_nifs.rs) + per-block-baseline / heap-bound / translation-completeness siblings) |
| Cumulative NIFs swept | 184,886 (full mesh-archive sweeps, per-game counts in [Game Compatibility](game-compatibility.md)) |
| BGSM / BGEM references | Surfaced as `ImportedMesh.material_path` when the NiNet name is a material file (BSVER ≥ 155); BGSM/BGEM sidecars surfaced on the lighting/effect shader data |
| Starfield `.mesh`     | `BSGeometry` carries external `.mesh` filenames; resolved via the `geometries\<hash>.mesh` canonical path (#1292), not `meshes\…` |
| Import cache          | Process-lifetime resource (#381) — each unique NIF parses once per process, not once per cell |
| OOM hardening         | Every stream-derived `Vec::with_capacity` routed through `stream.allocate_vec(count)?`, which bounds `count` against remaining file bytes (#388 + #408 + #1245/#1246) — a corrupt `u32::MAX` count errors cleanly instead of aborting the process |
| Canonical translation | Raw `Imported*` decode is fed through the **NIFAL** (NIF Abstraction Layer) `translate()` boundary into game-agnostic ECS types — see [NIFAL](nifal.md) |

## Module map

```
crates/nif/src/
├── lib.rs            Top-level parse_nif()/parse_nif_with_options() walker + per-block recovery loop
├── tests.rs          Lifted lib.rs recovery / walker unit tests (#1118 TD9-002)
├── version.rs        NifVersion (packed u32) + NifVariant feature flags
├── header.rs         NIF header parser (BSStreamHeader, block type table, strings)
├── stream.rs         NifStream — version-aware binary reader (allocate_vec OOM guard)
├── types.rs          NiPoint3, NiMatrix3, NiTransform, NiColor, BlockRef
├── rotation.rs       Degenerate rotation-matrix detection + SVD repair (#277, lifted here #1044)
├── shader_flags.rs   Named BSShaderFlags / SkyrimShaderPropertyFlags / Fallout4ShaderPropertyFlags constants
├── kfm.rs            KFM (KeyFrame Metadata) state-machine file parser (binary, v1.2.0.0–2.2.0.0)
├── scene.rs          NifScene container with downcast helpers (get_as<T>) + validate_refs()
├── anim/             KF animation file import (ImportedClip, channels) — Session 35 split
│                     into per-phase siblings (mod / coord / controlled_block / transform /
│                     sequence / keys / channel / bspline / entry / types / tests)
├── blocks/           Per-block parsers (one file/dir per category)
│   ├── mod.rs        parse_block / parse_block_with_name_arc / parse_block_inner dispatcher + NiObject trait
│   ├── traits.rs     HasObjectNET, HasAVObject, HasShaderRefs upcast traits
│   ├── base.rs       Shared base-class data structs (NiObjectNETData, …)
│   ├── node.rs       NiNode + BS variants (BSFadeNode, BSValueNode, BsRangeNode,
│   │                 NiBillboardNode, NiSwitchNode, NiLODNode, NiSortAdjustNode,
│   │                 NiCamera, BsMultiBoundNode, …)
│   ├── tri_shape/    Session 35 + #1118 (TD9-005) split — indexed triangle geometry:
│   │                 mod.rs (re-exports + parse_geometry_data_base*), ni_tri_shape.rs
│   │                 (NiTriShape/Strips/LodTriShape + data), bs_tri_shape.rs (BSTriShape +
│   │                 its 4 wire-distinct subclasses via BsTriShapeKind), agd.rs
│   │                 (NiAdditionalGeometryData / BSPackedAdditionalGeometryData, #547)
│   ├── bs_geometry.rs Starfield BSGeometry / BSGeometryMesh / BSGeometryMeshData (external
│   │                 .mesh refs + inline UDEC3-packed payload — wire layout from nifly)
│   ├── shader.rs     BSShaderPP / BSLightingShader (per-variant dispatch, #1279) /
│   │                 BSEffectShader (8 ST variants, FO76 stopcond, Root Material sidecar)
│   ├── light.rs      NiLight hierarchy (ambient, directional, point, spot)
│   ├── properties.rs Material, Alpha, Stencil, Texturing, ZBuffer, VertexColor, …
│   ├── texture.rs    NiSourceTexture, NiPixelData, NiTextureEffect (projector)
│   ├── controller/   Session 36 split — NiTimeController family: mod.rs, legacy.rs
│   │                 (NiKeyframe/UV/Vis/Alpha/MaterialColor/Flip), sequence.rs
│   │                 (NiControllerManager/Sequence/MultiTargetTransform), morph.rs
│   │                 (NiGeomMorpherController + NiMorphData), shader.rs (BsShaderController
│   │                 quintet)
│   ├── interpolator.rs NiTransform/Float/Point3/Bool interpolators + Blend variants, NiUVData
│   ├── extra_data.rs NiStringExtraData, BSXFlags, BSBound, BSDecalPlacement,
│   │                 NiStringsExtraData + NiIntegersExtraData (array variants), …
│   ├── multibound.rs BSMultiBound + AABB/OBB shapes
│   ├── palette.rs    NiDefaultAVObjectPalette, NiStringPalette
│   ├── particle.rs   ~48 modern (NiPSys) particle system types — incl. typed NiPSysEmitter
│   │                 (EmitterBaseParams), NiPSysEmitterCtlr, NiPSysGrowFadeModifier (NIFAL)
│   ├── legacy_particle.rs  Pre-NiPSys particle stack (Oblivion / Morrowind):
│   │                 NiParticleSystemController, NiAutoNormal/Rotating Particles,
│   │                 NiParticleColorModifier / GrowFade / Rotation / Bomb,
│   │                 NiGravity, NiPlanarCollider, NiSphericalCollider
│   ├── skin.rs       NiSkinInstance/Data/Partition, BSSkin, BsDismemberSkinInstance
│   └── collision/    bhk* collision shapes — Session 35 split into siblings + sibling
│                     test files: collision_object / rigid_body / ragdoll /
│                     shape_primitive / shape_compound / shape_mesh / compressed_mesh /
│                     constraints / phantom_action, with shared low-level readers in mod.rs
└── import/           NIF→ECS scene import (raw Imported* tier; NIFAL translate() lives downstream)
    ├── mod.rs        import_nif / import_nif_scene / import_nif_with_collision /
    │                 import_nif_lights / import_nif_particle_emitters entry points
    ├── types.rs      ImportedNode / Mesh / Scene / Light / Collision / Skin / Bone /
    │                 ParticleEmitter(+Flat) / EmitterParams data structs
    ├── walk/         Hierarchical + flat scene graph traversal (mod.rs + tests.rs, #1118)
    ├── mesh/         Session 35 split into production siblings (material_path / decode /
    │                 ni_tri_shape / bs_tri_shape / bs_geometry / tangent / sse_recon /
    │                 skin) plus per-topic *_tests.rs siblings
    ├── material/     Session 36 split — MaterialInfo extraction (mod.rs + walker.rs +
    │                 shader_data.rs) plus per-topic *_tests.rs siblings (alpha / decal /
    │                 double-sided / emissive-source / PBR / texture-slot / sky-water / …)
    ├── collision.rs  Havok→engine transform + bhk*Shape → CollisionShape / RigidBodyData
    ├── transform.rs  Transform composition
    └── coord.rs      Z-up (Gamebryo) → Y-up (renderer) quaternion conversion
```

The split between `blocks/` (binary parsers) and `import/` (scene to
ECS-friendly mesh) lets the parser be tested in isolation against bytes
without dragging in `glam` or any renderer types.

> **Session 34/35/36 layout note.** Several files referenced by older
> audits as single `.rs` files are now submodule directories:
> `blocks/tri_shape.rs` → `blocks/tri_shape/`, `blocks/collision.rs` →
> `blocks/collision/`, `blocks/controller.rs` → `blocks/controller/`,
> `import/mesh.rs` → `import/mesh/`, `import/material.rs` →
> `import/material/`, `import/walk.rs` → `import/walk/`, `anim.rs` →
> `anim/`. The `Imported*` structs moved out of `import/mod.rs` into
> `import/types.rs`, and the SVD rotation repair moved out of
> `import/transform.rs` to the top-level `rotation.rs`. Translate stale
> paths through this map.

## Parse pipeline

`parse_nif(data)` (and `parse_nif_with_options(data, &ParseOptions)`,
which adds the geometry-only `skip_animation` mode) in
[`lib.rs`](../../crates/nif/src/lib.rs) is the top-level entry point. It
runs three phases:

1. **Header parse** ([`header.rs`](../../crates/nif/src/header.rs)) — read
   the ASCII header line, the binary version + endianness, the
   `BSStreamHeader` if present, the block-type table, the per-block size
   array (when the format has one), the global string table, and the
   group count.
2. **Block walk** — for each block index, look up its type in the header's
   block-type table, dispatch to the per-block parser in
   [`blocks/mod.rs`](../../crates/nif/src/blocks/mod.rs) via
   `parse_block_with_name_arc`, and append the parsed block to the scene.
3. **Root identification** — find the first `NiNode` in the result and
   record it as the scene root.

The output is a `NifScene` containing a `Vec<Box<dyn NiObject>>`. Each
parsed block implements the `NiObject` trait, which exposes a type-name
string and an `as_any()` downcast for callers that want concrete types
(via `scene.get_as::<NiTriShape>(idx)`).

### groupID consume (early-Gamebryo)

Before the dispatch match, `parse_block_inner` consumes a 4-byte
`groupID` field for files in the version range `[10.0.0.0, 10.1.0.114)`
(O5-3 / #688). nifly's `NiObject::Get` reads this on every `NiObject`
before the subclass payload; pre-fix it was misread as the first u32 of
the block (usually the `NiObjectNET.Name` length), truncating ~154 of
8032 Oblivion-era files at root. The value is read and discarded —
vanilla content always ships zero.

## Version handling

NIF files identify themselves with three pieces of metadata:

- A 4-byte packed `version` (`major << 24 | minor << 16 | patch << 8 | build`)
- A `user_version` (Bethesda BSStream marker, since v10.0.1.8)
- A `user_version_2` aka `BSVER` (game-specific Bethesda version, in `BSStreamHeader`)

The `(version, user_version, user_version_2)` triplet maps deterministically
to a [`NifVariant`](../../crates/nif/src/version.rs) enum:

```rust
pub enum NifVariant {
    Morrowind,    // NIF ≤ 4.x, NetImmerse era
    Oblivion,     // NIF 20.0.0.5, user_version < 11
    Fallout3,     // NIF 20.2.0.7, uv=11, uv2 < 34 (pre-retail authoring tools)
    FalloutNV,    // NIF 20.2.0.7, uv=11, uv2=34 (retail FO3 also detects here — binary-identical)
    SkyrimLE,     // uv=12, uv2=83
    SkyrimSE,     // uv=12, uv2=100
    Fallout4,     // uv=12, uv2=130
    Fallout76,    // uv=12, uv2=155
    Starfield,    // uv=12, uv2 ≥ 170
    Unknown,
}
```

`NifVariant::detect()` does the dispatch once at header read time;
afterwards, every block parser asks **semantic feature flags** on the
variant rather than checking raw version numbers:

```rust
if stream.variant().has_properties_list()  { ... }    // pre-Skyrim NiAVObject
if stream.variant().avobject_flags_u32()   { ... }    // FO3+ uses u32 not u16
if stream.variant().has_material_crc()     { ... }    // Skyrim+ NiGeometryData
if stream.variant().uses_bs_lighting_shader() { ... } // Skyrim+ shader split
if stream.variant().uses_bs_tri_shape()    { ... }    // SSE+ packed geometry
if stream.variant().uses_fo76_shader_flags() { ... }  // FO76+ CRC32 flag arrays
```

This is the **GameVariant trait pattern** mentioned in the project memory:
keep all the per-game quirks in one place instead of scattering version
checks through 150+ block parsers. When a new game variant lands the
work is "add an enum variant + a few feature flags", not "audit every
block parser". The #1277 epic migrated the remaining variant-aligned raw
`bsver` comparisons to `NifVariant` helpers and added a typed
[`ShaderFlags`](../../crates/nif/src/shader_flags.rs) variant view so a
single mesh can't pick the wrong flag vocabulary.

> **Detection note (#943 / #1219).** `V20_0_0_4` / `V20_0_0_5` are routed
> to `Oblivion` ahead of the uv/uv2 match — no other game uses those
> versions — and the ambiguous `(V20_0_0_4, user_version=11, _)` routing
> emits a one-shot warning rather than silently picking a variant.

## Header parser

The header parser handles every supported NIF version with the right
field layout. Three subtle correctness fixes that shipped during M26+:

### 1. `user_version` threshold

Per nif.xml, `user_version` exists from **v10.0.1.8 onward**, not from
v10.0.1.0 as the original parser assumed. Older NetImmerse files (parts
of Oblivion's BSA — minotaur horns, evil sconce lights) jump straight from
`version` to `num_blocks` without a `user_version` field. The wrong
threshold corrupted `num_blocks` and the block walker ran off the end of
the buffer.

### 2. `BSStreamHeader` presence

The original parser gated the BSStreamHeader on `user_version >= 10`, but
the format actually applies to:

- Any v10.0.1.2 file (Bethesda's first Gamebryo era — unconditional)
- Other versions when `user_version >= 3`

Per nif.xml's `#BSSTREAMHEADER#` macro. Without the right gate, FO4 (BSVER
130) and FO76 (BSVER 155) get the wrong number of trailing short strings:
FO4 needs an extra `Max Filepath`, and FO76 has an extra `Unknown Int u32`
**plus** drops `Process Script` entirely. The header parser now matches
nif.xml exactly:

```
Author        ExportString
Unknown Int   u32,           if BS Version > 130   (FO76, Starfield)
Process Script ExportString, if BS Version < 131  (≤ FO4)
Export Script ExportString
Max Filepath  ExportString,  if BS Version >= 103  (FO4+)
```

### 3. Pre-Gamebryo NetImmerse fallback

NIF v3.3.0.13 files (Oblivion's `meshes/marker_*.nif` debug placeholders)
inline each block's type name as a sized string instead of using a global
type table. We don't currently parse those (nothing in the engine consumes
them — markers are filtered by name at render time anyway), but the
top-level walker now returns an empty scene with a debug log when the
type table is empty instead of returning an error. Soft-fail keeps the
integration test thresholds meaningful.

## Per-block recovery

The block walker in [`lib.rs`](../../crates/nif/src/lib.rs) is the heart
of the "soft fail and keep going" philosophy. When a block parser errors
and the format has a per-block size table (FO3+), the walker seeks past
the broken block, pushes a `NiUnknown` placeholder (refcount-cloning the
already-built type-name `Arc<str>`, #1261), and continues — a single
buggy block parser doesn't kill the file. When parse *succeeds* but
consumes the wrong number of bytes, the walker reconciles the stream
position against `block_size`, which also absorbs trailing bytes of
coverage-first stubs like `BSFaceGenNiNode` (Starfield, #727).

This is what bumped the Skyrim SE smoke test from 42% → 100% during
N23.10: a single Havok block layout quirk was killing every Skyrim NIF
that contained it, but the bytes per block were structurally fine.
Recovery is only available when `block_size` is known (FO3+), since
Oblivion v20.0.0.5 NIFs have no per-block size table — for those, the
walker stops at the first error but keeps the blocks parsed so far.
A runtime size cache (#324) plus a stream-drift detector (#395) make
Oblivion under-reads measurable rather than an unexplained parse-rate
regression.

## Block coverage

Block types fall into a handful of families. The dispatch table in
[`blocks/mod.rs`](../../crates/nif/src/blocks/mod.rs) `parse_block_inner`
carries ~260 match arms covering ~309 distinct type-name literals
(2026-07-05 — verify the live count from source; it grows). Coverage summary:

### Nodes and geometry
`NiNode`, `BSFadeNode`, `BSLeafAnimNode`, `BSTreeNode`, `BSMultiBoundNode`,
`RootCollisionNode`, `BSOrderedNode`, `BSValueNode`, `NiTriShape`,
`NiTriStrips`, `NiLodTriShape`, `BSSegmentedTriShape`, `BSTriShape`,
`BSMeshLODTriShape`, `BSSubIndexTriShape`, `NiTriShapeData`,
`NiTriStripsData`, plus `NiAdditionalGeometryData` /
`BSPackedAdditionalGeometryData` (#547, in `tri_shape/agd.rs`).

### Starfield geometry (BSGeometry)
Starfield replaced the FO4-era `BSTriShape` family with a top-level
`BSGeometry` container that splits the geometry out of the `.nif` into
companion `.mesh` files. [`blocks/bs_geometry.rs`](../../crates/nif/src/blocks/bs_geometry.rs)
parses up to 4 `BSGeometryMesh` slots per node — each holds either an
external `.mesh` filename (the 99% Starfield case, resolved through the
`geometries\<hash>.mesh` canonical path, #1292) or, when bit `0x200` of
the parent NiAVObject flags is set, an inline `BSGeometryMeshData`
payload (UDEC3-packed normals/tangents, half-float UVs, meshlets, cull
data). The wire layout is taken from the out-of-repo nifly checkout at
`/mnt/data/src/reference/nifly` (`src/Geometry.cpp::BSGeometry::Sync`),
since nif.xml has no top-level `BSGeometry` schema.

### Node subtypes (N26 audit follow-up)
`NiBillboardNode` (camera-facing children, u16 mode since 10.1.0.0),
`NiSwitchNode` (u16 flags + u32 active index), `NiLODNode`
(inherits NiSwitchNode + NiLODData ref — `NiRangeLODData` is the typed
subclass, now a parsed block carrying per-level near/far distances and a
Y-up LOD center; `lod_group` is surfaced on `ImportedNode` as forward-compat
coverage — 0 occurrences in shipped vanilla archives but present in mods and
future titles; see `633729f0`), `NiSortAdjustNode`
(transparency sorter override), `NiCamera` (embedded cinematic
frustum + viewport + lod_adjust for cutscene rigs),
`BsRangeNode` (BSRangeNode / BSBlastNode / BSDamageStage / BSDebrisNode
— identical `(min, max, current)` byte triple),
`AvoidNode` / `NiBSAnimationNode` / `NiBSParticleNode` (legacy NiNode
pure-aliases with no trailing fields), `BSFaceGenNiNode` (Starfield
coverage-first NiNode alias, #727). All gated on the per-version
layouts pulled straight from `nif.xml`.

### Shaders
- **FO3/FNV**: `BSShaderPPLightingProperty` (with refraction/parallax),
  `BSShaderNoLightingProperty`, `BSShaderTextureSet`
- **Oblivion specializations** (alias to `BSShaderPPLightingProperty`
  since they share the base texture-set + flags layout — see #145):
  `SkyShaderProperty`, `TallGrassShaderProperty`, `Lighting30ShaderProperty`,
  `HairShaderProperty`, `VolumetricFogShaderProperty`,
  `DistantLODShaderProperty`, `BSDistantTreeShaderProperty`,
  `BSSkyShaderProperty`. `WaterShaderProperty` / `BSWaterShaderProperty`
  split out of the aliased group into the base-only arm (#474/#717) and
  reach a dedicated consumer at import (#1243).
- **`TileShaderProperty`** (#455) — FO3 HUD / UI tile shader. Split out
  of the aliased group with its own parser matching nif.xml's
  `BSShaderLightingProperty` + File Name SizedString layout. Pre-fix the
  aliased PPLighting parser over-read 20-28 bytes and dropped the filename.
- **`BsShaderController`** family (#350, in `controller/shader.rs`) — the
  five Skyrim+ shader property controllers (`BSEffectShaderPropertyFloatController` /
  `...ColorController`, `BSLightingShaderPropertyFloatController` /
  `...ColorController` / `...UShortController`) each trail
  `NiSingleInterpController` with a `uint` enum naming the driven slot.
  Preserved on the block as
  `ShaderControllerKind::{EffectFloat, EffectColor, LightingFloat, LightingColor, LightingUShort}(u32)`
  so the animation importer can route key streams to the correct uniform.
- **Skyrim+/FO4**: `BSLightingShaderProperty` (8 shader-type variants —
  EnvironmentMap, SkinTint, HairTint, ParallaxOcc, MultiLayerParallax,
  SparkleSnow, EyeEnvmap, None — dispatched per-variant since #1279),
  `BSEffectShaderProperty`. Captures the `.mat`/Root-Material sidecar
  (#976 / #1183) and the PBR / backlight scalars (#1175 / #1241).
- **FO76+/Starfield**: CRC32 flag arrays (`Num SF1` / `SF1[]` since BSVER ≥ 132,
  `Num SF2` / `SF2[]` since BSVER ≥ 152), `BSShaderType155` enum dispatch,
  `BSSPLuminanceParams`, `BSSPTranslucencyParams`, `BSTextureArray`, plus
  the **stopcond on `Name`** — when BSVER ≥ 155 and the Name field is a
  non-empty BGSM/BGEM file path, the rest of the block is absent and the
  parser short-circuits to a material-reference stub.

### Properties (older games)
`NiMaterialProperty`, `NiAlphaProperty`, `NiTexturingProperty` (with bump
map / parallax fields), `NiStencilProperty` (version-aware), `NiZBufferProperty`,
`NiVertexColorProperty`, `NiSpecularProperty`, `NiWireframeProperty`,
`NiDitherProperty`, `NiShadeProperty`. `NiFogProperty` is a deliberate
non-dispatch (the gap is accepted and documented, #1224).

### Textures
`NiSourceTexture`, `NiPixelData`, `NiPersistentSrcTextureRendererData`,
`NiTextureEffect` (projected env-map / gobo / fog projector with full
NiDynamicEffect base — gated `bsver < FALLOUT4`, #1240 — texture filtering /
clamping / type / coord-gen enums, clipping plane, and version-gated max
anisotropy and PS2 L/K fields — see #163).

### Lights
Full `NiLight` hierarchy (#156): `NiAmbientLight`, `NiDirectionalLight`,
`NiPointLight`, `NiSpotLight`. All share a common `NiLightBase` covering
the NiDynamicEffect (switch_state + affected-node ptr list) + NiLight
(dimmer + ambient/diffuse/specular color3) wire layout. Point lights
add 3-float attenuation, spot lights add outer/inner cone angles +
exponent with correct version gating (inner angle since 20.2.0.5).
FO4+ (BSVER ≥ 130) reparents NiLight onto NiAVObject and is
intentionally not implemented yet. Downstream, `import_nif_lights()`
walks the scene graph and emits `ImportedLight` records; the cell
loader spawns a `LightSource` ECS entity per parsed light, feeding
them into the existing GpuLight buffer.

### Extra data
`NiStringExtraData`, `NiBinaryExtraData`, `NiIntegerExtraData`, `BSXFlags`,
`NiBooleanExtraData`, `NiStringsExtraData` / `NiIntegersExtraData` (array
variants — material override lists, bone LOD metadata), `BSBound`,
`BSDecalPlacementVectorExtraData`, `BSBehaviorGraphExtraData`, `BSInvMarker`,
`BSClothExtraData`, `BSConnectPoint::Parents`, `BSConnectPoint::Children`.
The generic `"NiExtraData"` dispatch arm (#1073) closes FO4 FaceGen
truncation.

### Controllers and interpolators
After the Session 36 [`controller/`](../../crates/nif/src/blocks/controller/)
split: `NiTimeController`, `NiSingleInterpController`,
`NiMaterialColorController`, `NiMultiTargetTransformController`,
`NiControllerManager`, `NiControllerSequence`, `NiTextureTransformController`,
`NiTransformController`, `NiKeyframeController` (pre-Skyrim per-bone driver,
aliases to `NiSingleInterpController` — see #144), `NiVisController`,
`NiAlphaController`, `BSEffect/Lighting Shader Property {Float,Color}Controller`,
`NiGeomMorpherController`, `NiMorphData`, `NiUVController` + `NiUVData`
(scrolling UV animation for water / fire / banners — see #154),
`NiFlipController` (texture-flipbook driver), `NiSequenceStreamHelper`
(pre-Skyrim KF animation root), `NiLookAtInterpolator` (replaces the
deprecated `NiLookAtController` from Oblivion-era cinematics).
Interpolators: `NiTransformInterpolator`, `BSRotAccumTransfInterpolator`,
`NiTransformData`/`NiKeyframeData`, `NiFloatInterpolator`, `NiFloatData`,
`NiPoint3Interpolator`, `NiPosData`, `NiBoolInterpolator`, `NiBoolData`,
`NiTextKeyExtraData`, plus the four `NiBlend*Interpolator` variants used
by `NiControllerManager` blending.

### Skinning
`NiSkinInstance`, `NiSkinData` (per-bone transforms + vertex weights),
`NiSkinPartition`, `BsDismemberSkinInstance`, `BSSkin::Instance`,
`BSSkin::BoneData`. M29 GPU skinning Phase 1+2 ships end-to-end through
the per-skinned-entity BLAS refit; #638 added the SSE `BSTriShape`
12-byte VF_SKINNED block decoder so SSE skin payloads flow from parser
through to compute, and #1203 resolves the Starfield `BSGeometry`
skin instance via the `BsSkinInstance` chain.

### Particle systems (~48 types)
`NiParticles`, `NiParticleSystem`, `NiMeshParticleSystem`,
`BSStripParticleSystem`, `BSMasterParticleSystem`, plus
`NiParticlesData`/`NiPSysData`/`NiMeshPSysData`/`BSStripPSysData`/
`NiPSysEmitterCtlrData`, 18 modifiers, 5 emitters, 2 colliders, 6 field
modifiers, 21 controllers via shared base parsers. As of the 2026-05-28
NIFAL particle slice, `NiPSysEmitter` is a *typed* block carrying decoded
`EmitterBaseParams` (speed / declination / life + variations), the
`NiPSysEmitterCtlr` carries its interpolator ref for authored birth
rate, and `NiPSysGrowFadeModifier` captures `base_scale` for authored
size — see [NIFAL § Particles](nifal.md).

### Legacy (pre-NiPSys) particle stack — Oblivion / Morrowind (#143)
nif.xml marks these `until="V10_0_1_0"` but Bethesda kept them alive
through Oblivion v20.0.0.5, and every magic FX / fire / dust / blood
mesh depends on them. Full parsers live in
[`legacy_particle.rs`](../../crates/nif/src/blocks/legacy_particle.rs):

- `NiParticleSystemController` (32-field scalar chain + variable particle
  record array + trailing emitter / modifier plumbing), `NiBSPArrayController`
  (aliases the same parser — Bethesda subclass with zero added fields)
- `NiAutoNormalParticles` / `NiRotatingParticles` — NiGeometry body in
  Oblivion form, shared `NiLegacyParticles` struct with a type-name tag
- `NiAutoNormalParticlesData` / `NiRotatingParticlesData` — NiParticlesData
  scalar tail (has_radii / num_active / has_sizes / has_rotations /
  has_rotation_angles / has_rotation_axes) on top of
  `tri_shape::parse_geometry_data_base`
- Seven leaf modifiers — `NiParticleColorModifier`, `NiParticleGrowFade`,
  `NiParticleRotation`, `NiParticleBomb`, `NiGravity`, `NiPlanarCollider`,
  `NiSphericalCollider` — sharing `parse_particle_modifier_base` and
  `parse_particle_collider_base` helpers

### Havok collision
The [`collision/`](../../crates/nif/src/blocks/collision/) directory
parses 14 `bhk*Shape` variants for byte-correctness:
`bhkBoxShape`, `bhkSphereShape`, `bhkCapsuleShape`, `bhkCylinderShape`,
`bhkMultiSphereShape`, `bhkConvexVerticesShape`, `bhkConvexListShape`,
`bhkListShape`, `bhkTransformShape`, `bhkMoppBvTreeShape`,
`bhkNiTriStripsShape`, `bhkPackedNiTriStripsShape`,
`bhkCompressedMeshShape`, `bhkSimpleShape`, plus
`hkPackedNiTriStripsData` / `bhkCompressedMeshShapeData`,
`bhkCollisionObject` / `bhkRigidBody`, and the constraint / ragdoll /
phantom families. The constraint family (`bhkRagdollConstraint`,
`bhkLimitedHingeConstraint`, `bhkBreakableConstraint`, …) is parsed for
byte-alignment but its pivot/axis/limit data has no consumer until
physics wiring lands (#331, closed as deferred).

### Spatial / palettes
`BSMultiBound`, `BSMultiBoundAABB`, `BSMultiBoundOBB`,
`NiDefaultAVObjectPalette`, `NiStringPalette`.

## NIF→ECS import

[`crates/nif/src/import/`](../../crates/nif/src/import/) takes a parsed
`NifScene` and walks it into a flat list of ECS-friendly meshes (the raw
`Imported*` tier — see [NIFAL](nifal.md) for how this is then resolved to
the canonical, game-agnostic types the engine consumes). The
`Imported*` structs live in
[`import/types.rs`](../../crates/nif/src/import/types.rs). Key
transformations:

- **Z-up → Y-up coordinate change** with the documented CW→CCW rotation
  conversion (see [Coordinate System](coordinate-system.md))
- **SVD-based rotation repair** for degenerate NIF rotation matrices
  (some legacy content has skewed/sheared transforms — `nalgebra`'s SVD
  finds the closest valid rotation). The repair now happens once at parse
  time in [`rotation.rs`](../../crates/nif/src/rotation.rs) (#277)
- **Editor marker filtering** by name prefix (`marker_*`, `editor_*`,
  light effect FX meshes, fog volumes) plus the Skyrim+ `EditorMarker`
  flag bit and the `MapMarker` NiNode subclass (#165)
- **Material property extraction** in one walk
  ([`import/material/`](../../crates/nif/src/import/material/)): diffuse
  texture, normal map, alpha flags, decal flags, emissive/specular/
  glossiness, UV transform, two-sided flag, all 8 TXST slots (#357), and
  the FO4 BGSM/BGEM PBR / translucency / model-space-normals flags
  (#1076/#1077)
- **Strip-to-triangle conversion** for `NiTriStripsData`
- **Tangent synthesis** — empty BSGeometry / SSE-reconstructed tangents
  route through Mikkelsen Y-up synthesis (#1086 / #1204 / #1232)
- **Collision import** ([`import/collision.rs`](../../crates/nif/src/import/collision.rs))
  with the Havok→engine transform (via `import_nif_with_collision`)
- **Depth + cycle guards** on the import walkers (#1269) so malformed
  scene graphs can't blow the stack or loop forever

The output is `Vec<ImportedMesh>` plus optional `Vec<ImportedCollision>` /
`Vec<ImportedLight>` / particle emitters. Each `ImportedMesh` has
positions / normals / UVs / vertex colors / indices / a `glam::Quat`
rotation / a `glam::Vec3` translation / a scale, plus the texture path
(so the consumer can extract DDS bytes from a BSA), plus material flags.
The cell loader in `byroredux/src/cell_loader/` consumes this directly.

## Stream reader

[`stream.rs`](../../crates/nif/src/stream.rs) is the version-aware binary
reader. It wraps a byte cursor with the parsed NIF header and exposes
**version-dependent** reads:

```rust
let s: Option<Arc<str>> = stream.read_string()?;     // string-table index OR inline
let r: BlockRef = stream.read_block_ref()?;          // i32 with -1 = null
let t: NiTransform = stream.read_ni_transform()?;    // 13 floats: rot + pos + scale
let b: bool = stream.read_bool()?;                   // u32 NiBool OR u8 inline
let bb: bool = stream.read_byte_bool()?;             // always 1-byte (NiGeometryData flags)
```

`read_string` is the most consequential one: 20.1+ files use a global
string table indexed by u32, older files use length-prefixed inline
strings. Returning `Arc<str>` makes the table-indexed path a cheap atomic
clone instead of a fresh allocation per read — the per-file allocation
count for a typical Skyrim NIF dropped by ~40× when this landed (issue
#55). `allocate_vec(count)` is the OOM-hardened vector allocator: every
stream-derived `Vec::with_capacity` routes through it so a corrupt count
bounds-checks against the remaining file budget instead of aborting the
process (#388 / #408 / #1245 / #1246).

## Test infrastructure

- **~738 in-crate unit tests** with synthetic byte streams covering every
  parser, including the **67-test `blocks::dispatch_tests`** suite (split
  into per-topic siblings — `nodes` / `shader` / `effects` / `controllers` /
  `interpolators` / `extra_data` / `havok` / `starfield`) that drive every
  audit block through `parse_block` on a minimal Oblivion-shaped header and
  assert exact stream consumption — so any future byte-width or version-gate
  drift fails fast on the block-sizes-less Oblivion path. Per-category
  `*_tests.rs` siblings live next to their parsers (tri_shape, collision,
  extra_data, interpolator, properties, shader, material, mesh).
- **Integration sweeps** in [`crates/nif/tests/`](../../crates/nif/tests/):
  [`parse_real_nifs.rs`](../../crates/nif/tests/parse_real_nifs.rs)
  (per-game parse-rate sweeps),
  [`per_block_baselines.rs`](../../crates/nif/tests/per_block_baselines.rs)
  (checked-in per-block histograms, regenerated on coverage changes),
  [`heap_allocation_bounds.rs`](../../crates/nif/tests/heap_allocation_bounds.rs)
  (dhat-gated allocation-bound regression, #1247),
  [`translation_completeness.rs`](../../crates/nif/tests/translation_completeness.rs)
  (#1277 Task 8 — how much of each parsed scene survives the NIF→ECS
  translation boundary), and
  [`mtidle_motion_diagnostic.rs`](../../crates/nif/tests/mtidle_motion_diagnostic.rs).
  Real-data sweeps skip cleanly when a game's `BYROREDUX_*_DATA` dir is unset.
- **`nif_stats` example binary** at [`crates/nif/examples/nif_stats.rs`](../../crates/nif/examples/nif_stats.rs)
  for manual sweeps — accepts a single `.nif`, a directory, or a `.bsa` /
  `.ba2` archive, prints total/ok/fail counts, a block-type histogram,
  and grouped failure messages with example file paths. Companion examples
  include `dump_transforms` (NIF rotation-matrix fidelity measurement),
  `emitter_dump` (particle emitter rate / radius / speed / life), and
  `material_dump`.

Run a per-game sweep:

```bash
cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored
```

Run `nif_stats` against a single archive:

```bash
cargo run -p byroredux-nif --example nif_stats --release -- \
    "/path/to/Fallout - Meshes.bsa"
```

See [Game Compatibility](game-compatibility.md) for the live per-game
parse rate matrix.

## Reference materials

- [`docs/legacy/nif.xml`](../legacy/nif.xml) — niftools' authoritative NIF
  format spec. Almost every parser cross-references this.
- nifly — the niftools fork with Starfield read/write support, cloned
  outside the repo at `/mnt/data/src/reference/nifly`; the authority for
  the `BSGeometry` wire layout (`src/Geometry.cpp`) and the early-Gamebryo
  `groupID` field (`include/BasicTypes.hpp`)
- [`docs/legacy/api-deep-dive.md`](../legacy/api-deep-dive.md) — class
  hierarchy of `NiObject`/`NiAVObject`/`NiStream` and how the legacy
  serializer worked
- [Gamebryo 2.3 Architecture](../legacy/gamebryo-2.3-architecture.md)
  for the original engine context
- [NIFAL — NIF Abstraction Layer](nifal.md) — the canonical translation
  tier the import layer feeds

## N26 audit — Oblivion coverage sweep

After the N23 series closed the per-game parse rate at 100% on the
mesh archives we had on disk, a second audit (N26) walked `nif.xml`
against the dispatch table to find block types that were parsing
"well enough" on the archives we'd tested but would hard-fail on
Oblivion content the integration sweep didn't cover (every Oblivion
interior, every magic FX, every Oblivion cinematic). Oblivion's
v20.0.0.5 header has no `block_sizes` table, so a single missing
dispatch arm takes down the entire mesh. The audit landed 9 PRs
addressing every known critical / high-severity gap:

| # | Issue | Block types added |
|---|-------|-------------------|
| #145 | Oblivion specialized BS shader variants (Sky / Water / TallGrass / Lighting30 / Tile / Hair / VolumetricFog / DistantLOD / BSDistantTree / BSSky / BSWater) — all alias `BSShaderPPLightingProperty` | 11 |
| #144 | `NiKeyframeController` + `NiSequenceStreamHelper` — pre-Skyrim KF animation root + per-bone driver | 2 |
| #164 | `NiStringsExtraData` + `NiIntegersExtraData` — array-form extra data | 2 |
| #142 | `NiBillboardNode`, `NiSwitchNode`, `NiLODNode`, `NiSortAdjustNode`, BSRangeNode family, plus 3 NiNode pure-aliases | 13 |
| #156 | Full `NiLight` hierarchy (ambient / directional / point / spot) with downstream `LightSource` ECS wiring | 4 |
| #154 | `NiUVController` + `NiUVData` — scrolling UV animation | 2 |
| #153 | Embedded `NiCamera` — cinematic frustum + viewport | 1 |
| #163 | `NiTextureEffect` — projected env-map / gobo / fog projector | 1 |
| #143 | Legacy (pre-NiPSys) particle stack — `NiParticleSystemController` + 7 leaf modifiers + `NiAutoNormal/Rotating Particles` + data | 13 |

Every audit fix comes with a `dispatch_tests` regression test that
asserts exact stream consumption on a minimal Oblivion-shaped payload.
At the time of the N26 closeout the dispatch table held 154 arms; it has
since grown to 254 arms / 310 distinct type names through the FO4 / FO76 /
Starfield coverage work below.

## Per-game NIF coverage (Oblivion → Starfield)

Live numbers in [Game Compatibility](game-compatibility.md); summary as of
the 2026-04 sweeps:

| Game | NIF clean rate | Recoverable | Notes |
|------|----------------|-------------|-------|
| Oblivion | 96.24% (7 730 / 8 032) | 99.99% | `block_sizes`-less; remaining ~149 NetImmerse-era files + 1 corrupt-by-design debug marker (#687 / #688 / #698 closed) |
| Fallout 3 | 100% (10 989) | 100% | shared FNV parser |
| Fallout NV | 100% (14 881) | 100% | reference title |
| Skyrim SE | 100% (18 862) | 100% | BSTriShape packed-vertex format |
| Fallout 4 | 96.46% (33 757 / 34 995) | 100% | FaceGen NIFs dominate the truncation tail (1 235 / 1 238) |
| Fallout 76 | 97.34% (56 915 / 58 469) | 100% | CRC32 shader flag arrays |
| Starfield | 98.6% aggregate (5 archives) | 100% | BSGeometry / SkinAttach / BoneTranslations dispatch (#708, #754 BSWeakReferenceNode) |

The two big bring-ups since the N26 era were:

- **FO4 / FO76 / Skyrim SE** — `BSTriShape` and its four wire-distinct
  subclasses (LOD / MeshLOD / SubIndex / Dynamic), the packed-vertex
  `vertex_desc` bitfield decode, the 8-variant `BSLightingShaderProperty`
  split (#1279), `BGSM`/`BGEM` material-path surfacing, and the FO76+
  CRC32 shader-flag arrays.
- **Starfield** — went from `no parser` to a walkable Cydonia interior:
  `BSGeometry` external `.mesh` resolution (#1292), the
  `geometries\<hash>.mesh` canonical path, `BSWeakReferenceNode` (#754),
  `BSFaceGenNiNode` coverage stub (#727), and the FO76-shared shader
  vocabulary. Per-LOD per-material sub-decomposition trips the cell
  loader's `base_layer` static-trimesh gate (#1294).

## Session 11 closeout — audit bundle #341–#438

A 72-commit bug-bash on top of the session-10 audit bundle. No new milestones
landed; the focus was paying down every known CRITICAL/HIGH issue surfaced by
the `/audit-nif` and `/audit-renderer` sweeps.

**Parser correctness (Oblivion / v20.0.0.5 stability):**
- `#324` — runtime size cache prevents cascade failure when one block's parser
  under-reads on Oblivion (no per-block size table to resync).
- `#395` — stream-drift detector for Oblivion NIF parses emits a warning with
  the first block that goes off the rails, so drift is measurable rather than
  an unexplained parse-rate regression.
- `#429` — gate `NiTexturingProperty` normal + parallax slots on v20.2.0.5+;
  older files have no normal slot and the previous unconditional reads shifted
  the stream past the real trailing fields.

**Import path correctness (what ends up on `ImportedMesh` / `ImportedLight`):**
- `#106` — `BSBehaviorGraphExtraData.controls_baseline_level` is a bool (1 byte),
  not a u32 (4 bytes). Would shift every block after it on FO3/FNV rigs with
  behaviour graphs.
- `#128` — `BsTriShape.two_sided` lookup also checks `BSEffectShaderProperty`
  (was LightingShader only) so foliage / particles respect the no-cull flag.
- `#165` — editor culling now catches the Skyrim+ `EditorMarker` flag bit on
  top of name-prefix matching, plus the `MapMarker` NiNode subclass.
- `#166` — `BSEffectShaderProperty.emissive_*` renamed to
  `base_color`/`base_color_scale` to match nif.xml semantics — the field is the
  tint modulating the base texture, not an emissive added on top.
- `#335` — `NiDynamicEffect.affected_nodes` pointer list now surfaces on
  `ImportedLight`, so the import layer can honour per-light affect-set scoping
  instead of flooding every fragment.
- `#341` — `BSDynamicTriShape` import path now extracts vertices (was dropping
  every Skyrim NPC head / face mesh silently).
- `#344` — `material_kind` now flows through `ImportedMesh` → `Material` so the
  RT path can route metal / glass / translucent distinctly.
- `#346` — `BsTriShape` import reads `BSEffectShaderProperty` in addition to
  `BSLightingShaderProperty` (previously only the lighting path was wired).
- `#357` — extract **all 8 TXST texture slots**, not just TX00 (diffuse).
  Normal, glow, parallax, cubemap, env mask, multilayer, and specular now
  land on `ImportedMesh` for FO4 architecture.
- `#358`/`#359` — `VF_INSTANCE` constant introduced and `BSTriShape.data_size`
  is sanity-checked against `num_vertices * vertex_size`.
- `#363`/`#364` — `BSTreeNode` bones and `BSRangeNode` discriminator now surface
  on the import scene (Skyrim SpeedTree wind-bone lists + FO4 LOD range tags).
- `#369` — surface VMAD script attachments via a `has_script` flag.
- `#381` — NIF import cache promoted from per-cell to a **process-lifetime
  resource**. Each unique mesh is now parsed + imported exactly once per
  process, eliminating the O(cells × unique meshes) re-parse on exterior
  grid streaming.
- `#401` — end-to-end CPU particle system so torches + FX are visible.
  Previously the particle parsers landed but the import → render plumbing
  was incomplete.
- `#403` — widen `BSLightingShaderProperty.wetness.unknown_1` read to BSVER
  ≥ 130 (was gated too narrowly).
- `#407` — parse FO4 `NiParticleSystem` + `BS*ShaderProperty` controllers that
  were falling through to the generic skip path.

**Robustness:**
- `#383` — catch missing trailing fields on FNV particle blocks (Bethesda
  ships NIFs with the declared fields absent; hard-fail was wrong).
- `#388` — bound file-driven `Vec::with_capacity` against the remaining stream
  budget so a corrupted count field can't OOM the process.

Dispatch table is unchanged; this session was all semantic corrections and
import-layer follow-through.

## Session 12 closeout — 2026-04 audit sweep

Second bug-bash driven by `AUDIT_FO3_2026-04-19.md` and
`AUDIT_FNV_2026-04-20.md`. Focus: latent correctness issues behind
`block_sizes` recovery — cases where parse stayed at 100% but the
structured data landed zero-initialised, wrong, or in the wrong field.

**Parser correctness:**
- `#408` — blanket sweep: every stream-derived `Vec::with_capacity(N)`
  routed through `stream.allocate_vec(N)?`. 60+ sites across 12 files
  plus inline byte-budget guards on the pre-stream header reader.
  Subsumes #388. A malicious or drifted u32/u16 count now returns
  `InvalidData` instead of abort.
- `#440` / NIF-010 — `NiGeometryData.dataFlags` bit decode now splits
  on `bsver > 0 && version == 20.2.0.7`. Bethesda `BSGeometryDataFlags`
  uses **bit 0** as `Has UV` (0 or 1 UV set) and **bit 12** as
  `Has Tangents`; non-Bethesda `NiGeometryDataFlags` is the Gamebryo
  layout with **bits 0-5** as a 6-bit UV count and **bits 12-15** as
  an NBT method enum. Pre-fix every Bethesda stream used the Gamebryo
  decode, so a FO3 FaceGen head with `data_flags = 0x1003` asked for 3
  UV sets when only 1 was serialised — the 20,912-byte over-read then
  blew past EOF and demoted the `NiTriShapeData` to `NiUnknown`.
  `headfemalefacegen.nif` now parses clean.
- `#402` — Oblivion KF files: `NiControllerSequence` for v ∈
  `[10.1.0.113, 20.1.0.1)` trails a `Ref<NiStringPalette>` after
  `accum_root_name` per Gamebryo 2.3 source. Without it, every
  Oblivion KF drifted 4 bytes and `import_kf` returned zero clips
  across all 1843 files. Also added palette-backed string resolution
  to `import_sequence` for the offset-indexed ControlledBlocks.
  Measured impact on full FO3 KF corpus: `NiTransformData` went from
  3 → 40,623 parsed.
- `#455` — `TileShaderProperty` broke out of the aliased PPLighting
  dispatch and got its own parser.
- `#333` — `matrix3_to_quat` fast path now normalises its quaternion
  output. The determinant gate `|det - 1.0| < 0.1` admits matrices
  scaled ~3.5%, so Shepperd's formula produced non-unit quats on
  export-tool-drifted rotations; downstream `Quat::from_xyzw` doesn't
  normalise. Fix is 1 sqrt + 4 muls at the end of the helper.

**Import path correctness:**
- `#441` — removed the bogus `SF_DOUBLE_SIDED = 0x1000` check on FO3/FNV
  `BSShaderPPLightingProperty` + `BSShaderNoLightingProperty`. Verified
  against nif.xml: the Fallout3ShaderFlags enum has no Double_Sided bit
  — flags1 bit 12 is `Unknown_3` (crash bit), flags2 bit 4 is
  `Refraction_Tint`. Skyrim+/FO4 `SkyrimShaderPropertyFlags2` is where
  Double_Sided actually lives. FO3/FNV meshes use `NiStencilProperty`
  for backface control (Gamebryo-canonical mechanism).
- `#454` — factored shared `is_decal_from_shader_flags(flags1, flags2)`
  helper so PP / NoLighting / BSLighting decal detection stays in
  lockstep.
- `#452` — `BSShaderTextureSet` slots 3/4/5 (parallax height / env
  cubemap / env mask) now reach `MaterialInfo` on both PPLighting
  (FO3/FNV) and BSLighting (Skyrim+) paths.
- `#350` — Skyrim+ shader controllers preserve the controlled-variable
  enum (see Shaders section above).
- `#329` / `#330` — `read_extra_data_name()` with the `since=10.0.1.0`
  gate + a 3-way `NiExtraData::parse` version branch.

**Dispatch additions:**
- `#443` — `SCPT` pre-Papyrus bytecode records parse (1257 scripts in
  Fallout3.esm). `#442` / `#448` — `CREA` + `LVLC`. `#458` — WATR / NAVI /
  NAVM / REGN / ECZN / LGTM / HDPT / EYES / HAIR stub parsers.

## Session 13 → 42 — coverage, splits, and the canonical translation tier

The Session-12 closeout was the last point at which this doc was frozen.
The work since (Sessions 13–42, 2026-04 → 2026-05-28), reconstructed from
git history:

**FO4 / FO76 / Starfield coverage.** `BSTriShape` and its four wire-distinct
subclasses landed (`BsTriShapeKind`, #560 / #404), surfaced on `ImportedMesh`
(#1206 / #1207); the FO4 generic `"NiExtraData"` arm closed FaceGen truncation
(#1073); the FO76 `BSEffectShaderProperty` quintet is captured onto
`BsEffectShaderData` (#1205); Starfield `BSGeometry` parses external `.mesh`
refs and inline UDEC3 payloads (with the `geometries\<hash>.mesh` canonical
resolution, #1292), iterating every LOD slot (#1209) and routing empty
tangents through Y-up synthesis (#1232), with `BSWeakReferenceNode` (#754)
and the `BSFaceGenNiNode` coverage stub (#727). The `BSLightingShaderProperty`
parser was split into per-variant dispatch (#1279) and now captures the
`.mat` / Root-Material sidecar (#976 / #1183) and the PBR / backlight scalars
(#1175 / #1241).

**Session 35/36 submodule splits.** The >1500-LOC monoliths were broken into
directories: `blocks/tri_shape.rs` → `tri_shape/` (#1118 TD9-005, agd / bs_tri_shape /
ni_tri_shape), `blocks/collision.rs` → `collision/` (9 production + test siblings),
`blocks/controller.rs` → `controller/` (legacy / sequence / morph / shader),
`import/mesh.rs` → `mesh/`, `import/material.rs` → `material/` (walker + shader_data),
`import/walk.rs` → `walk/`, and `anim.rs` → `anim/`. The `Imported*` structs moved
to `import/types.rs`, the SVD rotation repair to the top-level `rotation.rs` (#1044),
and the lib.rs recovery tests to a sibling `tests.rs` (#1118 TD9-002). New
top-level files: `shader_flags.rs` (named flag constants + typed variant view,
#1277 Tasks 5/6), `kfm.rs` (KFM state-machine parser).

**Tech-debt & robustness.** `impl_ni_object!` macro + `read_array_of` combinator
(#1043), named NIF version / BSVER literals (#1042), depth + cycle guards on the
import walkers (#1269), `#[must_use]` on the `read_pod_vec` wrappers + KFM
`allocate_vec` (#1245 / #1246), a dhat-gated allocation-bound regression test
(#1247), and a NIF-parse perf pass (BSGeometry bulk-read + `mem::take` tangents,
Arc<str> dispatch regression fix, rayon serial fast path — #1261 / #1262 /
#1263 / #1265).

**NIFAL — NIF Abstraction Layer (this session, 2026-05-28).** The #1277 epic
formalised the long-standing "translate at the parser→Material boundary"
directive into the **NIF Abstraction Layer**: every game's raw `Imported*`
decode is resolved to one game-agnostic canonical representation through a
single `translate()` boundary, consumed identically downstream (full design
in [`docs/engine/nifal.md`](nifal.md)). Slices landed this session:

- **Material slice** (the reference template) — ECS `Material.metalness` /
  `roughness` resolved to plain clamped `f32` at translate time; the
  `Option<f32>` "resolve-later" overrides + per-draw `classify_pbr` fallback
  removed. BGSM spec-glossiness → metallic-roughness translation (#1031 area),
  BGEM `glass_enabled` as the authoritative glass signal, `EmissiveSource`
  discriminator (#1280).
- **Particle slice** — `NiPSysEmitter` promoted to a typed block carrying
  decoded `EmitterBaseParams`; authored speed / declination / life now
  override the name-heuristic preset, authored birth rate is read from the
  `NiPSysEmitterCtlr` interpolator chain, and authored size from the
  `NiPSysGrowFadeModifier` `base_scale`. (Size-over-life *curve* and
  per-emitter attribution are noted follow-ups.)
- **Node-passthrough triage** — node leaks classified.
- **Collision audit** — `BhkMultiSphereShape` now translates to a `Compound`
  of `Ball` children (single centred sphere unwraps to a plain `Ball`), and
  `BhkConvexListShape` to a `Compound` of resolved convex sub-shapes; all 13
  translatable parsed `bhk*Shape` variants now reach `CollisionShape`. The
  remaining non-leaks (`BhkNPCollisionObject` Havok blobs, `BhkPCollisionObject`
  phantoms) are documented limitations, not gaps.
- **Emissive-scale measurement** — the three candidate emissive sources were
  ground-truth measured and the proposed normalization resolved as a no-op.

Per-game **recoverable** parse rates stayed at 100% throughout (clean rates
per the [Game Compatibility](game-compatibility.md) matrix).

## Open items

The N23 series is complete (10/10 milestones) and N26 has addressed
every known CRITICAL / HIGH audit item. Known follow-ups:

- `BSSubIndexTriShape` segment data (`BSGeometrySegmentData`,
  `BSGeometrySegmentSharedData`) — surfaced only where the renderer needs
  per-segment metadata. Tracked under N23.9.
- **Particle size-over-life curve + per-emitter attribution** (NIFAL
  particle-slice follow-up) — only the authored *magnitude* of grow/fade
  size is translated today; the bell-shaped curve needs a richer canonical
  size model, and multi-emitter NIFs are attributed scene-first rather than
  per-emitter.
- **NiFlipController GPU sample** (#545 follow-up) — channel data is
  captured into `AnimationClip::texture_flip_channels`; the renderer-side
  sample-and-bind that drives `GpuInstance.albedo_texture` is deferred.
- **NiLight FO4+ inheritance flip** (#156 follow-up) — FO4+ (BSVER
  ≥ 130) reparents `NiLight` directly onto `NiAVObject`. Not implemented
  until FO4 light rendering becomes a target.
- **Per-variant shader specialization** (#145 follow-up) —
  `SkyShaderProperty`, etc. still alias the `BSShaderPPLightingProperty`
  base so Oblivion doesn't hard-fail; their per-variant fields (sky scroll,
  etc.) are not yet extracted. `WaterShaderProperty` now reaches a
  consumer (#1243).
- **Havok constraint data** (#331, deferred) — the constraint family parses
  for byte-alignment but pivot/axis/limit data has no consumer until physics
  wiring lands.
- **`BhkNPCollisionObject` / `BhkPCollisionObject`** — FO4+ Havok-serialised
  collision blobs need a separate decoder; phantoms need a `TriggerVolume`
  ECS path. The cell loader falls back to synthesized static trimesh.
- **`BSFaceGenNiNode` morph data** (#727) — aliased as a coverage-first NiNode
  stub; FaceGen coefficient bytes are skipped via `block_size`. A dedicated
  parser is follow-up once a sample face NIF is reverse-engineered.
