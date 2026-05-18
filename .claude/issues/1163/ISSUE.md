**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Thread Safety
**Severity**: HIGH

## Observation

`crates/renderer/src/vulkan/ssao.rs:149-166`:

```rust
let ao_allocation = match allocator.lock().expect("allocator lock").allocate(
    &gpu_allocator::vulkan::AllocationCreateDesc {
        name: &format!("ssao_output_{fi}"),
        requirements: unsafe { device.get_image_memory_requirements(ao_image) },
        location: gpu_allocator::MemoryLocation::GpuOnly,
        linear: false,
        allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
    },
) {
    Ok(a) => a,
    Err(e) => {
        unsafe { partial.destroy(device, allocator) };  // re-locks the still-held Mutex
        return Err(anyhow::anyhow!("Failed to allocate AO memory {fi}: {e}"));
    }
};
```

Per Rust temporary-lifetime rules for match scrutinees, the intermediate `MutexGuard` (produced by `.expect(...)`) lives until end of `match`, so the lock is **still held inside the `Err(e)` arm**. That arm calls `unsafe { partial.destroy(device, allocator) }`, and `SsaoPartial::destroy` re-acquires `allocator.lock()` at `ssao.rs:596` (`allocator.lock().expect("allocator lock").free(a).ok();`). `std::sync::Mutex` is **non-reentrant** — the same thread blocks forever.

## Why it's a bug

Process hang (not a clean error return) on the first VRAM-pressure failure during SSAO init. The engine deadlocks instead of bubbling up OOM. Same shape as #908 / REN-D1-NEW-04 / #952 but on the allocator lock rather than fence reset.

## Trigger Conditions

VRAM allocation failure during `SsaoPipeline::new` — the per-frame `R8_UNORM` AO image allocation returns `Err`. Reachable on near-OOM startup or with an undersized allocation block pool.

## Fix

Hoist the lock-and-allocate result into a `let` binding so the `MutexGuard` drops at end-of-statement before the match runs:

```rust
let result = allocator.lock().expect("allocator lock").allocate(&desc);
match result {
    Ok(a) => a,
    Err(e) => {
        unsafe { partial.destroy(device, allocator) };
        return Err(...);
    }
}
```

Apply the same fix to `composite.rs:935` (uses `?` — fine but worth normalising) and `context/helpers.rs:313` (latent — TS-D4-NEW-03 / sibling issue).

## Completeness Checks

- [ ] **UNSAFE**: confirm `partial.destroy` safety comment still accurate post-fix (Err arm no longer holds lock)
- [ ] **SIBLING**: check `composite.rs:935`, `context/helpers.rs:313`, `mesh.rs:720` for same temporary-extension pattern
- [ ] **DROP**: verify `SsaoPartial::destroy` reverse-order destruction still correct
- [ ] **LOCK_ORDER**: confirm allocator-then-device order unchanged (no new lock acquired in Err arm)
- [ ] **FFI**: N/A
- [ ] **TESTS**: regression test that injects an allocation failure and asserts a clean `Err` return (no hang)

## Related

- #908 / REN-D1-NEW-01 — original "mirror" issue (fence-reset path, closed)
- #952 / REN-D1-NEW-04 — fence-reset-before-fallible-recording (open; same shape, different lock)
- TS-D4-NEW-02 — sibling latent variant on the bind-failure path (`ssao.rs:170-182`)
- TS-D4-NEW-03 — sibling latent variant on the depth-image path (`context/helpers.rs:313-332`)
