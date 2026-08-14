# PHYS-D5-02

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2857

---

Found by `/audit-physics` Dimension 5 (Character Controller). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: HIGH · **Status**: NEW
**Location**: `byroredux/src/systems/character.rs:236-252` (the probe), `crates/physics/src/world.rs:832-906` (`move_character`); rationale documented at `docs/engine/physics.md:237-242`

## Trigger Conditions
`PlayerMode::Character`, `CharacterController.is_grounded == true` (the frame *after* the character first touches ground), no jump this frame, and the supporting collider is a **convex primitive** — `CollisionShape::Cuboid`, which comes from `BhkBoxShape` (`crates/nif/src/import/collision/shape.rs:148`) **and** from the synthesized AABB proxy (`byroredux/src/cell_loader/spawn.rs:246`, the FO4+/packed-Havok fallback collider).

Whether it fires is a knife-edge numerical condition: it depends on the collider's absolute world Y and extents, not on anything a content author controls.

## Description
While grounded, the controller discards the integrated vertical motion and sends a **fixed `-step_height` (= -32 BU)** desired translation (`character.rs:247-251`), relying entirely on `KinematicCharacterController::move_shape`'s swept cast to clamp it.

The capsule at rest sits 3.963 BU above the surface — *inside* the KCC's `target_distance = offset = 4.0` band — so every grounded frame's cast starts in the "already within target distance" configuration. In that configuration parry's shape cast against a convex primitive frequently returns **no interference**, at which point rapier takes the `else` branch (`character_controller.rs:317-322`) and applies the **entire -32 BU**. The character passes straight through the floor, keeps reporting `grounded = true` for 2-3 more frames (each sinking another 32 BU), then goes ungrounded and free-falls out of the world.

The stated rationale is also wrong on the current rapier: `snap_to_ground` is guarded by `result.translation.dot(&self.up) < -1.0e-5` (`rapier3d-0.22.0/src/control/character_controller.rs:370-371`), so on a truly resting frame (`dy == 0`) it does **not** engage — the very thing `docs/engine/physics.md:237-242` and `character.rs:240-246` say the probe exists to guarantee. On the sinking frames it *does* engage, but snap-to-ground only pulls the character further down, so it cannot rescue the fall. `check_and_fix_penetrations` (`character_controller.rs:182`) is an empty stub, the capsule is `KinematicPositionBased` so the solver never pushes it out, and `pull_dynamic` skips non-`Dynamic` bodies.

## Evidence
Measured with production parameters, 120 frames at dt = 1/60 starting from the exact spawn pose `floor_top + half_height + radius + kcc_offset`; 40 distinct floor heights x 2 slab extents x 2 shape kinds:

| shape | probe branch | outcome |
|---|---|---|
| Cuboid, half 50 | `-step_height` (current code) | **sank 20/40** |
| Cuboid, half 500 | `-step_height` (current code) | **sank 28/40** |
| TriMesh (`FIX_INTERNAL_EDGES` + 1.0 skin) | `-step_height` (current code) | sank 0/40 |
| Cuboid, both extents | pure `v*dt` gravity (probe removed) | sank 0/40 |
| TriMesh | pure `v*dt` gravity | sank 0/40 |

Per-frame trace of a sinking case (floor top `-5.0`, spawn centre `63.0`):

```
f0 desired= -0.339 dy= -0.037 y=62.963 feet= -1.037 grounded=true   <- settles
f1 desired=-32.000 dy=-32.000 y=30.963 feet=-33.037 grounded=true   <- through the floor
f2 desired=-32.000 dy=-31.974 y=-1.011 feet=-65.011 grounded=true
f3 desired=-32.000 dy=-32.000 y=-33.011 feet=-97.011 grounded=true
f4 desired=-32.000 dy=-32.000 y=-65.011 feet=-129.011 grounded=false <- free fall
```

Identical relative geometry with the floor top at `0.0` instead of `-5.0` does **not** sink (`dy = 0.000` every frame) — the failure is selected by absolute world Y, which is why it reads as intermittent.

## Impact
The player falls through the floor and out of the world **while standing still**, with no in-engine recovery: the kill-plane (`world.rs:57`) only freezes *dynamic* bodies, so the kinematic capsule falls forever.

Presents as the same black-screen / "0 draws" symptom as the closed #2202 and #2013, so it will be mis-triaged as a missing-collider problem. TriMesh architecture is immune — which is why interiors mostly work and why this survived — but box-shaped platforms/stairs (`BhkBoxShape`) and **every** synthesized packed-Havok proxy (the FO4/FO76/Starfield fallback) are exactly the affected class.

`PhysicsWorld::move_character` currently has **zero** unit tests in the repository, which is why this is invisible to `cargo test`.

## Suggested Fix
Stop sending an unclamped fixed 32 BU probe. Either:
- (a) clamp the grounded probe to a small multiple of `kcc_offset_bu` (e.g. `-(kcc_offset_bu * 2)`), or
- (b) keep `-step_height` but reject any result whose `translation.y` is more negative than the offset while `result.grounded` is still true, treating it as a failed cast and holding position.

Then add `move_character` unit tests covering a resting capsule on a `Cuboid` floor across a sweep of absolute Y values — the assertion is `|dy| <= kcc_offset` for a stationary grounded character. Correct the rationale in `docs/engine/physics.md:237-242` and `character.rs:240-246` at the same time.

## Related
- PHYS-D5-01, PHYS-D5-03 (the other two halves of the door-threshold gap)
- #2013 / #2202 (CLOSED — same observable symptom, different cause)
