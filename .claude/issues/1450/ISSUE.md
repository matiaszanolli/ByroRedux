# #1450 — WAT-01: Submersion state has no hysteresis band (low-confidence)

_Snapshot as filed (2026-06-02) from AUDIT_RENDERER_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: LOW (low-confidence; design observation, not a regression)
- **Dimension**: Water
- **Location**: `byroredux/src/systems/water.rs:92` (`head_submerged: depth > 0.0`)
- **Status**: NEW

## Description
The submersion-state flip has no hysteresis band: `head_submerged = depth > 0.0`. If the camera is parked exactly at the waterline, underwater FX (tint/fog) could strobe as `depth` dithers across 0.0.

## Impact
Speculative — requires the camera held precisely at the waterline, which is not a normal gameplay state. No repro observed. Filed for tracking; **no pre-emptive fix recommended without a repro.**

## Suggested Fix
If strobing is ever observed, add a small hysteresis band (enter submerged at `depth > +eps`, exit at `depth < -eps`) so the boundary doesn't oscillate.

## Completeness Checks
- [ ] Repro captured before any fix (do not fix speculatively)
- [ ] **SIBLING**: if hysteresis added, apply the same band to any other waterline-gated FX toggle

_Filed from [docs/audits/AUDIT_RENDERER_2026-06-02.md](../blob/main/docs/audits/AUDIT_RENDERER_2026-06-02.md)._
