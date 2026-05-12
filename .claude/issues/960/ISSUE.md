# #960 — REN-D8-NEW-16: evict_unused_blas safety tied to MAX_FRAMES_IN_FLIGHT==2 without const_assert

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM8_v2.md`
**Dimension**: Acceleration Structures
**Severity**: LOW
**Confidence**: HIGH
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/960

## Locations

- `crates/renderer/src/vulkan/acceleration.rs:2548-2563` — immediate destroy in eviction loop
- `crates/renderer/src/vulkan/acceleration.rs:551-570` — `drop_blas` (deferred-destroy path) for comparison
- `crates/renderer/src/vulkan/sync.rs:33-35` — `MAX_FRAMES_IN_FLIGHT == 2` pinned

## Summary

`evict_unused_blas` destroys immediately (not via `pending_destroy_blas`). Safety relies on `min_idle = MAX_FRAMES_IN_FLIGHT + 1 = 3` meaning evicted BLAS haven't been referenced in 3+ frames. Correct under current pin, fragile to a future `MAX_FRAMES_IN_FLIGHT == 3` bump.

## Fix (preferred)

`static_assertions::const_assert!(min_idle > MAX_FRAMES_IN_FLIGHT)` near the constant. Alternative (heavier): route through `pending_destroy_blas` for invariant-free safety.

## Tests

N/A for const_assert (compile-time). Existing deferred-destroy tests cover the heavier route.
