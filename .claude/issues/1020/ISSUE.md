# Issue #1020

**Title**: REN-D15-NEW-11: Cloud parallax is screen-space, not world-XY — rotating camera carries clouds with view

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D15-NEW-11
**Severity**: MEDIUM
**File**: triangle.frag (cloud sample site)

## Issue

Cloud parallax direction is screen-XY rather than world-XY. Rotating the camera makes clouds appear to follow the view rather than scroll along world-space wind direction. Audit-checklist item #5 explicitly requires world-XY.

## Fix

Project the parallax direction through view-inverse before sampling cloud layers. Magnitude should still scale with TOD wind multiplier.

## Completeness Checks
- [ ] **SIBLING**: Affects all 4 cloud layers — verify scroll vectors all routed through the same world-space transform
- [ ] **TESTS**: Camera-yaw integration with fixed cloud position should produce stationary clouds

