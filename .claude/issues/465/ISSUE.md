# Issue #465

E-02: resource_2_mut has no try_ variant — panics on missing resource

---

## Severity: Low (API parity)

**Location**: `crates/core/src/ecs/world.rs:514-571`

## Problem

`resource<R>()` and `resource_mut<R>()` each have `try_` Option-returning siblings. `resource_2_mut<A, B>()` does not.

Systems wanting two resources conditionally have to fall back to sequential `try_resource_mut` + `try_resource_mut`, which **loses the TypeId-sorted acquisition ordering** that `resource_2_mut` enforces (the ABBA deadlock hazard guarded by #313).

## Impact

None today — every current caller has both resources installed at bootstrap. But any future optional-resource pairing has to choose between:
- Panic on missing (using `resource_2_mut`).
- Risk ABBA deadlock (sequential `try_resource_mut` calls).

Neither is acceptable long-term.

## Fix

Add `try_resource_2_mut<A, B>()` mirroring the existing TypeId-sorted body:

```rust
pub fn try_resource_2_mut<A: Resource, B: Resource>(&self)
    -> Option<(ResourceWrite<'_, A>, ResourceWrite<'_, B>)>
{
    assert_ne!(
        TypeId::of::<A>(), TypeId::of::<B>(),
        "try_resource_2_mut: A and B must be different resource types"
    );
    // Check both resources exist BEFORE acquiring any lock.
    if !self.resources.contains_key(&TypeId::of::<A>())
        || !self.resources.contains_key(&TypeId::of::<B>())
    {
        return None;
    }
    // Acquire in TypeId order (reuse resource_2_mut body structure).
    Some(self.resource_2_mut::<A, B>())
}
```

Important: the existence check must complete BEFORE the first lock is acquired, to preserve ordered-lock guarantee.

## Completeness Checks

- [ ] **TESTS**: Missing-A, missing-B, both-missing, both-present cases — each returns correct Option
- [ ] **TESTS**: Same-type panic still fires (assert_ne at top)
- [ ] **LOCK_ORDER**: TypeId-sorted acquisition preserved — matches #313 contract
- [ ] **SIBLING**: Consider `try_query_2_mut` / `try_query_2_mut_mut` for query API symmetry

Audit: `docs/audits/AUDIT_ECS_2026-04-19.md` (E-02)
