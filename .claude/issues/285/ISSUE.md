# #285: P1-01: GI ray query missing TerminateOnFirstHit — 30-60% slower traversal

## Finding
**Severity**: MEDIUM | **Dimension**: GPU Pipeline | **Type**: performance
**Location**: `crates/renderer/shaders/triangle.frag:735-740`
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-04-13.md`

## Description
The GI bounce ray uses `gl_RayFlagsOpaqueEXT` only. Shadow rays and window portal rays correctly use `TerminateOnFirstHitEXT`, but GI does not. For GI with pre-computed avg_albedo in the SSBO, any-hit suffices — we don't need geometric closest-hit.

## Impact
GI traversal 30-60% slower than necessary. ~0.1-0.3ms/frame at 1080p.

## Fix
Add `gl_RayFlagsTerminateOnFirstHitEXT` to GI `rayQueryInitializeEXT`:
```glsl
rayQueryInitializeEXT(
    giRQ, topLevelAS,
    gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF,
    giOrigin, 0.5, giDir, 500.0
);
```

## Completeness Checks
- [ ] **SIBLING**: Verify reflection ray correctly keeps closest-hit (needs geometric accuracy)
- [ ] **TESTS**: Visual comparison before/after on Prospector Saloon
