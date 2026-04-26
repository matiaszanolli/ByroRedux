# Issue #646: MEM-2-4: dead instance_address query in need_new_tlas block — footgun for re-introducing staging-buffer address into AS build

**File**: `crates/renderer/src/vulkan/acceleration.rs:1679-1681`
**Dimension**: GPU Memory

After #289 the AS-build reads `instance_buffer_device` (DEVICE_LOCAL) via `instance_address = get_buffer_device_address(...instance_buffer_device)` at line 1935, and the staging `instance_buffer` is correctly TRANSFER_SRC only. The split is sound today.

**However**, at line 1679 the buffer-device-address call happens inside the `need_new_tlas` block — its result is captured into a local `instance_address` that is never used for the build. The dead address query is a leftover from before the device/staging split. Not a bug today, but symptom of code where someone could re-introduce "use the staging buffer's address" by mistake.

**Fix**: Delete the unused `instance_address` query at lines 1679-1681 to remove the footgun; the live address call at 1935 stands on its own.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
