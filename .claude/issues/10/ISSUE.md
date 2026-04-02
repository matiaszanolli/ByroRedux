# Issue #10: Name lookup is O(N) per frame — cache entity-by-name mapping

## Metadata
- **Type**: enhancement
- **Severity**: low
- **Labels**: enhancement, animation, ecs
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: Optimization (when needed)
- **Affected Areas**: Animation system performance

## Problem Statement
animation_system rebuilds HashMap<String, EntityId> every frame: iterates all named entities, resolves interned strings, allocates String keys. O(N) with allocation per entity.

## Affected Files
- `byroredux/src/main.rs` — animation_system
- `crates/core/src/animation.rs` — clip channel keys could use FixedString

## Acceptance Criteria
- [ ] Name→entity mapping cached as resource, rebuilt only when dirty
- [ ] No per-frame string allocations
- [ ] Animation channels use FixedString keys

## Notes
Negligible for current scene sizes (< 1000 entities). Matters at scale.
