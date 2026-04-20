# Issue #453

FO3-REN-M2: GpuInstance lacks parallax + env cube bindings — material values die at the renderer boundary

---

## Severity: Medium

**Location**: `crates/renderer/shaders/triangle.frag:28-60` (GpuInstance struct), cross-cuts `crates/renderer/shaders/triangle.vert` and `ui.vert` per Shader Struct Sync

## Problem

`GpuInstance` has no `parallaxIndex` / `parallaxHeightScale` / `parallaxMaxPasses` fields and no env cube sampler binding. `MaterialInfo` at `crates/nif/src/import/material.rs:182-192` parses parallax values, but they die at the renderer boundary.

Grep of the entire `crates/renderer/shaders/` tree for `parallax|heightmap|envmap` returns zero matches.

## Impact

Every `shader_type=7` ParallaxOcc surface (a dominant FO3 architecture variant) renders as base-color-only. Combined with FO3-REN-M1 (import side), the full parallax pipeline is dead.

## Fix

1. Add 3 fields to `GpuInstance`: `parallaxIdx: u32`, `heightScale: f32`, `maxPasses: f32`.
2. Add env cube sampler binding (descriptor set index + texture handle).
3. Write frag-shader parallax-occlusion branch guarded on `parallaxIdx != 0`.
4. Bump all three shader files that reference `GpuInstance` (`triangle.vert`, `triangle.frag`, `ui.vert`) in one commit — per the Shader Struct Sync memory note, they must move in lockstep.

## Prerequisite

FO3-REN-M1 (import-side texture slots 3/4/5) needs to populate the paths first.

## Completeness Checks

- [ ] **TESTS**: Synthetic scene with parallax material → frag shader branch reached (renderdoc trace ok)
- [ ] **SIBLING**: Update `gpu_instance_struct` in triangle.vert, triangle.frag, AND ui.vert — all three must match exactly
- [ ] **DROP**: No new Vulkan objects owned; no Drop change needed
- [ ] **DOCS**: Shader Struct Sync memory reference at GpuInstance definition

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-REN-M2)
