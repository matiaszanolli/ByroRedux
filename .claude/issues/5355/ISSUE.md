# #5355: PHYS-D2-2026-10-05-03: `PhysicsWorld::articulation_joints` is never pruned, and `clamp_explosive_velocities` walks it every substep

**Labels**: low,physics,performance,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5355

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-05.md` — `PHYS-D2-2026-10-05-03` (HEAD `a2c24b16e`)

- **Severity**: LOW
- **Dimension**: Step & Sync (cost)
- **Location**: `crates/physics/src/world.rs:348-353` (field doc), `:1075`; `crates/physics/src/ragdoll.rs:529`
  (push), `:863-867` (`remove_ragdoll` does not prune)
- **Status**: NEW. This is a sibling of SAFE-D3-2026-10-05-01, which covers `explosion_offences` and
  `keyframe_refusals_logged` but not this vector.
- **Description**: every `build_ragdoll` pushes one handle per joint, about 17 for a humanoid. The field doc
  claims "Stale handles … are skipped, so the vector never needs sweeping". The handles are skipped, but they are
  never removed. Nothing removes entries: not `remove_ragdoll`, not `remove_body`, and not the restore or detach
  paths. Every substep of a stepping frame pays one `MultibodyJointSet::get_mut` miss per lifetime ragdoll joint.
- **Impact**: per-substep cost grows linearly over the session with the number of corpses ever activated. In
  absolute terms it stays small (thousands of misses after hundreds of deaths), and memory is a few bytes per
  joint.
- **Suggested Fix**: `retain` live joints at the top of `clamp_explosive_velocities`, the same way the
  `dynamic_bodies` compaction does at the snapshot. Alternatively, drop a ragdoll's `joints` in `remove_ragdoll`.
  Fix it together with SAFE-D3-2026-10-05-01.

## Publisher note

Sibling of SAFE-D3-2026-10-05-01 (#5272, `explosion_offences` / `keyframe_refusals_logged` never pruned in `remove_body`); all three belong in one change.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
