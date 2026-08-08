# FO3-D5-NEW-01: bhkRigidBody.havok_filter parsed then dropped at the NIFAL boundary -- FOL_NONCOLLIDABLE FO3 bodies spawn as solid world colliders

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2549
**Finding ID**: FO3-D5-NEW-01

**Severity**: MEDIUM
**Dimension**: FO3 Collision Import (Havok → CollisionShape)
**Location**: `crates/nif/src/blocks/collision/rigid_body.rs:19,69,204` (`havok_filter` parsed); `crates/core/src/ecs/components/collision.rs:100-107` (`RigidBodyData`, no filter/layer field)
**Status**: NEW

## Description
The Havok layer/group/part filter (`havok_filter`) is parsed off the wire on `BhkRigidBody` but never carried into the canonical `CollisionShape`/`RigidBodyData`, so bodies authored non-collidable (layer `FOL_NONCOLLIDABLE`) still spawn as solid colliders in the physics world.

## Evidence
20 such bodies found across a 19,229-collider real-data sweep (`Fallout - Meshes.bsa` + all 5 DLC mesh archives). Confirmed directly: `havok_filter` is read and stored on the raw `BhkRigidBody`/`BhkRigidBodyT` structs (`rigid_body.rs:69,204,239,275`), but `grep -rn havok_filter` outside `crates/nif` returns zero hits — `RigidBodyData` (`crates/core/src/ecs/components/collision.rs:100-107`) has no filter/layer field at all.

## Impact
A body authored non-collidable in the source content (an FO3-specific authoring convention using `FOL_NONCOLLIDABLE`) is silently promoted to a solid collider in the running physics world — the player and NPCs can be blocked by geometry the content author explicitly marked as non-solid. Bounded blast radius (20/19,229 ≈ 0.1% of real content), but a genuine authoring-intent violation, not cosmetic.

## Suggested Fix
Thread `havok_filter`'s layer field through to a canonical representation (either a new `RigidBodyData.collidable: bool` derived from `layer != FOL_NONCOLLIDABLE`, or a fuller layer/group/part triple if other layers turn out to matter) and gate collider spawning on it at the same site `motion_type` is already consumed.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: New field/flag flows through the single extract→translate boundary, not re-derived at spawn time
- [ ] **TESTS**: A regression test decodes a real `FOL_NONCOLLIDABLE` body and confirms it does not spawn a solid collider
