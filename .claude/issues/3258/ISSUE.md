# 3258: ECS-2026-08-24-11: advance_time's CycleType::Loop arm latches local_time to NaN on unvalidated clip.frequency

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-11)

## Description

`advance_time` has no finiteness guard on `delta = dt * player.speed * clip.frequency`, and `clip.frequency` is raw, unvalidated `NiControllerSequence` data. `CycleType::Clamp` self-heals and `CycleType::Reverse` early-returns via a duration guard, but `CycleType::Loop` does not: once `local_time` is NaN, `NaN % x == NaN` and `NaN < 0.0 == false`, so the corruption is permanent from the first bad tick onward.

## Location

`crates/core/src/animation/player.rs:88-119`; provenance `crates/nif/src/anim/sequence.rs:22` → `byroredux/src/anim_convert.rs:494`

## Evidence

```rust
CycleType::Loop => {
    player.local_time += delta;
    if clip.duration > 0.0 {
        player.local_time %= clip.duration;      // NaN % d == NaN
        if player.local_time < 0.0 {              // NaN < 0.0 == false, never repairs
            player.local_time += clip.duration;
        }
    }
}
```

Traced downstream: NaN flows through `find_key_pair` → `sample_translation`/`sample_rotation`/`sample_scale` → bone transform → `GlobalTransform` → GPU instance matrices, the same class `#3194` fixed for `apply_speedtree_wind`.

## Impact

A malformed or corrupt `NiControllerSequence.frequency` (NaN or ±inf) on any looping clip poisons that entity's pose permanently. MEDIUM rather than HIGH because the other two `CycleType` arms self-heal.

## Related

`#3194` (SpeedTree wind NaN clamp precedent).

## Suggested Fix

`let delta = if delta.is_finite() { delta } else { 0.0 };` at the top of `advance_time`, or validate at the translate boundary (`anim_convert.rs:494`).

## Completeness Checks
- [ ] **SIBLING**: Same NaN-guard pattern as `#3194`
- [ ] **TESTS**: A test asserting `local_time` stays finite after `advance_time` with a NaN-frequency clip
