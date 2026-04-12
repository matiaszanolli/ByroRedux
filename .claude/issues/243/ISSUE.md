# Issue #243 — PERF-04-11-M7

**Title**: gpu_instances/batches Vecs allocated fresh per frame in draw_frame
**Severity**: MEDIUM
**Dimension**: CPU Allocations
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/renderer/src/vulkan/context/draw.rs:279-280`

## Summary
`draw_frame` builds `gpu_instances` and `batches` as locals every frame. Move to fields on `VulkanContext`, use `.clear()` + `.reserve()` for capacity amortization. Matches the propagation-system scratch pattern.

## Fix with
`/fix-issue 243`
