# Issue #665: LIFE-L1: allocator-Arc-leak fall-through proceeds to device.destroy_device — driver-level UAF risk

**File**: `crates/renderer/src/vulkan/context/mod.rs:1390-1403`
**Dimension**: Resource Lifecycle

When `Arc::try_unwrap(alloc_arc)` fails (outstanding references — caused by a forgotten clone in some subsystem), the code logs an error, debug_asserts (panic in debug only), and FALLS THROUGH to `self.device.destroy_device(None)` at line 1403.

Release-build behavior: the allocator's Arc count never reaches zero before the device is destroyed. Any GPU memory still allocated lives on with a now-invalid device handle; the Allocator's eventual Drop tries to call `vkFreeMemory` on a destroyed device → driver-level use-after-free. The `debug_assert` masks the issue from CI.

**Fix**: After the `Err(arc)` arm, `return;` instead of falling through. Leak the allocator entirely (already happening) AND don't destroy the device. The OS reclaims on process exit. Better than UB.

Even cleaner: track Arc clone owners with a debug counter so the leak source is reportable.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
