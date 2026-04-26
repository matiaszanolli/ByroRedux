# Issue #664: CMD-5: per-mesh fallback path rebinds VB/IB every batch when global VB/IB is absent

**File**: `crates/renderer/src/vulkan/context/draw.rs:1305-1339` (the dispatch_direct closure)
**Dimension**: Command Recording (perf)

When `global_bound == false` (early-startup / spinning-cube demo, or after a future failure mode), the closure re-binds `mesh.vertex_buffer` + `mesh.index_buffer` for every batch invocation, even when consecutive batches share `mesh_handle`. Batch coalescing already merges consecutive same-mesh draws into one batch, so this is rare in practice — but a two-sided alpha-blend split (lines 1341-1352) calls `dispatch_direct` twice for the same batch, which means two redundant VB/IB binds for that case.

**Fix**: Cache `last_bound_mesh_handle` across `dispatch_direct` invocations and skip the bind when `mesh_handle == last_bound_mesh_handle`. Trivial; no API change.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
