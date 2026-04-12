# Issue #239 — PERF-04-11-M1

**Title**: Texture uploads bypass StagingPool
**Severity**: MEDIUM
**Dimension**: GPU Memory
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/renderer/src/vulkan/texture.rs:45-95`, `:304-314`
**Follow-up to**: #99

## Summary
`Texture::from_rgba` and `from_bc` allocate staging directly via allocator. #99 built StagingPool for this — mesh uploads use it, textures don't. 200-texture cell = 200 round-trips that could collapse to a handful of pool reuses.

## Fix with
`/fix-issue 239`
