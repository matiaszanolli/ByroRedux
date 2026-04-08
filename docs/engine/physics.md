# Physics

ByroRedux runs a Rapier3D-backed physics simulation on top of the
`CollisionShape` / `RigidBodyData` components the NIF importer has been
populating since N23.6. The bridge lives in its own crate
(`byroredux-physics`) so that `core` stays physics-agnostic and the
loose-NIF viewer path can opt out just by not inserting the
`PhysicsWorld` resource.

Source: [`crates/physics/src/`](../../crates/physics/src/)

## At a glance

| | |
|---|---|
| Backend             | [Rapier3D 0.22](https://rapier.rs) (simd-stable) |
| Fixed tick rate     | 60 Hz with a sub-step accumulator, capped at 5 substeps/frame |
| Gravity             | −686.7 BU/s² (≈ −9.81 m/s² scaled to Bethesda units, 1 m ≈ 70 BU) |
| Units               | Bethesda units throughout — Rapier never sees metres |
| Tests               | 14 unit tests (shape mapping, gravity, static floor, accumulator cap) |

## Data pipeline

Physics is additive on top of everything the importer already does:

```
NIF (.nif)
  └─ parse_nif()                        crates/nif/src/
      └─ bhkCollisionObject → bhkRigidBody → bhkShape chain
         └─ import_nif_with_collision() crates/nif/src/import/
            └─ ImportedCollision { shape: CollisionShape, body: RigidBodyData }
               └─ cell_loader spawns    byroredux/src/cell_loader.rs
                  - Transform + GlobalTransform
                  - CollisionShape
                  - RigidBodyData
                     └─ physics_sync_system registers in Rapier
                        - RigidBodyHandle + ColliderHandle → RapierHandles
```

No parser or importer changes were needed for M28 Phase 1 — the
collision data was already sitting on entities since N23.6, it just
wasn't being read by anything. The physics system is the thing that
makes it live.

## Crate layout

```
crates/physics/src/
├── lib.rs         Crate root, re-exports
├── convert.rs     glam ↔ nalgebra conversions + collision_shape_to_shared_shape
├── components.rs  RapierHandles (body + collider) and PlayerBody marker
├── world.rs       PhysicsWorld resource — sets, pipeline, accumulator, step()
└── sync.rs        physics_sync_system — the 4-phase per-tick bridge
```

All Rapier types are confined to this crate. Engine code talks glam;
`convert.rs` does the Vec3 / Quat / Isometry3 translation at the
boundary and never leaks nalgebra types outside the crate.

## The physics_sync_system

The system lives in [`sync.rs`](../../crates/physics/src/sync.rs) and
runs each frame after transform propagation. It's structured as four
phases:

**Phase 1 — Register newcomers.** Query every entity that has
`(CollisionShape, RigidBodyData, GlobalTransform)` but not yet a
`RapierHandles`. For each:

- Build a `RigidBodyBuilder` from `RigidBodyData.motion_type`
  (`Static → fixed()`, `Keyframed → kinematic_position_based()`,
  `Dynamic → dynamic()`).
- Set initial position from `GlobalTransform` via `iso_from_trs`.
- Apply `mass` / `friction` / `restitution` / damping from
  `RigidBodyData`.
- Build one `Collider` per `CollisionShape` variant via
  `convert::collision_shape_to_shared_shape`. `Compound` walks its
  children recursively.
- Attach a `RapierHandles { body, collider }` component so the entity
  is skipped on subsequent frames.

Phase 1 also picks up entities that carry the `PlayerBody` marker
without a `CollisionShape`, synthesizes a capsule + dynamic body with
rotations locked, and inserts `RapierHandles` the same way. This is
how the fly camera becomes a simulated body.

**Phase 2 — Push kinematic transforms.** For every `Keyframed` body,
copy the current `GlobalTransform` into Rapier via
`set_next_kinematic_position`. This is how animated doors and lifts
drive physics without fighting the solver.

**Phase 3 — Step.** Call `PhysicsWorld::step(frame_dt)`, which drains
the accumulator at the fixed 60 Hz tick rate (see below).

**Phase 4 — Pull dynamic transforms back.** Query every
`RapierHandles` + `RigidBodyData` entity where the body is `Dynamic`,
read its `Isometry3` out of Rapier, and write it back into the local
`Transform` via `quat_from_na` / `vec3_from_translation`. Static and
keyframed bodies are driven the *other* way (engine → Rapier) so they
never flow back.

## Fixed tick with accumulator

Rapier is a variable-dt solver in theory but stability and
determinism are much better if you feed it a fixed step. `PhysicsWorld`
owns an accumulator that collects wall-clock `DeltaTime`, drains it in
`PHYSICS_DT = 1.0/60.0` slices, and caps at `MAX_SUBSTEPS = 5` per
frame to prevent spiral-of-death on hitches:

```rust
self.accumulator += frame_dt.max(0.0);
if self.accumulator > MAX_SUBSTEPS as f32 * PHYSICS_DT {
    self.accumulator = MAX_SUBSTEPS as f32 * PHYSICS_DT;  // drop oldest
}
while self.accumulator >= PHYSICS_DT && steps < MAX_SUBSTEPS {
    self.pipeline.step(&self.gravity, &self.integration_parameters, ...);
    self.accumulator -= PHYSICS_DT;
    steps += 1;
}
```

60 Hz matches Skyrim and FO4's internal physics rate. The cap means
worst case is a 5× frame skew recovery window, beyond which the engine
visibly slows down but never catastrophically blows up.

## Player body

When the scene loads actual NIF/cell content (not the loose-NIF viewer
demo), [`byroredux/src/scene.rs`](../../byroredux/src/scene.rs) attaches
a `PlayerBody::HUMAN` marker to the active camera entity. First frame,
Phase 1 of `physics_sync_system` picks it up, builds a dynamic capsule
with rotations locked (so the player stays upright), and inserts the
handles. The fly-camera system then writes player motion via
`byroredux_physics::set_linear_velocity()` instead of mutating
`Transform` directly, so the camera respects world collision:

```rust
let has_physics = world
    .query::<RapierHandles>()
    .map(|q| q.contains(cam_entity))
    .unwrap_or(false);
if has_physics {
    byroredux_physics::set_linear_velocity(world, cam_entity, velocity);
} else {
    // Legacy free-fly path for --mesh or --bsa loose NIF demo
    transform.translation += desired_move * speed;
}
```

This is a **dynamic-body character controller**, which is the
minimum to prove the pipeline end-to-end but not great for gameplay.
A proper kinematic character controller with step-up / slope limiting
is tracked as M28.5.

## Shape mapping

`CollisionShape` → Rapier `SharedShape` is one-to-one and lives in
[`convert.rs`](../../crates/physics/src/convert.rs):

| Engine variant | Rapier constructor |
|---|---|
| `Ball { radius }` | `SharedShape::ball` |
| `Cuboid { half_extents }` | `SharedShape::cuboid` |
| `Capsule { half_height, radius }` | `SharedShape::capsule_y` |
| `Cylinder { half_height, radius }` | `SharedShape::cylinder` |
| `ConvexHull { vertices }` | `SharedShape::convex_hull` (falls back to a tiny ball if Rapier rejects the hull as degenerate) |
| `TriMesh { vertices, indices }` | `SharedShape::trimesh` (falls back to a tiny ball if the mesh is empty) |
| `Compound { children }` | `SharedShape::compound` (recursive) |

The fallbacks matter — some NIF collision data is broken in ways that
only Rapier's BVH builder spots. A soft-failure keeps the cell loading
instead of panicking on one corrupt shape.

## Gravity and units

Gravity is `Vector::new(0.0, -686.7, 0.0)` — that's −9.81 m/s²
multiplied by 70 (Bethesda's 1 m ≈ 70 BU convention). The NIF importer
already removes Havok's 7.0 scale factor during extraction, so by the
time the shape reaches Rapier it's directly in Bethesda units and no
further conversion is needed.

## What M28 Phase 1 does NOT include

These are intentionally deferred:

- **Proper character controller** — we use a dynamic capsule body this
  milestone. A kinematic controller with step-up and slope limiting is
  M28.5.
- **Constraints / joints** — `bhkRagdollConstraint`,
  `bhkHingeConstraint`, `bhkLimitedHingeConstraint`, etc. are still
  parse-only (N23.6 skipped them). Ragdolls come with M29 alongside
  skeletal animation.
- **Collision events into scripting** — `ActivateEvent` / `HitEvent`
  plumbing stays deferred.
- **Havok MOPP BVtree consumption** — Rapier builds its own BVH over
  trimeshes, so the MOPP bytecode stays opaque.

See [ROADMAP.md](../../ROADMAP.md) for the full milestone plan.
