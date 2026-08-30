# #3707 — ECS-2026-08-30-D10-07: write_root_motion's zero-motion early return latches the previous tick's delta

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: Animation Runtime
**Location**: `byroredux/src/systems/animation.rs` (`write_root_motion`, ~:73-83); consumer `byroredux/src/systems/cinematic.rs` (`cinematic_root_motion_system`)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-07)

## Description

`write_root_motion` returns before touching `RootMotionDelta` when this tick's motion is exactly `Vec3::ZERO`, so the component keeps whatever the last non-zero tick wrote. The only drain, `cinematic_root_motion_system`, zeroes it only for actors currently awaiting `ExitCartEnd`. An actor outside that window carries a stale delta indefinitely; when it later enters the window, the first tick has `current_time ~= prev_time`, so the write is skipped and the consumer applies the stale value.

## Evidence

```rust
// byroredux/src/systems/animation.rs
fn write_root_motion(world: &World, entity: EntityId, motion: Vec3) {
    if motion == Vec3::ZERO {
        return;                                   // component keeps the old value
    }
```

## Impact

A single-frame position pop of up to one prior frame's root displacement at the start of a cart-exit cinematic. Not an integration leak — the write is an assignment, not `+=`.

## Suggested Fix

Write unconditionally (including `Vec3::ZERO`) for entities that already have the component, keeping the `query_mut`/`get_mut` guards so entities without it stay untouched.

## Completeness Checks
- [ ] **SIBLING**: Both the player and stack call sites of `write_root_motion` verified against the new unconditional write
- [ ] **TESTS**: A regression test pins that a zero-motion tick clears a previously written delta
