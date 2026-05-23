# ECS-D5-NEW-02: undeclared_count walks parallel + exclusive — metric scope contradicts M27 Phase 3 claim

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1237
**Filed from**: `docs/audits/AUDIT_ECS_2026-05-23_DIM5.md`
**Severity**: LOW
**Labels**: `low`, `ecs`, `bug`
**Paired with**: #1236 (the API fix that makes the "all declared" target achievable)

## Description

```rust
// crates/core/src/ecs/scheduler.rs:452-458
pub fn undeclared_count(&self) -> usize {
    self.stages
        .iter()
        .flat_map(|s| s.systems.iter())   // ← walks PARALLEL + EXCLUSIVE
        .filter(|row| row.declared.is_none())
        .count()
}
```

The metric counts every undeclared system regardless of phase. But exclusive systems contribute zero information to the conflict analyzer (which only walks parallel pairs at `scheduler.rs:382-395`), and — per #1236 — they have no API path to declare access in the first place. So the metric mixes two distinct populations:

```
undeclared_count = (parallel systems not yet migrated)
                 + (exclusive systems that structurally can't declare today)
```

M27 Phase 3's commit message claims this metric is `0`, which is only achievable for the first population.

## Suggested Fix

Two options — pick one after #1236 lands:

1. **Keep metric scope, split the count.** Add `undeclared_parallel_count()` + `undeclared_exclusive_count()`; `sys.accesses` prints both.
2. **Scope metric to parallel only.** Change the iteration to `s.systems.iter().filter(|r| !r.is_exclusive)`. Matches the M27 Phase 3 commit-message semantic.

Either fix is one-line plus a test.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: if option 1 picked, `known_conflict_count` + `unknown_pair_count` already scope to parallel pairs only — no API symmetry break; if option 2 picked, audit other `AccessReport` accessors for consistency
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: update the `sys.accesses` smoke / `access_report` tests to pin the chosen semantic
