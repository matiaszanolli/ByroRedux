# #820 — REN-D9-NEW-01: IOR-refraction roughness-spread basis NaN at normal incidence

**Severity**: MEDIUM
**Location**: `crates/renderer/shaders/triangle.frag:1457-1464`
**Source**: `docs/audits/AUDIT_RENDERER_2026-05-04_DIM9.md`
**Created**: 2026-05-04

## Summary

`normalize(cross(refractDir, N_geom_view))` is exactly zero at normal
incidence (camera dead-on glass), producing NaN ray direction.
Triggered for any glass material with `roughness > 0.0067`. Vulkan
spec violation (`VUID-RuntimeSpirv-OpRayQueryInitializeKHR-04347`).

## Fix

Replace manual basis with `buildOrthoBasis` (Frisvad — already in use
at GI / shadow / metal-reflection sites). One-line refactor.

## Sibling sweep

Audit other compute / fragment shaders for the same
`normalize(cross(a, b))` basis-construction anti-pattern.

## How to fix

```
/fix-issue 820
```
