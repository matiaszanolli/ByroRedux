# #3729 — ESM-2026-08-30-D7-03: ESM parse cost is 1.2-3.4 s per master, on the critical path of the first cell load, with no owner audit

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: ESM→ECS Handoff
**Location**: `crates/plugin/src/esm/records/mod.rs` (`parse_esm_with_load_order`)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D7-03)

## Description

ESM parse cost is **1.2–3.4 s per master**, on the critical path of the first cell load, with no owner audit.

| master | file | parse |
|---|---|---|
| Oblivion.esm   | 265 MB | 1.41 s |
| Fallout3.esm   | 275 MB | 1.23 s |
| FalloutNV.esm  | 234 MB | 1.17 s |
| Skyrim.esm     | 238 MB | 1.27 s |
| Fallout4.esm   | 315 MB | 1.69 s |
| SeventySix.esm | 880 MB | 3.41 s |

A vanilla FO4 load order (base + 7 DLC masters) is ~2.6 s of single-threaded ESM parsing before the first cell can load; FO76 alone is 3.4 s.

## Impact

`/audit-performance` Dim 8 owns NIF parse cost; **nothing owns this**. The walk is inherently sequential per plugin, but plugins are independent up to the `merge_from`, and each plugin's `FormIdRemap` is computed from its header alone *before* the record walk — nothing in the design prevents parsing them in parallel.

## Suggested Fix

Two parts:
1. Adopt ESM parse cost into `/audit-performance` Dim 8 alongside NIF parse cost, so it has an owner.
2. Scope parallel per-plugin parsing: the remap is header-derived and `merge_from` is the only join point.

## Completeness Checks
- [ ] **SIBLING**: If parsing is parallelised, `merge_from`'s ordering semantics (see #3403, #3384) are preserved deterministically
- [ ] **LOCK_ORDER**: If parallel parsing introduces shared state, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins that a parallel parse produces a byte-identical `EsmIndex` to the sequential one
