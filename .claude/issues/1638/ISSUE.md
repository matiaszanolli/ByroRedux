# Issue #1638 — REG-06: GPU-timer pool destroy outside allocator Drop guard (#1483) has no test

_Snapshot as filed (immutable). GitHub is authoritative for current state._

**Source:** `docs/audits/AUDIT_REGRESSION_2026-06-14.md` — REG-06 (PARTIAL hardening gap, LOW)

The fix for **#1483** is **present and correct**; this issue tracks the missing guard test, not a regression.

## Description
GPU-timer query-pool destroys were moved out of the `allocator.is_some()` guard so they also run on the allocator-`None` Drop path. Structurally correct; the original issue marked TESTS N/A (Drop ordering).

## Evidence
- `crates/renderer/src/vulkan/context/mod.rs` — `Drop` impl; `gpu_timers` destroy hoisted out of the allocator-`Some` guard so it runs regardless of allocator state.

## Impact
A revert leaks query pools on the allocator-`None` teardown path.

## Suggested Fix
Drop-ordering is awkward to unit-test; acceptable as structural. Leave/confirm the explanatory comment naming why the query-pool destroy lives outside the allocator guard.

## Completeness Checks
- [ ] **DROP**: Reverse-order teardown still correct; query-pool destroy runs on both allocator-`Some` and allocator-`None` paths
- [ ] **TESTS**: Drop-ordering rationale documented (not cargo-observable)
