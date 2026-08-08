# COORD-5: cell_rot_sweep example hand-copies the four-mode Euler dispatcher

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2438
**Finding ID**: COORD-5 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — Coordinate-system correctness
**Location**: `crates/plugin/examples/cell_rot_sweep.rs:22-27`
**Status**: NEW

## Description
The `cell_rot_sweep` example reproduces all four rotation-mode formulas verbatim (byte-identical to `byroredux/src/cell_loader/euler.rs:67,73,76,79` today) instead of calling the dispatcher.

## Impact
Example-only, no shipping-path risk — but the example exists specifically to triage REFR rotation disputes (#1277); if the dispatcher is retuned and the example isn't, the sweep reports conclusions about a convention the engine no longer uses.

## Suggested Fix
Move the four-mode match into a shared `pub` function both the dispatcher and the example call.

## Completeness Checks
- [ ] **TESTS**: Example still compiles and produces identical output after de-duplication (`cargo run -p byroredux-plugin --example cell_rot_sweep`)
