# Issue #4: Animation blending and priority-based layering

## Metadata
- **Type**: enhancement
- **Severity**: medium
- **Labels**: enhancement, animation, ecs
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: Future (post-M21)
- **Affected Areas**: Animation system, ECS

## Problem Statement
Single AnimationPlayer, no blending. Gamebryo supports priority-based layering (ControlledBlock.priority), weighted blending (NiControllerSequence.weight), and cross-fade transitions. Both fields are parsed but ignored.

## Affected Files
- `crates/core/src/animation.rs` — AnimationPlayer → AnimationStack
- `byroredux/src/main.rs` — animation_system multi-layer evaluation

## Acceptance Criteria
- [ ] Multiple clips play simultaneously on overlapping nodes
- [ ] Priority determines override order
- [ ] Weight enables cross-fade blending
- [ ] Smooth transition API: play_clip(handle, blend_time)
