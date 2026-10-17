# PHYS-D6-2026-10-16-03: `cast_ray_corridor_where` duplicates `cast_ray_corridor`'s shape-cast body and differs on two edge cases

Labels: low,physics,tech-debt,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (tech debt)
- **Location**: `crates/physics/src/world/queries.rs:353-390` vs `:304-345`
- **Status**: NEW
- **Description**: the new function is a copy of `cast_ray_corridor` with a predicate in place of `excluded_body`. Divergences: it has no `corridor_radius <= 0` fallback to `cast_ray` (it clamps to a 0.01 BU ball), and its predicate (`collider.parent().is_some_and(allow)`) excludes parentless colliders while `cast_ray_corridor` includes them (none exist in production today, every collider is inserted with a parent). The user's rule is to extend existing code rather than duplicate logic.
- **Impact**: none today; future edits (e.g. the ShapeCastOptions flags, which carry the "start inside a wall" semantics) can drift between the two.
- **Related**: #5160.
- **Suggested Fix**: implement `cast_ray_corridor` as `cast_ray_corridor_where` with an `exclude_rigid_body` predicate (keeping its zero-radius `cast_ray` fallback), or share a private helper taking a `QueryFilter`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
