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
| Animation files supported | Bethesda `.kf` (NIF v20.2.0.7 family) |
| Controller types parsed   | 14 (Transform, Visibility, Alpha, Material color, Texture transform, Geometry morpher, BSEffect/Lighting Shader Property color/float, ...) |
| Interpolation modes       | Linear, Quadratic (Hermite tangents), TBC (Kochanek-Bartels), Constant (step), XYZ Euler |
| Cycle modes               | Clamp, Loop, Reverse (ping-pong) |
| Blending                  | NiControllerManager-style stack with weighted layers |
| Targeting                 | By string `Name` component |

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

```rust
pub struct AnimationClip {
    pub duration: f32,                      // seconds
    pub channels: Vec<AnimationChannel>,
    pub text_keys: Vec<TextKey>,
}

pub struct AnimationChannel {
    pub target_name: String,                // matches an ECS Name component
    pub kind: ChannelKind,
}

pub enum ChannelKind {
    Translation { keys: Vec<Vec3Key> },
    Rotation    { keys: Vec<QuatKey>, key_type: KeyType },
    Scale       { keys: Vec<FloatKey> },
    Float       { property: FloatProperty, keys: Vec<FloatKey> }, // shader-bound floats
    Color       { property: ColorProperty, keys: Vec<ColorKey> }, // material color
    Visibility  { keys: Vec<BoolKey> },
    Alpha       { keys: Vec<FloatKey> },
}

pub enum KeyType {
    Linear,
    Quadratic,           // Hermite tangents (NiTransformData::Tangents)
    TBC,                 // Kochanek-Bartels (Tension/Bias/Continuity)
    XyzRotation,         // separate per-axis float key streams
    Constant,            // step function — value holds until next key
}

pub enum CycleType {
    Clamp,
    Loop,
    Reverse,             // ping-pong
}
```

Each key carries `(time, value, [tangents])`. The interpolation kernels in
`interpolation.rs` handle each `KeyType` variant, including the
quaternion SLERP path for `Rotation`, the cubic Hermite spline for
`Quadratic`, and the TBC weights for `TBC`. Constant keys are step
functions — they hold their value until the next key.

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

This isn't yet wired into a character controller (M28 physics + M29
skeletal animation are deferred), but the data model is ready for it.

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

The animation pipeline is **feature-complete for keyframe animation** as
of M21. Skeletal animation (GPU skinning of weighted meshes) is M29 and
deferred — the parser side has all the data (`NiSkinInstance`, `NiSkinData`,
`NiSkinPartition`, etc., done in N23.5) but the runtime side needs a
compute shader to apply per-vertex bone weights at draw time.

## Related docs

- [NIF Parser](nif-parser.md) — keyframe data parsing
- [Coordinate System](coordinate-system.md) — Z-up→Y-up rotation conversion
- [Scripting](scripting.md) — text key marker → event flow
- [ECS](ecs.md) — `AnimationPlayer` as a component, `AnimationClipRegistry`
  as a resource
