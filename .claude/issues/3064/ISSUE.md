# PHYS-D1-2026-08-16-01: synthesized static-trimesh colliders are scaled twice

**Issue**: #3064
**Severity**: HIGH
**Labels**: `high,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_PHYSICS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_PHYSICS_2026-08-16.md` (Dimension 1 — collider construction).

**Location**: `byroredux/src/cell_loader/spawn.rs`:327-368 (producer) · `crates/physics/src/convert.rs` (consumer)

## Description

Synthesized static-trimesh colliders are **scaled twice** — every scaled REFR on the missing-collision fallback gets a `scale²` collider.

## Evidence

Producer bakes the scale into the vertices:
```rust
// byroredux/src/cell_loader/spawn.rs:316 (doc) / :343
/// `RigidBodyData` from a render mesh's geometry, baking `world_scale`
…
.map(|p| Vec3::new(p[0] * world_scale, p[1] * world_scale, p[2] * world_scale))
```

Consumer applies it again:
```rust
// crates/physics/src/convert.rs
:175   let composed = parent_iso * iso_from_trs(*t * scale, *r);
:180   out.push((parent_iso, SharedShape::ball((*radius * scale).max(1e-3))));
:213   clamp_lane(half_extents.x * scale),
```

Re-verified 2026-08-17.

## Impact

Any REFR with `scale != 1.0` that falls back to synthesized trimesh collision gets a collider `scale²` the intended size. At scale 2.0 the collider is 4× the mesh; at 0.5 it is a quarter.

The visual mesh is correct, so the symptom is invisible geometry mismatch: the player collides with nothing where the object is, and with something where it is not.

## Suggested Fix

Pick one application site. Since `convert.rs` scales every other `CollisionShape` variant, the cleaner fix is to **stop baking `world_scale` into the synthesized vertices** and let the shared converter own scaling — and update the `spawn.rs`:316 doc comment, which currently advertises the baking as intentional.

## Related

- #3065 (PHYS-02 — the identical double-scale bug in the ragdoll path; same root, fix together)

## Completeness Checks
- [ ] **SINGLE-SITE**: Scale is applied exactly once, by one owner
- [ ] **SIBLING**: Fixed together with #3065 — same defect, two producers
- [ ] **DOC-COMMENT**: `spawn.rs`:316's "baking `world_scale`" claim matches the new behaviour
- [ ] **TESTS**: A regression test builds a scale-2.0 REFR collider and asserts its extent

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3064 --json state` when live state is needed.*
