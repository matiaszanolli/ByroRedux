# Animation

ByroRedux plays Bethesda `.kf` animation files (and NIF-embedded
controllers) on named ECS entities. The pipeline is split between
`byroredux-nif` (parsing the .kf / embedded controllers into generic
clip data), `byroredux-core::animation` (the runtime — clip registry,
players, blending, interpolation, the KFM-driven state machine), and
the `animation_system` in the binary crate (the per-frame tick that
walks ECS players/stacks and writes target transforms + sampled
channels).

Source:
- Parser / import: [`crates/nif/src/anim/`](../../crates/nif/src/anim/) — Session 35 split into per-phase siblings (`entry`, `sequence`, `controlled_block`, `transform`, `bspline`, `channel`, `keys`, `coord`, `types`)
- Runtime: [`crates/core/src/animation/`](../../crates/core/src/animation/) — Session 34 split (`types`, `registry`, `player`, `stack`, `controller`, `root_motion`, `interpolation`, `text_events`)
- Conversion glue: [`byroredux/src/anim_convert.rs`](../../byroredux/src/anim_convert.rs)
- Per-frame system: [`byroredux/src/systems/animation.rs`](../../byroredux/src/systems/animation.rs) — Session-34 split out of the old `systems.rs`

## At a glance

| | |
|---|---|
| Animation files supported | Bethesda `.kf` (NIF v20.2.0.7 family) + NIF-embedded `NiControllerManager` / per-node controller chains |
| Controller types imported | NiTransformController (via NiControllerSequence channels), NiVisController, NiAlphaController, NiMaterialColorController (diffuse/ambient/specular/emissive), NiTextureTransformController, NiUVController, NiGeomMorpherController, NiFlipController (#545), BSEffect/BSLightingShaderProperty Float + Color, plus the four `NiLight*Controller` types — color / dimmer / intensity / radius (#983) |
| Interpolation modes       | Linear, Quadratic (Hermite tangents), TBC (Kochanek-Bartels), Constant (step), XYZ Euler — plus compressed B-splines (`NiBSplineComp*Interpolator`, #155) |
| Cycle modes               | Clamp, Loop, Reverse (ping-pong) |
| Blending                  | `AnimationStack` with weighted layers + blend-in/out timers; per-channel priority from `ControlledBlock.priority` |
| State machine             | `AnimationController` — KFM-shaped sequence catalog + transition table → `AnimationStack::play` with blend duration (#338 / AR-09) |
| Targeting                 | Pre-interned `FixedString` channel keys + text-key labels (no per-frame allocations — #340 / #231 / SI-04) |
| GPU skinning              | M29 / M29.5 / M29.6 closed — GPU bone-palette compute (`skin_palette.comp`) → pre-skin (`skin_vertices.comp`) → per-skinned-entity BLAS refit. Raster reads pre-skinned verts (M29.3) still deferred. |

## Module map

```
crates/core/src/animation/
├── mod.rs            Public re-exports + #[cfg(test)] integration tests
├── types.rs          CycleType, KeyType, key structs (Translation/Rotation/Scale + AnimFloat/Color/Bool),
│                     TransformChannel/Float/Color/Bool/TextureFlip channels, FloatTarget/ColorTarget, AnimationClip
├── registry.rs       AnimationClipRegistry — Resource holding clips by u32 handle, with a path→handle memo
├── player.rs         AnimationPlayer — ECS Component + free fn advance_time(player, clip, dt)
├── stack.rs          AnimationLayer, AnimationStack — weighted layer mixing + blend-in/out, sample_blended_transform
├── controller.rs     AnimationController — KFM-driven state machine, apply_pending_transition()
├── root_motion.rs    RootMotionDelta(Vec3) component + split_root_motion() helper
├── interpolation.rs  find_key_pair, hermite, TBC tangents, sample_translation/rotation/scale/float/color/bool
└── text_events.rs    visit_text_key_events() / collect_text_key_events() — text key crossing detection
```

## Pipeline overview

```
Bethesda .kf bytes (or embedded controllers inside a .nif)
        │
        ▼  byroredux_nif::anim::import_kf(scene)            (entry.rs)
        ▼  …or import_embedded_animations(scene)           (entry.rs)
byroredux_nif::anim::AnimationClip
  ├── name, duration, cycle_type, frequency, weight
  ├── accum_root_name
  ├── channels: Vec<(node_name, TransformChannel)>
  ├── float / color / bool / texture_flip channels
  └── text_keys: Vec<(time, label)>
        │
        ▼  byroredux::anim_convert::convert_nif_clip(nif, &mut StringPool)
byroredux_core::animation::AnimationClip
  (channel keys + text-key labels interned to FixedString — #340 / #231)
        │
        ▼  AnimationClipRegistry::add(clip)  → u32 handle
        ▼  (or get_or_insert_by_path(path, build) for cache reuse)
ECS resource AnimationClipRegistry
        │
        ▼  scene/cell loader spawns AnimationPlayer { clip_handle, .. }
        ▼  …or AnimationStack { layers, .. } for blended playback
ECS Component on the target NIF root entity
        │
        ▼  byroredux::systems::animation::animation_system runs per frame
        ▼  advance_time(player, clip, dt)  /  advance_stack(stack, registry, dt)
        ▼  for each channel: sample_<kind>(channel, local_time) → write to
        ▼    target entity (resolved by name through NameIndex / SubtreeCache)
        ▼  crossed text keys → AnimationTextKeyEvents marker on the player entity
Target Transform / AnimatedAlpha / AnimatedDiffuseColor / AnimatedVisibility / … updated
```

## Clip data model

`AnimationClip` is split by channel kind so the runtime can iterate
each kind without per-channel locking. Transform channels are keyed by
`FixedString` symbols pre-interned at clip-conversion time (#340); the
non-transform channels carry the node name alongside the channel in a
`Vec<(FixedString, Channel)>`.

```rust
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub cycle_type: CycleType,
    pub frequency: f32,
    pub weight: f32,                                                  // NiControllerSequence.weight, #469
    pub accum_root_name: Option<FixedString>,                         // root motion node
    pub channels: HashMap<FixedString, TransformChannel>,             // pos/rot/scale
    pub float_channels: Vec<(FixedString, FloatChannel)>,             // alpha, UV, shader floats, morph weights, light dimmer/intensity/radius
    pub color_channels: Vec<(FixedString, ColorChannel)>,             // diffuse/ambient/specular/emissive/shader/light
    pub bool_channels: Vec<(FixedString, BoolChannel)>,               // visibility
    pub texture_flip_channels: Vec<(FixedString, TextureFlipChannel)>,// flipbook (#545)
    pub text_keys: Vec<(f32, FixedString)>,                           // labels interned (#231 / SI-04)
}

pub enum FloatTarget {
    Alpha,
    UvOffsetU, UvOffsetV, UvScaleU, UvScaleV, UvRotation,
    ShaderFloat,
    MorphWeight(u32),               // index into NiGeomMorpherController target list
    LightDimmer, LightIntensity, LightRadius,   // NiLight*Controller, #983
}

pub enum ColorTarget {
    Diffuse,        // #517 — was a single AnimatedColor slot pre-fix
    Ambient,
    Specular,
    Emissive,
    ShaderColor,
    LightDiffuse, LightAmbient,     // NiLightColorController, #983
}

pub struct TextureFlipChannel {
    pub texture_slot: u32,                 // raw TexType enum (0=BASE_MAP, 4=GLOW_MAP, ...)
    pub source_paths: Vec<Arc<str>>,       // resolved at clip-load from NiFlipController.sources → NiSourceTexture.filename
    pub keys: Vec<AnimFloatKey>,           // cycle-position keys (typically a stepped 0..N ramp)
}
```

Transform keys carry `(time, value, [forward/backward tangents], [tbc])`
(translation + scale carry tangents; rotation keys carry just
`value`/`tbc`). The interpolation kernels in
[`interpolation.rs`](../../crates/core/src/animation/interpolation.rs)
handle each `KeyType` variant: linear, quaternion SLERP, cubic Hermite,
TBC weights, and step (Constant). XYZ-Euler rotations and Constant-typed
rotations are decoded into a single quaternion stream at parse time
(`crates/nif/src/anim/keys.rs`) — they come out tagged `KeyType::Linear`
so the runtime stays uniform and never has to recompose Euler angles
per-frame.

`text_keys` are timestamped labels ("hit", "sound: wpn_swing",
"FootLeft", "FootRight", "start", "end", ...) imported from
`NiTextKeyExtraData`. The animation system surfaces the ones crossed
since last frame as an `AnimationTextKeyEvents` marker on the playing
entity; the script cleanup system drops the marker at end-of-frame so
it's strictly transient.

## Player and state

```rust
pub struct AnimationPlayer {
    pub clip_handle: u32,
    pub local_time: f32,
    pub playing: bool,
    pub speed: f32,
    pub reverse_direction: bool,        // ping-pong direction for CycleType::Reverse
    pub root_entity: Option<EntityId>,  // scopes name lookups to this subtree
    pub prev_time: f32,                 // last frame's local_time, for text-key crossing detection
}

// advance_time is a free function (not a method): it mutates the player
// in place and records prev_time before stepping local_time.
pub fn advance_time(player: &mut AnimationPlayer, clip: &AnimationClip, dt: f32);
```

`AnimationPlayer` is an ECS component on the **target NIF root entity**.
The animation system looks up the clip in the registry by `clip_handle`,
walks its channels, and for each channel resolves the target entity by
`Name` — scoped to the player's subtree via `SubtreeCache` /
`build_subtree_name_map` (in `anim_convert.rs`), not globally — and
writes the sampled value. A single clip therefore drives a whole tree of
entities through name lookups without pre-baked entity references.

The CLI demo path and the cell loader both spawn an `AnimationPlayer` on
the root entity (for `--kf <path>`, the scene loader wires it up in
[`byroredux/src/scene.rs`](../../byroredux/src/scene.rs)). When the
`inspect` feature is enabled the player is `Serialize`/`Deserialize`, so
debug snapshots round-trip the full playback state including
`reverse_direction` (#486).

## Blending: AnimationStack

For blended playback (idle → walk transitions, or NIF files that drive
multiple sequences through a `NiControllerManager`), the runtime uses a
layered stack instead of a single player:

```rust
pub struct AnimationLayer {
    pub clip_handle: u32,
    pub local_time: f32,
    pub playing: bool,
    pub speed: f32,
    pub weight: f32,                    // crossfade weight (0.0–1.0)
    pub reverse_direction: bool,
    pub blend_in_remaining: f32,        // ramp-in timer
    pub blend_in_total: f32,
    pub blend_out_remaining: f32,       // ramp-out timer
    pub blend_out_total: f32,
    pub prev_time: f32,
}

pub struct AnimationStack {
    pub layers: Vec<AnimationLayer>,
    pub root_entity: Option<EntityId>,
}

impl AnimationStack {
    pub fn play(&mut self, clip_handle: u32, blend_time: f32);   // cross-fade in a new top layer
    pub fn cleanup_finished(&mut self);                          // drop fully-blended-out layers
}

pub fn advance_stack(stack: &mut AnimationStack, registry: &AnimationClipRegistry, dt: f32);
pub fn sample_blended_transform(
    stack: &AnimationStack,
    registry: &AnimationClipRegistry,
    channel_name: FixedString,
) -> Option<(Vec3, Quat, f32)>;   // (translation, rotation, scale)
```

`AnimationLayer::effective_weight()` folds the crossfade weight together
with the blend-in/out ramp progress. `sample_blended_transform` does a
priority-aware weighted blend in a single fused walk (#288): it finds the
maximum channel priority present across all contributing layers, sums the
effective weights at that priority, and blends only those layers
(translations by weighted sum, scale by weighted sum, rotation by
incremental SLERP with hemisphere correction). The clip's authored
`weight` pre-attenuates each layer (#469) so a sequence author can make a
clip contribute less than full even at full layer weight — distinct from
the runtime crossfade `AnimationLayer::weight`. Single-clip
`AnimationPlayer` playback does **not** apply `clip.weight`; it's a
blend-only scaler.

The blend-interpolator NIF blocks (`NiBlendTransformInterpolator` /
`NiBlendFloatInterpolator`) are parsed in
[`crates/nif/src/blocks/interpolator.rs`](../../crates/nif/src/blocks/interpolator.rs).

## State machine: AnimationController

[`controller.rs`](../../crates/core/src/animation/controller.rs)

`AnimationController` is the `NiControllerManager` / KFM equivalent for
Redux — it closes the "missing glue" gap noted in the 2026-04-15 legacy
audit (AR-09 / #338). It carries a sequence catalog
(`sequence_id → clip_handle`), an explicit transition table
(`(src_id, dst_id) → kind + duration`), sync-group membership, and the
two top-level transition defaults (sync / non-sync). Gameplay code calls
`request_sequence(id)`; `apply_pending_transition(controller, stack)`
resolves the blend duration per the KFM rules (explicit entry →
`DefaultSync`/`DefaultNonSync` indirection → sync-group fallback) and
drives `AnimationStack::play` with the matching clip handle.

The controller is deliberately decoupled from the NIF/KFM parser crate:
the caller assembles it from `byroredux_nif::kfm` data in their own crate
(`TransitionKind::from_kfm_discriminant` maps the raw transition-type
value), so `byroredux-core` never pulls in the parser. Several transition
styles (`Morph`, `Chain`) currently collapse to a single `Blend`/`play`
to the final target — text-key-driven morphing and multi-step chains are
follow-up work.

## Root motion

[`root_motion.rs`](../../crates/core/src/animation/root_motion.rs)

Some animations carry "root motion" — translation on the skeleton's
accumulation root meant to drive the entity's world position rather than
displace the mesh in place. `RootMotionDelta(pub Vec3)` is the ECS
component that carries the per-frame horizontal delta into the gameplay
layer; `split_root_motion(translation: Vec3) -> (anim, delta)` is the
pure helper that the animation system calls per sample to partition a
sampled root translation:

- `anim` — **vertical only** (`(0, y, 0)`): the jump / crouch bob,
  written back to the root entity's `Transform` so the skeleton still
  moves relative to its root.
- `delta` — **horizontal only** (`(x, 0, z)`): consumed by whichever
  system advances the entity through the world.

Both input and output are in **renderer Y-up space** — the accum-root
translation has already passed through `zup_to_yup_pos` at import
(Gamebryo `(x, y, z) → (x, z, -y)`), so Gamebryo's XY walking plane has
become renderer XZ by the time the split runs. A character controller
reading the delta must rotate it by the entity's yaw before applying it
(#526). The split runs against `clip.accum_root_name` (set on import from
the `NiControllerManager` cumulative flag / sequence accum root).

## Text key events as ECS markers

[`text_events.rs`](../../crates/core/src/animation/text_events.rs)

`visit_text_key_events(clip, prev_time, curr_time, visit)` walks the
clip's `text_keys` and invokes the visitor once per label crossed in
`(prev_time, curr_time]`, handling loop wrap-around (a step from 4.8 →
0.3 in a 5 s clip fires keys in both `[4.8, dur]` and `[0, 0.3]`). It
passes the interned `FixedString` symbol — zero allocations, zero string
comparisons — so the hot per-frame path never `to_owned()`s a label that
didn't fire (#339). `collect_text_key_events` is the allocating wrapper
kept for test ergonomics. The stack equivalent
`visit_stack_text_events` dedups labels across overlapping layers via a
reusable `&mut Vec<FixedString>` seen-set.

The animation system uses each player's `prev_time`/`local_time` (and
each layer's, after `advance_stack`) to drive the visitor, then writes a
single transient marker on the playing entity:

| Source | Marker component |
|---|---|
| Crossed text keys this frame | `byroredux_scripting::events::AnimationTextKeyEvents(Vec<AnimationTextKeyEvent>)` |

`AnimationTextKeyEvent { label: FixedString, time: f32 }` carries the
interned label (resolve via `StringPool::resolve`) and the key's clip
time. Systems query for `AnimationTextKeyEvents` to trigger sounds, hit
detection, footsteps, or state transitions — there's no per-label marker
type (no `FootstepEvent` / `AttackEndEvent`); the label string *is* the
discriminator. The script cleanup system (`crates/scripting/src/cleanup.rs`)
drains the marker at end-of-frame so it's strictly transient. See
[Scripting](scripting.md) for the broader event model and how scripts
subscribe to markers without a VM.

## Coordinate conversion

Animation channel data is in Gamebryo's Z-up coordinate system, same as
the NIF mesh data. The conversion to Y-up happens in the keyframe import
path (`crates/nif/src/anim/coord.rs`, sharing the consolidated core
helpers per #1044 / TD3) so the runtime always sees Y-up data and doesn't
rotate per-frame. See [Coordinate System](coordinate-system.md) for the
rotation convention and the SVD degenerate-rotation repair pass.

## B-splines

[`crates/nif/src/anim/bspline.rs`](../../crates/nif/src/anim/bspline.rs)

`NiBSplineCompTransformInterpolator` / `NiBSplineCompFloatInterpolator`
store quantized control points instead of explicit keyframes (#155). The
importer runs a de Boor evaluation over `NiBSplineData` + the basis from
`NiBSplineBasisData`, resampling at a fixed `BSPLINE_SAMPLE_HZ` into the
ordinary `TransformChannel` / `FloatChannel` key arrays so the runtime
never has to know it came from a spline. These interpolators are
reachable on FNV and FO3 too, not only Skyrim+ — don't rule them out by
game era.

## Tests

Animation runtime tests live alongside the modules in
[`crates/core/src/animation/`](../../crates/core/src/animation/):

- **Interpolation kernels** (`mod.rs` tests) — linear lerp, SLERP,
  Hermite, TBC weights (zero-param == SLERP baseline, full-tension
  collapse, non-zero divergence, three-key Catmull-Rom), plus edge cases
  (empty channel → `None`, single key → constant, out-of-range clamp).
- **Player advance** — `advance_time` under each `CycleType` (loop wrap,
  clamp, reverse ping-pong) and `prev_time` tracking for text keys.
- **Registry** — `add` / `get` / handle reuse.
- **Stack blending** — `sample_blended_transform` applies `clip.weight`
  (#469); priority-aware blend.
- **Controller** — `resolve_blend_time` rules and `apply_pending_transition`
  (first-play zero blend, explicit/default duration, sync-group fallback,
  unknown-sequence drop, last-write-wins catalog).
- **Root motion** — `split_root_motion` vertical/horizontal partition.
- **Text events** — forward crossing, loop wrap, empty clip.
- **Inspect round-trip** (`feature = "inspect"`) — JSON snapshot recovery
  of `AnimationPlayer` / `AnimationStack` state (#486).

NIF-side keyframe import tests live in
`crates/nif/src/anim/tests.rs` and
`crates/nif/src/blocks/interpolator_tests.rs`, covering
`NiTransformData` / `NiKeyframeData` parsing across the key types and the
B-spline path. End-to-end coverage runs through `mtidle.kf` →
`animation_system` in the binary crate's animation e2e tests (M41.0).

## Wiring it up

To play an animation on a NIF mesh from the CLI:

```bash
cargo run -- path/to/mesh.nif --kf path/to/anim.kf
```

The binary's scene loader:

1. Parses the NIF and builds the ECS subtree (root + named children).
2. Parses the KF file (or extracts it from a BSA) and converts its clips
   to `byroredux_core::animation::AnimationClip` via `convert_nif_clip`.
3. Inserts each clip into `AnimationClipRegistry` via `add`, getting back
   a `u32` handle (loose-file paths also memoise through
   `get_or_insert_by_path` so re-loads reuse the handle).
4. Spawns an `AnimationPlayer { clip_handle, .. }` on the root entity.
5. The `animation_system` advances the player every frame and writes the
   sampled transforms / float / color / bool channels to the target
   entities by name.

For embedded `NiControllerManager` / per-node controller chains (cycles
already inside a .nif, no separate .kf), `import_kf` follows the
manager's `sequence_refs`, and `import_embedded_animations` walks every
`NiObjectNET`-bearing block's `controller_ref → next_controller_ref`
chain to capture the *ambient* loops (UV scroll, alpha fade, vis flicker,
material-color pulse, texture flipbook, light flicker). See the M21 entry
in the [ROADMAP](../../ROADMAP.md) for the original acceptance details.

## Status

The animation pipeline is **feature-complete for keyframe animation**
(M21 — `.kf`, linear/Hermite/TBC, the controller set above) and the **GPU
skinning chain** is closed: M29/M29.3 verified the
`SkinnedMesh → bone-palette → vertex shader` chain end-to-end on the FNV
NiTriShape path (CPU palette eval), then **M29.5 (Session 40, 2026-05-20)
replaced the host-side per-frame bone-palette upload with a GPU compute
pass** (`skin_palette.comp`), and **M29.6** promoted a persistent
per-entity SSBO slot pool so allocation amortises across frames. Pre-skin
runs in `skin_vertices.comp`; that output drives an in-place per-skinned-
entity BLAS refit (`mode = UPDATE`) so RT shadows / reflections / GI see
this-frame's pose. The TLAS build is relocated after the skin chain for
zero-lag.

The remaining deferred piece is **M29.3** (raster reads the pre-skinned
vertices from the per-entity `SkinSlot` output buffer, retiring the
inline weighted-bone-matrix-sum in `triangle.vert`): the rasterized
skinning path is well-understood and the compute path is newer, so it
ships only once the M41 NPC rollout proves the compute + BLAS-refit chain
stable on visible animated content. See
[ROADMAP M29.3](../../ROADMAP.md) for the gating rationale.

Embedded controller channel emission (#261) is closed; the
`NiFlipController` follow-up (#545) shipped — texture-flipbook channels
(Oblivion / FO3 / FNV fire / smoke / explosion meshes) are captured into
`AnimationClip::texture_flip_channels` with source paths pre-resolved at
import. The four `NiLight*Controller` types (#983) now feed the
`Light{Diffuse,Ambient}` / `Light{Dimmer,Intensity,Radius}` channel
sinks, consumed by the `animation_system` in
[`byroredux/src/systems/animation.rs`](../../byroredux/src/systems/animation.rs)
(`ColorTarget::LightDiffuse`/`LightAmbient` write `LightSource.color`;
`FloatTarget::LightDimmer`/`LightIntensity`/`LightRadius` write
`LightSource.dimmer`/`intensity`/`radius`) — distinct from the procedural
`animate_lights_system` (`light_anim`), which only drives `LightFlicker`.
Renderer-side flipbook
sample-and-bind is still deferred — channel data is captured first
(matching the `MorphWeight` precedent), GPU plumbing follows in a later
milestone.

## Related docs

- [NIF Parser](nif-parser.md) — keyframe data parsing
- [Coordinate System](coordinate-system.md) — Z-up→Y-up rotation conversion
- [Scripting](scripting.md) — text key marker → event flow
- [ECS](ecs.md) — `AnimationPlayer` / `AnimationStack` / `AnimationController`
  as components, `AnimationClipRegistry` as a resource
