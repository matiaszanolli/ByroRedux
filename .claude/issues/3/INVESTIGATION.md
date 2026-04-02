# Investigation: Issue #3 — Non-transform controller channels

## Controller → Interpolator → Data mapping

| Controller | Interpolator | Data | Channel Type |
|------------|-------------|------|-------------|
| NiMaterialColorController | NiPoint3Interpolator | NiPosData | ColorChannel (RGB) |
| NiAlphaController | NiFloatInterpolator | NiFloatData | FloatChannel (alpha) |
| NiVisController | NiBoolInterpolator | NiBoolData | BoolChannel (visibility) |
| NiTextureTransformController | NiFloatInterpolator | NiFloatData | FloatChannel (UV param) |
| BSEffectShaderPropertyFloatController | NiFloatInterpolator | NiFloatData | FloatChannel |
| BSLightingShaderPropertyFloatController | NiFloatInterpolator | NiFloatData | FloatChannel |
| BSEffectShaderPropertyColorController | NiPoint3Interpolator | NiPosData | ColorChannel |
| BSLightingShaderPropertyColorController | NiPoint3Interpolator | NiPosData | ColorChannel |

All interpolator/data blocks are already parsed. The gap is:
1. `anim.rs` `import_sequence()` skips non-NiTransformController
2. No channel types for float/color/bool in nif anim or core animation
3. No ECS components for animated material properties
4. animation_system only applies TransformChannel

## Design

### New channel types in nif anim.rs (import side)
- `FloatChannel { keys, key_type, target: FloatTarget }` — for alpha, UV transform, shader float
- `ColorChannel { keys, key_type, target: ColorTarget }` — for material/shader color
- `BoolChannel { keys }` — for visibility

### New channel types in core animation.rs (runtime side)  
Mirror the nif types with glam types.

### AnimationClip extension
Add `float_channels`, `color_channels`, `bool_channels` alongside existing `channels` (transform).
Each keyed by `(node_name, target)` to disambiguate multiple float channels on same node.

### New ECS components
- `AnimatedVisibility(bool)` — toggled by BoolChannel  
- `AnimatedAlpha(f32)` — animated alpha override
- `AnimatedColor([f32; 3])` — animated material color override

### Runtime
animation_system samples each channel type and writes to the corresponding component.
Renderer checks AnimatedVisibility to skip invisible entities, AnimatedAlpha for blend,
AnimatedColor for vertex color modulation.

## Files to change
1. `crates/nif/src/anim.rs` — new channel types, import non-transform controllers
2. `crates/core/src/animation.rs` — new channel types, sampling functions
3. `crates/core/src/ecs/components/` — new AnimatedVisibility, AnimatedAlpha, AnimatedColor
4. `crates/core/src/ecs/components/mod.rs` — register
5. `crates/core/src/ecs/mod.rs` — export
6. `byroredux/src/main.rs` — animation_system + convert_nif_clip + renderer

**6 files — above threshold. Confirmed by user to proceed.**
