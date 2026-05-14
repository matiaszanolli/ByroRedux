# Issue #1029

**Title**: REN-D9-NEW-06: Reflection-miss Fresnel weight short-circuits — metal vs glass callers see different miss semantics

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D9-NEW-06
**Severity**: LOW
**File**: `crates/renderer/shaders/triangle.frag` (`traceReflection` callers ~lines 2058-2060, glass caller)

## Issue

`traceReflection` returns `.a = 0.0` on miss as a "no hit" sentinel. Two callers interpret it inconsistently:
- Metal caller: `mix(ambient, rgb, clarity * a)` collapses to ambient on miss, *discarding* the pre-computed `skyTint*0.5` half.
- Glass caller: reads `rgb` directly and sees the sky.

One function, two semantics — confusing and a bug-magnet.

## Fix

Either: pick a single semantic for `.a` (probably "1.0 = hit confidence, weight your blend by this"), or split into `traceReflectionMetal()` / `traceReflectionGlass()` helpers. Document the contract at the function header.

## Completeness Checks
- [ ] **SIBLING**: Are there other ray-trace helper return-convention asymmetries?
- [ ] **TESTS**: Visual regression on metal grazing-angle vs glass grazing-angle miss

