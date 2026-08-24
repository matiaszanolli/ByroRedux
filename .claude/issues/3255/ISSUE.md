# 3255: ECS-2026-08-24-07: NavPath missing from clear_ambient_behavior's AI teardown list

**Severity**: LOW · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-07)

## Description

`NavPath`, written by six of the seven M42 procedures (wander/travel/follow/escort/guard/patrol), is not among the sixteen roster members `clear_ambient_behavior` tears down on death or package switch.

## Location

`byroredux/src/npc_spawn/ai_package.rs:416-437` (`clear_ambient_behavior`), against `byroredux/src/components.rs:1755-1767` (`NavPath`)

## Impact

Not a corpse-reanimation bug — `NavPath` has no independent iterating system. On **package switch**, the previous procedure's `NavPath` survives into the new one. Frozen-goal callers self-heal on the first tick; live-goal callers (`FOLLOW_REPATH_THRESHOLD = 64.0`, similar for escort) can reuse a stale cached path within 64 units of the new target. Also leaks one `VecDeque<Vec3>` per corpse until cell unload.

## Suggested Fix

Add `remove_component::<NavPath>(world, actor);` to `clear_ambient_behavior`, with a comment pinning "this list must cover every per-actor pathing/runtime component."

## Completeness Checks
- [ ] **TESTS**: A regression test asserting `NavPath` is removed alongside the Behavior/State pairs
