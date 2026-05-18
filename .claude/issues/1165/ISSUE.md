**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Thread Safety
**Severity**: MEDIUM

## Observation

`crates/renderer/src/vulkan/context/helpers.rs:313-332`:

```rust
let allocation = match allocator.lock().expect("allocator lock poisoned").allocate(
    &vk_alloc::AllocationCreateDesc {
        name: "depth_buffer",
        requirements,
        location: MemoryLocation::GpuOnly,
        linear: false,
        allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
    },
) {
    Ok(a) => a,
    Err(e) => {
        unsafe {
            device.destroy_image(image, None);
        }
        return Err(anyhow::Error::from(e).context("Failed to allocate depth image memory"));
    }
};
```

Same temporary-extension pattern as TS-D4-NEW-01. The `MutexGuard` from `.expect("allocator lock poisoned")` lives throughout the `Err(e) => { … }` arm because temporaries in a match scrutinee live until end of match. The Err arm here only calls `device.destroy_image` — no re-lock — so **no deadlock today**.

## Why it's a bug

Latent. The global allocator Mutex is held across a Vulkan API call for no reason (contention surface). More importantly: someone adding a second cleanup path that touches the allocator (e.g. freeing a sibling allocation in a future refactor) would silently introduce a deadlock identical to TS-D4-NEW-01.

## Trigger Conditions

`allocate(...)` fails for the depth image at startup. Today: Err arm only destroys the image. Future: any added cleanup path that re-locks the allocator deadlocks.

## Fix

Hoist `let result = allocator.lock().expect(...).allocate(&desc);` to a `let` binding so the guard drops at end of statement, then match the result.

## Completeness Checks

- [ ] **UNSAFE**: `device.destroy_image` safety comment unchanged
- [ ] **SIBLING**: full sweep — `grep -rn "match allocator.lock()" crates/renderer/` for every site that uses this pattern
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: confirm no new locks acquired
- [ ] **FFI**: N/A
- [ ] **TESTS**: not strictly needed (latent); could add a doc-test asserting the hoisted pattern

## Related

- TS-D4-NEW-01 — active deadlock variant of the same shape on SSAO OOM path
- TS-D4-NEW-02 — latent double-free variant on SSAO bind path
