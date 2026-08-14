# PHYS-D6-02

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2871

---

Found by `/audit-physics` Dimension 6 (WATAL Physics Sink). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW (interaction; **root cause already filed as PHYS-D2-01** — do not double-fix)
**Location**: `crates/physics/src/water.rs:272-277` (quiesced fast path), `:399-404` + `:471-473` (the one-shot wake), interacting with `crates/physics/src/world.rs:371-386`

## Trigger Conditions
Frame rate above 60 fps (so `frame_dt < PHYSICS_DT` on the transition frame) **and** an otherwise fully quiesced physics scene (no awake dynamic body, no kinematic motion) **and** a body entering water via the streamed-in/asleep path. Interior or exterior, any game.

## Description
`PhysicsWorld::step` clears `pending_wake` **before** the `while self.accumulator >= PHYSICS_DT` loop, so on any frame above 60 fps the wake is consumed with **zero** substeps run. That is PHYS-D2-01.

What is specific to buoyancy is that its wake is a **latched one-shot** and its own fast path then locks the state in:

1. Frame N (body streams in already submerged, spawned asleep): `n_new > 0` lets `apply_buoyancy` run (`sync.rs:129-133`), it calls `b.wake_up(true)` and `pw.wake()`, and writes `WaterContact { submerged_fraction: frac > 0 }`.
2. `step` consumes `pending_wake`, runs 0 substeps. `RigidBody::wake_up` only touches `activation` (`rapier3d-0.22.0/src/dynamics/rigid_body.rs:685-691`); `IslandManager::active_dynamic_set` is rebuilt **only inside** `PhysicsPipeline::step`. So `awake_counts().0` is still `0`.
3. Frame N+1: `prior_wet` is now `true` -> no new wake, no `pw.wake()`. The quiesced guard `pw.awake_counts().0 == 0 && !pw.pending_wake() && !had_newcomers` (`water.rs:274`) is satisfied -> `apply_buoyancy` returns early. `step`'s fast path then returns 0 **and zeroes the accumulator**.

The pair is now self-sustaining: the body is non-sleeping but out of the island set, buoyancy refuses to look at it, and the step refuses to run.

## Evidence
The live-system test `body_in_water_volume_floats_and_drifts_via_physics_sync` (`water.rs:690`) calls `physics_sync_system(&world, PHYSICS_DT)` with `dt` exactly equal to `PHYSICS_DT`, so the accumulator always reaches one substep — **the test suite is structurally blind to this**.

## Impact
A dynamic body that streams into a cell already submerged — the exact case the `n_new > 0` escape hatch exists for — can stay frozen mid-water-column: not sinking, not floating, nothing integrating, until some *unrelated* caller arms `pw.wake()`.

In normal play the player's kinematic capsule does that on the next frame it moves (`sync.rs:87-91`), so the artifact is a visible hang rather than a permanent one. It **is** permanent for a parked camera — including `--bench-hold` / `byro-dbg` smoke runs, which is precisely how WATAL §7 Phase 2's remaining real-data GPU smoke gate would be executed. **That gate could report "buoyancy doesn't work" for a reason that isn't buoyancy.**

## Suggested Fix
Fix PHYS-D2-01 (clear `pending_wake` only when a substep actually ran). Buoyancy-local hardening if PHYS-D2-01 is deferred: keep the quiesced guard from latching by also re-arming when any target's Rapier body is non-sleeping while `awake_counts().0 == 0`, or re-`pw.wake()` on any tick where a wet contact exists and the body is not sleeping.

## Related
- **PHYS-D2-01** (root cause, HIGH)
- `docs/engine/watal.md` §7 Phase 2 wake/sleep discipline
- `crates/physics/src/sync.rs:129-133` (the `n_new > 0` escape hatch, present and correct)
