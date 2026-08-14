# PHYS-D2-01

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2856

---

Found by `/audit-physics` Dimension 2 (Step Determinism & Budget). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: HIGH · **Status**: NEW
**Location**: `crates/physics/src/world.rs:345-386` — the defect is the pairing of line 375 with lines 371-374 and the loop guard at 386.

## Trigger Conditions
All four, and all are the *normal* case on the dev box:
1. `frame_dt < PHYSICS_DT` — the engine running faster than 60 fps. `dt` is raw unclamped wall-clock (`byroredux/src/app_events.rs:419`).
2. `islands.active_dynamic_bodies()` is empty — a settled cell. This is the fast path's *design goal*, and `register_newcomers` deliberately spawns every `Dynamic` newcomer asleep (`crates/physics/src/sync.rs:604-606`).
3. Motion is introduced by a **one-shot** `wake()` — ragdoll activation, `apply_impulse`, `set_motion_type`, a single `set_linear_velocity` — rather than a per-frame recurring one.
4. Nothing else re-arms `wake()` on a later frame.

## Description
`step()` clears `pending_wake` **before** the substep loop, but the loop legitimately runs **zero** substeps when `accumulator < PHYSICS_DT`:

```rust
// world.rs:371-375
if self.islands.active_dynamic_bodies().is_empty() && !self.pending_wake {
    self.accumulator = 0.0;
    return 0;
}
self.pending_wake = false;          // cleared even if 0 substeps follow
...
// world.rs:386
while self.accumulator >= PHYSICS_DT && steps < MAX_SUBSTEPS {
```

The wake is consumed without a step ever running. On the next frame the island lists are still stale (they only update inside `pipeline.step`), `pending_wake` is now `false`, so the fast path fires and **zeroes the accumulator** — throwing away the sub-tick time that was about to cross the threshold.

The two behaviours compose into an **absorbing state**: in a quiesced scene above 60 fps the accumulator resets every frame, so it can never reach `PHYSICS_DT` again, so every subsequent `wake()` is swallowed the same way.

Island staleness confirmed against the vendored solver: `IslandManager::wake_up` (rapier3d-0.22.0 `src/dynamics/island_manager.rs:93-113`) is the only writer of `active_dynamic_set`, and it is called from `handle_user_changes` **inside** `PhysicsPipeline::step`. `RigidBody::wake_up` / `set_linvel(_, true)` / `add_force(_, true)` — what every `PhysicsWorld` mutator actually calls — only set `RigidBodyChanges::SLEEP`.

## Evidence
Measured with a temporary integration test against the unmodified crate (deleted afterwards; tree clean). Instrumented trace at 120 fps (`dt = PHYSICS_DT/2`):

```
frame 0: steps=0 acc=0.008333334      <- constructor's pending_wake consumed, no step
frame 1: steps=0 acc=0                <- fast path zeroes the 8.3 ms of banked time
frame 6 (post-wake): steps=0 acc=0.008333334   <- explicit wake(), still 0 substeps
frame 7 (post-wake): steps=0 acc=0             <- wake already cleared; backlog discarded
```

```
wake_at_high_fps_eventually_steps ... FAILED
  one-shot wake was swallowed: total substeps = 0     (600 frames = 5 s of wall time)
wake_at_60fps_steps ... ok                            (control: dt == PHYSICS_DT)
```

## Impact
Anything that starts motion from a single event never simulates until some *other* per-frame wake source resumes the pipeline.

- **Ragdoll activation — the PHYSAL headline path.** `activate_ragdoll` is driven from the debug server, which runs as a `Stage::Late` exclusive, i.e. *after* `Stage::Physics`. Its `pw.wake()` (`crates/physics/src/ragdoll.rs:263`) is therefore **always** consumed by the next frame's `step()`. The actor stays in bind pose until the player moves; `docs/smoke-tests/m41-ragdoll.sh` only passes because the operator moves the camera.
- Scripted `SetMotionType` / `apply_impulse` / a one-shot `set_linear_velocity` — object frozen mid-air with a live velocity.
- The **first** `step()` after construction (`pending_wake: true`) is also swallowed whenever the first frame is sub-tick, so the "settle any bodies present at startup" guarantee does not hold for embedders/tests.

Blast radius: every game, every cell, whenever fps > 60 — which is the project's own target on an RTX 4070 Ti.

## Suggested Fix
Move the `pending_wake` clear to *after* the loop and gate it on work actually having happened:

```rust
if steps > 0 { self.pending_wake = false; }
```

A still-armed wake bypasses the fast path on the following frame, so the accumulator keeps accruing until it crosses `PHYSICS_DT` and the step runs (2 frames at 120 fps, 17 at 1000 fps). Keep the fast-path `accumulator = 0.0` as-is: with the wake preserved it is only reached when the scene is genuinely idle. Add a regression test feeding `PHYSICS_DT / 2` (see the sibling test-coverage issue).

## Related
- Dependents: PHYS-D2-02 (`remove_body` also fails to re-arm), PHYS-D6-02 (buoyancy's one-shot wake swallowed the same way)
- PHYS-D2-04 — no test feeds a sub-tick `dt`, which is why 72 green tests miss this
- The `#1698` budget machinery is **not** implicated; the bug is upstream of the loop
