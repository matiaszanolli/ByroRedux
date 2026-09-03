# #3255 — ECS-2026-08-24-07: NavPath missing from clear_ambient_behavior's AI teardown list

**Severity**: LOW · **Dimension**: ECS
**Location**: `byroredux/src/npc_spawn/ai_package.rs::clear_ambient_behavior`

## Fix

`clear_ambient_behavior` tears down all sixteen roster members
(Sandbox/Seated, Wander+State, Travel+State+Traveled, Follow+State,
Escort+State+Escorted, Guard+State, Patrol+State) plus the
`SeatReservations` claim, but not `NavPath` — the shared per-actor
pathing cache six of the seven M42 procedures write. Added
`remove_component::<NavPath>(world, actor);` per the issue's own
suggested fix, with a comment pinning "this list must cover every
per-actor pathing/runtime component, not just the Behavior/State pairs."
Added the missing `NavPath` import.

Confirmed the issue's described consequence still holds against current
code: `ambient_ai_package_system` calls `clear_ambient_behavior` then
installs the new winning procedure, so a stale `NavPath` previously
survived the handover. Live-goal callers (`FOLLOW_REPATH_THRESHOLD` /
`ESCORT_COLLECT_REPATH_THRESHOLD = 64.0`) could reuse it if the new
target happened to land within 64 units of the previous procedure's
cached waypoints.

## TESTS (issue's own checklist item)

Added `clear_ambient_behavior_removes_nav_path` — spawns an entity with a
`NavPath`, calls `clear_ambient_behavior`, asserts the component is gone.
Also registered `NavPath` in `register_runtime` (the shared fixture
`setup_actor` uses), matching the other sixteen roster components already
registered there, so future package-switch tests built on that fixture
can exercise `NavPath` too.

**Reintroduce-and-revert verification**: temporarily removed the new
`remove_component::<NavPath>` call — confirmed the new test failed with
the expected message. Restored the fix and reran — all 18 tests in
`npc_spawn::ai_package::tests` pass again.

## Verification

- `cargo check -p byroredux --tests`: clean, zero warnings.
- `cargo test -p byroredux npc_spawn::ai_package::`: 18 tests passing, 0
  failing (+1 new).
- `cargo test -q -p byroredux`: passing.
- `cargo test -q --no-fail-fast` (full workspace): **7142 passing, 0
  failing**.
