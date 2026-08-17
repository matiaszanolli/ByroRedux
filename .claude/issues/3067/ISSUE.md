# PHYS-D3-2026-08-16-04: register_newcomers' parts.is_empty() skip is unreachable

**Issue**: #3067
**Severity**: LOW
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_PHYSICS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_PHYSICS_2026-08-16.md` (Dimension 3 — ECS sync).

**Location**: `crates/physics/src/sync.rs`:787-789

## Description

`register_newcomers`' `parts.is_empty()` skip is **unreachable** — no producer can hand it an empty part list.

## Evidence

```rust
// crates/physics/src/sync.rs:787-789 (re-verified 2026-08-17)
if parts.is_empty() {
    continue;
}
```

Every path that reaches this point has already produced at least one part (`collision_shape_to_parts` pushes unconditionally on every `CollisionShape` variant, including the ball fallback in #3066).

## Impact

None functional — dead defensive code. The cost is that it reads as a handled case, so a reader assumes empty part lists are possible and expected here when they are not.

Worth recording rather than silently deleting because #3066's fix touches the same producer chain: if degenerate shapes ever start yielding zero parts, this guard becomes live and the reasoning changes.

## Suggested Fix

Either remove it, or convert it to a `debug_assert!(!parts.is_empty())` that states the invariant it currently only implies.

The assertion is the better choice given #3066 — it documents the producer contract rather than silently tolerating a violation.

## Related

- #3066 (PHYS-03 — the degenerate-shape path that would make this reachable if it produced zero parts)

## Completeness Checks
- [ ] **INVARIANT-STATED**: Replaced by an assertion or removed with a comment, not left ambiguous
- [ ] **SIBLING**: Other unreachable defensive skips in `sync.rs`'s four-phase tick checked
- [ ] **CO-RESOLVE**: Decided with #3066 in view, since its fix touches the same producer
- [ ] **TESTS**: `cargo test -p byroredux-physics` green

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3067 --json state` when live state is needed.*
