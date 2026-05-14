# Issue #1036

**Title**: F-WAT-08: water.vert declares vUV / vInstanceIndex but fragment shader never reads them

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — F-WAT-08
**Severity**: LOW (dead code / minor perf)
**File**: `crates/renderer/shaders/water.vert` ; consumer `water.frag`

## Issue

`water.vert` declares `vUV` (location 4) and `vInstanceIndex` (location 5) as outputs but `water.frag` never reads them. Dead interpolators consume varying slots and a minor amount of vertex-output bandwidth.

## Fix

Drop the unused output declarations from `water.vert`. Spec-legal as-is but wasteful.

## Completeness Checks
- [ ] **SIBLING**: Audit other shaders for orphan interpolator outputs (triangle.vert, ui.vert, composite.vert)

