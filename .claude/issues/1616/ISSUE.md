# LC-D4-01: Ragdoll writeback omits the inverse body-local offset → skinned mesh displaced

**Severity**: MEDIUM · **Dimension**: D4 (PHYSAL — sink boundary)
**Location**: `byroredux/src/ragdoll.rs:189-191` (seed) vs `byroredux/src/ragdoll.rs:255-264` (writeback)
**From**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-06-14.md`

## Description
`activate_ragdoll` seeds each Rapier body at the bone world pose **composed with the body-local offset**:
`translation = gt.translation + gt.rotation * (b.local_translation * gt.scale)`, `rotation = gt.rotation * b.local_rotation` (`ragdoll.rs:190-191`).
The per-frame `ragdoll_writeback_system` then copies the body's simulated **world** pose *directly* back onto the bone's `GlobalTransform` — `gt.translation = t; gt.rotation = r` (`ragdoll.rs:261-262`) — with **no inverse** of `local_translation` / `local_rotation`. The seed and the writeback are asymmetric: the offset is added going in but never removed coming out.

## Evidence
Seed at `ragdoll.rs:190-191` applies `body_local`; writeback at `ragdoll.rs:261-262` writes `body_pose` verbatim. `local_translation` / `local_rotation` come from `ImportedRagdollBody.translation` / `.rotation` (`crates/nif/src/import/collision.rs:299-304`), which is the `BhkRigidBody.translation` / `.rotation`. Ragdoll bodies are commonly authored as **`bhkRigidBodyT`** — the active-transform variant whose translation/rotation are non-zero (`crates/nif/src/blocks/collision/rigid_body.rs:14`). The unit test `activate_then_writeback_moves_bones` uses `local_translation: Vec3::ZERO` / `local_rotation: Quat::IDENTITY` (`ragdoll.rs:320-321`), so it never exercises a non-zero offset — the bug is invisible to `cargo test`.

## Impact
Every real ragdoll whose bodies carry a non-zero bone offset (the normal `bhkRigidBodyT` case) writes the **body** origin onto the **bone** transform. The skinned mesh — which reads bone `GlobalTransform` — is systematically displaced by the body offset for the lifetime of the ragdoll: limbs render offset from where the simulated bodies actually are, producing a visibly wrong crumple. Bounded blast radius: the path is gated behind the `ragdoll <id>` debug-server command (slice 1), not a default content path, and no crash. This is **not** the documented "Havok cone+2-plane → Rapier per-axis limit" approximation (`physal.md` §3) — that concerns limit fidelity, not body placement; this offset bug is undocumented.

## Suggested Fix
In `ragdoll_writeback_system`, recover the bone pose from the body pose by inverting the seed composition — store the body-local offset on the `Ragdoll`/`RagdollBodySpec` (it is currently dropped after seeding) and apply `bone_rotation = body_rotation * local_rotation⁻¹`, `bone_translation = body_translation - bone_rotation * (local_translation * scale)`. Then extend `activate_then_writeback_moves_bones` with a **non-zero** `local_translation`/`local_rotation` so the regression is pinned.

## Related
PHYSAL §3 "Build + step + writeback" (`docs/engine/physal.md` does not note this asymmetry). Distinct from the concurrency finding #1601 (declared `GlobalTransform` write conflict in `ragdoll_writeback_system`).

## Completeness Checks
- [ ] **SIBLING**: Seed/writeback symmetry holds — every offset applied at seed has its inverse at writeback (and vice versa)
- [ ] **LOCK_ORDER**: If the `GlobalTransform` write scope changes, TypeId-sorted acquisition is preserved (see #1601)
- [ ] **TESTS**: `activate_then_writeback_moves_bones` extended with a non-zero `local_translation`/`local_rotation` to pin this fix
