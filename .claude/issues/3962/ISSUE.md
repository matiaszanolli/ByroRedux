# PHYS-D4-2026-09-06-01: `seed_joint_from_body_poses` dispatches on `ndofs()`, so #3792's new prismatic joints are seeded with a rotation angle on their linear axis

Issue: #3962 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.


- **Severity**: MEDIUM
- **Dimension**: Ragdoll Articulation
- **Location**: `crates/physics/src/ragdoll.rs:623-652` (the function and its now-false docstring), reached from `:392` (the unconditional per-edge call in `build_ragdoll`); the joint it cannot distinguish is built at `:575-620`, its lock mask at `:489-496`
- **Status**: NEW
- **Trigger Conditions**: activate a ragdoll (death, or the `ragdoll <id>` console command) on any actor whose `RagdollTemplate` contains at least one `RagdollJointSpec::Prismatic` edge. Known vanilla content: `meshes\creatures\protectron\skeleton.nif` (FNV `Fallout - Meshes.bsa`, and its FO3 counterpart). Error magnitude scales with how far the two bodies' relative rotation is from the joint frames' rest alignment at activation — smallest in bind pose, largest mid-animation, which is the normal case.
- **Description**: #3792 added a third `RagdollJointSpec` variant, `Prismatic`, whose lock mask leaves **`LinX`** free. `build_joint` and `scaled_pivots` both gained a `Prismatic` arm — the compiler forced them, because both are exhaustive `match`es on the enum. `seed_joint_from_body_poses` is the one dispatch on joint kind in the whole path that is **not** a match on the enum: it switches on `MultibodyJoint::ndofs()`, an integer.
- **Evidence**: `LimitedHinge` locks `lin_locked() | ANG_Y | ANG_Z` = `LIN_X|LIN_Y|LIN_Z|ANG_Y|ANG_Z` — 5 axes. `Prismatic` locks `LIN_Y|LIN_Z|ANG_X|ANG_Y|ANG_Z` (`prismatic_locked()`, `:489-496`) — also 5. Rapier 0.22 computes `ndofs() = 6 - locked.count_ones()`, so **both are `ndofs == 1`** and are indistinguishable here:
  ```rust
  let angular_displacement = joint_rotation.scaled_axis();
  match joint.ndofs() {
      // Limited hinge: only local angular X is free.
      1 => joint.apply_displacement(&[angular_displacement.x]),
      3 => joint.apply_displacement(angular_displacement.as_slice()),
      ndofs => log::warn!("ragdoll: cannot seed unsupported {ndofs}-DOF …"),
  }
  ```
  `apply_displacement` walks the **linear** axes first (`multibody_joint.rs:88-95`), so for a prismatic edge `coords[LinX] = angular_displacement.x` — the X component of a rotation vector in radians assigned to a slide distance in engine units. No clamp against the joint's own `[min_distance, max_distance]` is applied. The `ndofs => warn!` catch-all cannot fire.
- **Impact**: two defects on one line, scoped to `Prismatic` edges. (1) **The #2337 guarantee is lost for this kind** — the function exists to carry the animated pose into the multibody's reduced coordinates before the first step overwrites it; the correct quantity (the along-rail component) is never computed, so the child link snaps to the rail's zero position. That is precisely the failure #2337 was filed against, reintroduced. (2) **A type-confused value is injected** — `|angular_displacement.x| ≤ π`, so the child is displaced along its rail by up to ~π units × bone scale (`havok_scale = 7.0` on FNV/FO3), which can exceed the joint's authored travel range; `apply_displacement` does not clamp, leaving the solver's limit constraint to resolve an impulse at t=0. That is the "ragdoll pops on activation" shape, not a cosmetic offset. #3792's own measurement records the Protectron as *2 Prismatic* of 12 joints, so exactly two edges take this path per Protectron ragdoll, every time.
- **Why the tests cannot see it**: the crate contains **exactly one** `RagdollJointSpec::Prismatic` — the production arm at `:575`. Zero tests construct one. #3792's headline test asserts NIF-import connectivity and is `#[ignore]`d; it never calls `build_ragdoll`. 156/156 green with the defect present.
- **Related**: #3792 (`13fdb48e`, introduced the third `ndofs == 1` kind), #2337 (the issue this function implements), #3330 / #1539 / #1850.
- **Suggested Fix**: dispatch on the joint kind, not on `ndofs()`. Either pass the `&RagdollJointSpec` down (the call site at `:392` has it in hand) or read `joint.data.locked_axes` and branch on whether the free axis is linear or angular — for the prismatic case seed `(frame1⁻¹ ∘ parent_to_child ∘ frame2).translation.x`. Update the docstring's now-false *"All ragdoll linear DOFs are locked"* precondition. Add a `prismatic_seed_uses_the_slide_distance_not_the_twist_angle` test; the module currently has no `Prismatic` coverage at all.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
