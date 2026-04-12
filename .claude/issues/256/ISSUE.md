# #256: R-01 — Cluster cull CameraUBO missing prevViewProj

**Severity**: HIGH | **Domain**: renderer | **Type**: bug
**Location**: `crates/renderer/shaders/cluster_cull.comp:32-38`
**Source**: `AUDIT_RENDERER_2026-04-12.md`

## Problem
CameraUBO in cluster_cull.comp is missing `prevViewProj` mat4 field (64 bytes). Every field after `viewProj` reads from wrong UBO offset. Corrupts cluster AABB computation during camera movement.

## Fix
Add `mat4 prevViewProj;` after `viewProj` in the shader's CameraUBO block. Recompile SPIR-V.
