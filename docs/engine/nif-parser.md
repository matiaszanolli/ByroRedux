# NIF Parser

NIF is the binary mesh format used by every Bethesda Gamebryo and
Creation engine game. The `byroredux-nif` crate parses every supported
game's mesh archive at **100% success across the full archive sweep**
(177,286 NIFs total — see [Game Compatibility](game-compatibility.md)).
This document explains how that's organised and what each piece does.

Source: [`crates/nif/src/`](../../crates/nif/src/)

## At a glance

| | |
|---|---|
| Block types parsed       | ~180 (+30 Havok types skipped via `block_size`) |
| Distinct type names      | 210+ |
| Game variants supported  | 8 (Morrowind → Starfield) |
| Tests (unit)             | 128 (includes 10 in the `dispatch_tests` module) |
| Integration sweeps       | 7 games, 100% each |
| Cumulative NIFs parsed   | 177,286 (full mesh archive sweeps) |

## Module map

```
crates/nif/src/
├── lib.rs            Top-level parse_nif() walker + per-block recovery loop
├── version.rs        NifVersion (packed u32) + NifVariant feature flags
├── header.rs         NIF header parser (BSStreamHeader, block type table, strings)
├── stream.rs         NifStream — version-aware binary reader
├── types.rs          NiPoint3, NiMatrix3, NiTransform, NiColor, BlockRef
├── scene.rs          NifScene container with downcast helpers (get_as<T>)
├── anim.rs           KF animation file import (ImportedClip, channels)
├── blocks/           Per-block parsers (one file per category)
│   ├── mod.rs        parse_block dispatcher (190+ entries) + NiObject trait
│   ├── traits.rs     HasObjectNET, HasAVObject, HasShaderRefs upcast traits
│   ├── base.rs       Shared base-class data structs (NiObjectNETData, ...)
│   ├── node.rs       NiNode + BS variants (BSFadeNode, BSValueNode, BsRangeNode,
│   │                 NiBillboardNode, NiSwitchNode, NiLODNode, NiSortAdjustNode,
│   │                 NiCamera, ...)
│   ├── tri_shape.rs  NiTriShape, NiTriStrips, BSTriShape (FO4+ packed format),
│   │                 shared parse_geometry_data_base helper for particle data
│   ├── shader.rs     BSShaderPP / BSLightingShader / BSEffectShader (8 ST variants, FO76 stopcond)
│   ├── light.rs      NiLight hierarchy (ambient, directional, point, spot)
│   ├── properties.rs Material, Alpha, Stencil, Texturing, ZBuffer, VertexColor, ...
│   ├── texture.rs    NiSourceTexture, NiPixelData, NiTextureEffect (projector)
│   ├── controller.rs NiTimeController + 14 subclasses, NiControllerManager,
│   │                 NiUVController, NiSequenceStreamHelper
│   ├── interpolator.rs NiTransform/Float/Point3/Bool interpolators + Blend
│   │                 variants, NiUVData
│   ├── extra_data.rs NiStringExtraData, BSXFlags, BSBound, BSDecalPlacement,
│   │                 NiStringsExtraData + NiIntegersExtraData (array variants), ...
│   ├── multibound.rs BSMultiBound + AABB/OBB shapes
│   ├── palette.rs    NiDefaultAVObjectPalette, NiStringPalette
│   ├── particle.rs   ~48 modern (NiPSys) particle system types
│   ├── legacy_particle.rs  Pre-NiPSys particle stack (Oblivion / Morrowind):
│   │                 NiParticleSystemController, NiAutoNormal/Rotating Particles,
│   │                 NiParticleColorModifier / GrowFade / Rotation / Bomb,
│   │                 NiGravity, NiPlanarCollider, NiSphericalCollider
│   ├── skin.rs       NiSkinInstance/Data/Partition, BSSkin, BsDismemberSkinInstance
│   └── collision.rs  bhk* collision shapes (rigid bodies, MOPP, CompressedMesh)
└── import/           NIF→ECS scene import
    ├── mod.rs        ImportedNode/Mesh/Scene types, import_nif()
    ├── walk.rs       Hierarchical + flat scene graph traversal
    ├── mesh.rs       NiTriShape + BSTriShape geometry extraction
    ├── material.rs   MaterialInfo, texture/alpha/decal property extraction
    ├── transform.rs  Transform composition, degenerate rotation SVD repair
    └── coord.rs      Z-up (Gamebryo) → Y-up (renderer) quaternion conversion
```

