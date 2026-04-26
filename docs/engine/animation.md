# Animation

ByroRedux plays Bethesda `.kf` animation files on named ECS entities. The
pipeline is split between `byroredux-nif` (parsing the .kf binary into
generic clip data), `byroredux-core::animation` (the runtime — clip
registry, players, blending, interpolation), and the `animation_system`
in the binary crate (the per-frame tick that walks ECS players and
writes target transforms).

Source:
- Parser / import: [`crates/nif/src/anim.rs`](../../crates/nif/src/anim.rs)
- Runtime: [`crates/core/src/animation/`](../../crates/core/src/animation/)
- Conversion glue: [`byroredux/src/anim_convert.rs`](../../byroredux/src/anim_convert.rs)
- Per-frame system: [`byroredux/src/systems.rs`](../../byroredux/src/systems.rs)

## At a glance

| | |
|---|---|
| Animation files supported | Bethesda `.kf` (NIF v20.2.0.7 family) + NIF-embedded `NiControllerManager` |
| Controller types imported | NiTransformController, NiVisController, NiAlphaController, NiMaterialColorController, NiTextureTransformController, NiUVController, NiGeomMorpherController, NiFlipController (#545), BSEffect/BSLightingShaderProperty Float + Color (10+ types end-to-end) |
| Interpolation modes       | Linear, Quadratic (Hermite tangents), TBC (Kochanek-Bartels), Constant (step), XYZ Euler |
| Cycle modes               | Clamp, Loop, Reverse (ping-pong) |
| Blending                  | `AnimationStack` with weighted layers; per-channel priority from `ControlledBlock.priority` |
| Targeting                 | Pre-interned `FixedString` (no per-frame allocations — #340) |
| GPU skinning              | M29 Phase 1+2 done — compute pre-skin → per-skinned-entity BLAS refit (raster Phase 3 deferred behind M41) |

## Module map

```
crates/core/src/animation/
├── mod.rs            Public re-exports + AnimationClip top-level type
├── types.rs          CycleType, KeyType, key structs (Translation/Rotation/Scale/Float/Color/Bool),
│                     channels, AnimationClip
├── registry.rs       AnimationClipRegistry — Resource holding loaded clips by name
├── player.rs         AnimationPlayer — ECS Component, advance_time(), state machine
├── stack.rs          AnimationLayer, AnimationStack — weighted layer mixing
├── root_motion.rs    RootMotionDelta, split_root_motion() — extract delta from root channel
├── interpolation.rs  find_key_pair, hermite, TBC tangents, sample_translation/rotation/scale/...
└── text_events.rs    collect_text_key_events() — emit ECS marker components from text keys
```

## Pipeline overview

```
Bethesda .kf bytes
        │
        ▼  byroredux_nif::anim::import_kf()
ImportedKfClip
  ├── name
  ├── duration
  ├── channels: Vec<(target_node_name, channel kind, key data)>
  └── text_keys: Vec<(time, "<event_name>")>
        │
        ▼  byroredux::anim_convert::imported_to_clip()
byroredux_core::animation::AnimationClip
  ├── duration
  ├── channels: Vec<AnimationChannel>
  └── text_keys: Vec<TextKey>
        │
        ▼  AnimationClipRegistry::insert()
ECS resource AnimationClipRegistry
        │
        ▼  System::startup spawns AnimationPlayer { clip_name, ... }
ECS Component AnimationPlayer
        │
        ▼  byroredux::systems::animation_system runs per frame
        ▼  for each AnimationPlayer { player.advance_time(dt) }
        ▼  for each channel: sample_<kind>(time) and write to target entity
        ▼  if text key crossed: spawn marker component (TimerExpired etc.)
Target entity Transform / MaterialColor / VisibilityState updated
```

## Clip data model

`AnimationClip` is split by channel kind so the runtime can iterate
each kind under one lock instead of per-channel. Channel keys are
`FixedString` symbols pre-interned at clip-load time (#340).

```rust
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub cycle_type: CycleType,
    pub frequency: f32,
    pub weight: f32,                                                  // #469
    pub accum_root_name: Option<FixedString>,                         // root motion node
    pub channels: HashMap<FixedString, TransformChannel>,             // pos/rot/scale
    pub float_channels: Vec<(FixedString, FloatChannel)>,             // alpha, UV, shader floats, morph weights
    pub color_channels: Vec<(FixedString, ColorChannel)>,             // diffuse/ambient/specular/emissive/shader (#517)
    pub bool_channels: Vec<(FixedString, BoolChannel)>,               // visibility
    pub texture_flip_channels: Vec<(FixedString, TextureFlipChannel)>,// flipbook (#545)
    pub text_keys: Vec<(f32, String)>,
}

pub enum FloatTarget {
    Alpha,
    UvOffsetU, UvOffsetV, UvScaleU, UvScaleV, UvRotation,
    ShaderFloat,
    MorphWeight(u32),               // index into NiGeomMorpherController target list
}

pub enum ColorTarget {
    Diffuse,        // #517 — was a single AnimatedColor slot pre-fix
    Ambient,
    Specular,
    Emissive,
    ShaderColor,
}

pub struct TextureFlipChannel {
    pub texture_slot: u32,                 // raw TexType enum (0=BASE, 4=GLOW, ...)
    pub source_paths: Vec<Arc<str>>,       // resolved at clip-load from NiSourceTexture.filename
    pub keys: Vec<AnimFloatKey>,           // cycle position keys
}
```

Each transform key carries `(time, value, [tangents], [tbc])`. The
interpolation kernels in `interpolation.rs` handle each `KeyType`
variant: linear, quaternion SLERP, cubic Hermite spline, TBC weights,
and step (Constant). XYZ Euler rotations are decoded into a single
quaternion stream at parse time so the runtime stays uniform.

`text_keys` are timestamped strings ("attackend", "FootRight",
"BeginCastVoice", ...) that the animation system surfaces as transient
ECS marker components on the playing entity at the moment they're crossed.
The cleanup system drops the markers at end-of-frame.

## Player and state

```rust
pub struct AnimationPlayer {
    pub clip_name: String,
    pub time: f32,
    pub speed: f32,
    pub cycle: CycleType,
    pub paused: bool,
    pub finished: bool,
    pub last_text_key_index: usize,
}

impl AnimationPlayer {
    pub fn advance_time(&mut self, dt: f32, clip_duration: f32) -> f32 {
        // returns the new normalised time, with cycle handling
    }
}
```

The player is an ECS component on the **target NIF root entity**. The
animation system looks up the clip in the registry by `clip_name`, walks
its channels, and for each channel resolves the target entity by Name
(within the player's subtree, not globally) and writes the sampled value.
This means a single clip can drive a tree of entities through Name lookups
without needing pre-baked entity references.

The cell loader and the loose-NIF demo path both spawn an
`AnimationPlayer` on the root entity when `--kf <path>` is on the CLI.

## Blending: AnimationStack

For NIF files that use `NiControllerManager`, the parser surfaces multiple
clips at once with playback weights. The runtime supports a stack:

```rust
pub struct AnimationLayer {
    pub clip_name: String,
    pub player: AnimationPlayer,
    pub weight: f32,
}

pub struct AnimationStack {
    pub layers: Vec<AnimationLayer>,
}

pub fn advance_stack(stack: &mut AnimationStack, dt: f32, registry: &AnimationClipRegistry);
pub fn sample_blended_transform(stack: &AnimationStack, target: &str, registry: &AnimationClipRegistry) -> Option<NiTransform>;
```

`sample_blended_transform` walks every layer, samples its current value
for the target name, and weighted-averages translations + slerps rotations
toward the highest-weighted contributor. For two-layer blending (idle →
walk transition) this is the standard "lerp pose A and pose B" approach.

The blending stack is wired up via `NiBlendTransformInterpolator` /
`NiBlendFloatInterpolator` parsing in
[`crates/nif/src/blocks/interpolator.rs`](../../crates/nif/src/blocks/interpolator.rs).

## Root motion

[`root_motion.rs`](../../crates/core/src/animation/root_motion.rs)

Some animations carry "root motion" — a non-zero translation/rotation
delta on the skeleton's root bone meant to drive the entity's world
position rather than displace the mesh in place. `split_root_motion()`
walks a clip and extracts the per-frame root delta into a separate
`RootMotionDelta` channel that the gameplay layer can apply to the
entity's `Transform` instead of letting it fall through to the mesh.

The split happens against `clip.accum_root_name` — set on import from
the NiControllerSequence accum root, defaults to the skeleton root
otherwise. The character controller (M28 + M29) consumes
`RootMotionDelta` rather than re-deriving it per frame.

## Text key events as ECS markers

[`text_events.rs`](../../crates/core/src/animation/text_events.rs)

The animation system maintains a per-player `last_text_key_index` cursor.
On each tick, any text key whose time was crossed since the last frame
emits an ECS marker component on the player's entity:

| Text key | Marker component |
|---|---|
| `"attackend"` | `AttackEndEvent` |
| `"hit"` | `HitEvent` |
| `"FootLeft"` / `"FootRight"` | `FootstepEvent` (variant in event data) |
| `"BeginCastVoice"` | `VoiceCueEvent` |
| `"PickNewIdle"` | `PickNewIdleEvent` |

The script cleanup system drops these markers at end-of-frame so they're
strictly transient. See [Scripting](scripting.md) for the broader event
model and how scripts subscribe to markers without a VM.

## Coordinate conversion

Animation channel data is in Gamebryo's Z-up coordinate system, same as
the NIF mesh data. The conversion to Y-up happens in the keyframe import
path (`crates/nif/src/anim.rs`) so the runtime always sees Y-up data and
doesn't need to rotate per-frame. See [Coordinate System](coordinate-system.md)
for the rotation convention details and the SVD repair pass.

## Tests

Animation runtime tests live alongside the modules in
[`crates/core/src/animation/`](../../crates/core/src/animation/):

- **Interpolation kernels** — linear lerp, SLERP, Hermite tangents, TBC
  weights, edge cases (single key, t at exactly a key boundary)
- **Player advance** — time advance with each `CycleType`, pause, speed,
  finished flag, text-key index advancement
- **Registry** — insert / lookup / clip reuse across players
- **Stack blending** — two-layer weighted blend, weight zero, weight one,
  identity case

NIF-side keyframe import tests live in `crates/nif/src/blocks/interpolator.rs`
and cover NiTransformData/NiKeyframeData parsing across the four key types.

## Wiring it up

To play an animation on a NIF mesh from the CLI:

```bash
cargo run -- path/to/mesh.nif --kf path/to/anim.kf
```

The binary's scene loader:

1. Parses the NIF and builds the ECS subtree (root + named children)
2. Parses the KF file and converts it to an `AnimationClip`
3. Inserts the clip into `AnimationClipRegistry` keyed by file name
4. Spawns an `AnimationPlayer { clip_name, time: 0, speed: 1, cycle: Loop }`
   on the root entity
5. The `animation_system` advances the player every frame and writes the
   sampled transforms to the target entities by name

For embedded `NiControllerManager` animations (cycles already inside a
NIF, no separate .kf file), the import path discovers them at NIF parse
time, registers them in the clip registry under their internal sequence
names, and spawns the player automatically. See the M21 entry in the
[ROADMAP](../../ROADMAP.md#m21-animation-playback--done) for the M21
acceptance details.

## Status

The animation pipeline is **feature-complete for keyframe animation**
(M21) and the **GPU skinning compute path** (M29 Phase 1+2) ships
end-to-end through to the per-skinned-entity BLAS refit. The shader
side: `skin_compute.comp` runs one workgroup per skinned mesh, pulls
the bone palette from a per-frame SSBO, applies the skinning matrix
sum, and writes pre-skinned vertices into a per-entity `SkinSlot`
output buffer. The renderer side: that output drives an in-place BLAS
refit (`mode = UPDATE`) so RT shadows and reflections see the
animated geometry.

Phase 3 (raster reads pre-skinned vertices via the same SkinSlot
output, retiring `triangle.vert:147-204`'s CPU-side palette eval) is
deferred behind M41 — the workload to validate at scale doesn't
exist until skinned NPCs populate the cell loader's draw list. See
[ROADMAP M29.3](../../ROADMAP.md) for the gating rationale.

Embedded controller channel emission (#261) is closed; the
`NiFlipController` follow-up (#545) shipped — texture-flipbook
channels (Oblivion / FO3 / FNV fire / smoke / explosion meshes) are
captured into `AnimationClip::texture_flip_channels`. Renderer-side
flipbook sample-and-bind is deferred — channel data is captured first
(matches the `MorphWeight` precedent), GPU plumbing follows in a
later milestone.

## Related docs

- [NIF Parser](nif-parser.md) — keyframe data parsing
- [Coordinate System](coordinate-system.md) — Z-up→Y-up rotation conversion
- [Scripting](scripting.md) — text key marker → event flow
- [ECS](ecs.md) — `AnimationPlayer` as a component, `AnimationClipRegistry`
  as a resource
