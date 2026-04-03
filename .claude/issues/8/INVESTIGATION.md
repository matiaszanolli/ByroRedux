# Investigation: Issue #8 — Accumulation root for root motion

## Code Path
1. `NiControllerSequence.accum_root_name` parsed in controller.rs:313
2. Not carried to nif `AnimationClip` or core `AnimationClip`
3. Animation system applies all translation keys including root motion directly

## Fix
- Carry `accum_root_name` through both AnimationClip types
- In animation_system: when a clip has accum_root_name, for that node's translation channel:
  - Extract horizontal (XZ) delta between frames as `RootMotionDelta`
  - Apply only vertical (Y) component as animation
- New `RootMotionDelta(Vec3)` component on the AnimationPlayer/Stack entity

## Files: 3 (nif anim.rs, core animation.rs, main.rs) — within threshold
