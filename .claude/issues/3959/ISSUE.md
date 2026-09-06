# PHYS-D1-2026-09-06-01: `synthesize_packed_havok_proxy` pre-bakes `ref_scale`, so FO4+/Starfield packed-Havok proxies get an `XSCL²` collider — the third site of #3064/#3065, closed on the first two only

Issue: #3959 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.


- **Severity**: HIGH
- **Dimension**: Shape Translation
- **Location**: `byroredux/src/cell_loader/spawn.rs:101-105` (the stale rationale), `:227` (the first application), `:277-291` (the parented ghost); consumer `crates/physics/src/sync.rs:907` → `crates/physics/src/convert.rs:246-251`
- **Status**: NEW — partial close of #3064 / #3065 (both CLOSED, fixed in `b8c4e6af`)
- **Trigger Conditions**: all of — (1) FO4 / FO76 / Starfield, so collision routes through the opaque `BhkSystemBinary` blob and `needs_packed_havok_fallback()` is true; (2) `collisions.is_empty()` — no decodable classic `bhk` body; (3) the REFR's base layer is `Clutter` or `Actor` (Architecture takes the precise `ArchitectureTriMesh` arm, which is correct); (4) at least one eligible mesh with finite geometry; (5) **the REFR's `XSCL` is not 1.0**. At `XSCL == 1.0` the bug is a no-op, which is why it survived four passes.
- **Description**: `collision_shape_to_parts` has owned scale application since #2860. `synthesize_packed_havok_proxy` predates that and bakes `ref_scale.abs()` into the cuboid half-extents itself, under a rationale that is now false and still written down: *"only the cuboid half-extents need the REFR scale baked because physics ignores `GlobalTransform::scale`."* Physics stopped ignoring it twelve days after that comment was written.
- **Evidence** — the full chain, each link re-verified by the orchestrator:
  1. Producer bakes: `let half_extents = ((max - min) * 0.5 * ref_scale.abs()).max(Vec3::splat(0.5));` (`spawn.rs:227`). `min`/`max` come from `transformed_mesh_aabb`, mesh-local TRS only.
  2. The ghost is **parented** with local scale 1.0: `world.insert(ghost, Transform::new(local_center, Quat::IDENTITY, 1.0)); world.insert(ghost, GlobalTransform::new(world_center, world_rot, 1.0)); … world.insert(ghost, Parent(placement_root)); add_child(...)` (`spawn.rs:277-291`).
  3. Propagation **overwrites** the seeded `1.0`. `placement_root` carries `GlobalTransform::new(ref_pos, ref_rot, ref_scale)` (`spawn.rs:851-854`); the BFS reaches the ghost and does `if let Some(g) = gq.get_mut(entity) { *g = composed; }` where `composed = GlobalTransform::compose(&parent_global, local.t, local.r, local.scale)` (`crates/core/src/ecs/systems.rs:316-325`), whose scale term is `parent_scale * local_scale` = `ref_scale`.
  4. Ordering is unconditional: cell loading runs after `scheduler.run(...)`, and `Stage::PostUpdate` (propagation) precedes `Stage::Physics`. `collect_newcomers` always reads the propagated value.
  5. Consumer applies again: `collision_shape_to_parts(&n.shape, n.global.scale, &cfg)` (`sync.rs:907`) → `SharedShape::cuboid(clamp_shape_extent(half_extents.x * scale), …)` (`convert.rs:246-251`).

  Net: **half-extents = authored_local × `ref_scale²`**.

  The sibling that was fixed: `synthesize_static_trimesh(positions, mesh_indices)` takes **no scale parameter** at HEAD, and `b8c4e6af`'s replacement test is named `placement_scale_is_applied_once_by_the_shared_converter`. The proxy's own test, `packed_proxy_bakes_outer_scale_into_cuboid_extent` (`spawn/synthesize_trimesh_tests.rs:234`), pins the **producer in isolation** at `ref_scale = 2.0` → `(2,4,6)` — it certifies the pre-bake rather than catching the doubling.
- **Impact**: every FO4 / FO76 / Starfield Clutter or Actor placement matching the trigger gets a proxy `XSCL²` the intended size. `XSCL 2.0` → a 4× cuboid: an invisible wall metres from anything visible. `XSCL 0.5` → a 0.25× cuboid the player walks through. The visual mesh is correct in both directions and nothing is logged — the same invisible-failure profile #3064 was filed HIGH for. Bounded, not catastrophic: `RT_ABSOLUTE_PRECISION_CEILING` and `clamp_shape_extent`'s `MAX_SANE_SHAPE_EXTENT` (both 2^20) cap the product, so this is wrong size, not broadphase poisoning.
- **Related**: #3064, #3065, #2860, #2355, #2543; **PHYS-D3-2026-09-06-02** (the doc rot that preserved the false premise — fix together); PHYS-D7-2026-09-06-03 (the diagnostic that cannot see this).
- **Suggested Fix**: drop `* ref_scale.abs()` from `spawn.rs:227` and let the shared converter own scaling, exactly as `b8c4e6af` did for the trimesh sibling. Keep `ref_scale` as a parameter (still needed for the `is_finite` gate and `local_center`). Rewrite the `:101-105` rationale — it is the false premise, not a comment. Convert `packed_proxy_bakes_outer_scale_into_cuboid_extent` into the end-to-end form its sibling already has: spawn the ghost under a scaled `placement_root`, run propagation + `physics_sync_system`, assert the resulting collider AABB.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
