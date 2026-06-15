# Issue #1637 — REG-05: egui render-pass balance on cmd_draw error (#1491) has no dedicated test

_Snapshot as filed (immutable). GitHub is authoritative for current state._

**Source:** `docs/audits/AUDIT_REGRESSION_2026-06-14.md` — REG-05 (PARTIAL hardening gap, LOW)

The fix for **#1491** is **present and correct**; this issue tracks the missing guard test, not a regression.

## Description
On the egui draw-error path, the `cmd_draw` result is captured, `cmd_end_render_pass` runs unconditionally, and only then does the error propagate — so no submitted command buffer is left with an open render pass. The renderer suite compiles/runs this code, but no test exercises the error branch specifically.

## Evidence
- `crates/renderer/src/vulkan/egui_pass.rs` (~`:204-209`) — `cmd_draw` result captured, `cmd_end_render_pass` unconditional, error propagated after.

## Impact
A revert would re-arm an unbalanced render pass on the egui error path — a Vulkan spec violation (HIGH if it regressed). The coverage gap itself is LOW.

## Suggested Fix
Hard to unit-test (needs a forced `cmd_draw` failure). Leave/confirm a tracking comment at the site naming the "render pass must end even on draw error" balance invariant; optionally a fault-injection test if the draw call can be made fallible in a test harness.

## Completeness Checks
- [ ] **DROP**: Render-pass begin/end balance preserved on every early-return / error path in `egui_pass.rs`
- [ ] **TESTS**: Balance invariant documented at the site (error-branch hard to unit-test without fault injection)
