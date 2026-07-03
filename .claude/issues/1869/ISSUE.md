# #1869: TD1-2026-07-03-01: crates/core/src/ecs/resources.rs crossed 2000 LOC (now 2077)

- **Severity**: LOW
- **Labels**: `low`, `tech-debt`, `bug`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-07-03.md` (TD1-2026-07-03-01)
- **Dimension**: 1 (Complexity)

## Location
`crates/core/src/ecs/resources.rs` (whole file, 2077 lines)

## Description
Grew from 1867 to 2077 lines via #1791/#1796 (pose-hash rollback for `SkinSlotPool`). Four natural domains exist unsplit; the `SkinSlotPool` segment (struct+impl+tests) is ~1057 lines, over half the file.

## Impact
Every unrelated resource-type edit recompiles/re-reviews against the same file as the large skinning-telemetry block. Pure edit/review-cost debt, not a correctness bug.

## Suggested Fix
Extract `SkinSlotPool` into `crates/core/src/ecs/resources/skin_slot_pool.rs`; optionally follow with a second pass grouping remaining resources into `resources/{core,debug,world}.rs`, mirroring the `cell_loader.rs`/`scene.rs` thin-dispatch pattern.
