# #5488: PHYS-D2-2026-10-09-01: A ragdoll explosion is integrated and broad-phased inside ONE `pipeline.step`, so no #5161/#5246 guard can run before rapier panics (live Skyrim SE `sap_axis.rs:61` crash)

**Labels**: bug, game:skyrim, high, physics

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-09.md` — finding `PHYS-D2-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: HIGH. A process-killing panic under ordinary exterior play (Skyrim SE grid 0,0 radius 2, about
  3.5 min in). Nothing in the engine can intercept it today. This is the same class #5352 is rated for.
- **Dimension**: Step & Sync
- **Location**:
  - `crates/physics/src/world/mod.rs:942-972`: the `pipeline.step` call. Hooks are `&()` at `:970`.
  - `crates/physics/src/world/mod.rs:974-998`: the restore and clamp, which run only after the call returns.
  - `crates/physics/src/world/mod.rs:74-101`: the cap docs that state the wrong failure model.
  - `crates/physics/src/world/recovery.rs:284-441`: `clamp_explosive_velocities`. The wrong NaN model is at
    `:301-309`.
  - `crates/physics/src/broad_phase.rs:44-63`: `FixedPairFilterBroadPhase::update` delegates with no AABB check.
  - `crates/physics/src/ragdoll.rs:391`: `.additional_solver_iterations(12)`.
- **Status**: NEW. This is a residual of the closed #5161/#5246 class, and it is distinct from #5352. No issue
  matched in the open snapshot. Closed-issue searches for "in-call explosion pipeline.step broad phase" and
  "sap_axis" returned only #5161.
- **Trigger Conditions**: an activated ragdoll whose solve blows up on any internal TGS substep except the last.
  The input state can be fully sane: finite, within the restore bound, and inside every cap. In the field this was
  a streamed Skyrim humanoid corpse (actor 16468, `meshes\Actors\Character\Character Assets\skeleton.nif`,
  18 bodies), 52 s after its cell streamed in.
- **Description**: the #5161 containment assumes that an explosion born in one `pipeline.step` call reaches
  positions only in the *next* call. It is built on that assumption: `VELOCITY_SANITY_CAP_BU_PER_S`'s doc says
  "the *next* call's position integration then places its AABB near … the grid boundary". So it clamps
  velocities, and restores displaced bodies, between calls. Rapier 0.22 does not behave that way.
  - **Many integrations per call.** `island_solver.rs:45-49` runs
    `num_solver_iterations + additional_solver_iterations` internal substeps at `dt / n`. That is 4 (the default,
    which the engine never overrides) + 12 for a ragdoll island, giving 16.
  - `velocity_solver.rs:161-221` integrates positions in every one of them, including multibody forward
    kinematics (`:247-258`), and then runs the stabilization pass.
  - A velocity that blows up in internal substep *k* is therefore integrated in substeps *k+1…15* of the same
    call. The #5161 premise holds only when the blow-up is born in the last substep's stabilization pass.
  - **Collision detection inside the same call.** `physics_pipeline.rs:613-640` runs
    `advance_to_final_positions` and then `detect_collisions` (broad phase, then the narrow phase's
    `compute_contacts`) before `step` returns. The engine supplies no code at that point: hooks are `&()`, and
    `FixedPairFilterBroadPhase::update` forwards to the multi-SAP without looking at AABBs.
  - **First-rung response is too weak.** Rapier overwrites a multibody link's `RigidBody` velocity from the
    generalized velocities at every solve (`velocity_solver.rs:137` → `multibody.rs:503,521`). So the body-level
    clamp on the 18 bones was a no-op, which the test comment at `world/mod.rs:1576-1578` already concedes. Only the
    DOF clamp bounded the rig. Offence rung 1 also leaves the exploding constraint configuration (the pose is not
    rolled back) to be re-solved on the very next call. In the field, that next call was fatal.
  - **The failure model in the docs is wrong.** Two statements need correcting:
    - "≈2.68e11 multi-SAP grid boundary" is not a clamp. `clamp_point` bounds at ±`f32::MAX/4`
      (`sap_utils.rs:22-24`). The 2.68e11 figure is `point_key`'s saturating `floor() as i32` at the 125-BU region
      width: `i32::MAX × 125` = 2.684e11. The threshold is about 2.1e9 for a 1-BU layer.
    - `recovery.rs:301-309` says a NaN AABB "clamps to finite grid corners". In fact
      `handle_modified_collider` rejects non-finite AABBs before `clamp_point`
      (`broad_phase_multi_sap.rs:376-385`).

    The panicking proxy fits the real model: it is a degenerate point AABB
    `[2.684e11, 1.193e11, 2.684e11]` with mins equal to maxs. That is a region created at the saturated key, which
    trips `batch_insert`'s bounds assert when it propagates to the larger layer.
