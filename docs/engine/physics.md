# Physics

ByroRedux runs a Rapier3D-backed physics simulation on top of the
`CollisionShape` / `RigidBodyData` components the NIF importer has been
populating since N23.6. The bridge lives in its own crate
(`byroredux-physics`) so that `core` stays physics-agnostic and the
loose-NIF viewer path can opt out just by not inserting the
`PhysicsWorld` resource.

Source: [`crates/physics/src/`](../../crates/physics/src/)

> History note: this doc opened as the M28 Phase 1 ("make the parsed
> collision data live") writeup. The simulation has since grown a
> kinematic character controller (M28.5), a one-collider-per-part spawn
> path (#373), a render-geometry trimesh fallback for FO4+/Starfield
> architecture, and a `ContactConfig` resource that unifies the contact
> tunables. Sections below are reconciled against the tree as of Session
> 42 / 2026-05-28; the milestone-by-milestone narrative is kept and
> date-stamped where it matters.

## At a glance

| | |
|---|---|
| Backend             | [Rapier3D 0.22](https://rapier.rs) (`simd-stable`) over parry3d 0.17.6 |
| Fixed tick rate     | 60 Hz with a sub-step accumulator, capped at 5 substeps/frame (`PHYSICS_DT`, `MAX_SUBSTEPS`) |
| Gravity             | −686.7 BU/s² (≈ −9.81 m/s² scaled to Bethesda units, 1 m ≈ 70 BU) |
| Units               | Bethesda units throughout — Rapier never sees metres |
| Character           | M28.5 kinematic capsule controller — gravity + collide-and-slide + jump |
| Tests               | 21 in `crates/physics` + 8 in the controller (`byroredux/src/systems/character.rs`) |

## Data pipeline

Physics is additive on top of everything the importer already does:

```
NIF (.nif)
  └─ parse_nif()                              crates/nif/src/
      └─ bhk*CollisionObject → bhkRigidBody → bhkShape chain
         └─ import/collision.rs::extract_collision  crates/nif/src/import/collision.rs
            └─ (CollisionShape, RigidBodyData)       physics-agnostic ECS components
               └─ cell_loader / scene spawns          byroredux/src/cell_loader/spawn.rs
                  - Transform + GlobalTransform
                  - CollisionShape
                  - RigidBodyData
                     └─ physics_sync_system registers in Rapier
                        - RigidBodyHandle + ColliderHandle(s) → RapierHandles
```

No parser or importer changes were needed for M28 Phase 1 — the
collision data was already sitting on entities since N23.6, it just
wasn't being read by anything. The physics system is the thing that
makes it live. (FO4 / FO76 / Starfield are the exception — see
[Synthesized static-trimesh fallback](#synthesized-static-trimesh-fallback)
below.)

## Crate layout

```
crates/physics/src/
├── lib.rs         Crate root, re-exports
├── config.rs      ContactConfig resource — TriMesh flags, contact skin, KCC offset
├── convert.rs     glam ↔ nalgebra conversions + collision_shape_to_parts
├── components.rs  RapierHandles (body + collider) + CharacterController (M28.5)
├── world.rs       PhysicsWorld resource + KCC move_character / cast_ray_down helpers
└── sync.rs        physics_sync_system — the 4-phase per-tick bridge
```

All Rapier types are confined to this crate. Engine code talks glam;
`convert.rs` does the Vec3 / Quat / Isometry3 translation at the
boundary and never leaks nalgebra types outside the crate. The handful
of helpers that *must* surface a Rapier handle (the KCC's
`exclude_collider`) keep it behind a `CharacterMoveParams` field so
callers still don't `use rapier3d::prelude::*`.

The public surface (`lib.rs`):

- `components::{CharacterController, RapierHandles}`
- `config::{ContactConfig, TriMeshFlagBits}`
- `sync::{physics_sync_system, set_kinematic_translation, set_linear_velocity}`
- `world::{CharacterMoveParams, CharacterMoveResult, PhysicsWorld, PHYSICS_DT}`

## The physics_sync_system

The system lives in [`sync.rs`](../../crates/physics/src/sync.rs) and
runs in `Stage::Physics`, after transform propagation. It early-returns
if no `PhysicsWorld` resource is present (the loose-NIF viewer opt-out).
It's structured as four phases:

**Phase 1 — Register newcomers.** Collect every entity that has
`(CollisionShape, RigidBodyData, GlobalTransform)` but not yet a
`RapierHandles` into a `Vec<Newcomer>` (so the read locks release before
the `PhysicsWorld` + `RapierHandles` write locks are taken). For each:

- Map `RigidBodyData.motion_type` to a Rapier `RigidBodyType` via
  `motion_type_to_rapier`: `Static → Fixed`, `Keyframed →
  KinematicPositionBased`, `Dynamic → Dynamic`, `CharacterKinematic →
  KinematicPositionBased`.
- Set initial position from `GlobalTransform` via `iso_from_trs`, and
  apply `linear_damping` / `angular_damping` from `RigidBodyData`.
  `CharacterKinematic` bodies additionally lock rotations (so the player
  stays upright); everything else follows the body data verbatim.
- Flatten the engine shape into a `Vec<(Isometry3, SharedShape)>` via
  `convert::collision_shape_to_parts` and attach **one `Collider` per
  part** — friction / restitution / a `default_contact_skin_bu` margin
  copied onto each, and the body's mass distributed evenly across parts.
  This one-collider-per-part shape is the #373 fix; see
  [Shape mapping](#shape-mapping).
- Attach a `RapierHandles { body, collider }` component (the first
  collider is the representative handle the ECS keeps; Rapier owns the
  rest through the parent-body relationship) so the entity is skipped on
  subsequent frames.

There is no longer a separate "synthesize a capsule for an unshaped
`PlayerBody` marker" branch. As of M28.5 the player character is spawned
explicitly (capsule `CollisionShape` + `CharacterKinematic`
`RigidBodyData`) and flows through this same unified newcomer path — see
[Player character (M28.5)](#player-character-m285).

**Phase 2 — Push kinematic transforms.** For every `Keyframed` body
(doors, lifts, scripted props), copy the current `GlobalTransform` into
Rapier via `set_next_kinematic_position`. `CharacterKinematic` bodies are
**skipped here** — they're driven explicitly by the controller's
`set_kinematic_translation` call, and pushing the ECS transform would
race the KCC-corrected pose write.

**Phase 3 — Step.** Call `PhysicsWorld::step(dt)`, which drains the
accumulator at the fixed 60 Hz tick rate (see below). Logs at `trace`
when more than one substep ran.

**Phase 4 — Pull dynamic transforms back.** For every `RapierHandles`
entity whose `RigidBodyData.motion_type == Dynamic`, read the
`Isometry3` out of Rapier and write it back into the local `Transform`
via `quat_from_na` / `vec3_from_translation`. Static / keyframed /
character bodies are driven the *other* way (engine → Rapier) so they
never flow back. Updates are collected into a `Vec` before the
`Transform` write lock is taken.

## Fixed tick with accumulator

Rapier is a variable-dt solver in theory but stability and
determinism are much better if you feed it a fixed step. `PhysicsWorld`
owns an accumulator that collects wall-clock `dt`, drains it in
`PHYSICS_DT = 1.0/60.0` slices, and caps at `MAX_SUBSTEPS = 5` per
frame to prevent spiral-of-death on hitches:

```rust
self.accumulator += frame_dt.max(0.0);
let max_acc = MAX_SUBSTEPS as f32 * PHYSICS_DT;
if self.accumulator > max_acc {
    self.accumulator = max_acc;       // drop oldest
}
let mut steps = 0u32;
while self.accumulator >= PHYSICS_DT && steps < MAX_SUBSTEPS {
    self.pipeline.step(&self.gravity, &self.integration_parameters, /* … */);
    self.accumulator -= PHYSICS_DT;
    steps += 1;
}
```

`step()` returns the number of substeps it ran. 60 Hz matches Skyrim and
FO4's internal physics rate. The cap means worst case is a 5× frame
skew recovery window, beyond which the engine visibly slows down but
never catastrophically blows up.

## Player character (M28.5)

The M28 Phase 1 player was a **dynamic capsule** steered by writing
`set_linear_velocity` — enough to prove the pipeline end-to-end, but it
fought `physics_sync_system` Phase 4 over `Transform` writes and never
behaved well for gameplay. **M28.5 replaced it with a kinematic
character controller** (`byroredux/src/systems/character.rs`):

- `scene::setup_scene` spawns a player body entity carrying
  `CharacterController::HUMAN`, a `CollisionShape::Capsule`, and a
  `RigidBodyData { motion_type: CharacterKinematic, .. }`, and records
  the entity in the `PlayerEntity` resource. It is gated on
  `PlayerMode::Character` (interior cell / exterior grid / explicit
  `--player`; `--mesh` / `--tree` / `--fly` boot in `FlyCam`).
- `physics_sync_system` Phase 1 registers it as a
  `KinematicPositionBased` Rapier body with rotations locked.
- `character_controller_system` (`Stage::Early`) integrates gravity +
  jump, builds a `desired_translation`, asks Rapier's
  `KinematicCharacterController.move_shape` (via
  `PhysicsWorld::move_character`) for the collide-and-slide-corrected
  motion, writes the result onto the body `Transform`, and pushes it
  into Rapier with `set_kinematic_translation`.
- `camera_follow_system` (`Stage::Late`, after the physics step) pins
  the active camera to `body_pos + eye_height * Y`, with rotation from
  `InputState.{yaw, pitch}`. It writes both `Transform` and
  `GlobalTransform` on the camera because there's no propagation pass
  left in `Late`, and the audio listener / water submersion both read
  `GlobalTransform` later in the frame.

`fly_camera_system` and `character_controller_system` are
runtime-mutually-exclusive (each early-returns on the wrong
`PlayerMode`). Registering them as separate parallel-stage systems made
the M27 declared-access analyzer pair them and surface a `Transform` +
`PhysicsWorld` `WriteWrite` conflict that's structurally impossible at
runtime, so they're folded under one `player_controller_system`
dispatcher whose declared access is the union of both. `F` toggles
between modes (`toggle_player_mode`, modelled on Bethesda's `tcl`): on
Fly → Character it snaps the body to the camera's position minus
`eye_height` and re-arms gravity.

### Controller tuning

`CharacterController::HUMAN` is sized to a vanilla-Skyrim Nord male:

| Field | Value | Notes |
|---|---|---|
| `half_height` | 46.0 BU | capsule, excludes caps |
| `radius` | 18.0 BU | |
| `eye_height` | 52.0 BU | camera mount above body centre |
| `move_speed` | 220.0 BU/s | ~3.14 m/s; `ControlLeft` sprints ×2 |
| `jump_velocity` | 380.0 BU/s | |
| `gravity` | −1373.4 BU/s² | 2× `PhysicsWorld` gravity, for snappier arcade jumps |
| `terminal_velocity` | −2000.0 BU/s | clamp so high-`dt` falls don't tunnel thin floors |
| `step_height` | 32.0 BU | auto-step; covers canonical Bethesda stairs |
| `max_slope_climb_deg` | 50° | matches Bethesda NavMesh slope limit |
| `snap_to_ground` | 32.0 BU | holds the capsule on terrain rolls |

Runtime state (`vertical_velocity`, `is_grounded`, `wants_jump`) lives
on the same component and is written each frame by the controller.

Two subtleties worth keeping in mind when reading the controller:

- **First-frame `dt` clamp.** The controller clamps `dt` to 1/30 s. The
  first scheduler tick after boot ships a `dt` equal to wall-clock from
  `App` construction to first frame — for a Whiterun cell load that's
  ~8 s of BSA decode + NIF parse + Vulkan upload. Unclamped, gravity ×
  dt would teleport the character ~15 km below the cell on frame 0.
- **Grounded probe instead of gravity integration.** When grounded and
  not jumping, the controller sends a fixed `-step_height` downward
  probe rather than the gravity-integrated motion, so Rapier's
  `snap_to_ground` re-engages every frame without accumulating drift
  that would otherwise let the capsule creep through inclined floor
  trimeshes after a few hundred frames.

### Spawn placement

Because the player must spawn on real architecture, `setup_scene` runs
one early `physics_sync_system(world, 0.0)` (register-only, no step),
calls `PhysicsWorld::update_query_pipeline()` to flush the BVH (the
query pipeline otherwise only learns about new colliders as a
side-effect of `pipeline.step()`), then picks a spawn position in
precedence order:

1. **Door teleporter** — the `Transform` of any `DoorTeleport` (XTEL)
   REFR, nudged inward toward the static-collider AABB centre by 64 BU
   so the capsule lands on architecture rather than projecting off a
   thin threshold floor. (#1295: when there are no static colliders the
   inward nudge degrades to zero and the spawn log warns explicitly.)
2. **Ray-cast down** — `PhysicsWorld::cast_ray_down` from `aabb.max.y +
   50` against fixed colliders only, placing the capsule above the first
   solid floor.
3. **AABB + slack** — `aabb.max.y + 200` when the ray finds nothing.
4. **No static colliders** — bare `cam_pos − eye_height`.

`PhysicsWorld::static_colliders_aabb()` (the diagnostic backing this and
the controller's frame-0 sanity log) walks every fixed-body collider and
returns the combined AABB + count, or `None` if there are no statics.

## Shape mapping

`convert::collision_shape_to_parts(shape, cfg)` flattens a
`CollisionShape` into a **flat `Vec<(Isometry3, SharedShape)>`** — every
`Compound` tree is walked depth-first and its leaves are emitted as
individual parts with composed transforms. `physics_sync_system` then
attaches one `Collider` per part. This replaced the original
single-`SharedShape::compound` mapping: parry forbids
composite-inside-compound (TriMesh / HeightField / Polyline / Compound),
and the old path fired a "Nested composite shapes are not allowed" panic
storm (~9,555 panics / 30 s on exterior cells) on Oblivion `bhkListShape`
chains and any TriMesh-in-compound mix. One-collider-per-part is Rapier's
idiomatic answer and works for every valid mix. See #373.

| Engine variant | Rapier part |
|---|---|
| `Ball { radius }` | `SharedShape::ball` (clamped to ≥ 1e-3) |
| `Cuboid { half_extents }` | `SharedShape::cuboid` |
| `Capsule { half_height, radius }` | `SharedShape::capsule_y` |
| `Cylinder { half_height, radius }` | `SharedShape::cylinder` |
| `ConvexHull { vertices }` | `SharedShape::convex_hull` (falls back to a tiny ball if Rapier rejects the hull as degenerate) |
| `TriMesh { vertices, indices }` | `SharedShape::trimesh_with_flags` using `ContactConfig::trimesh_flags` (falls back to a tiny ball if the mesh is empty) |
| `Compound { children }` | depth-first flatten — emits N primitive/mesh parts, never a nested compound |

An input that produces no viable leaves (an empty compound) still emits
a single tiny-ball part so the caller can register a collider rather
than skipping the entity. The soft fallbacks matter — some NIF collision
data is broken in ways only Rapier's BVH builder spots, and a
soft-failure keeps the cell loading instead of panicking on one corrupt
shape.

### ContactConfig

`config::ContactConfig` is an ECS resource that unifies the contact
tunables that previously lived as inline literals at three sites
(`convert.rs` TriMesh flags, `sync.rs` per-collider skin, `world.rs` KCC
offset). Defaults match the pre-unification inline values:

| Field | Default | Meaning |
|---|---|---|
| `trimesh_flags` | `FIX_INTERNAL_EDGES` | transitively ORs in `ORIENTED \| MERGE_DUPLICATE_VERTICES`; fixes per-edge normal flips at shared triangle seams so a sliding capsule isn't pushed *through* a wall |
| `default_contact_skin_bu` | 1.0 BU | per-collider Rapier margin — a stable gap the narrow phase resolves penetration from |
| `kcc_offset_bu` | 4.0 BU | `KinematicCharacterController.offset`; at 70 BU/m a 0.5 BU skin was only 7 mm and let the swept cast graze TriMesh edges |

`TriMeshFlagBits` mirrors `rapier3d::parry::shape::TriMeshFlags` (u16)
1:1, with pin tests in `config.rs` asserting the bit values against
parry's definitions so a Rapier upgrade can't silently change what gets
applied at collider creation.

## NIF collision extraction

`crates/nif/src/import/collision.rs` walks the bhk shape tree and
produces the physics-agnostic `(CollisionShape, RigidBodyData)`.
`extract_collision` dispatches on the concrete `bhk*CollisionObject`
subclass, which is effectively the per-game boundary:

| Block | Game line | Extractable today |
|---|---|---|
| `BhkCollisionObject` → `BhkRigidBody` | Universal (dominant pre-FO4) | **yes** (`extract_from_classic`) |
| `BhkNPCollisionObject` | FO4 / FO76 / Starfield ("Niagara Physics") | **no** — Havok-serialised blob; surfaced as `CollisionAuthoring::NewPhysicsStub`, render-geometry trimesh fallback fires instead |
| `BhkPCollisionObject` | Skyrim+ trigger volumes / phantoms | **no** — `CollisionAuthoring::Phantom`; needs a dedicated `TriggerVolume` ECS path |

`examine_collision_kind` returns the `CollisionAuthoring` discriminator
(`None` / `Classic` / `NewPhysicsStub` / `Phantom` / `Unrecognised`)
without attempting extraction, so telemetry and fallbacks can tell "FO4
NP collision authored but undecodable" from "no collision authored".

The classic path's `resolve_shape` handles every parsed `bhk*Shape`
variant and maps each to a `CollisionShape`. The **NIFAL collision audit
(Session 42)** closed two long-standing leaks where parsed shapes fell
through to the "unsupported shape" `debug!` and the authored collision
silently vanished:

- `BhkMultiSphereShape` → a `Compound` of `Ball` children, one per
  offset sphere (a single centred sphere collapses to a bare `Ball`).
- `BhkConvexListShape` → a `Compound` of resolved convex sub-shapes
  (FO3 / FNV / Skyrim destructibles + debris), mirroring `BhkListShape`.

The full live set is: `Sphere`, `MultiSphere`, `Box`, `Capsule`,
`Cylinder`, `ConvexVertices`, `MoppBvTree` (skips MOPP, recurses into the
wrapped shape — Rapier builds its own BVH), `List`, `ConvexList`,
`Transform`, `NiTriStrips`, `PackedNiTriStrips`, `CompressedMeshShape`
(Skyrim+), and `SimpleShapePhantom`. The remaining non-leaks are the
two undecodable container kinds in the table above, not parsed-but-dropped
shapes.

Havok coordinates are Z-up and live in **Havok metres**, so the importer
**multiplies** every position / radius by the per-game Havok scale and
converts (x, z, −y) to engine Y-up. The scale is detected at parse time
by `havok_scale_for` and stored on `NifScene::havok_scale`: **7.0** for
Morrowind / Oblivion / FO3 / FNV, **69.99125** for Skyrim LE/SE / FO4 /
FO76 / Starfield (unknown variants fall back to 7.0). By the time a
shape reaches Rapier it's already in Bethesda units — no conversion
happens in the physics crate.

> Correction (was stale in the M28 writeup): the importer does **not**
> "remove" a 7.0 Havok scale — it *applies* `havok_scale` (7.0 or
> 69.99125 depending on game) to convert Havok metres to Bethesda units.

## Synthesized static-trimesh fallback

FO4 / FO76 / Starfield moved static-architecture collision into the
Havok content-system blob (`bhkNPCollisionObject` → `bhkPhysicsSystem`),
which `extract_collision` does not deserialize yet (a multi-day project).
Without any static collider the M28.5 character controller has nothing
to ground against and the player falls through the floor.

`cell_loader/spawn.rs::synthesize_static_trimesh` builds a static
`CollisionShape::TriMesh` + `RigidBodyData::STATIC` from the render
mesh's geometry, baking the composed `ref_scale × mesh.scale` into the
vertices (the physics sync places bodies by translation + rotation only;
bhk shapes already bake their scale at extract time, so loose render
verts must be pre-scaled to match). The render mesh is a coarse but
serviceable stand-in for the authored hull on structural architecture.

The fallback is gated tightly so it never turns clutter, decals, or
skinned actors into expensive trimesh colliders. It fires only when:

- `collisions.is_empty()` — the NIF authored no bhk shape (so FNV / FO3 /
  Skyrim aren't double-covered),
- the REFR's **`base_layer`** is `RenderLayer::Architecture` — and note
  this is the *pre-escalation* record-type classification, **not** the
  post-escalation render layer (#1294: the small-STAT → Clutter render
  z-bias escalation is a rendering optimization that was stripping
  colliders off Starfield's per-LOD per-material sub-decomposed walls,
  each sub-mesh < 50 units but composing into 1000-unit architecture),
- `mesh.skin.is_none()` — never synthesize for animated bodies,
- `!mesh.is_decal && !mesh.alpha_test` — skip overlay planes,
- there's at least one triangle of geometry.

This (plus the #1294 gate fix) is what makes the M28.5 character walk on
Starfield's Cydonia and other FO4+ cells rather than free-falling from
frame 0.

## Gravity and units

`PhysicsWorld` gravity is `Vector::new(0.0, -686.7, 0.0)` — that's
−9.81 m/s² × 70 (Bethesda's 1 m ≈ 70 BU convention). All collision
geometry is already in Bethesda units by the time it reaches the physics
crate (see [NIF collision extraction](#nif-collision-extraction)), so no
further conversion is needed inside Rapier.

## What the physics layer does NOT include yet

These are intentionally deferred:

- **Constraints / joints** — `bhkRagdollConstraint`,
  `bhkHingeConstraint`, `bhkLimitedHingeConstraint`, etc. are still
  parse-only (N23.6 skipped them); the importer does not emit joint data.
  Ragdolls remain future work alongside skeletal animation.
- **FO4+ Havok content-system blob** — `bhkNPCollisionObject`'s
  serialised physics system isn't decoded; the render-geometry trimesh
  fallback above stands in for static architecture.
- **Trigger volumes / phantoms** — `bhkPCollisionObject` wrapping a
  `bhkPhantom` needs a dedicated `TriggerVolume` ECS path rather than a
  rigid body.
- **Collision events into scripting** — `ActivateEvent` / `HitEvent`
  plumbing from physics contacts stays deferred.
- **Havok MOPP BVtree consumption** — Rapier builds its own BVH over
  trimeshes, so the MOPP bytecode stays opaque (`BhkMoppBvTreeShape`
  recurses straight into its wrapped shape).

See [ROADMAP.md](../../ROADMAP.md) for the full milestone plan.
