# FNV-D7-02: Rapier multibody forward-kinematics overwrites seeded ragdoll poses on the first physics step

Source: `docs/audits/AUDIT_FNV_2026-08-03.md`, Dimension 7 (PHYSAL Ragdoll — FNV Reference Slice), finding FNV-D7-02.
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2337
Labels: medium, legacy-compat, bug

**Severity**: MEDIUM
**Dimension**: Dimension 7 — PHYSAL Ragdoll (FNV reference slice; PHYSAL-wide, not FNV-specific)
**Location**: `crates/physics/src/ragdoll.rs:137-224` (`build_ragdoll`, `build_joint`); rapier3d `0.22.0` (`multibody_joint.rs`, `multibody.rs`, `pipeline/physics_pipeline.rs` — vendored dependency, not this repo)
**Related**: Depends on FNV-D7-01 being fixed first (the seed poses this finding is about are only correct once that transform bug is resolved).

## Description

Even with FNV-D7-01 fixed, Rapier's multibody forward-kinematics pass
(`forward_kinematics` + `update_rigid_bodies_internal`, run unconditionally at
the top of every `PhysicsPipeline::step`) recomputes every non-root link's
pose from the authored joint frames starting at `coords = 0`. `build_ragdoll`
is explicitly a reduced-coordinate multibody (module doc, line 9); bodies are
inserted at seeded Cartesian positions via `RigidBodyBuilder::position(...)`,
but the multibody joints are then built purely from the authored Havok
rest-configuration data (`build_joint`), with no derivation from the seeded
relative poses. There is no post-insertion step anywhere in the crate that
sets each link's joint coordinates from `parent_seed⁻¹ ∘ child_seed`.

Confirmed against the actual vendored rapier3d 0.22.0 source
(`~/.cargo/registry/.../rapier3d-0.22.0/`):
- `pipeline/physics_pipeline.rs:470-474` calls `forward_kinematics` +
  `update_rigid_bodies_internal` unconditionally at the top of every `step()`.
- `dynamics/joint/multibody_joint/multibody_joint.rs:29-35` constructs each
  joint with `coords: na::zero()`.
- `dynamics/joint/multibody_joint/multibody.rs:1008-1015` recomputes every
  non-root link's world pose purely from `joint.body_to_parent()` (using
  those zero coords), overwriting `rb.pos.position`.

## Impact

A ragdolled actor snaps from its animated pose to the rest configuration on
the first physics tick, rather than falling continuously from its
just-activated pose. Visually jarring "pop to rest pose" on every ragdoll
activation. (The multibody root is spared, since it uses a `free` joint whose
coords are explicitly synced.)

## Suggested Fix

After inserting the multibody joints, derive each link's joint coordinates
from the seeded relative poses (`parent_seed⁻¹ ∘ child_seed`) and set them on
the inserted joint before the first step — or, if a rest-pose start is
actually intentional, document that explicitly and correct
`activate_ragdoll`'s doc comment (which currently implies the seed poses
persist into simulation) to match reality.

## Validation

CONFIRMED — verified directly against `crates/physics/src/ragdoll.rs` (no
counter-mechanism present) and independently re-confirmed by a background
agent that read the actual vendored rapier3d 0.22.0 source and quoted the
exact `coords: na::zero()` / forward-kinematics call sites. No open-issue
duplicate found.