- **Evidence**:
  ```
  :1697 ERROR physics: invalid-solve evidence (first of 18): … [actor 2443 bone npc com] at |t|=1.947e4 |v|=2.958e6 …
  :1698 ERROR physics: restored 18 dynamic body/bodies after an invalid solve …        ← the run's ONLY restore
  :4111 WARN  physics: clamped 1 exploding articulation DOF velocities …               (00:53:27)
  :4112-4129 WARN physics: clamped explosive velocity on … [actor 16468 bone npc com / l thigh / … / l hand] (18 bones, 1st offence)
  :4130 WARN  physics: clamped 40 exploding articulation DOF velocities to the sanity cap (#5161)
  :4132 thread 'main' panicked at rapier3d-0.22.0/src/geometry/broad_phase_multi_sap/sap_axis.rs:61:13:
        proxy.aabb.maxs 268435460000 (in Aabb { mins: [268435460000.0, 119289000000.0, 268435460000.0],
        maxs: [268435460000.0, 119289000000.0, 268435460000.0] }) >= min_bound 268435470000
  ```
  Scratch probe (`/tmp/audit/physics/probe/src/main.rs`):
  - Setup: raw rapier 0.22 with the engine's `IntegrationParameters`, 686.7 gravity and a fixed ground cuboid.
    The rig is a multibody chain of spherical joints with `additional_solver_iterations(12)`.
  - Between calls the probe applies the engine's body and DOF clamp, and it stops at the first call that moves any
    link more than 2,048 BU. So every call it runs starts from a state the engine's guards accept.
  - Results:
    - With 12 extra iterations, single calls moved a link **1.6e11 to 2.2e16 BU** on frames 1-3. Examples: `n=6 ratio=2000 pen=20 → 5.6e11`;
      `n=12 ratio=20 pen=20 → 1.4e15`.
    - `n=12 ratio=2000 pen=60` (balls) **panicked inside the call** at `parry3d-0.17.6
      clip_aabb_line.rs:141` ("Matrix index out of bounds"). The path was `physics_pipeline.rs:616` →
      `narrow_phase.rs:941`, the same-call narrow phase on an existing contact pair.
  - The probe rig is synthetic (tight limits, inverted masses). It proves the mechanism, not the content trigger.
    The `sap_axis` assert itself was not reproduced; it depends on a rounding window.
- **Impact**:
  - Any activated ragdoll in any game can kill the process. Two Skyrim humanoid rigs exploded within 3.5 min of
    one exterior session; the second crashed it.
  - #5161's and #5246's closing A/Bs held only on their cells. The whole guard family (velocity cap, DOF cap,
    offence ladder, restore) acts between calls and cannot see this path.
  - Fixing #5352 would not close it.
- **Related**: #5352 (separate restore-substep hole, still open); #5353 (the free-root DOF cap; field log
  `:781-923`, `:3779-3855` and `:4096-4111` show its per-substep floods on Skyrim corpses; a possible contributor,
  not established); #5161, #5246 (closed); #4772; PHYS-D6-2026-10-09-01 (the DOF log could not attribute the
  pre-panic floods). The rig's root cause (upstream mass-inverted instability) still has no issue: see the
  Known-Open Register.
- **Suggested Fix**: contain the explosion at the two engine-owned points rapier calls *inside* `step`, so the
  existing restore can roll the body back afterwards.
  - **Broad phase**: in `FixedPairFilterBroadPhase::update`, pass the inner multi-SAP a copy of
    `modified_colliders` that leaves out every collider whose `compute_collision_aabb` has a coordinate beyond a
    sane bound. Reusing `KEYFRAME_TARGET_SANE_BOUND_BU` (1e8, below every layer's saturation point) works. The
    proxy keeps its last sane AABB.
  - **Narrow phase**: replace the `&()` hooks with a `PhysicsHooks` whose `filter_contact_pair` returns `None`
    for any pair with a collider past that bound. Set `ActiveHooks::FILTER_CONTACT_PAIRS` on ragdoll colliders in
    `build_ragdoll`. Rapier ORs the two colliders' flags (`narrow_phase.rs:880-897`), so ground pairs are covered
    without hooking every static collider.
  - **Escalation**: escalate an articulation's first burst straight to park or detach, with a snapshot rollback,
    instead of clamp-only.
  - **Docs**: correct the `:74-101` and `recovery.rs:301-309` failure-model docs.
  - **Test**: pin the probe's panicking configuration as a `PhysicsWorld` test that asserts no panic and a counted
    restore.
  - **Longer term**: a later rapier release with a BVH broad phase would remove the `sap_axis` class entirely.
    Verify the exact version before planning on it.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
