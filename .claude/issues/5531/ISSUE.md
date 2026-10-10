# #5531: PHYS-D6-2026-10-09-01: The articulation-DOF clamp logs no articulation, so its per-substep floods (and the pre-panic DOF clamps) cannot be attributed to an actor

**Labels**: bug, low, physics

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-09.md` — finding `PHYS-D6-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. A diagnostics gap.
- **Dimension**: Queries & Diagnostics
- **Location**: `crates/physics/src/world/recovery.rs:413-440`
- **Status**: NEW
- **Trigger Conditions**: any activated ragdoll whose generalized velocity crosses `ARTICULATION_DOF_SANITY_CAP`.
  Because of #5353, that is every falling corpse.
- **Description**:
  - The body clamp logs a label (`body_labels`, `actor <id> bone <name>`). On a multibody link, though, that
    clamp is a no-op: rapier overwrites the link velocity from the generalized DOFs.
  - The DOF clamp is the one that actually bounds a ragdoll. Its single summary line ("clamped N exploding
    articulation DOF velocities") carries no handle and no label.
  - The field log has dozens of anonymous "clamped 1 …" lines (#5353 falling roots). It cannot show whether actor
    16468 was the corpse flooding them in the seconds before D2-01's panic.
- **Evidence**: the `log::warn!` at `recovery.rs:436-439` formats only `clamped_dofs`. The loop holds the
  multibody (`:419`), and rapier 0.22 exposes `Multibody::root().rigid_body_handle()`.
- **Impact**: the explosion telemetry cannot be attributed, which slowed this triage. #5353's floods are
  indistinguishable from a real articulation explosion.
- **Related**: #5353, PHYS-D2-2026-10-09-01, #4683 (counted recovery).
- **Suggested Fix**: tally per multibody and log `body_labels[multibody.root().rigid_body_handle()]` with each
  count, deduplicating by multibody: `articulation_joints` holds about 17 handles that resolve to the same
  multibody.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
