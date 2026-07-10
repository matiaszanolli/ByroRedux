# REN-D3-03: Stale lockstep-protocol comments in the layout-contract source files describe the pre-#1190/#1590 world

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1919

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:27-33`, `crates/renderer/src/vulkan/material.rs:36,63,140,385-387,406-408,1343`, `crates/renderer/src/vulkan/scene_buffer/constants.rs:267,280`, `crates/renderer/src/vulkan/context/mod.rs:230`
**Status**: NEW

## Description
The doc comments that teach the lockstep update protocol have themselves drifted: (1) `gpu_types.rs:27-33` names the five `GpuInstance` declaration sites incorrectly (still lists `triangle.frag`, which hasn't declared the struct since #1583/#1590; omits `include/bindings.glsl`). (2) `material.rs:385-387` claims MAT_FLAG bits are "mirrored shader-side as raw `0x...u` literals" — the exact opposite of the live generated-header contract (`shader_constants_data.rs:159-164` says "never hand-write them"). (3) `material.rs` references pre-Session-35 paths (`scene_buffer.rs:NNN`) that no longer exist as a single file. (4) `constants.rs:267,280` and `context/mod.rs:230` anchor the MATERIAL_KIND catalog on `GpuInstance::material_kind`, which moved to `GpuMaterial.material_kind` in R1 Phase 6.

## Evidence
All greps confirmed; `grep -rl "struct GpuInstance" crates/renderer/shaders/` returns bindings.glsl + 4 mirrors, not triangle.frag; `grep -c "MAT_FLAG_" triangle.frag` = 20 (all named, zero bare literals).

## Impact
These comments are the first thing an editor reads before touching a GPU struct; a wrong site list or an inverted single-source-of-truth claim invites exactly the drift class this dimension polices.

## Related
Session-35 layout split memory note; #1190/#1285 (generated-header migration these comments predate)

## Suggested Fix
One comment-only pass: fix the five-site list and sentinel wording in `gpu_types.rs`, rewrite the `material_flag` module doc to point at the generated-header contract, translate the four stale path refs, re-anchor the two `GpuInstance.material_kind` mentions to `GpuMaterial.material_kind`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
