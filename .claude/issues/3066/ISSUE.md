# PHYS-D1-2026-08-16-03: a ConvexHull with fewer than 3 vertices panics inside parry

**Issue**: #3066
**Severity**: MEDIUM
**Labels**: `medium,safety,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_PHYSICS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_PHYSICS_2026-08-16.md` (Dimension 1 — collider construction).

**Location**: `crates/physics/src/convert.rs`:243-254

## Description

A `ConvexHull` with **fewer than 3 vertices panics inside parry** — the documented `None` fallback does not cover that input.

## Evidence

```rust
// crates/physics/src/convert.rs:243-254 (re-verified 2026-08-17)
CollisionShape::ConvexHull { vertices } => {
    let pts: Vec<Point3<f32>> = vertices.iter().map(|v| vec3_to_point(*v * scale)).collect();
    let shape = SharedShape::convex_hull(&pts).unwrap_or_else(|| {
        log::warn!("convex hull with {} pts rejected by Rapier; falling back to ball", pts.len());
        SharedShape::ball(1e-3)
    });
```

The `unwrap_or_else` handles `convex_hull` **returning `None`**. It cannot handle `convex_hull` **panicking**, which is what parry does below 3 points — so the fallback is unreachable for exactly the degenerate input it was written for.

## Impact

A NIF authoring a degenerate `bhkConvexVerticesShape` (0–2 vertices) panics the engine during cell load rather than falling back to the ball. NIF collision data is untrusted archive input, so a malformed or unusual mod asset takes the process down.

## Suggested Fix

Guard the vertex count before calling `convex_hull` — `if pts.len() < 3 { …ball fallback… }` — so the existing `log::warn!` + ball path handles both the degenerate and the rejected case.

Prefer the explicit length check over `catch_unwind`: the precondition is knowable, and panicking across a library boundary should not be load-bearing.

## Related

- #3064, #3065 (the scale findings in the same converter)
- `crates/nif/src/import/collision/shape.rs` (`resolve_shape` — the producer of `CollisionShape::ConvexHull`)

## Completeness Checks
- [ ] **SIBLING**: Every other parry constructor in `convert.rs` checked for panic-on-degenerate preconditions
- [ ] **PRODUCER-SIDE**: Consider rejecting degenerate hulls at the NIF import boundary too
- [ ] **NO-CATCH-UNWIND**: The guard is an explicit precondition check, not a caught panic
- [ ] **TESTS**: A regression test converts a 2-vertex `ConvexHull` and asserts the ball fallback

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3066 --json state` when live state is needed.*
