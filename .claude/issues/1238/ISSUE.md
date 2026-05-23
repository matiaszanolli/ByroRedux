# ECS-D5-NEW-03: Two scheduler test coverage gaps — all-five-stages chain + cross-feature parity

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1238
**Filed from**: `docs/audits/AUDIT_ECS_2026-05-23_DIM5.md`
**Severity**: LOW
**Labels**: `low`, `ecs`, `tech-debt`

## Description

Two small coverage gaps in `crates/core/src/ecs/scheduler.rs`:

1. **All-stages-in-order**: `stages_run_in_order` (line 570-594) exercises `Early → Update → PostUpdate` (3 of 5 stages). `Physics` and `Late` never appear in any ordering test. The `BTreeMap<Stage, _>` ordering by discriminant is robust against accidental reordering (the `Stage` enum has explicit `= N` discriminants), but a test that pins all five would document the contract and catch enum drift.

2. **Parallel-scheduler feature parity**: The test module runs against whatever feature set `cargo test` resolves — default-on (`parallel-scheduler` enabled). The two `cfg` branches at `scheduler.rs:318-329` diverge (rayon vs plain for-loop) and have different panic-propagation semantics. Neither path has a CI gate guaranteeing both build + run.

## Suggested Fix

```rust
#[test]
fn all_five_stages_run_in_order() {
    // Variant of stages_run_in_order with Early/Update/PostUpdate/Physics/Late
    // each incrementing the counter to expected ordinal.
}

// In CI: cargo test --no-default-features --features=<minimum required>
// as a sibling job.
```

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: if a new `Stage` variant gets added later, update both the test name and the assertion count
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: this issue IS the test addition — closes when the all-five-stages test + (optionally) the CI matrix bit land
