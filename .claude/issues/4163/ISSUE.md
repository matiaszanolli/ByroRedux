# NIF-D5-2026-09-04-03: bhkPlaneShape parses fully but hard-resolves to None with no measured coverage

URL: https://github.com/matiaszanolli/ByroRedux/issues/4163
Labels: bug, nif-parser, low, nif, test-gap

---

**Severity**: LOW
**Dimension**: 5 — Collision/Shader Parsing
**Location**: `crates/nif/src/import/collision/shape.rs:102-104`
**Status**: NEW (carry-forward of NIF-2026-09-04's Dimension 5 detail; no matching GitHub issue)

**Description**: `bhkPlaneShape` parses fully but hard-resolves to `None` in `resolve_shape_inner` by design (documented in-code as the correct choice — no half-space `CollisionShape` variant exists, and approximating a bounded plane as a solid `Cuboid` from its AABB would fill the volume instead of presenting a surface). The design decision itself is reasonable and re-verified sound this session, but it is unmeasured: no test or `summarize_collision_authoring` counter distinguishes "no `bhkPlaneShape` present" from "one is present and intentionally dropped."

**Evidence** (`shape.rs:95-104`):
```rust
if block.as_any().downcast_ref::<BhkPlaneShape>().is_some() {
    return None;
}
```

**Impact**: Test-gap only — a future regression that changes this arm's behavior (e.g. accidentally routing `BhkPlaneShape` through a different fallback) would be invisible to any existing test, since nothing measures the population today.

**Suggested Fix**: Add a counter to `summarize_collision_authoring` (or an equivalent measured assertion) so a future change to this arm is caught rather than silently drifting.

## Completeness Checks
- [ ] **TESTS**: A measured counter/assertion pins the current `bhkPlaneShape` → `None` behavior so a future change is caught

