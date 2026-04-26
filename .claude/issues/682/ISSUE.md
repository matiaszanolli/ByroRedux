# Issue #682: MEM-2-7: TLAS scratch buffer never shrinks (parallel of MEM-2-3 but for scratch_buffers[frame])

**File**: `crates/renderer/src/vulkan/acceleration.rs:1752-1778`
**Dimension**: GPU Memory

`scratch_buffers[frame_index]` follows `scratch_needs_growth` (grow-only, no shrink) for TLAS scratch. The BLAS scratch path now has `shrink_blas_scratch_to_fit` (#495). The TLAS path doesn't.

After a single big exterior frame (8k+ instances → ~MB-scale scratch), the frame-slotted scratch holds that size for the rest of the session. Same hysteresis-band + LRU watermark as the BLAS scratch shrink would apply.

**Fix**: Add `shrink_tlas_scratch_to_fit` mirroring `shrink_blas_scratch_to_fit` (#495), called from the same telemetry tick. Pair with TLAS instance buffer shrink (sibling MEM-2-3).

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
