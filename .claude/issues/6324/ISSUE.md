# #6324: PHYS-D2-2026-10-21-01: `set_linear_velocity` wakes a newcomer but does not take it off the spawn re-sleep list

**Labels**: low,bug,physics

**Source**: docs/audits/AUDIT_PHYSICS_2026-10-21.md · **Severity**: LOW · **Dimension**: Step & Sync · **Location**: `crates/physics/src/sync.rs:55-77`; compare `crates/physics/src/world/mod.rs:1054-1057` (`forget_spawn_sleep`) and its five callers (`:958`, `:982`, `:1172`, `:1195`, `:1232`)

- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/sync.rs:55-77`; compare `crates/physics/src/world/mod.rs:1054-1057` (`forget_spawn_sleep`) and its five callers (`:958`, `:982`, `:1172`, `:1195`, `:1232`)
- **Status**: NEW. A residual of #5936 (closed by `00ec063ec`, which covered the `PhysicsWorld` methods but not the crate's free helper).
- **Trigger Conditions**: a dynamic body registered this frame (on `spawn_sleep_pending`) that gets a nonzero `set_linear_velocity` before its first stepping frame, and moves less than `SPAWN_SLEEP_DRIFT_BU` (4 BU) on that frame.
- **Description**: `set_linear_velocity` calls `body.set_linvel(v, true)` and `pw.wake()`. It does not drop the handle from `spawn_sleep_pending`. After the stepping frame, `resleep_spawned_newcomers` finds the body awake, within 4 BU of its spawn, and calls `body.sleep()`, which zeroes the requested velocity. This is the #5936 swallow, reached through the one wake path the fix did not touch. The guard `explicit_impulse_keeps_newcomer_awake_through_first_step` covers impulses only.
- **Evidence**: `sync.rs:68-75` (no `forget_spawn_sleep`). `world/mod.rs:1037-1046` (re-sleep on `!drifted && is_dynamic && !is_sleeping`).
- **Impact**: There is one live caller today, `fly_camera_system`'s physics branch (`systems/camera.rs:121`), and only on the camera body's first frame. The risk is to the next producer that launches a fresh body by velocity (thrown or dropped items, scripted `SetVelocity`).
- **Related**: #5936, #5544.
- **Suggested Fix**: Route the helper through a `PhysicsWorld::set_linear_velocity(handle, v)` method that calls `forget_spawn_sleep` on a nonzero velocity, beside the other motion starters. Extend the #5936 test to the velocity path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_PHYSICS_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
