# #5452: PHYS-D4-2026-10-08-01: The force-greet and Eat/Sleep movers drive the NPC KCC through `step_toward`, discarding `blocked` and skipping navmesh waypoints, so a KCC-blocked actor never completes its procedure

**Labels**: low,physics,ai,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5452

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-08.md` — `PHYS-D4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW. Latent: ECS-2026-10-08-D6-01 currently stops these actors moving at all.
- **Dimension**: Character & NPC Controller
- **Location**:
  - `byroredux/src/systems/forcegreet.rs:44-48` (the doc claims routing), `:80-97`
  - `byroredux/src/systems/eat_sleep.rs:17-19` (doc), `:38` (`ARRIVE_RADIUS`), `:121-140`
  - The contract: `byroredux/src/systems/locomotion.rs:134-152` (`step_toward`), `:154-158` (`blocked`), `:17-26`
    (`step_toward` "knows nothing about NAVM")
- **Status**: NEW. Introduced by `14cff35ae` and `00f580e09`. Distinct from ECS-2026-10-08-D6-01 (the step is
  written to `GlobalTransform`) and CONC-D5-2026-10-08-01 (lock hold); it cross-references both.
- **Trigger Conditions**: either condition below, once ECS-D6-01 is fixed so the step is committed:
  - a force-greet or Eat/Sleep actor whose straight line to its goal crosses solid architecture;
  - a destination the KCC capsule cannot reach within the arrival radius.

  The radius is 64 BU for Eat/Sleep and the directive radius (default 128) for force-greet. A `NearReference`
  `PLDT` resolves to the referenced REFR's origin (`travel::resolve_near_reference_target`). If that REFR is solid
  furniture whose collider extends more than about 40 BU from its origin along the approach, and is too tall to
  autostep, the actor can never reach 64 BU: the capsule radius is 20 BU, the KCC offset is 4 BU, and the step
  height is 32 BU. Real furniture extents were not measured.
- **Description**: M42.10 gives the KCC step two outputs that callers must handle.
  - `step_toward_detailed` reports `blocked` (less than 25% of the requested XZ move delivered). The oscillating
    walkers consume it through `advance_stuck_repick`.
  - Every frozen-goal mover routes through `step_along_waypoints`, which steps toward the next resident-tile
    `NavPath` waypoint and pins `target.y = current.y`. That mover family is Travel, Guard, Escort and Follow.

  The two new movers call bare `step_toward`, so they get neither:
  - `blocked` is discarded. Neither system has a stuck timer, a give-up or a re-pick.
  - They follow no waypoints and walk straight at the goal. The docs nevertheless say "navmesh-routing when a
    resident tile covers the actor" (`forcegreet.rs:47-48`) and "KCC-backed, single resident NAVM tile"
    (`eat_sleep.rs:18`), while `locomotion.rs:17` says `step_toward` "knows nothing about NAVM".
  - Completion is a pure distance test (`flat.length() > ARRIVE_RADIUS` / `> directive.radius`). The KCC can hold
    an actor outside that radius forever, either at a wall on the straight line or at the destination furniture's
    own collider. The actor then grinds in place every frame, and Eat/Sleep never reaches `seat_at_marker`.

  A minor point: `eat_sleep` passes the resolved `destination`, whose `.y` is not `current.y`. This violates
  `step_toward`'s documented contract (`locomotion.rs:134-137`). On the KCC path it is harmless, because the move is
  computed from XZ only. On the no-physics fallback, `move_towards` drifts Y toward the authored or hash-picked
  height, and the XZ stride shrinks by the Y component.
- **Evidence**:
  ```rust
  // eat_sleep.rs:130 (forcegreet.rs:87 is the same shape)
  let (new_pos, new_rotation) = crate::systems::locomotion::step_toward(
      current, /* rotation */, destination /* .y != current.y */, dt, speed, physics);
  // vs travel.rs:291 — step_along_waypoints(p.current, p.rotation, p.waypoints, p.destination, …)
  // vs wander.rs:212 — step_toward_detailed(…) → advance_stuck_repick(…, blocked, dt)
  ```
  `forcegreet.rs` has no tests. The `eat_sleep` tests run with no `PhysicsWorld`, so none of them exercises the KCC.
- **Impact**: after ECS-D6-01 is fixed, any FO3/FNV Eat/Sleep actor whose path or destination is obstructed walks
  into the obstacle indefinitely and never sits. A force-greet that starts behind a wall or counter never opens.
  PERF-D1-2026-10-08-03 (per-frame furniture gather for an arrived-but-unseatable actor) is the cost-side sibling.
  Here the actor never arrives at all.
- **Related**: ECS-2026-10-08-D6-01 (masks this finding; fix it first); CONC-D5-2026-10-08-01; PERF-D1-2026-10-08-03.
  `/audit-gameplay` owns the behaviour policy (give-up versus re-pick); the KCC contract misuse is reported here.
- **Suggested Fix**:
  - Route both movers through `step_along_waypoints` with a cached `NavPath`, as Travel does, which also fixes the
    Y contract. Then correct the two module docs.
  - Consume `blocked` through `step_toward_detailed`. For Eat/Sleep, attempt the seat pick once the actor is
    blocked within the seat-search radius. For force-greet, drop the directive or open in place after
    `LOCOMOTION_STUCK_REPICK_SECS`.
  - Add a `PhysicsWorld`-backed test with a wall between the actor and its goal.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Every other bare `step_toward` caller (combat, cinematic) checked for the same discarded-`blocked` / no-waypoint shape
- [ ] **TESTS**: A regression test pins this specific fix
