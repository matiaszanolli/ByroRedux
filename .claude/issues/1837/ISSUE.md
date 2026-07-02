# ECS-2026-07-02-04: insert_resource silently swallows a poisoned prior-value lock instead of re-panicking loud

**Labels**: low, ecs, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1837
**Source**: docs/audits/AUDIT_ECS_2026-07-02.md

**Severity**: LOW
**Dimension**: 4 — Resource Lifetimes (poison-on-panic resolution)
**Location**: `crates/core/src/ecs/world.rs:541-550`

## Description
The documented poison policy (dim 1 checklist; pinned by `poisoned_resource_lock_panics_with_type_name`) is that a post-panic lock access re-panics loud with the type name. `remove_resource` follows it (`resource_lock_poisoned::<R>()`, `world.rs:557-559`). `insert_resource` still does not: when replacing an existing resource whose lock was poisoned, the chain `old.and_then(|lock| lock.into_inner().ok().and_then(|boxed| boxed.downcast::<R>().ok()).map(|b| *b))` maps the `PoisonError` to `None` — indistinguishable from "no prior value existed."

## Evidence
`world.rs:541-550` vs. `remove_resource` at `world.rs:555-561`.

```rust
pub fn insert_resource<R: Resource>(&mut self, resource: R) -> Option<R> {
    let old = self
        .resources
        .insert(TypeId::of::<R>(), RwLock::new(Box::new(resource)));
    old.and_then(|lock| {
        lock.into_inner()
            .ok()
            .and_then(|boxed| boxed.downcast::<R>().ok())
            .map(|b| *b)
    })
}
```
still uses `.ok()` (swallows `PoisonError`), while `remove_resource` uses `.unwrap_or_else(|_| resource_lock_poisoned::<R>())`.

## Impact
Post-panic-only. No torn-state read occurs (the old value is dropped, not dereferenced, and the replacement installs a fresh un-poisoned `RwLock`), but a recovery path that replaces a resource after a caught panic gets a silent `None` where the sibling API panics loud — masking the original failure. Diagnostics/consistency gap under the fail-fast policy.

## Related
#466 (loud-poison policy); `ECS-2026-07-02-03` (same class of poison-diagnostics gap, `clear_entities`). Same finding as `ECS-2026-07-01-03` (2026-07-01 report) — unfixed, carried over. Neither day's report had previously been filed as a GitHub issue.

## Suggested Fix
Resolve the old lock via `.unwrap_or_else(|_| resource_lock_poisoned::<R>())` and the downcast via `.expect("resource type mismatch")`, matching `remove_resource`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`remove_resource`'s existing poison-resolution as the reference implementation)
- [ ] **TESTS**: A regression test pins this specific fix (mirroring `poisoned_resource_lock_panics_with_type_name`)
