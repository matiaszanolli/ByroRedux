# Issue #658: AS-8-3: static BLAS single-shot path lacks ALLOW_COMPACTION flag — only batched path compacts

**File**: `crates/renderer/src/vulkan/acceleration.rs:443-447, 522-527`
**Dimension**: Acceleration Structures

`build_blas` (single-shot) sets `flags(PREFER_FAST_TRACE)` only. `build_blas_batched` sets `flags(PREFER_FAST_TRACE | ALLOW_COMPACTION)` and runs a compaction pass that typically saves 30-50% of BLAS storage.

The single-mesh path is on the cell-load slow path for one-off meshes (UI quad registration calls it via the `false` rt_enabled bit, so it's not actually called for RT-eligible single meshes today — every RT mesh goes through the batched path) but if a future caller routes an RT mesh through `build_blas_for_mesh` (e.g. lazy mesh upload after first sight), that BLAS will be uncompacted and consume the budget twice as fast as a batched-path peer.

**Fix**: Add `ALLOW_COMPACTION` to the single-shot `build_blas` flags so the path is consistent with the batched path. The build won't auto-compact (no copy phase wired in), but the flag is a no-op cost; if a future caller wants compaction it can run `cmd_copy_acceleration_structure(MODE = COMPACT)` later. Document the flag-only choice in a comment.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
