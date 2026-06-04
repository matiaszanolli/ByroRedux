# Investigation — #1409 NIFAL-S4 collision shape non-finite floats

**Domain:** nif (NIFAL collision translation) / safety

## Finding
`resolve_shape_inner` (`crates/nif/src/import/collision.rs`) built
`CollisionShape` primitives straight from raw NIF floats × havok_scale with no
finiteness check. A NaN/±Inf radius / half-extent / center from a corrupt or
adversarial NIF flows into the parry3d/Rapier collider builder, where it panics
or poisons the broadphase (cf. the existing "parry3d panics on nested compound"
catch_unwind guard).

## Fix
Added `finite(f32) -> Option<f32>` + `finite_vec(Vec3) -> Option<Vec3>` helpers
and guarded every primitive construction site with `?`:
- Ball (sphere radius), Cuboid (half-extents), Capsule (points + radius),
  Cylinder (points + radius), ConvexHull (any non-finite vertex → None).
- MultiSphere drops only the corrupt sub-sphere (continue); empty residue → None.
Non-finite → `None` from the construction site → the synthesized-trimesh
fallback (`spawn.rs`) fires, exactly as the issue recommends.

## SIBLING / CANONICAL-BOUNDARY
All six primitive arms covered (not just sphere). The same finite-guard pattern
already exists for particle emitters (`emitter_param_tests::non_finite_scalars_rejected`).
Guards live at the NIF→`CollisionShape` translation boundary — never pushed into
the physics crate or renderer.

## Tests
- `non_finite_sphere_radius_drops_to_none` (NaN / +Inf / -Inf)
- `finite_sphere_radius_resolves_to_scaled_ball` (control — guard doesn't reject valid radii)
- `non_finite_box_dimension_drops_to_none`

cargo test 2801 passed; no warnings.
