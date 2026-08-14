# PHYS-D6-06

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2889

---

Found by `/audit-physics` Dimension 6 (WATAL Physics Sink — tech debt / doc rot). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/world.rs:246-294`, doc claim at `crates/physics/src/water.rs:15-17`

## Trigger Conditions
None at runtime — documentation / maintenance only.

## Description
`crates/physics/src/water.rs:15-17` states *"The force **application** path lives on [`crate::world::PhysicsWorld`] (`add_force` / `apply_impulse` / `reset_forces`)"*.

`apply_buoyancy` does **not** call any of them. It reaches through to the Rapier body directly (`b.reset_forces(false)` / `b.add_force(..., false)`, `water.rs:429-430`) precisely because the wrappers hard-code `wake_up = true` plus `self.wake()` (`world.rs:253-254`), which would defeat the wake discipline the same module is built around.

A workspace-wide grep for `add_force|apply_impulse|reset_forces` outside `world.rs`/`water.rs` returns **nothing**; inside `world.rs` the three are exercised only by their own unit tests.

## Impact
The one consumer the API was built for **cannot use it as written**, so the module doc points a reader at the wrong code and the three methods are an untested-in-production public surface. `apply_impulse` in particular is the documented hook for the not-yet-built splash kick, so the mismatch would be inherited by WATAL Phase 3.

## Suggested Fix
Give the wrappers a `wake_up: bool` parameter (or an `add_force_no_wake` sibling) and route `apply_buoyancy` through them — or correct the `water.rs` module doc to say the buoyancy phase mutates bodies directly, and explain why.

## Related
- `docs/engine/watal.md` §7 Phase 2 ("Force/reset APIs ... are live")
- PHYS-D6-02 (the same wake-vs-force tension is why the wrappers are bypassed)
