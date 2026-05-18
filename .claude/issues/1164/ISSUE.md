**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Thread Safety
**Severity**: HIGH

## Observation

`crates/renderer/src/vulkan/ssao.rs:170-182`:

```rust
if let Err(e) = unsafe {
    device.bind_image_memory(ao_image, ao_allocation.memory(), ao_allocation.offset())
} {
    allocator
        .lock()
        .expect("allocator lock")
        .free(ao_allocation)
        .ok();
    unsafe { partial.destroy(device, allocator) };
    return Err(anyhow::anyhow!("Failed to bind AO image memory {fi}: {e}"));
}
partial.ao_allocations.push(Some(ao_allocation));  // AFTER the bind-error early return
```

After a bind failure: (1) explicit `allocator.lock()...free(ao_allocation).ok()` (lines 173-177), then (2) `unsafe { partial.destroy(device, allocator) }` (line 180). The locks **don't overlap** today — first drops at end-of-statement, second re-locks per-allocation. **No deadlock at present.**

**However**: `partial.ao_allocations.push(Some(ao_allocation))` is at line 183, AFTER the bind-error early return. The freed allocation is **not** in `partial.ao_allocations` when `partial.destroy` iterates, so we don't double-free today. But:

- The asymmetry is fragile: a future maintainer reordering `partial.ao_allocations.push(...)` to before the bind would silently introduce a double-free of the same allocation handle.
- `partial.ao_images` DOES contain the unbound `ao_image` (pushed at line 147), so `partial.destroy` destroys it — that part is correct.

## Why it's a bug

Latent. A reordering refactor (or copying the pattern to another image-init path) silently produces a double-free on bind failure.

## Trigger Conditions

`device.bind_image_memory` fails on the SSAO AO image. Reachable on driver / extension issues, not just OOM.

## Fix

Option A (structural): Wrap the `(image, allocation, view)` trio in a single RAII guard that frees on drop. Each successful step swaps the guard's payload to `None` so the drop path becomes inert. Eliminates the entire class of bind-failure asymmetry.

Option B (minimal): Hoist `partial.ao_allocations.push(Some(ao_allocation));` to immediately after bind success so the partial's invariant ("every pushed image has a matching allocation slot") is structural, and free the allocation via `partial.destroy()` only (drop the explicit `allocator.lock()...free(ao_allocation)` at 173-177).

## Completeness Checks

- [ ] **UNSAFE**: bind-failure cleanup comment explains the partial-state invariant
- [ ] **SIBLING**: check every other `device.bind_image_memory` failure path for the same asymmetry (composite, gbuffer, taa, svgf, caustic, bloom, water, ssao)
- [ ] **DROP**: confirm `partial.destroy` iterates `ao_images` / `ao_allocations` symmetrically post-fix
- [ ] **LOCK_ORDER**: no change to lock order
- [ ] **FFI**: N/A
- [ ] **TESTS**: regression test that simulates a bind failure and asserts no double-free under Vulkan validation layer

## Related

- TS-D4-NEW-01 — sibling allocator-lock deadlock on the allocate path
- TS-D4-NEW-03 — depth-image init has the same temporary-extension pattern
