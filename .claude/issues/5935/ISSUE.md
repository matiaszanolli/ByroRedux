# PHYS-D5-2026-10-16-03: Two comments left false by the delta

Labels: low,physics,documentation,doc-rot
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Water / Buoyancy (doc rot, reported once for Dim 2/6 owners)
- **Location**: `crates/physics/src/world/mod.rs:1043` and `:1089-1093`
- **Status**: NEW
- **Description**: (a) `apply_multibody_link_damping` still says "Cheap body flags before the SipHash set lookup" after #5709 made `water_force_targets` an `FxHashSet`. (b) `PhysicsWorld::apply_impulse` still says "No production consumer yet (#2889)", but `scripted_havok_impulse_system` (`byroredux/src/systems/cinematic.rs:376`, 696da4f47) now calls it; only the WATAL splash-kick consumer the comment names is still unbuilt.
- **Impact**: misleads the next reader about hashing cost and about who exercises `apply_impulse`.
- **Related**: #5709, 696da4f47, #2889.
- **Suggested Fix**: drop "SipHash"; reword (b) to name the scripted consumer and keep the splash-kick note.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
