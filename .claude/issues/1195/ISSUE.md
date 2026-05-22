title:	PERF-DIM7-01: skin compute dispatch fires per-entity every frame regardless of bones-changed
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, M29, medium, performance, renderer
comments:	0
assignees:	
projects:	
milestone:	
number:	1195
--
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-05-19.md (Dim 7, MEDIUM)
**Status**: NEW, CONFIRMED
**Paired with**: #PERF-DIM7-02 (BLAS refit must gate on the same bool)
**Blocked on**: #1194 (PERF-DIM7-INSTR — needs GPU timer + dispatches_skipped counter first)

## Symptom

An idle skinned NPC (no animation playing, paused, T-pose waiting for animation player) still pays full `skin_vertices.comp` dispatch + BLAS refit every frame. On FNV Prospector (34 skinned NPCs, typically ~20 idle) that's 34 dispatches × ~5K vertices and 34 BLAS refits per frame regardless of motion.

## Cause

[crates/renderer/src/vulkan/context/draw.rs:887-911](https://github.com/matiaszanolli/ByroRedux/blob/main/crates/renderer/src/vulkan/context/draw.rs#L887-L911) — the steady-state dispatch loop walks `dispatches` (every skinned entity in `draw_commands`) unconditionally and calls `skin_pipeline.dispatch` for each. No per-entity "bones changed this frame" gate.

`build_skinned_palettes` (byroredux/src/render/skinned.rs) re-uploads `bone_world` for every allocated slot per frame irrespective of whether per-bone GlobalTransforms moved.

## Fix (two viable strategies — pick one after instrumentation lands)

1. **CPU hash gate**: xxhash3 the per-entity bone_world slice in `build_skinned_palettes`; cache previous-frame hash on the entity (next to `SkinSlotPool` last-seen-frame book); skip dispatch + refit when hash unchanged AND slot already has populated output buffer + live BLAS. Cost: ~0.3 µs / slot at xxhash3's ~30 GB/s; ~10 µs total at 34 slots.
2. **ECS-side gate**: route `AnimationPlayer::dirty` through the `SkinnedMesh` query in `build_skinned_palettes` so unchanged-AnimationPlayer entities skip palette construction entirely.

## Estimated Impact

Needs measurement (gate on #1194). Theoretical upper bound at 60% idle NPC rate: ~0.5–1 ms / frame on Prospector compute side. Combined with PERF-DIM7-02's BLAS refit gate: ~3 ms / frame upper bound.

## Regression Risk: HIGH

- Missed gate (animation finishes but `dirty` not flipped; `mark_skinned_visible` runs before the gate) leaves the BLAS holding stale geometry.
- Pre-refit COMPUTE→AS_BUILD barrier doesn't help if the skin compute itself was skipped.
- Mitigation: gate ONLY in steady state once the entity already has a populated output buffer + live BLAS. Never skip first-sight.
- MUST gate the BLAS refit on the same bool (see PERF-DIM7-02). Split decisions are the trap.

## Testability (post-#1194)

- `SkinCoverageFrame.dispatches_skipped` reaches expected count on synthetic idle scene
- `tex.skin` reports dispatch / refit time delta after fix
- Synthetic `--bench-hold` test with NPC held still: refit count → 0 within N frames

## Completeness Checks

- [ ] **UNSAFE**: hash gate path doesn't bypass any unsafe Vulkan ordering
- [ ] **SIBLING**: PERF-DIM7-02 BLAS refit gate uses the same bool
- [ ] **DROP**: N/A (no Vulkan object lifetime change)
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: regression test pins skip behavior for idle and motion cases; `last_used_frame` still bumps on skip paths so LRU doesn't reap quiescent slots
