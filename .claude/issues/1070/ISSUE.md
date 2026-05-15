# #1070 — F-WAT-10: traceWaterRay constant hit colour — needs tracking

**Severity**: LOW  
**Audit**: `docs/audits/AUDIT_RENDERER_2026-05-14_DIM17.md`  
**Location**: `crates/renderer/shaders/water.frag:216`

## Summary

`traceWaterRay` returns `mix(skyTint.xyz, vec3(0.65, 0.7, 0.75), 0.4)` for all geometry hits — stone, sand, wood all look the same neutral grey. Water pipeline lacks SSBO bindings needed to fetch actual hit albedo. Known limitation with no tracking issue or TODO comment.

## Fix (short-term)

Add TODO comment at `water.frag:216` recording the architectural limitation and pointing to M38 Phase 2 work.

## Fix (long-term, M38 Phase 2)

Plumb `rayQueryGetIntersectionInstanceCustomIndexEXT` + MaterialBuffer/GlobalVertexBuffer/GlobalIndexBuffer bindings into the water pipeline descriptor set.
