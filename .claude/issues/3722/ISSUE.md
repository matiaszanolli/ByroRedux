# #3722 — ESM-2026-08-30-D1-02: unknown top-level GRUP labels are skipped with no telemetry

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: Header & GRUP Walk
**Location**: `crates/plugin/src/esm/records/mod.rs` (the dispatcher's catch-all `_ => { reader.skip_group(&group); }`, ~:486-487)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D1-02)

## Description

The behaviour is correct — skip by declared size, continue, no per-record warn spam on a 5.6 M-record master. But the catch-all neither logs nor records into `index.skipped_unconsumed_groups` (`crates/plugin/src/esm/records/index.rs:421`), a field that exists and is currently used for exactly **one** label (`PDCL`, `records/mod.rs:386`).

## Impact

The routing-coverage signal is unavailable at runtime and had to be computed with an external walker. From this run's coverage matrix:

| game | records | routed | % | unrouted labels |
|---|---|---|---|---|
| Skyrim SE  |   869 687 | 98.7 % | 34 |
| FO4        | 1 549 276 | 98.4 % | 46 |
| FO76       | 5 635 950 | 98.1 % | 89 |
| Starfield  | 3 829 246 | 96.2 % | 95 |

## Suggested Fix

Push the label on the catch-all; cost is O(distinct labels).

## Completeness Checks
- [ ] **SIBLING**: `dispatch_misc_stub`'s equivalent silent path (`dispatch_misc_stub.rs:50` already names the gap) covered in the same change
- [ ] **TESTS**: A regression test asserts an unrouted label lands in `skipped_unconsumed_groups`
