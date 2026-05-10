---
issue: 0
title: REN-D8-NEW-05: Single-shot build_blas lacks pre-build budget eviction guard (build_blas_batched has it)
labels: renderer, medium, vulkan, memory
---

**Severity**: MEDIUM (VRAM OOM during streaming bursts on smaller GPUs)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 8)

## Location

- `crates/renderer/src/vulkan/acceleration.rs::build_blas` — single-shot BLAS build path
- `crates/renderer/src/vulkan/acceleration.rs::build_blas_batched` — batched path (has the guard)

## Why it's a bug

`build_blas_batched` runs `evict_unused_blas` pre-batch and every 64 iterations, but the single-shot `build_blas` path never does. A streaming refactor that promotes single-shot to the hot path would silently bypass budget enforcement on smaller-VRAM GPUs (per `feedback_vram_baseline.md`, RT minimum is 6 GB; we target a budget under ~4 GB).

## Fix sketch

Hoist the `evict_unused_blas` call into a shared prelude that both paths invoke, OR call `evict_unused_blas` at the top of `build_blas` mirroring the batched path.

## Completeness Checks

- [ ] **SIBLING**: Verify all BLAS-creating call sites (UI quad, particle quad) are either rt=false or covered by eviction.
- [ ] **TESTS**: Streaming stress test on a 6-GB GPU (or VRAM-limited budget): walk a large exterior cell, verify VRAM stays under budget.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
