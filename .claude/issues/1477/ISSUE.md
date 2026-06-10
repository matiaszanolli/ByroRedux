# #1477 — REN-D7-NEW-01: App field order drops VulkanContext before World on panic-unwind

_Snapshot as filed 2026-06-09 from docs/audits/AUDIT_RENDERER_2026-06-09.md. GitHub is authoritative for live state._

**Severity**: HIGH (recalibrated from the audit's CRITICAL — the `Arc::try_unwrap` guard prevents a live UAF; see Impact)
**Dimension**: Resource Lifecycle
**Source**: `docs/audits/AUDIT_RENDERER_2026-06-09.md`
**Status**: NEW

## Description
`App` declares `renderer: Option<VulkanContext>` (`byroredux/src/main.rs:295`) **before** `world: World` (`:296`). Rust drops struct fields in declaration order, so any teardown that does **not** go through the `WindowEvent::CloseRequested` arm — a panic unwind anywhere inside `event_loop.run_app(&mut app)` (many `expect()`/`unwrap()` sites are reachable from `draw_frame`/resize), or any future early return — drops `renderer` (firing `VulkanContext::Drop`) while `world` still holds the `AllocatorResource` `Arc` clone.

The #1406 fix (`299e6a84`) is procedural — it lives only in the `CloseRequested` handler (`main.rs:1890-1892`: `world.remove_resource::<AllocatorResource>()` then `renderer.take()`). It is not structural, so the unwind path bypasses it.

## Evidence
- `struct App { window, renderer: Option<VulkanContext> /*:295*/, world: World /*:296*/, … }` — renderer before world.
- No `impl Drop for App`, no `catch_unwind`, no panic hook (verified).
- On unwind, `VulkanContext::Drop` reaches `Arc::try_unwrap(alloc_arc)` at `crates/renderer/src/vulkan/context/mod.rs:2846`, which returns `Err` (clone still held by `world`) and takes the leak branch (`:2849-2882`): logs an error, `debug_assert!(false, …)`, returns without destroying device/instance/allocator.

## Impact
The `Arc::try_unwrap` guard **prevents a true use-after-free** (hence HIGH, not CRITICAL): in release it leaks device/instance/allocator handles (OS-reclaimed at process exit) and the subsequent `AllocatorResource` drop runs `vkFreeMemory` on the still-valid leaked device — no UAF. The real harm:
1. **Debug builds**: `debug_assert!(false)` fires *during* the unwind → panic-during-unwind → `process::abort()`, masking the original panic and crippling debuggability of every render-loop panic.
2. The trigger #1406 classified CRITICAL is re-armed and would become a live UAF the moment the `try_unwrap` guard is weakened.

## Suggested Fix
Add an `impl Drop for App` that does `self.world.remove_resource::<AllocatorResource>(); self.renderer.take();` in that order, so every exit path (normal, panic, early return) gets the #1406 ordering for free, independent of the `CloseRequested` arm. (Field reorder alone is fragile — a `Drop` impl is the robust fix.)

## Related
#1406 (only patched the normal exit), #1426 (VKC-005, open — allocator Arc leak / missing `wait_idle`), REN-D23-NEW-02 (sibling abnormal-teardown leak). Consider one combined "abnormal-teardown hardening" change.

## Completeness Checks
- [ ] **DROP**: `App::drop` removes `AllocatorResource` before `renderer.take()`; `VulkanContext::Drop` ordering still correct.
- [ ] **SIBLING**: audit any other ECS resource holding an `Arc<Device>` clone (or other Vulkan-owned handle) for the same world-outlives-renderer hazard.
- [ ] **TESTS**: add a teardown-ordering test (or a panic-unwind harness) asserting `AllocatorResource` is gone before `VulkanContext::Drop`.
- [ ] **UNSAFE**: N/A (no new unsafe).
- [ ] **FFI / LOCK_ORDER / CANONICAL-BOUNDARY**: N/A.
