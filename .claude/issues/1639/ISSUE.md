# Issue #1639 — REG-07: SVGF firefly-clamp hoist (#1481) has no test

_Snapshot as filed (immutable). GitHub is authoritative for current state._

**Source:** `docs/audits/AUDIT_REGRESSION_2026-06-14.md` — REG-07 (PARTIAL hardening gap, LOW)

The fix for **#1481** is **present and correct**; this issue tracks the missing guard test, not a regression.

## Description
The spatial firefly clamp was hoisted ahead of the `hasHistory` branch so the disocclusion frame is also clamped. The original issue marked TESTS N/A (visual).

## Evidence
- `crates/renderer/shaders/svgf_temporal.comp` — firefly clamp applied before the `hasHistory` branch.

## Impact
A revert re-arms a one-frame un-clamped firefly on disocclusion — visual only, invisible to `cargo test`.

## Suggested Fix
Manual/RenderDoc gate — acceptable. If desired, leave/confirm a comment naming that the clamp must precede `hasHistory` so the disocclusion frame is covered.

## Completeness Checks
- [ ] **SIBLING**: Clamp-before-history ordering consistent with any other denoiser stage relying on it
- [ ] **TESTS**: Visual/RenderDoc gate documented (shader path not cargo-observable)
