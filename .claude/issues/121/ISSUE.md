# NIF-401: bhkRigidBody translation/rotation discarded in collision import

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: MEDIUM | **Dimension**: Import Pipeline

**Location**: `crates/nif/src/import/collision.rs:26-53`
**Game Affected**: All games with dynamic collision objects

### Description

`extract_collision()` at line 35 calls `resolve_shape(scene, body.shape_ref)` to get the collision shape, but never applies `body.translation` or `body.rotation` to the result. These fields represent the rigid body's initial center-of-mass offset and orientation relative to the parent NiNode.

For static architecture (walls, floors), these offsets are typically zero — no visible bug. For dynamic objects (crates, bottles, ragdoll bones), the collision shape will be misaligned from the rendered mesh.

### Suggested Fix

After resolving the shape, convert `body.translation` (Havok coords, scaled by HAVOK_SCALE) and `body.rotation` (quaternion) to engine space. If non-identity, wrap the shape in a `CollisionShape::Compound` or add offset fields to `RigidBodyData`.

### Completeness Checks

- [ ] **SIBLING**: Check bhkRigidBodyT (variant with explicit transform)
- [ ] **TESTS**: Test with NIF where rigid body has non-zero translation
- [ ] **UNSAFE**: N/A

🤖 Generated with [Claude Code](https://claude.com/claude-code)
