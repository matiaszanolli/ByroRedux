# PHYS-D4-01

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2868

---

Found by `/audit-physics` Dimension 4 (Ragdoll Articulation). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `byroredux/src/ragdoll.rs:292-330` (seed), `crates/physics/src/ragdoll.rs:150-187` + `:331-410` (build/joint)

> Sibling of PHYS-D1-01 (uniform scale dropped at the *static* collider boundary). `build_ragdoll` shares the `collision_shape_to_parts` call site but **not** the pivot problem — D1-01's fix would cover the shape half and leave this open.

## Trigger Conditions
Any actor whose placement `Transform.scale != 1.0` (REFR `XSCL`, or a skeleton NIF with a non-unit node scale on the chain) **and** which carries a decodable classic-chain ragdoll (Oblivion / FO3 / FNV / Skyrim). Fires the instant `activate_ragdoll` runs — a single `pw.step` is enough. Invisible on the scale = 1.0 majority, which is why every existing test misses it.

## Description
`activate_ragdoll` composes the body world seed **with** the live bone scale — `translation = gt.translation + gt.rotation * (b.local_translation * gt.scale)` (`ragdoll.rs:298`) — and snapshots `scale: gt.scale` (`:308`) for the writeback inverse only. It never applies that scale to:
- (a) `RagdollBodySpec::shape`, or
- (b) the `RagdollJointSpec` pivot vectors, carried verbatim from `joint_from_imported` (`ragdoll.rs:216-272`) in NIF/`havok_scale` units.

`build_ragdoll` then locks all three linear DOF (`lin_locked()`, `crates/physics/src/ragdoll.rs:326-329`) and builds `local_frame1`/`local_frame2` from those unscaled pivots (`:365-366`, `:404-405`). Because the joint is a **multibody** (reduced-coordinate) joint, the child link's translation is not a soft constraint — forward kinematics *defines* it as `parent_pose . frame1 . joint_rot . frame2^-1`. The animated (scaled) separation the seed established is therefore discarded on the first step and replaced by the bind-scale one.

`RagdollBodySpec::scale` exists and is threaded all the way to `build_ragdoll`, so the value is **dropped, not unavailable**.

## Evidence
Measured with a throwaway `crates/physics/tests` probe (deleted after the run). Two bodies seeded 100 units apart (a 2x actor whose authored pivots are +/-25 -> bind separation 50), gravity zeroed:

```
seeded child = Vec3(100.0, 1000.0, 0.0); after 1 step = Vec3(50.0, 1000.0, 0.0);
separation 100 -> 50
```

The seeded pose is thrown away in exactly one step. This is the *same* forward-kinematics behaviour the existing `first_step_preserves_seeded_child_pose` test relies on (`crates/physics/src/ragdoll.rs:598-627`) — that test passes only because its hand-written pivots happen to match its hand-written body separation at scale 1.

## Impact
A scaled NPC (child / creature / giant REFR, or any mod that rescales an actor) collapses to bind-scale skeleton proportions the frame it ragdolls, while `ragdoll_writeback_system` keeps writing `gt.scale = seed_scale` onto the bones — so the skinned mesh renders scaled-up bones packed at unscaled-apart positions: a visibly crushed, interpenetrating corpse rather than a crumple. The colliders are simultaneously the wrong size (the ragdoll half of PHYS-D1-01). **No workaround exists at the console.**

## Suggested Fix
Multiply every `RagdollJointSpec` pivot by the seed-time scale inside `activate_ragdoll` (the single translate boundary — do **not** scale in `build_joint`, which must stay unit-agnostic), and pass the same scale into `collision_shape_to_parts` alongside the PHYS-D1-01 fix. Add a regression test asserting a `scale = 2.0` two-body spec preserves its 100-unit separation after one step.

**Axes (`twist_*` / `plane_*` / `axis_*` / `perp_*`) are unit directions and must stay unscaled.**

## Related
- PHYS-D1-01 (the static-collider half)
- #1852 (the writeback-inverse scale snapshot — currently the only consumer of `RagdollBodySpec::scale`), #2543
