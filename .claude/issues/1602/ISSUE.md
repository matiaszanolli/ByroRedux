# CONC-2026-06-14-02: Build-time scheduler guard checks only undeclared_parallel_count(), not declared conflicts or unknown pairs

- **Issue**: #1602
- **Severity**: LOW
- **Labels**: low, sync, bug
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/main.rs:933-938`
- **Source**: `docs/audits/AUDIT_CONCURRENCY_2026-06-14.md` (CONC-2026-06-14-02)
- **Status when filed**: NEW, CONFIRMED — structural coverage gap that let #1601 ship undetected.

## Description
The #1394 guard was scoped to the migration KPI (undeclared parallel count → 0). It does not guard the post-migration invariant (0 known conflicts, 0 unknown pairs). `known_conflict_count()` / `unknown_pair_count()` exist on `AccessReport` and feed `sys.accesses` but are never asserted at construction.

## Impact
A declared write-write / write-read conflict between two parallel same-stage systems compiles, runs, and only surfaces if a human reads `sys.accesses`. No automated gate.

## Suggested Fix
Add `debug_assert_eq!` on `known_conflict_count()` and `unknown_pair_count()` alongside the existing assertion. Optionally a CI integration test that builds the real engine `Scheduler` and asserts all three counts are 0.

## Related
#1601 (the conflict this gap let through); #1394.
