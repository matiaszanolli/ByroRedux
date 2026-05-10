---
issue: 0
title: REN-D12-NEW-03: evict_unused_blas LRU thrash — skinned BLAS bytes counted but only static slots are eviction candidates
labels: bug, renderer, M29, medium, vulkan, memory
---

**Severity**: MEDIUM (LRU thrash on NPC-heavy scenes post-M41)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 12)

## Location

- `crates/renderer/src/vulkan/acceleration.rs:2336-2397` — `evict_unused_blas` body
- `crates/renderer/src/vulkan/acceleration.rs:973` — skinned BLAS adds to `total_blas_bytes`

## Why it's a bug

`evict_unused_blas` gates on `total_blas_bytes` which includes skinned-BLAS bytes (added at acceleration.rs:973), but the eviction loop only walks `self.blas_entries` (static slots) for eviction candidates. With many concurrent NPCs (post-M41 spawning) the budget can sit permanently over-budget and LRU-thrash static BLAS every frame — evicting and rebuilding the same static meshes repeatedly because the over-budget condition is driven by skinned BLAS that aren't candidates.

## Fix sketch

Separate `static_blas_bytes` accumulator from skinned. Compare against budget using `static_blas_bytes` only, since only static BLAS are evictable. (Skinned BLAS lifecycles are tied to entity visibility, managed elsewhere.)

```rust
struct AccelerationManager {
    total_blas_bytes: u64,    // total VRAM cost (telemetry)
    static_blas_bytes: u64,   // evictable subset
    // ...
}
```

## Completeness Checks

- [ ] **SIBLING**: Verify telemetry / tex.stats console reporting still uses `total_blas_bytes`.
- [ ] **TESTS**: Spawn 50 NPCs in an interior cell, verify static BLAS aren't evicted/rebuilt.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
