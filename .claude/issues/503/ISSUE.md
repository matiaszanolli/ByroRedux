# PERF-2026-04-20 D2-L1: gpu-allocator single global pool — no per-usage separation, no fragmentation metrics

**Severity:** LOW | vulkan, memory, performance
**Source:** `docs/audits/AUDIT_PERFORMANCE_2026-04-20.md` (D2-L1)

## Problem
Single `Arc<Mutex<gpu_allocator::vulkan::Allocator>>` mixes long-lived (G-buffer, BLAS, vertex/index) with short-lived (per-frame staging) allocations. `log_memory_usage` only reports allocated-vs-reserved sums, no fragmentation visibility.

## Short-term fix (this issue)
Add fragmentation metric: per-block `largest_free_range / total_free`. Warn if worst block <0.5. Signals when a restart-to-defrag is due.

## Long-term (deferred)
gpu-allocator v0.28 per-scope budgets — track upstream.

## Completeness
- TEST: fragmentation-inducing alloc pattern asserts the metric reports <0.5
- PERF: report only on explicit call (`mem.frag` debug command), not every frame
