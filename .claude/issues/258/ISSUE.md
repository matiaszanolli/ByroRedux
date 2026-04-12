# #258: R-03 — Global geometry SSBO has no growth mechanism

**Severity**: MEDIUM | **Domain**: renderer | **Type**: enhancement
**Location**: `crates/renderer/src/mesh.rs:136-180`
**Source**: `AUDIT_RENDERER_2026-04-12.md`

## Problem
`build_geometry_ssbo()` is one-shot. Meshes loaded after the initial build are not in the SSBO. RT reflection UV lookups will return garbage for late-loaded meshes.

## Fix
Track dirty flag on MeshRegistry; rebuild SSBO when new meshes added.
