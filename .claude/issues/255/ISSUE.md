# Issue #255 — PERF-04-11-E2

**Title**: Per-mesh vertex/index offsets duplicated across every GpuInstance
**Severity**: ENHANCEMENT
**Dimension**: Draw Call Overhead / GPU Memory Bandwidth
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/renderer/src/vulkan/scene_buffer.rs:65-70`

## Summary
`GpuInstance` carries `vertex_offset`, `index_offset`, `vertex_count` — identical per instance of same mesh. 12 B/instance × 1000 instances = 12 KB wasted SSBO bandwidth. Move to per-mesh `MeshInfo` buffer indexed by mesh ID. Net win only at ~10k instances — profile before implementing.

## Fix with
`/fix-issue 255`
