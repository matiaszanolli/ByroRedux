# #5353: PHYS-D2-2026-10-05-02: The articulation-DOF clamp caps a dynamic ragdoll root's free-joint LINEAR velocity at 100 BU/s, so every ragdoll falls at about 1.4 m/s and floods the explosion telemetry

**Labels**: medium,physics,test-gap,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5353

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-05.md` — `PHYS-D2-2026-10-05-02` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM. The behaviour is wrong in every game with ragdolls, and the containment's diagnostics are
  corrupted. No crash.
- **Dimension**: Step & Sync (user-visible in Ragdoll & Constraint Seam; diagnostics in Queries & Diagnostics)
- **Location**: `crates/physics/src/world.rs:91-99` (`ARTICULATION_DOF_SANITY_CAP` and its doc),
  `:1071-1097` (the DOF loop); `crates/physics/src/ragdoll.rs:383-398` (every ragdoll body
  `RigidBodyBuilder::dynamic()`)
- **Status**: NEW (introduced by `5ae7f8ad4`, #5161)
- **Trigger Conditions**: any activated ragdoll whose root's speed along one world axis exceeds 100 BU/s. That
  includes any fall longer than about 0.15 s (gravity is 686.7 BU/s²), a corpse dropping off a ledge, and an actor
  killed mid-air.
- **Description**: the cap's doc lists the DOFs it is meant for: "rad/s for the ragdoll/hinge joints' angular axes,
  BU/s for the prismatic rail; every authored class moves far slower than this". The doc omits the root.
  - `build_ragdoll` makes every body dynamic, and nothing in production pins the root. A grep of
    `byroredux/src/ragdoll.rs` finds no `set_body_type` or `Fixed`.
  - Rapier's `Multibody::with_root` / `update_root_type` therefore gives the multibody a `MultibodyJoint::free`
    root with `SPATIAL_DIM` = 6 DOFs at the front of `generalized_velocity`. The first three are the root's linear
    velocity in BU/s.
  - The loop clamps every entry to ±100 regardless of which joint owns it.
- **Evidence**: scratch probe. A production `build_ragdoll` with three ball links, ragdoll joints and
  `ContactConfig::DEFAULT`, seeded at y = 10,000 BU with no floor, run for 120 frames:
  ```
  [free body]  2 s fall: dropped 1376 BU, |v|=1373 BU/s, clamps=0
  [ragdoll] frame  10: root dropped 10 BU,  root DOFs[0..6]=[~0, -100.0, 0, 0, 0, ~0], clamps=2
  [ragdoll] frame  30: root dropped 45 BU,  root DOFs[0..6]=[~0, -100.0, 0, 0, 0, ~0], clamps=22
  [ragdoll] frame 120: root dropped 204 BU, root DOFs[0..6]=[~0, -100.0, 0, 0, 0, ~0], clamps=112
  free-fall reference: 2 s => 1373 BU, v=1373 BU/s
  ```
  The existing guards do not see this:
  - `exploding_articulation_dofs_are_capped_before_forward_kinematics` asserts only an upper bound.
  - `ragdoll_chain_swings_but_stays_jointed` pins the root `Fixed`, so its multibody has no free joint.
  - No test asserts that a ragdoll falls at gravity.
- **Impact**:
  - Every ragdoll in every game descends at 100 BU/s or less per axis. A corpse falling 1,000 BU takes about 10 s
    instead of about 1.7 s.
  - Every substep of that descent adds one to `velocity_clamps_total`, the `phys.stats` "velocity clamps" line
    that #5161/#5246 rely on, and logs `warn` "clamped N exploding articulation DOF velocities". That is about 60
    lines per second per falling corpse, and a normal death now looks like a solver explosion in the telemetry.
  - Ragdoll seeding passes no velocity from the live animation, so horizontal momentum is not affected today.
- **Related**: #5161, PHYS-D2-2026-10-05-01, PHYS-D2-2026-10-05-04.
- **Suggested Fix**: skip the root free joint's six DOFs. A dynamic-root multibody's first `SPATIAL_DIM` entries
  belong to `links[0]`; read `link(0).joint().ndofs()` rather than hard-coding 6. Those DOFs are already covered by
  the body-level clamp at 20k BU/s and 100 rad/s, because the root is a dynamic body in `dynamic_bodies`.
  Alternatively, clamp the root's linear DOFs at `VELOCITY_SANITY_CAP_BU_PER_S`. Add a free-fall test on a
  `build_ragdoll` articulation that asserts a gravity-rate drop and `velocity_clamps_total() == 0`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
