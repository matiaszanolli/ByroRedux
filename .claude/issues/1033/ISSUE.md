# Issue #1033

**Title**: REN-D15-NEW-12: Cloud wind_speed byte parsed but never wired — 0.018 literal still in scroll path

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D15-NEW-12
**Severity**: LOW
**File**: `byroredux/src/systems/weather.rs:343`

## Issue

WTHR `wind_speed` byte is parsed from the record but the cloud scroll-rate uses a hardcoded `0.018` literal. Real per-weather wind variation (calm vs storm) doesn't reach the cloud animation.

## Fix

Replace `0.018` literal with `wd.wind_speed as f32 * scale` (calibrate scale against vanilla bench captures).

## Completeness Checks
- [ ] **SIBLING**: Verify wind_speed reaches all consumers (cloud scroll, particle systems, foliage if any)
- [ ] **TESTS**: Calm-WTHR vs storm-WTHR scroll-rate diff

