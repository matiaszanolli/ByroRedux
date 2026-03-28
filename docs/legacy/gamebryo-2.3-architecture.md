# Gamebryo 2.3 Architecture Reference

Source: `/media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/`

Used in: The Elder Scrolls IV: Oblivion, Fallout 3, Fallout: New Vegas

## Directory Structure

```
Gamebryo_2.3/
├── CoreLibs/          (11MB — core engine)
│   ├── NiMain/        (245 headers — scene graph, rendering, materials)
│   ├── NiAnimation/   (107 headers — controllers, interpolators, keys)
│   ├── NiCollision/   (39 headers — OBB trees, raycasting)
│   ├── NiParticle/    (particle systems)
│   ├── NiDX9Renderer/ (76 headers — DirectX 9 backend)
│   ├── NiD3D10Renderer/ (45 headers — D3D10 backend)
│   ├── NiFloodgate/   (task-based parallelism)
│   ├── NiPortal/      (portal/room visibility)
│   ├── NiSystem/      (40 headers — memory, threading, I/O)
│   └── NiAudio/       (Miles Sound System wrapper)
├── SDK/               (97MB — 1,592 public headers + libs)
├── AppFrameworks/     (3.6MB — NiApplication, NiEntity, NiInput, NiFont, NiUI)
├── ToolLibs/          (16MB — Max/Maya exporters, 60+ viewer/converter tools)
└── ThirdPartyCode/    (73MB — Bison, Flex, LibPNG, Miles, OpenEXR, TinyXML, ZLib)
```

## Core Object Hierarchy

```
NiRefObject                          (reference counting)
└── NiObject                         (base streamable object, RTTI, cloning)
    ├── NiObjectNET                  (name + extra data attachments)
    │   └── NiAVObject               (transforms, properties, dynamic effects, bounds)
    │       ├── NiNode               (children, hierarchical transforms)
    │       │   ├── NiBillboardNode
    │       │   ├── NiBSPNode
    │       │   ├── NiLODNode
    │       │   └── NiSwitchNode
    │       └── NiGeometry           (renderable mesh, skin instance, material)
    │           ├── NiTriShape       (indexed triangles)
    │           ├── NiTriStrips      (triangle strips)
    │           └── NiLines
    ├── NiProperty                   (rendering state)
    │   ├── NiAlphaProperty
    │   ├── NiTexturingProperty
    │   ├── NiMaterialProperty
    │   ├── NiZBufferProperty
    │   ├── NiStencilProperty
    │   └── ... (12 property types total)
    ├── NiExtraData                  (arbitrary user data)
    │   ├── NiStringExtraData
    │   ├── NiFloatExtraData
    │   ├── NiBinaryExtraData
    │   └── ... (10+ types)
    ├── NiTimeController             (animation base)
    │   ├── NiTransformController
    │   ├── NiFloatController
    │   ├── NiGeomMorpherController
    │   └── ... (20+ controllers)
    └── NiInterpolator               (value interpolation)
        ├── NiFloatInterpolator
        ├── NiPoint3Interpolator
        ├── NiQuaternionInterpolator
        ├── NiBlendInterpolator      (animation blending)
        ├── NiBSplineInterpolator    (B-spline curves)
        └── ... (10+ interpolators)
```

## Key Architectural Patterns

### 1. Reference Counting & Smart Pointers
- `NiSmartPointer(ClassName)` macro creates `ClassNamePtr` typedef
- All NiObject subclasses are ref-counted via NiRefObject
- No manual delete — pointers automatically release

### 2. RTTI System
- `NiDeclareRTTI` / `NiImplementRTTI(Class, Base)` macros
- `NiIsKindOf(Class, obj)` — runtime type checking with inheritance
- `NiDynamicCast(Class, obj)` — safe runtime casting
- Tied into the streaming system for deserialization

### 3. Scene Graph
- Hierarchical tree: NiNode contains children (NiAVObject*)
- Each NiAVObject has local + world transforms
- `Update(time)` cascades: parent→children (transforms down), children→parent (bounds up)
- Properties inherit down the tree unless overridden

### 4. Property System
- NiProperty subclasses attached to NiAVObject nodes
- 12 property types control rendering state (alpha, texturing, material, zbuffer, etc.)
- NiPropertyState aggregates active properties for a node
- Properties propagate down the scene graph

### 5. Streaming/Serialization (NIF Format)
Three-phase loading:
1. **LoadBinary** — deserialize raw data
2. **LinkObject** — resolve object references by 32-bit ID
3. **PostLinkObject** — finalize relationships

Each streamable class declares:
- `NiDeclareStream` macro → `LoadBinary()`, `SaveBinary()`, `LinkObject()`, `RegisterStreamables()`, `PostLinkObject()`, `CreateObject()`

### 6. Animation System
- **NiTimeController** — base, attached to NiAVObject or properties
- **NiInterpolator** — produces values (float, vec3, quat, color, transform)
- **NiControllerManager** — state machine for sequences
- **NiControllerSequence** — named animation clip
- **NiKFMTool** — manages .kfm files (sequences, transitions, blend pairs)
- Supports: blend, morph, crossfade, chain transitions

### 7. Floodgate (Parallel Processing)
- Task-based parallelism (not ECS)
- NiSPTask, NiSPKernel, NiSPWorkflow
- Used for particle updates, skinning, morphing

## NIF File Format

```
[ASCII header]: "Gamebryo File Format, Version X.X.X"
[u32]: version (e.g. 0x14020007 = 20.2.0.7)
[u8]:  little_endian flag
[u32]: user_version (game-specific)
[u32]: object_count
[...]: serialized object blocks
[...]: RTTI string table (class names)
[...]: fixed string table (global string pool)
[...]: object size table
[...]: object groups (for deferred loading)
```

Versions: min 20.0.0.3, max 34.1.1.3 in this codebase.

## Related File Formats

- **.nif** — NetImmerse File (scene graph with meshes, materials, animation)
- **.kf** — Key Frame file (animation controller sequences)
- **.kfm** — Key Frame Master (animation state machine: sequences, transitions, sync groups)

## Compatibility Mapping: Gamebryo → Redux

| Gamebryo Concept | Redux Equivalent | Notes |
|---|---|---|
| NiObject (base) | — | No single base needed; ECS replaces hierarchy |
| NiAVObject (transforms) | Transform component (PackedStorage) | Hot-path, read every frame |
| NiNode (children) | Parent/Children components | Sparse, structural |
| NiProperty (render state) | Material/RenderState components | Or per-pipeline state |
| NiTimeController | System + DeltaTime resource | Systems replace controller pattern |
| NiInterpolator | Animation data in components | Keyframe data as component storage |
| NiFixedString | FixedString (string-interner) | Already implemented |
| NiStream | NIF loader module (future) | Parse binary format into ECS entities |
| NiRefObject (ref counting) | Rust ownership / Arc | Language-level, not engine-level |
| NiRTTI | TypeId / Component trait | Rust's type system replaces RTTI |
| NiSmartPointer | Rc/Arc or owned values | No manual ref counting needed |
| Floodgate (parallelism) | Scheduler + rayon (future) | RwLock-per-storage already supports it |
| NiRenderer | VulkanContext | Already implemented (ash) |
| NiTexturingProperty | Texture binding in render pass | Vulkan descriptor sets |
