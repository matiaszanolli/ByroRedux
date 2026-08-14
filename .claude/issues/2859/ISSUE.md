# PHYS-D7-01

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2859

---

Found by `/audit-physics` Dimension 7 (Queries & Diagnostics). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: HIGH · **Status**: NEW
**Location**: `crates/physics/src/world.rs:563-588` (`cast_ray_down`); `byroredux/src/render/mod.rs:42-47` + `:670` (`fog_height_reference`, per-frame); `byroredux/src/systems/character.rs:444-453` (camera pose); `crates/physics/src/components.rs:120-127`

## Trigger Conditions
`PlayerMode::Character` (interior cell / exterior grid / `--player` — i.e. every real content path), any frame after the player capsule has been registered in Rapier and the query pipeline has been refreshed once. Fires on **100 % of frames** thereafter.

Does *not* fire in `--mesh` / `--tree` / `--fly` (`FlyCam`) boots, which is exactly why the existing tests and the renderer sign-off missed it.

## Description
`fog_height_reference` casts a downward ray from the **camera's world position** to find the ground datum for height fog. In Character mode the camera is pinned to `body_pos + eye_height*Y` with the *same XZ as the body*, and `CharacterController::HUMAN` is deliberately sized so the eye sits inside the capsule — `components.rs:122-123` states the invariant outright and a unit test asserts `eye_height < half_height + radius` "to keep the eye inside the visible capsule" (52 < 46 + 18 = 64). So the ray originates **inside the player's own collider**.

`cast_ray_down` passes `solid = true` and filters with `QueryFilter::exclude_dynamic()`. The player body is `MotionType::CharacterKinematic` -> `RigidBodyType::KinematicPositionBased`, which `exclude_dynamic` does **not** filter. Rapier's own doc for the `solid` parameter (`rapier3d-0.22.0/src/pipeline/query_pipeline/mod.rs:367-369`) is explicit:

> *"if this is `true` an impact at time 0.0 (i.e. at the ray origin) is returned if it starts inside of a shape."*

A toi of 0 is the minimum possible, so the self-hit always wins the closest-hit search, and `cast_ray_down` returns `origin.y - 0.0 == cam_pos.y`. That value is **numerically identical to the `.unwrap_or(cam_pos.y)` fallback**, so the fix silently degrades to the exact pre-#2225 behaviour it was written to remove — with no log, no `None`, and no test failure.

Critically, `cast_ray_down` has **no exclusion parameter at all** — unlike the sibling `cast_ray`, which grew `excluded_body: Option<RigidBodyHandle>` precisely for this reason and documents it: *"a camera ray can begin inside that body, which would otherwise return an immediate self-hit"* (`world.rs:590-596`). The caster-exclusion plumbing exists on one cast and is structurally absent on the other three (`cast_ray_down`, `cast_capsule_down`, `cast_capsule_down_onto_walkable_surface`).

## Evidence
Empirically confirmed with a throwaway test compiled against the real `PhysicsWorld` (added, run, then reverted — working tree clean):

```
// floor cuboid at y=0 (Fixed) + KinematicPositionBased capsule_y(46,18) centred at y=100
// eye = (0, 152, 0)  ->  capsule spans y in [36, 164]
cast_ray_down from eye inside capsule -> Some(152.0)   // == origin.y, i.e. the fallback
```

Call chain: `render/mod.rs:670 build_render_data` -> `fog_height_reference(world, cam_pos)` -> `world.rs:578 query_pipeline.cast_ray(.., solid=true, QueryFilter::exclude_dynamic())`.

The three existing tests (`render/mod.rs:57-110`) all construct a world with a floor collider and **no player capsule**, so they cannot observe this.

## Impact
Height fog is anchored to eye level again in every interior and exterior cell: `proceduralDensityScale` (froxel injection) and `heightFogOpticalDepth` (aerial-perspective continuation) track the camera vertically, climbing a hill never clears the fog, and pure vertical camera motion changes density at a fixed world point.

That is the ghost-band failure mode the renderer audit rated HIGH as **REN-D16-01** and recorded as fixed in `docs/audits/AUDIT_RENDERER_2026-08-03.md:32`. **That "fixed" record needs correcting** — the renderer-side change is correct, but it is nullified from the physics side. Blast radius is every frame of every game.

## Suggested Fix
Give `cast_ray_down` (and both capsule probes) the same `excluded_body: Option<RigidBodyHandle>` parameter `cast_ray` already carries, and have `fog_height_reference` pass the `PlayerEntity`'s `RapierHandles::body` — the same resolution `byroredux/src/interaction.rs:311-322` already performs correctly. Add a regression test that registers a kinematic capsule around the camera and asserts the floor height is still returned.

The same signature change is what PHYS-D7-02 needs, so fix them together.

## Related
- REN-D16-01 / #2225 (the fix this nullifies)
- PHYS-D7-02 (same missing-exclusion root cause, different caller)
- #1024 (CLOSED — same class of self-hit bug on the TLAS water path)