The split between `blocks/` (binary parsers) and `import/` (scene to ECS-friendly mesh) lets the parser be tested in isolation against bytes
without dragging in `glam` or any renderer types.

## Parse pipeline

`parse_nif(data)` in [`lib.rs`](../../crates/nif/src/lib.rs) is the
top-level entry point. It runs three phases:

1. **Header parse** ([`header.rs`](../../crates/nif/src/header.rs)) — read
   the ASCII header line, the binary version + endianness, the
   `BSStreamHeader` if present, the block-type table, the per-block size
   array (when the format has one), the global string table, and the
   group count.
2. **Block walk** — for each block index, look up its type in the header's
   block-type table, dispatch to the per-block parser in
   [`blocks/mod.rs`](../../crates/nif/src/blocks/mod.rs), and append the
   parsed block to the scene.
3. **Root identification** — find the first `NiNode` in the result and
   record it as the scene root.

The output is a `NifScene` containing a `Vec<Box<dyn NiObject>>`. Each
parsed block implements the `NiObject` trait, which exposes a type-name
string and an `as_any()` downcast for callers that want concrete types
(via `scene.get_as::<NiTriShape>(idx)`).

## Version handling

NIF files identify themselves with three pieces of metadata:

- A 4-byte packed `version` (`major << 24 | minor << 16 | patch << 8 | build`)
- A `user_version` (Bethesda BSStream marker, since v10.0.1.8)
- A `user_version_2` aka `BSVER` (game-specific Bethesda version, in `BSStreamHeader`)

The `(version, user_version, user_version_2)` triplet maps deterministically
to a [`NifVariant`](../../crates/nif/src/version.rs) enum:

