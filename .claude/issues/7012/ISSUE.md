# #7012 — PHYS-D2-01: A delivered link impulse also wipes the link's user TORQUE, so the angular-damping torque is lost for the rest of the frame's substeps

**Labels**: low,physics,bug · **Filed from**: `docs/audits/AUDIT_PHYSICS_2026-11-15.md` (2026-11-15)

- **Severity**: LOW
- **Dimension**: Dim 2 (Step & Wake) / link-damping interaction
- **Location**: `crates/physics/src/world/mod.rs:1235-1262` (`deliver_link_impulses` / `release_link_impulse_forces`); `crates/physics/src/world/mod.rs:1302-1306` (`reset_forces`)
- **Trigger Conditions**: A multibody link with nonzero angular damping receives `apply_impulse` (e.g. `PushActorAway`, `ApplyHavokImpulse`), and the same stepping frame runs two or more substeps (catch-up at <60 fps, or a sub-tick accumulator carry-over).
- **Status**: NEW (introduced by `eac3f73f9`)
- **Description**: `deliver_link_impulses` snapshots only `body.user_force()` into `before`. `release_link_impulse_forces` then calls `self.reset_forces(handle, false)`, which clears forces AND torques (`b.reset_forces(..); b.reset_torques(..)` at 1304-1305), and re-adds only the saved force. The commit and doc say "restores the body's previous user force exactly". That is true for the force, but the user torque is not part of the snapshot and is silently zeroed. The torque is real state. `apply_multibody_link_damping` (1137-1175) puts the authored angular drag `-c·I·ω` (#5697) on every dry awake link through `b.add_torque`, and the water scan does the same for wet links (`pw.add_torque`, water.rs ~1409).
- **Evidence**: `release_link_impulse_forces`: `self.reset_forces(handle, false); self.add_force(handle, force, false);`. The torque is never saved or re-added. The tests `a_link_impulse_is_a_single_kick_and_leaves_other_forces_alone` only assert `user_force()`, never `user_torque()`.
- **Impact**: Small. After substep 0 of the impulse frame, the link has no angular damping for substeps 1..N of that frame (N ≤ MAX_SUBSTEPS-1, only on catch-up frames). Both the dry pass (top of the next stepping frame) and the water scan (each full scan) re-derive the torque, so nothing persists past the frame. The cost is a slightly under-damped spin during a knock-down, the case where the angular damping is wanted most.
- **Related**: #6487, #5697, #5540, #5698 (force ownership: "anything else added to a dry link's force is replaced each frame").
- **Suggested Fix**: Capture `body.user_torque()` beside `user_force()` in `deliver_link_impulses` and re-add it in the release (`b.add_torque`), or restore with `b.reset_forces(false)` plus the force only, leaving torques untouched. Extend the single-kick test with a held torque.

Source: `docs/audits/AUDIT_PHYSICS_2026-11-15.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
