# Investigation: #90 — No lock ordering for ad-hoc multi-query or resources

## Current State
- `query_2_mut` and `query_2_mut_mut` enforce TypeId-sorted lock ordering ✓
- Same-type double-lock panics immediately ✓
- `resource_mut<R>` acquires a single write lock — no ordering with other resources
- No `resource_2_mut` or N-resource ordered locking API
- Scheduler is sequential — no parallel system dispatch yet

## Risk Assessment
Currently safe: all systems run sequentially via the single-threaded scheduler.
Becomes a deadlock risk ONLY when parallel dispatch (M27) is implemented.

## Fix Plan
1. Add `resource_2_mut<A, B>` with TypeId-sorted lock ordering (like query_2_mut)
2. Add debug-mode lock-order tracking (thread-local Vec<TypeId> tracking acquisition order)
3. Tests: same-resource double-lock panic, cross-resource ordering

## Scope
1 file: `crates/core/src/ecs/world.rs` (+ tests)
