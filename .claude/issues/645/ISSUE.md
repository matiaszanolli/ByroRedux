# Issue #645: MEM-2-3: TLAS instance staging buffer never shrinks after exterior-cell padding

**File**: `crates/renderer/src/vulkan/acceleration.rs:1656-1666` + `acceleration.rs:1866-1892`
**Dimension**: GPU Memory

`instance_buffer` is HOST_VISIBLE at `padded_count × 64 B` (8192 instances → 512 KB), grow-only. After a single big exterior frame (32k+ instances → ~MB-scale buffer), two 2 MB host-visible BAR buffers (staging + double-buffered) plus a 2 MB DEVICE_LOCAL stage stay resident for the rest of the session.

BLAS scratch has `shrink_blas_scratch_to_fit` (#495); TLAS doesn't.

**Fix**: Add `shrink_tlas_to_fit` mirroring #495's hysteresis (2× ratio + slack rule), called from the same end-of-frame path that already shrinks `tlas_instances_scratch` (#504). Verify against the TLAS fence's frame slot before destroying the old buffers.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
