# PHYS-D7-2026-09-06-03: the diagnostic channel has no *wrong-size* arm — `SpawnCensusEntry` discards two thirds of an AABB it already computed, and nothing reports placement scale

Issue: #3967 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.


- **Severity**: MEDIUM
- **Dimension**: Queries & Diagnostics
- **Location**: `crates/physics/src/sync.rs:517-531` (`SpawnCensusEntry`), `:644-670` (the lossy map), `:754-768` (the render line); source data `crates/physics/src/world.rs:122-135` (`NearbyCollider.aabb_min` / `aabb_max`, both `[f32; 3]`); `byroredux/src/commands/physics.rs:158-199` (`phys.stats`)
- **Status**: NEW. **Not a re-file of PHYS-D1-2026-09-06-01** — that is the defect; this is the answer to whether the channel meant to surface it can.
- **Trigger Conditions**: any investigation of *"collision does not match what I see"* where the collider exists, is Fixed or Keyframed, passes the walkable test, and is simply the wrong **size** — D1-01's `XSCL²` proxy, any future producer/converter scale drift, or a mis-decoded `bhk` half-extent.
- **Description**: three independent reasons the surface cannot express it. (1) **The XZ extents are computed and discarded** — `colliders_near_xz` returns a full 3-D AABB per collider; `spawn_collider_census_report` narrows it to `center_y` / `min_y` / `max_y` before anything is rendered, and the render line prints only `y=[min, max] centre=`. A cuboid four times too wide in X and Z and correct in Y is byte-identical in the output to a correct one. (2) **There is no expected value** — nothing reports the placement's `GlobalTransform.scale` / `XSCL` or the authored half-extent, so even the Y range that *is* printed has no reference. (3) **The verdict enum has no arm** — `NoHit | RejectedNonWalkable | Walkable`. An oversized-but-walkable collider lands in `Walkable`, which the census renders as *"the census probe and the spawn rungs disagree"*, blaming the probe; an undersized one lands in `NoHit`, whose text says *"walkability is not the cause; the column census below is"* — and the census then shows the collider present and looking normal.
- **Evidence**: the codebase already knows this query answers the scale² question — but only in a test. `byroredux/src/ragdoll.rs:895-901`: `let diameter = root[0].aabb_max[0] - root[0].aabb_min[0]; assert!((diameter - 20.0).abs() < 1e-3, "a 2× actor with an authored radius-5 limb needs diameter 20, not scale²: {diameter}");` — the X extent, the exact field the census drops one line after computing it. The *test* surface can catch scale²; the *operator* surface cannot. `phys.stats` adds nothing: `static_colliders_aabb` reports one world-wide envelope that an oversized collider inflates indistinguishably.
- **Impact**: the confirmed HIGH is invisible through the entire diagnostic channel — `phys.census` prints the collider and it looks normal, `phys.stats` prints a healthy envelope, no log fires. An operator hitting invisible walls on a scaled REFR has no console route to the evidence. That is a real gap in the channel whose stated purpose is answering *"why does collision not match what I see"*, and it costs two struct fields and one format string to close.
- **Related**: PHYS-D1-2026-09-06-01 (the defect this cannot see); #2202, #2874, #2875.
- **Suggested Fix**: carry the whole AABB — replace `min_y`/`max_y`/`center_y` with the `aabb_min`/`aabb_max` pair already in `NearbyCollider`, render `extent=[dx, dy, dz]` alongside the existing `y=[…]`, and add the owning entity's `GlobalTransform.scale` (the census already opens the ECS storages to resolve `entity` / `form` / `layer`, so it is one more query in the same guarded block). `scale != 1.0` on a census line is the single highest-value hint for this defect class.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
