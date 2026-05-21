# #1195 — PERF-DIM7-01: skin compute dispatch lacks bones-changed gate

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-05-19.md (Dim 7, MEDIUM)
**Severity**: medium
**Labels**: bug, medium, renderer, performance, M29
**State**: OPEN (filed 2026-05-19)
**Paired**: #1196 (refit must use same bool)
**Blocked on**: #1194 (instrumentation prerequisite)

## Cause

crates/renderer/src/vulkan/context/draw.rs:887-911 — dispatch loop walks `dispatches` unconditionally; no per-entity bones-changed gate. `build_skinned_palettes` re-uploads `bone_world` for every allocated slot per frame.

## Fix candidates

1. CPU hash gate: xxhash3 the per-entity bone_world slice; skip when unchanged.
2. ECS gate: route `AnimationPlayer::dirty` through `SkinnedMesh` query.

## Risk

HIGH — split gate vs. refit (#1196) would leave BLAS holding stale geometry. Must gate only in steady state with populated output + live BLAS.

## Estimated impact

Needs measurement (gated on #1194). Upper bound ~0.5-1 ms / frame on Prospector at 60% idle.
