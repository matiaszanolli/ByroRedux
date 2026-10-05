# #5352: PHYS-D2-2026-10-05-01: A recovery substep skips `clamp_explosive_velocities`, so every body the restore did not claim carries its exploded velocity into the next frame's integration unclamped

**Labels**: high,physics,test-gap,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5352

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-05.md` — `PHYS-D2-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: HIGH
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/world.rs:1295-1314` (the `if restored > 0 { …; break; }` arm and the comment
  above `self.clamp_explosive_velocities()`); compare `:846-878` (`recover_pre_broken_bodies`, non-finite only)
- **Status**: NEW. This is a third hole in the #5161 containment, after #5246's wake-rearm hole.
- **Trigger Conditions**: one substep in which the invalid-solve restore claims at least one body, while another
  dynamic body or articulation ends the same substep with a finite velocity above the cap but has not yet moved
  past `MAX_DYNAMIC_SUBSTEP_DISPLACEMENT`. The natural case is an exploding ragdoll: some links have already
  moved more than 2,048 BU and are restored, while sibling links have exploded only in velocity.
- **Description**: `step` runs the restore after each `pipeline.step`. When `restored > 0` it zeroes the
  accumulator and `break`s, and the clamp never runs. The comment justifies this: "The restore branch above already
  sanitises its bodies (rolled back + slept), so skipping the clamp there loses nothing". That is true only of the
  restored bodies themselves (`RigidBody::sleep` zeroes their velocities).
  - The restore claims a body only if it is non-finite or has moved more than 2,048 BU (`body_needs_recovery`).
  - In rapier 0.22, `VelocitySolver::solve_constraints` integrates positions and *then* runs the stabilization
    pass (`solve_wo_bias`, `solve_restitution_wo_bias`). That pass rewrites velocities that are only integrated on
    the next call. A body can therefore leave a substep with a finite, exploded velocity and a still-sane pose.
    This is exactly the mechanism the `VELOCITY_SANITY_CAP_BU_PER_S` doc describes ("explodes … within ONE
    `pipeline.step` call …; the *next* call's position integration then places its AABB near … the grid
    boundary").
  - Next frame, `recover_pre_broken_bodies` parks only non-finite state. The first `pipeline.step` then integrates
    the unclamped velocity and runs `detect_collisions` on the result inside the same call.
  - `remove_multibody_articulations` detaches the articulations that contain a restored body. Their un-restored
    links become free bodies that keep their exploded velocities. Articulations with no restored link keep their
    exploded generalized velocities, which forward kinematics integrates.
- **Evidence**: scratch probe, `PhysicsWorld::step` through the public API, two balls, indexed via
  `set_motion_type(Dynamic)`:
  ```
  [control]      steps=1 B |v|=20000 clamps=1 recovery=(0, 0, 0)
  [with restore] steps=1 B |v|=100000 sleeping=false clamps=0 recovery=(1, 1, 0) B.x=1667
  [next frame]   steps=1 B moved 1667 BU this frame (cap-bounded would be <= 333); clamps=1
  ```
  - B (1e5 BU/s) sits in the clamp-only window.
  - A (1e9 BU/s, unrelated) is restored in the same substep.
  - B's velocity survives the recovery substep untouched, and the next frame integrates it at full speed before
    the clamp runs.
  - With the stabilization-pass explosion magnitudes recorded in #5161 (|v| ≈ 7.7e15 BU/s), that next-frame
    integration is the multi-SAP boundary crossing (≈2.68e11) that panics `sap_axis.rs`. The panic itself was not
    reproduced: the public API cannot stage a stabilization-only explosion.
- **Impact**: the class #5161 and #5246 were filed to close returns as a process panic. It needs a restore and a
  velocity explosion in the same substep, which is the common shape of a ragdoll explosion rather than an exotic
  one. The skill's Dim 2 text says the clamp "runs at the end of every substep", which is false on exactly the
  substeps where an explosion is in progress.
- **Related**: #5161, #5246 (closed); #4687 (restore detach); #4772 (the FO3 restore path fires the restore);
  PHYS-D2-2026-10-05-02.
- **Suggested Fix**: run `clamp_explosive_velocities()` in the restore arm too, before the `break`. It is
  idempotent on the restored bodies, which are already zeroed and asleep. Then correct the comment. Pin the fix
  with the probe's two-body shape as a `world.rs` test: one body restored and one in the clamp window in the same
  substep, then assert the second body is capped and counted.

## Publisher note

Most likely real-content trigger: the open #4772 (FO3 copied-save restore's first ragdoll solve), which is the live path that fires the invalid-solve restore.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
