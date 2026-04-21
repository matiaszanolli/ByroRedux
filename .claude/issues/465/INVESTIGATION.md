# Investigation — Issue #465 (E-02)

## Domain
ECS — `crates/core/src/ecs/world.rs`

## Current API surface

- `resource<R>()` panic-on-missing, `try_resource<R>()` Option
- `resource_mut<R>()` panic-on-missing, `try_resource_mut<R>()` Option
- `resource_2_mut<A, B>()` panic-on-missing → **no try_ sibling**

`resource_2_mut` is TypeId-sorted for deadlock prevention (#313). The
fallback of calling `try_resource_mut` twice sequentially loses that
ordered acquisition guarantee.

## Fix

Add `try_resource_2_mut<A, B>()` that:
1. `assert_ne!` on same-type (would deadlock).
2. Checks both TypeIds exist in `self.resources` BEFORE acquiring any lock — early return `None`.
3. Delegates to `resource_2_mut` once both existence checks pass (preserves TypeId-sorted acquisition + tracker behavior).

Existence check-before-lock is critical: without it, the caller could acquire lock_a, then discover lock_b missing, release lock_a — leaving a short window where another thread sees a "partial acquisition" pattern mid-sequence.

## Scope
1 file: `crates/core/src/ecs/world.rs` (new method + 3 regression tests).
