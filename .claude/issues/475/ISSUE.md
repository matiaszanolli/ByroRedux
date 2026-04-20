# Issue #475

FNV-AN-M2: No sequence-level start/stop phase handling inside an AnimationLayer

---

## Severity: Medium

**Location**: `crates/core/src/animation/stack.rs` (`advance_stack`, `sample_blended_transform`); `crates/nif/src/anim.rs` (ControlledBlock parsing)

## Problem

`ControlledBlock` records start/stop times per channel in the NIF (NiControllerSequence carries `start_time` + `stop_time`), but the current `TransformChannel` stores only keys without range metadata. `advance_stack` assumes a layer fills its clip's full `[0, duration]` range.

Partial-range sub-clips — a sequence referencing frames `[1.2s, 2.4s]` of a shared NIF skeleton — cannot be expressed. The stack always samples the whole clip.

## Impact

- Not triggered by bundled FNV `_male.kf` (each sequence = one full clip authored individually).
- Blocks accurate authoring for actor idle chains where multiple sequences share a master skeleton clip.
- FO3/FNV idle chains (NPCLeanIdleChair, IdleCroupier) rely on this pattern.
- Blocks full `NiControllerManager` (#338) because the manager's sub-sequence dispatch needs per-sequence range.

## Fix

1. Extend `ControlledBlock` -> `AnimationLayer` translation in `anim_convert.rs` to capture `start_time` / `stop_time`.
2. Add `clip_start_time: f32` / `clip_stop_time: f32` to `AnimationLayer` (or thread through `AnimationClip` as a default).
3. In `advance_stack`, clamp `local_time` against `[clip_start_time, clip_stop_time]` instead of `[0, duration]`.
4. Adjust `CycleType::Loop` / `Reverse` wrap logic to wrap within the explicit range.

## Related

Blocks #338 (NiControllerManager state machine — parked). Fixing this lands useful groundwork.

## Completeness Checks

- [ ] **TESTS**: Synthetic clip with `start_time=1.2`, `stop_time=2.4`, assert sampler returns IDENTITY outside the range
- [ ] **SIBLING**: Check float/color/bool channels for the same range support
- [ ] **LINK**: Cross-reference #338 in the fix commit

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-AN-M2)
