# #3703 — ECS-2026-08-30-D10-03 (LATENT): the AnimationStack path writes the accum root's absolute position as RootMotionDelta; the player path writes a per-tick delta

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: Animation Runtime
**Location**: `byroredux/src/systems/animation.rs` (stack path ~:945-964) vs the player path (~:706-713) and `sampled_root_motion_delta` (~:85-107); consumer `byroredux/src/systems/cinematic.rs` (`cinematic_root_motion_system`, ~:119-130)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-03)

> **LATENT — no live repro.** `AnimationStack` is never `world.register`-ed nor inserted by production code. `byroredux/src/boot.rs:1043` declares `.writes::<AnimationStack>()`, but the registration block (`byroredux/src/boot.rs:562-667`) registers only `AnimationPlayer` and `RootMotionDelta`. `World::query_mut` returns `None` for a storage that was never created, so `animation_system_inner`'s stack pass early-returns at `byroredux/src/systems/animation.rs:799-801` on every real frame. The layer/crossfade subsystem is fully implemented, save-serialised and debug-inspectable, but has no runtime producer. **Do not hunt for an in-game repro — there is none.** Severity is set for latent-not-live; the first consumer that registers `AnimationStack` inherits this defect.

## Description

`sampled_root_motion_delta` exists precisely because "applying the absolute sample every frame would compound hundreds of units of motion", and the `AnimationPlayer` path uses it. The `AnimationStack` path never got the same treatment: it feeds `split_root_motion(pos).1` — the raw sampled horizontal *position* — into `root_motion`, which `write_root_motion` stores into `RootMotionDelta`. The consumer, `cinematic_root_motion_system`, does `transform.translation += rotation * (local_delta * scale)` **every frame** it runs, so an absolute-position payload is integrated repeatedly.

## Evidence

```rust
// byroredux/src/systems/animation.rs (stack path) — `pos` is the blended ABSOLUTE translation
let (anim_pos, delta) = split_root_motion(pos);
transform.translation = anim_pos;
root_motion += delta;              // absolute, not a per-tick delta
```

```rust
// byroredux/src/systems/animation.rs (player path) — converted to a per-tick displacement
let (anim_pos, _) = split_root_motion(pos);
transform.translation = anim_pos;
root_motion += sampled_root_motion_delta(clip, channel, ps.prev_time, current_time);
```

## Impact

An `AnimationStack`-driven actor in the `ExitCartEnd` cinematic window would be displaced by the accum root's absolute offset every tick — the exact compounding `sampled_root_motion_delta` exists to prevent. Latent today.

## Suggested Fix

Route the stack path's accum-root contribution through `sampled_root_motion_delta` using the dominant layer's `(prev_time, local_time)` and clip, mirroring the player path, so both honour the per-tick-displacement contract `RootMotionDelta` documents.

## Completeness Checks
- [ ] **SIBLING**: Both producers of `RootMotionDelta` checked against the component's documented contract
- [ ] **TESTS**: A regression test asserts the stack path emits a per-tick delta, not an absolute position
