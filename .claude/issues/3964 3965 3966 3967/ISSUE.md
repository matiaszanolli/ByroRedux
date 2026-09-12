# Issues 3964, 3965, 3966, 3967

All four from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (7-dimension `/audit-physics` sweep, base `229306ce`). All MEDIUM.

## #3964 — PHYS-D6-2026-09-06-03 (Water/Buoyancy, ECS access declaration)
`#3492` (`1e4d83a7`) added a `world.query::<Ragdoll>()` read in
`apply_buoyancy_with_scratch` (`crates/physics/src/water.rs:747`) and
`clear_stale_water_contacts` (`:487`), but never updated `physics_sync_system`'s
`Access` declaration in `byroredux/src/boot.rs` (~1452-1494), nor the #3121
guard test `water_and_animation_parallel_accesses_are_complete`
(`byroredux/src/scheduler_access_tests.rs:148-172`). Fourth instance of this
exact class (#1787, #2676, PHYS-D3-2026-08-20-05 all fixed the same
declaration before). No live conflict today (nothing else reads/writes
`Ragdoll` in a parallel batch yet) but the scheduler's conflict detection is
silently blind to this read.
Fix: add `.reads::<byroredux_physics::Ragdoll>()` to the declaration with the
same "#3492 — buoyancy phase's second target source" comment style; extend
the guard test's needle list.

## #3965 — PHYS-D7-2026-09-06-01 (Queries & Diagnostics)
`spawn_collider_census_report` (`crates/physics/src/sync.rs:621-627`) hard-codes
`excluded_body: None` when casting the floor probe, even though
`cast_capsule_down_surface_and_normal`'s own doc says the parameter is
mandatory specifically because the origin can lie inside a body. Once
`PlayerMode::Character` exists (since `fa5f75f7`/#2876 exposed `phys.census`
as a live console command referencing the player's own position), the probe
capsule overlaps the player's own capsule and can self-hit, producing a
phantom `time_of_impact = 0` / `surface_y` above the player and both
`SpawnProbeVerdict` arms wrong. The sibling probe (`probe_walkable_floor_near`,
`byroredux/src/scene.rs:227-253`) already resolves and passes `Some(player)`.
Fix: add `excluded_body: Option<RigidBodyHandle>` to `SpawnCensusProbe`,
thread it into the cast; `PhysCensusCommand` resolves the player's
`RapierHandles.body` the same way `probe_walkable_floor_near` does. Pin with
a test: kinematic capsule at the probe origin → `NoHit` (not a phantom hit).

## #3966 — PHYS-D7-2026-09-06-02 (Queries & Diagnostics, same call site as #3965)
The console `phys.census` path (`byroredux/src/commands/physics.rs:129-132`)
hard-codes `authoring: None` with a comment "the live path has no NIF import
cache to sum" — false: `NifImportRegistry` is a `Resource`, the boot arm
(`byroredux/src/scene.rs:603-606`) reads it in two lines, and a sibling
console command (`commands/assets.rs:523`) already does
`world.try_resource::<NifImportRegistry>()` live. This disables the
classic/new_physics/phantom three-way split #2874 built specifically to
distinguish "nothing authored" from "dropped in translation" — on the one
route (`phys.census`) an operator can actually invoke live.
Fix: replace `authoring: None` with the two-line resource read from
`scene.rs:603-606`, delete the stale comment. Test: assert output does NOT
contain "authoring unavailable" when a `NifImportRegistry` resource exists.

## #3967 — PHYS-D7-2026-09-06-03 (Queries & Diagnostics, enhancement)
`SpawnCensusEntry` (`crates/physics/src/sync.rs:517-531`) narrows each
collider's full 3-D AABB (`NearbyCollider.aabb_min`/`aabb_max`, both
`[f32; 3]`) down to `center_y`/`min_y`/`max_y` before rendering — an
oversized-in-X/Z collider (the confirmed HIGH XSCL² bug, D1-01) is
byte-identical in `phys.census` output to a correctly-scaled one. No expected
value (authored scale/half-extent) is reported either, and the verdict enum
(`NoHit | RejectedNonWalkable | Walkable`) has no arm for "wrong size" — an
oversized-but-walkable collider reports as a probe/spawn disagreement
(blaming the probe), an undersized one reports as `NoHit` (blaming the
column census). `ragdoll.rs:895-901`'s own test proves the diameter
computation already exists, just not on the operator-facing surface.
Fix: replace `min_y`/`max_y`/`center_y` with the full `aabb_min`/`aabb_max`
pair, render `extent=[dx,dy,dz]` alongside `y=[…]`, and add the entity's
`GlobalTransform.scale` (one more query in the same guarded block).

## Domain
All `byroredux-physics` (`crates/physics`), with console-command call sites
in the binary crate `byroredux` (`commands/physics.rs`, `scheduler_access_tests.rs`,
`boot.rs`). #3965 and #3966 share the exact same call site
(`spawn_collider_census_report` / `phys.census`) — implement together per
#3965's own "Related" note.
