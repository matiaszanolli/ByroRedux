# CONC-D1-01: One-time command helper holds the graphics-queue Mutex across wait_for_fences(u64::MAX)

**Issue**: #1713
**Severity**: LOW
**Labels**: low, sync, bug
**Source audit**: `docs/audits/AUDIT_CONCURRENCY_2026-06-23.md`
**Dimension**: Vulkan Queue & AS Sync / Worker Threads
**Location**: `crates/renderer/src/vulkan/texture.rs:659-666` (`with_one_time_commands_inner`); mirrored at `crates/renderer/src/vulkan/context/draw.rs:3487-3491` (egui `dispatch`)

## Description
`with_one_time_commands_inner` locks `graphics_queue`, submits, then blocks on `wait_for_fences(&[fence], true, u64::MAX)` **before** dropping the guard. VUID-vkQueueSubmit-queue-00893 only requires external synchronization of the queue for the *submit call itself*, not for the subsequent fence wait. Holding the Mutex across the wait serializes any other graphics-queue user for the full GPU execution. The egui overlay `dispatch` path holds the same guard across `set_textures` for the same reason.

## Evidence
```rust
let q = queue.lock().expect("graphics queue lock poisoned");
device.queue_submit(*q, &[submit_info], fence)...;
device.wait_for_fences(&[fence], true, u64::MAX)...;   // guard still held
drop(q);
```
Contrast the main per-frame submit (`draw.rs:3590-3619`), which drops the guard immediately after `queue_submit`.

## Impact
None today. The engine is single-threaded in the draw loop; these helpers run only at load/cell-transition frequency or the debug-only egui path. Latent serialization point that would matter only if a future design submits to the graphics queue from a second thread.

## Related
CONC-D2-NEW-01 (audit 2026-05-16) justified holding the guard across the *submit*; the over-broad coverage of the *wait* is the incremental observation here.

## Suggested Fix
Copy the queue handle out (`let q = *queue.lock()...; drop`) for the submit, then wait on the fence after the guard is released. Defer unless/until a second graphics-queue thread is introduced.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked/fixed in the egui `dispatch` mirror at `draw.rs:3487-3491`
- [ ] **DROP**: Vulkan object lifetimes (fence, command buffer) remain valid after releasing the guard early
- [ ] **TESTS**: A regression test or comment pins the lock-scope-excludes-wait invariant

## Validation
CONFIRMED against current code (HEAD 2d4c350d): texture.rs:659 locks queue, :664 waits on fence under guard.
