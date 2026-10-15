# REN-D7-2026-10-14-02: With #5370 live, SVGF progressive accumulation engages in every standing session but its scene-unchanged key ignores emissive/material/ambient changes

**Labels**: low,renderer,bug
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D7-2026-10-14-02`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`caustic_scene_key`, `caustic_history_valid`); `crates/renderer/shaders/svgf_temporal.comp` (`floorC = params.w > 0.5 ? 0.0 : params.x`)

## Description
Before #5370, `camera_static` was almost never true for a standing player; now a parked player with an unchanged light rig gets `alpha = 1/(histAge+1)` down to 1/256. The key covers rigid instance motion, instance-set changes, `pose_dirty` and the every-float light rig, but not animated emissives, quest-state material swaps or ambient/DALC/fog/sky colour.

## Evidence
`caustic_scene_key` folds `current_model` of caustic sources, then `lights.len()` and the four `GpuLight` vec4s; a grep of `build_and_upload_instances.rs` for `emissive|ambient` returns nothing in the key path; `rigid_instance_moved` compares only `model_matrix`.

## Impact
Seconds of lag in indirect response to animated emissives (neon, TV, lava) while the player stands still. Not measured. Related: #5370, #4046, #4943.

## Suggested Fix
Only after a capture confirms it: fold a per-frame emissive/material generation counter into `caustic_scene_key`, the way light changes are. Do not widen the key speculatively.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
