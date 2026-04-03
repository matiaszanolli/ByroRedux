# Investigation: Issue #4 — Animation Blending & Priority

## Current State
- `AnimationPlayer` holds a single `clip_handle` — one clip at a time
- Multiple `AnimationPlayer` entities can exist but they all write to the same
  target transforms without awareness of each other (last-write-wins)
- `ControlledBlock.priority` (u8) and `NiControllerSequence.weight` (f32) are
  parsed in controller.rs but not carried through nif anim.rs or core animation.rs

## Design: AnimationLayer stack

Replace the single `AnimationPlayer` with an `AnimationStack` component containing
ordered layers. Each layer has:
- `clip_handle: u32`
- `local_time: f32`
- `playing: bool`
- `speed: f32`
- `weight: f32` (0.0–1.0, used for cross-fade blending)
- `priority: u8` (from ControlledBlock, per-channel override priority)
- `blend_in: f32` / `blend_out: f32` (transition timing)

The system evaluates layers by priority (highest first). For each target node,
the highest-priority layer's sample is used. If multiple layers share the same
priority, their weighted average is computed.

For the transition API: `play_clip(handle, blend_time)` fades out the current
top layer and fades in the new one over `blend_time` seconds.

## Also needed
- Carry `weight` from NiControllerSequence → nif AnimationClip → core AnimationClip  
- Carry per-channel `priority` from ControlledBlock → TransformChannel

## Files
1. `crates/core/src/animation.rs` — AnimationLayer, AnimationStack, blending logic, advance
2. `crates/nif/src/anim.rs` — carry weight from NiControllerSequence, priority from ControlledBlock
3. `byroredux/src/main.rs` — animation_system uses stack, convert_nif_clip carries weight/priority
4. `.claude/issues/4/` — investigation

**4 files — within threshold.**
