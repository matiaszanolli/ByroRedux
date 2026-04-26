# Issue #642: AS-8-1: per-frame refit_skinned_blas loop has no inter-build scratch barrier — write-write hazard with ≥2 skinned NPCs

**File**: `crates/renderer/src/vulkan/context/draw.rs:573-591` + `crates/renderer/src/vulkan/acceleration.rs:803-885`
**Dimension**: Acceleration Structures (sibling to the open MEM-2-2 audit finding for cross-submission ordering)

The per-frame skinned-refit loop records `cmd_build_acceleration_structures(mode = UPDATE, src=dst=entry.accel)` for every skinned entity into the same per-frame command buffer. Each refit reads from + writes to the shared `blas_scratch_buffer` (acceleration.rs:817-820, 849-853, 869-871).

Vulkan spec (`VkAccelerationStructureBuildGeometryInfoKHR > scratchData`) requires consecutive AS builds sharing scratch be separated by an `AS_BUILD_WRITE → AS_BUILD_READ` barrier. The static batched path enforces this between iterations (acceleration.rs:1183-1198) but the per-frame skinned refit loop does not.

With one skinned actor the loop runs once and the bug is invisible. On a populated FNV cell with ≥2 skinned NPCs sharing the same scratch in the same submission, GPU-level scratch corruption is possible. Most drivers serialize implicitly via cache flushes between AS builds, masking the bug — but it is UB per spec.

**Fix**: Between iterations, emit `MemoryBarrier(AS_WRITE → AS_WRITE)` at `AS_BUILD_KHR → AS_BUILD_KHR` stage. Cleanest is to lift into a helper on AccelerationManager:

```rust
fn record_scratch_serialize_barrier(&self, cmd: vk::CommandBuffer)
```

and call at the top of every iteration except the first.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