```rust
pub enum NifVariant {
    Morrowind,    // NIF ≤ 4.x
    Oblivion,     // NIF 20.0.0.4 / 20.0.0.5, user_version < 11
    Fallout3NV,   // NIF 20.2.0.7, uv=11, uv2 ≤ 34
    SkyrimLE,     // uv=12, uv2=83
    SkyrimSE,     // uv=12, uv2=100
    Fallout4,     // uv=12, uv2=130–154
    Fallout76,    // uv=12, uv2=155–169
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
block parser".

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
of the "soft fail and keep going" philosophy:

```rust
match parse_block(type_name, &mut stream, block_size) {
    Ok(block) => {
        // Verify we consumed exactly block_size bytes; adjust if not.
        if let Some(size) = block_size {
            if stream.position() - start_pos != size as u64 {
                stream.set_position(start_pos + size as u64);
            }
        }
        blocks.push(block);
    }
    Err(e) => {
        if let Some(size) = block_size {
            // Recovery: seek past the broken block, insert a NiUnknown,
            // continue. A single buggy block parser doesn't kill the file.
            stream.set_position(start_pos + size as u64);
            blocks.push(Box::new(NiUnknown { type_name, data: Vec::new() }));
            continue;
        }
        // No block_size (Oblivion v20.0.0.5 NIFs): can't recover, stop here
        // but keep the blocks parsed so far.
        break;
    }
}
```

This is what bumped the Skyrim SE smoke test from 42% → 100% during
N23.10: a single Havok block layout quirk was killing every Skyrim NIF
that contained it, but the bytes per block were structurally fine.
Recovery is only available when `block_size` is known (FO3+), since
Oblivion v20.0.0.5 NIFs have no per-block size table — for those, the
walker stops at the first error but keeps the blocks parsed so far.

## Block coverage

Block types fall into a handful of families. Coverage summary:

### Nodes and geometry
`NiNode`, `BSFadeNode`, `BSLeafAnimNode`, `BSTreeNode`, `BSMultiBoundNode`,
`RootCollisionNode`, `BSOrderedNode`, `BSValueNode`, `NiTriShape`,
`NiTriStrips`, `BSSegmentedTriShape`, `BSTriShape`, `BSMeshLODTriShape`,
`BSSubIndexTriShape`, `NiTriShapeData`, `NiTriStripsData`.

### Node subtypes (N26 audit follow-up)
`NiBillboardNode` (camera-facing children, u16 mode since 10.1.0.0),
`NiSwitchNode` (u16 flags + u32 active index), `NiLODNode`
(inherits NiSwitchNode + NiLODData ref), `NiSortAdjustNode`
(transparency sorter override), `NiCamera` (embedded cinematic
frustum + viewport + lod_adjust for cutscene rigs),
`BsRangeNode` (BSRangeNode / BSBlastNode / BSDamageStage / BSDebrisNode
— identical `(min, max, current)` byte triple),
`AvoidNode` / `NiBSAnimationNode` / `NiBSParticleNode` (legacy NiNode
pure-aliases with no trailing fields). All gated on the per-version
layouts pulled straight from `nif.xml`.

### Shaders
- **FO3/FNV**: `BSShaderPPLightingProperty` (with refraction/parallax),
  `BSShaderNoLightingProperty`, `BSShaderTextureSet`
- **Oblivion specializations** (all alias to `BSShaderPPLightingProperty`
  since they share the base texture-set + flags layout — see #145):
  `SkyShaderProperty`, `WaterShaderProperty`, `TallGrassShaderProperty`,
  `Lighting30ShaderProperty`, `TileShaderProperty`, `HairShaderProperty`,
  `VolumetricFogShaderProperty`, `DistantLODShaderProperty`,
  `BSDistantTreeShaderProperty`, `BSSkyShaderProperty`, `BSWaterShaderProperty`
- **Skyrim+/FO4**: `BSLightingShaderProperty` (8 shader-type variants —
  EnvironmentMap, SkinTint, HairTint, ParallaxOcc, MultiLayerParallax,
  SparkleSnow, EyeEnvmap, None), `BSEffectShaderProperty`
- **FO76+/Starfield**: CRC32 flag arrays (`Num SF1` / `SF1[]` since BSVER ≥ 132,
  `Num SF2` / `SF2[]` since BSVER ≥ 152), `BSShaderType155` enum dispatch,
  `BSSPLuminanceParams`, `BSSPTranslucencyParams`, `BSTextureArray`, plus
  the **stopcond on `Name`** — when BSVER ≥ 155 and the Name field is a
  non-empty BGSM/BGEM file path, the rest of the block is absent and the
  parser short-circuits to a material-reference stub

### Properties (older games)
`NiMaterialProperty`, `NiAlphaProperty`, `NiTexturingProperty` (with bump
map / parallax fields), `NiStencilProperty` (version-aware), `NiZBufferProperty`,
`NiVertexColorProperty`, `NiSpecularProperty`, `NiWireframeProperty`,
`NiDitherProperty`, `NiShadeProperty`.

### Textures
`NiSourceTexture`, `NiPixelData`, `NiPersistentSrcTextureRendererData`,
`NiTextureEffect` (projected env-map / gobo / fog projector with full
NiDynamicEffect base, texture filtering / clamping / type / coord-gen
enums, clipping plane, and version-gated max anisotropy and PS2 L/K
fields — see #163).

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

### Controllers and interpolators
`NiTimeController`, `NiSingleInterpController`, `NiMaterialColorController`,
`NiMultiTargetTransformController`, `NiControllerManager`,
`NiControllerSequence`, `NiTextureTransformController`, `NiTransformController`,
`NiKeyframeController` (pre-Skyrim per-bone driver, aliases to
`NiSingleInterpController` — see #144), `NiVisController`, `NiAlphaController`,
`BSEffect/Lighting Shader Property {Float,Color}Controller`,
`NiGeomMorpherController`, `NiMorphData`, `NiUVController` +
`NiUVData` (scrolling UV animation for water / fire / banners — see #154),
`NiSequenceStreamHelper` (pre-Skyrim KF animation root).
Interpolators: `NiTransformInterpolator`, `BSRotAccumTransfInterpolator`,
`NiTransformData`/`NiKeyframeData`, `NiFloatInterpolator`, `NiFloatData`,
`NiPoint3Interpolator`, `NiPosData`, `NiBoolInterpolator`, `NiBoolData`,
`NiTextKeyExtraData`, plus the four `NiBlend*Interpolator` variants used
by `NiControllerManager` blending.

### Skinning
`NiSkinInstance`, `NiSkinData` (per-bone transforms + vertex weights),
`NiSkinPartition`, `BsDismemberSkinInstance`, `BSSkin::Instance`,
`BSSkin::BoneData`. (GPU skinning runtime is M29; the parser is done.)

### Particle systems (~48 types)
`NiParticles`, `NiParticleSystem`, `NiMeshParticleSystem`,
`BSStripParticleSystem`, `BSMasterParticleSystem`, plus
`NiParticlesData`/`NiPSysData`/`NiMeshPSysData`/`BSStripPSysData`/
`NiPSysEmitterCtlrData`, 18 modifiers, 5 emitters, 2 colliders, 6 field
modifiers, 21 controllers via shared base parsers.

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

### Havok collision (~30 types)
**Fully parsed** (since N23.6): `bhkCollisionObject`, `bhkRigidBody`,
`bhkSimpleShapePhantom`, `bhkMoppBvTreeShape`, `bhkBoxShape`,
`bhkSphereShape`, `bhkCapsuleShape`, `bhkCylinderShape`,
`bhkConvexVerticesShape`, `bhkListShape`, `bhkTransformShape`,
`bhkNiTriStripsShape`, `bhkPackedNiTriStripsShape`,
`hkPackedNiTriStripsData`, `bhkCompressedMeshShape`, `bhkCompressedMeshShapeData`.
**Skip-only** (deferred to M28 physics): the Havok constraint family and
collision systems — `bhkRagdollConstraint`, `bhkLimitedHingeConstraint`,
etc.

### Spatial / palettes
`BSMultiBound`, `BSMultiBoundAABB`, `BSMultiBoundOBB`,
`NiDefaultAVObjectPalette`, `NiStringPalette`.

## NIF→ECS import

[`crates/nif/src/import/`](../../crates/nif/src/import/) takes a parsed
`NifScene` and walks it into a flat list of ECS-friendly meshes. Key
transformations:

- **Z-up → Y-up coordinate change** with the documented CW→CCW rotation
  conversion (see [Coordinate System](coordinate-system.md))
- **SVD-based rotation repair** for degenerate NIF rotation matrices
  (some legacy content has skewed/sheared transforms — `nalgebra`'s SVD
  finds the closest valid rotation)
- **Editor marker filtering** by name prefix (`marker_*`, `editor_*`,
  light effect FX meshes, fog volumes — all the things that should never
  draw at runtime)
- **Material property extraction** in one walk: diffuse texture, normal
  map (BSShaderPPLighting FO3/FNV path), alpha flags, decal flags,
  emissive/specular/glossiness, UV transform, two-sided flag
- **Strip-to-triangle conversion** for `NiTriStripsData`
- **Collision import** with the Havok→engine transform (via `import_nif_with_collision`)

The output is `Vec<ImportedMesh>` plus an optional `Vec<ImportedCollision>`.
Each `ImportedMesh` has positions / normals / UVs / vertex colors /
indices / a `glam::Quat` rotation / a `glam::Vec3` translation / a scale,
plus the texture path (so the consumer can extract DDS bytes from a BSA),
plus material flags. The cell loader in `byroredux/src/cell_loader.rs`
consumes this directly.

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
#55).

## Test infrastructure

- **128 unit tests** with synthetic byte streams covering every parser,
  including the 10-test `blocks::dispatch_tests` module that drives
  every new N26 audit block through `parse_block` on a minimal
  Oblivion-shaped header and asserts exact stream consumption — so
  any future byte-width or version-gate drift fails fast on the
  block-sizes-less Oblivion path
- **8 integration tests** in [`crates/nif/tests/parse_real_nifs.rs`](../../crates/nif/tests/parse_real_nifs.rs)
  walking real game archives, asserting ≥95% parse success per game
- **`nif_stats` example binary** at [`crates/nif/examples/nif_stats.rs`](../../crates/nif/examples/nif_stats.rs)
  for manual sweeps — accepts a single `.nif`, a directory, or a `.bsa` /
  `.ba2` archive, prints total/ok/fail counts, a block-type histogram,
  and grouped failure messages with example file paths

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
  format spec (8563 lines). Almost every parser cross-references this.
- [`docs/legacy/api-deep-dive.md`](../legacy/api-deep-dive.md) — class
  hierarchy of `NiObject`/`NiAVObject`/`NiStream` and how the legacy
  serializer worked
- [Gamebryo 2.3 Architecture](../legacy/gamebryo-2.3-architecture.md)
  for the original engine context

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
The dispatch table is now at 154 arms covering ~180 block types.

## Open items

The N23 series is complete (10/10 milestones) and N26 has addressed
every known CRITICAL / HIGH audit item. Known follow-ups that
**don't** affect the 100% per-game parse rate:

- `BSSubIndexTriShape` segment data (`BSGeometrySegmentData`,
  `BSGeometrySegmentSharedData`) — currently skipped via `block_size`,
  not parsed. Only meaningful when the renderer surfaces per-segment
  metadata. Tracked under N23.9.
- ~~Starfield BA2 v3 DX10 textures~~ — resolved in session 7. The issue
  was a missing compression method field in the v3 header + LZ4 block
  compression. See [Archives — Resolved gaps](archives.md#resolved-gaps-session-7).
- **NiUV animation importer** (#154 follow-up) — `NiUVController` +
  `NiUVData` parse correctly but the `anim.rs` importer doesn't yet
  emit scrolling-UV channels. Needs new `FloatTarget::UvOffsetU/V` +
  `UvTileU/V` variants. Pure parser work was the blocker.
- **NiSequenceStreamHelper animation importer** (#144 follow-up) —
  parses cleanly but the `anim.rs` importer doesn't yet build
  `AnimationClip`s from its controller chain. Tracked alongside #107
  (Oblivion string-palette format for `NiControllerSequence`).
- **NiLight FO4+ inheritance flip** (#156 follow-up) — FO4+ (BSVER
  ≥ 130) reparents `NiLight` directly onto `NiAVObject`, skipping the
  `NiDynamicEffect` base. Not implemented until FO4 cell rendering
  becomes a target.
- **Per-variant shader specialization** (#145 follow-up) —
  `WaterShaderProperty`, `SkyShaderProperty`, etc. currently alias to
  the `BSShaderPPLightingProperty` base so Oblivion doesn't hard-fail,
  but their per-variant fields (sky scroll, water reflection, etc.)
  are not yet extracted.
- **Billboard-mode renderer wiring** (#142 follow-up) — `NiBillboardNode`
  now parses correctly but the renderer doesn't yet rotate the node
  to face the camera each frame.
- **NiLegacyParticlesData parse-rate validation** (#143 follow-up) —
  the parser is exercised by the real-NIF sweeps, but there's no
  byte-level unit test because it would require hand-building a full
  NiGeometryData body. Oblivion integration sweeps will catch any
  regression.
- Soft shadows / emissive bypass / RT lighting polish (M22+) — render-side,
  not parser-side.
