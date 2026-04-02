# Issue #7: Text key events as ECS markers

## Metadata
- **Type**: enhancement
- **Severity**: low
- **Labels**: enhancement, animation, ecs
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: Future (post-M21)
- **Affected Areas**: Animation system, ECS events

## Problem Statement
NiTextKeyExtraData contains timed markers ("start", "end", "hit", "sound: wpn_swing", "FootLeft"). Block is parsed, NiControllerSequence.text_keys_ref points to it, but events are never emitted during playback.

## Affected Files
- `crates/nif/src/anim.rs` — import text keys into AnimationClip
- `crates/core/src/animation.rs` — add text_keys field, emit during advance_time
- `byroredux/src/main.rs` — animation_system emits marker components

## Acceptance Criteria
- [ ] Text keys imported into AnimationClip
- [ ] Events emitted as transient marker components when crossed
- [ ] Cleaned up at end of frame
- [ ] Looping animations re-fire events each cycle
