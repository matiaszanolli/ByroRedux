# PHYS-D5-01

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2858

---

Found by `/audit-physics` Dimension 5 (Character Controller). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: HIGH · **Status**: NEW — a latent defect *inside* the landed #2013 fix (which is CLOSED and whose ladder is present)
**Location**: `byroredux/src/scene.rs:990-1022` (probe origins), `crates/physics/src/world.rs:694-727`

## Trigger Conditions
Interior/exterior cold-start spawn (`setup_scene`) where a `DoorTeleport` REFR is selected, **and** the real walkable floor top lies within ~15 BU of the door REFR's own `Transform.translation.y`. Since the code's own stated premise is *"Doors sit at floor level by construction"* (`scene.rs:1006-1009`), this is the **normal** case, not an edge case.

## Description
Rungs 1 and 2 of the three-rung floor ladder cast a `CharacterController::HUMAN`-sized capsule downward from `door_pos.y + 50.0` (`scene.rs:994`, `:1014`). The capsule's half-extent is `half_height + radius = 46 + 18 = 64 BU`, so the probe capsule's **bottom starts at `door_pos.y - 14`** — already 14 BU *below* the floor it is looking for.

Any floor at or above `door_pos.y - 15` is either an initial-penetration configuration (which `ShapeCastOptions { stop_at_penetration: false }` discards, or reports at `time_of_impact = 0` with a degenerate normal that the `min_walkable_normal_y` filter then rejects) or simply out of the search half-space. The `+50.0` origin bump is 14 BU short of the value that would make the probe well-posed.

## Evidence
Measured against the real `PhysicsWorld` (`hh=46, r=18, origin = door_y + 50, range = 150, min_walkable_normal_y = cos(50 deg)`, door_y = 0, 4 BU-thick solid slab):

| true floor top | `cast_capsule_down` | `cast_capsule_down_onto_walkable_surface` |
|---|---|---|
| `0.0` (door level) | `None` | `None` |
| `-2.0` | `Some(-14.01)` | `None` |
| `-5.0` | `Some(-14.01)` <- 9 BU lie | `None` |
| `-10.0` | `Some(-14.02)` | `None` |
| `-13.0` | `None` | `None` |
| `-14.0` | `Some(-14.00)` | `None` |
| `-15.0` | `Some(-15.0)` OK | `Some(-15.0)` OK |
| `-20.0` | `Some(-20.0)` OK | `Some(-20.0)` OK |

Control: the identical world probed from `door_y + 150` returns `Some(0.0)` — correct. Note the raw probe additionally *lies* inside the band (reports `-14.01` for a floor at `-5.0`), which is only masked because the walkable filter then rejects the degenerate normal.

The existing unit test never touches this: `walkable_capsule_probe_accepts_floor` (`crates/physics/src/world.rs:1016-1030`) uses `half_height=10, radius=5, origin.y=100` against a slab top at `y=1` — probe bottom starts 84 BU *above* the floor, i.e. only the well-posed geometry the production call site never produces.

## Impact
The door rung of the spawn ladder never answers on an ordinary flat threshold. Every such spawn silently degrades to **rung 3**, the full-cell sweep from `aabb.max[1] + 50` at the *nudged* XZ — the rung the code itself documents as unreliable:

> *"starting from the ceiling picks up whatever clutter (shelves, beams, upper floors) happens to sit anywhere above the nudged XZ, which is **not** the floor the door actually opens onto"* (`scene.rs:982-989`)

On a multi-storey interior the player spawns on the wrong storey or on a beam; when rung 3 also misses, the door spawn is rejected entirely and the ladder falls back to `spawn_on_camera_ground`.

The `floor_rung` telemetry (`scene.rs:1059-1067`) therefore also **mis-attributes**: an operator reading `"full-cell sweep at nudged XZ"` will conclude the room isn't flat, when in fact it is flat and the probe was simply mis-aimed. All games affected; blast radius is every interior door cold-start.

## Suggested Fix
Raise the rung-1/rung-2 probe origin to at least `door_pos.y + (half_height + radius) + margin` (e.g. `+ 80`, keeping the documented "modest margin above the door, not the cell ceiling" intent) and extend `FLOOR_PROBE_RANGE_BU` by the same amount so the searched band is unchanged. Add a regression test whose probe capsule starts *penetrating* the target floor — the geometry the production call site actually produces.

## Related
- #2013 (CLOSED, introduced the ladder), #2193 (CLOSED, added the walkable filter)
- PHYS-D5-02 (the steady-state half of the same door-threshold gap), PHYS-D5-03 (runtime door walks)
- Part of the long-open door-threshold spawn gap. The #1832 mass=0 angle is CLOSED and verified still fixed at `crates/nif/src/import/collision/mod.rs:371` — **not** re-litigated here.
