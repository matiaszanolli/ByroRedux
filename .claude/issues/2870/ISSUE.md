# PHYS-D6-01

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2870

---

Found by `/audit-physics` Dimension 6 (WATAL Physics Sink). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/water.rs:390-459` (the `match surface` arms), with `:183` (`WATERLINE_HYSTERESIS`) and `:317-321` (`prior_wet`)

## Trigger Conditions
A cell with an authored water plane (`WaterPlane` + `WaterVolume`), plus a **dynamic** body that (a) becomes wet (`frac > 0`), then (b) rises so its collider-AABB bottom lands in `[surface_y, surface_y + 4]` for at least one tick before leaving the volume — i.e. any *slow* exit: current-driven beaching on a shallow bank, a player pushing clutter out of a pool, a body settling on a rock/dock whose top is 0-4 BU above the waterline.

A fast pop-out (>240 BU/s, clearing the 4 BU band inside one 1/60 s tick) takes the `None` arm and is fine.

## Description
`apply_buoyancy` has three reachable states but only two restore paths. The containment predicate accepts a body whose collider AABB bottom sits up to `WATERLINE_HYSTERESIS` (4 BU) **above** the surface (`min_y <= s.surface_y + WATERLINE_HYSTERESIS`, `:384`), while `submerged_fraction` returns exactly `0.0` for any `min_y >= surface_y` (`:175`). So the band `surface_y <= min_y <= surface_y + 4` yields `Some(surface)` **and** `frac == 0.0`. In that state:

1. the `if frac > 0.0` guard (`:393`) skips the whole body-mutation block, so the authored `linear_damping` / `angular_damping` are **not** restored and `b.reset_forces` is **not** called — the previous frame's buoyancy (+ current) force stays accumulated (Rapier forces persist until explicitly reset; proven by `world.rs::reset_forces_lets_body_fall_again`);
2. a `WaterContact` with `submerged_fraction: 0.0` **is** still written (`:434-443`), which clears `prior_wet` for the next tick (`:317-321`);
3. once `prior_wet` is `false`, the `None` (exit) arm's restore is gated off (`:450`) — so when the body later leaves the volume entirely, the authored damping is **never** restored and the stale force is **never** cleared.

The restore is therefore **lost permanently** for any body that exits *through* the 4 BU band rather than skipping over it in a single tick.

## Impact
Three persistent consequences, none self-healing except by the body re-entering water:

- **Stale force.** The body keeps a constant upward force equal to the last wet frame's `frac * 1.67 * mass * |g|` forever. On a gradual exit that is a small permanent gravity offset; on an abrupt exit (a body lifted several BU in one tick) it is up to a full gravity-cancelling force — the body hovers or creeps.
- **Stale damping.** `linear_damping`/`angular_damping` stay pinned at `PhysicsWaterConstants::linear_damping_in` = 1.5 instead of the authored value (0.0 for most `RigidBodyData`), so the object is permanently sluggish out of water.
- **Contract drift.** `WaterContact` is documented (`crates/core/src/ecs/components/water.rs:303-304`) as `material == None` when the body is out of every volume; the band writes `material: Some(..)` at `submerged_fraction == 0.0` every frame it stays there. That also means per-frame `WaterContact` insert churn for never-wet clutter parked within 4 BU above a waterline (docks, shoreline rocks).

It additionally **weakens the documented wake guarantee**: `water.rs:462-470` asserts the one-shot wake *"CANNOT pin the sim"* because *"a settled float stays wet, fires no new transition"*. A thin float whose `min_y` oscillates across `surface_y` alternates `frac == 0` -> `prior_wet false` -> `frac > 0` -> **fresh `wake_up` + `pw.wake()`**, re-arming the step every couple of frames during settle. The invariant as written is conditional, not absolute.

**The whole exit path is untested** — the five unit tests + three solver tests in `water.rs:497-894` cover entry, equilibrium, sleep and the pure math, never an exit.

## Suggested Fix
Treat `frac == 0.0` inside the band as the dry case: move the restore out of the `None` arm into a shared `else` / `frac == 0.0` path (restore authored damping + `reset_forces` when `prior_wet`, and write `WaterContact::default()` rather than a `material: Some(..)` zero contact). Alternatively make the band entry-only stickiness by gating it on `t.prior_wet`. Add a regression test for wet -> band -> dry asserting the authored damping is back and the body falls again.

## Related
- `WaterContact` doc contract (`crates/core/src/ecs/components/water.rs:280-304`)
- PHYS-D6-04 (same `Some(..)` arm)
