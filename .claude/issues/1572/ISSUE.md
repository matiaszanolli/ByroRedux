# documentation, low, memory

## REN-D5-DOC-01: memory-budget.md says warn fires at 75% of any heap; code is 80% of smallest DEVICE_LOCAL heap on allocated bytes

**Severity**: LOW
**Dimension**: Memory/Lifecycle (doc-vs-code)
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW

## Description
`docs/engine/memory-budget.md` says "A warning fires if heap utilisation exceeds 75% of any heap type." Actual: **80%** (`(heap/5)*4`) of the **smallest single DEVICE_LOCAL heap**, compared against `total_allocated_bytes` only.

## Evidence
- `crates/renderer/src/vulkan/allocator.rs:295` — `warn_threshold_bytes` returns `(heap / 5) * 4 // 80% without losing precision to floats` (heap = smallest device-local, 2 GB fallback).
- `log_memory_usage` warns when `total_allocated_bytes > threshold`.
- `warn_threshold_falls_back_when_heap_missing` pins the 80% math.
- `docs/engine/memory-budget.md:201` — "exceeds 75% of any heap type".

## Impact
Doc-only; could mislead VRAM-pressure triage (warn is later than documented, single-heap-scoped, on allocated not reserved bytes).

## Suggested Fix
Update the doc to the 80% / smallest-DEVICE_LOCAL-heap / allocated-bytes wording.

## Completeness Checks
- [ ] **SIBLING**: any other doc referencing the VRAM warn threshold uses the same 80%/smallest-heap wording
