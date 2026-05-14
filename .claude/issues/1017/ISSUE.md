# Issue #1017

**Title**: REN-D9-NEW-05: traceReflection uses tMin=0.01 inconsistent with 0.05 elsewhere — black speckle on metals at grazing angles

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D9-NEW-05
**Severity**: MEDIUM
**File**: `crates/renderer/shaders/triangle.frag` (search `traceReflection` ~line 401)

## Premise verified (current `main`)

`traceReflection` uses `tMin = 0.01`, inconsistent with the `tMin = 0.05` rule at every other ray-query site (shadow, GI, IOR-refract). Origin bias at the call site is `N_bias * 0.05`–`0.1`, so 0.01 is well inside the macro-surface envelope and self-intersects on bumpy normal-perturbed flips.

## Issue

Symptom: black speckle on metals at grazing angles where perturbed normal flips the bias direction. Consistent tMin=0.05 across all reflection/shadow/GI sites is what every other shader site uses.

## Fix

Raise `traceReflection` tMin to 0.05, or pass tMin as a parameter so reflection callers can tune. Mechanical one-line shader change.

## Test

Bench on Prospector saloon (interior metal surfaces near grazing camera angles); diff before/after for speckle pixel count.

## Completeness Checks
- [ ] **UNSAFE**: N/A — GLSL
- [ ] **SIBLING**: Cross-check shadow, GI, IOR-refract tMin sites
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Speckle pixel-count regression

