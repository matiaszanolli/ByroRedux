# #2398 — CONC-D3-2026-08-07-04: Two per-frame `Mutex` acquisitions silently recover from poison with `into_inner()` and no rationale comment

- **Severity**: LOW
- **Domain**: sync
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2398


- **Severity**: LOW
- **Dimension**: 3 — ECS Lock Ordering (poison-handling doctrine)
- **Location**: `byroredux/src/systems/metrics.rs:79` (`state.sys.lock()`), `byroredux/src/systems/metrics.rs:96` (`alloc_res.0.lock()`)
- **Status**: NEW

**Description**

`metrics_sample_system` acquires two inner `Mutex`es (`MetricsState::sys`, the `sysinfo::System` handle; `AllocatorResource.0`, the gpu-allocator) as `.lock().unwrap_or_else(|e| e.into_inner())` — deliberately continuing on a poisoned lock, the exact inverse of the `#466` fail-fast lock-poison doctrine applied to every ECS storage/resource, with no comment explaining the deviation at either site. For `AllocatorResource` specifically, recovering means calling `generate_report()` on a `gpu_allocator` instance whose invariants were mid-update when some other thread panicked.

**Evidence** (re-confirmed at publish time against commit `79bfc76e`):

```rust
let mut sys = state.sys.lock().unwrap_or_else(|e| e.into_inner());
...
let alloc = alloc_res.0.lock().unwrap_or_else(|e| e.into_inner());
```

Contrast `world.rs:22-28`/`:45-51`, where a poisoned ECS lock re-panics with the type name specifically so "a post-panic access fails loud, never silently reads torn state."

**Impact**

Bounded — feeds `MetricsSnapshot` (debug overlay/`byro-dbg`), so worst case is a wrong or torn diagnostics number, not gameplay state. The real cost is doctrine drift: the poison policy is stated as absolute in the ECS layer and quietly not followed two crates over.

**Related**: #466 (fail-fast poison doctrine), #1837 (the `insert_resource` follow-up removing the last `.ok()`-swallow in `world.rs`).

**Suggested Fix**: Either fail loud like the ECS layer, or keep `into_inner()` and add a one-line comment stating this metric is diagnostics-only and a torn read is preferred to losing the overlay.

## Completeness Checks
- [ ] **SIBLING**: Grep for other `unwrap_or_else(|e| e.into_inner())` sites outside the ECS core to check for the same undocumented poison-recovery deviation
- [ ] **TESTS**: N/A if the fix is a comment; if switched to fail-fast, a test poisoning the two `Mutex`es and asserting a panic with a diagnostic message

---
Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md` via `/audit-publish`.
