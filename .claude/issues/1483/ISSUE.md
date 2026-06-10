# #1483 — REN-D23-NEW-02: GPU timer query pools leak on allocator-None Drop path

_Snapshot as filed 2026-06-09 from docs/audits/AUDIT_RENDERER_2026-06-09.md. GitHub is authoritative for live state._

**Severity**: LOW (process-exit-only leak on abnormal teardown)
**Dimension**: Debug Overlay & GPU Telemetry
**Source**: `docs/audits/AUDIT_RENDERER_2026-06-09.md`
**Status**: NEW (interacts with but is not #1426)

## Description
`gpu_timers.destroy()` is nested **inside** the `if let Some(ref alloc) = self.allocator { … }` block in `VulkanContext::Drop` (`crates/renderer/src/vulkan/context/mod.rs:2684` opens the guard; the timer destroy is at `:2742-2744`). The timer query pools need no allocator, but if `self.allocator` is `None` at Drop (the #1426 early-return scenario, or any future allocator-taken-early path), the `Some(timers)` branch is never reached and `MAX_FRAMES_IN_FLIGHT` `VkQueryPool`s are leaked.

## Evidence
- `mod.rs:2684` — `if let Some(ref alloc) = self.allocator {` opens the allocator-dependent destroy block.
- `mod.rs:2742-2744` — `if let Some(ref mut timers) = self.gpu_timers { timers.destroy(&self.device); }` sits inside that block.
- By contrast, `device_wait_idle` and `egui_pass.destroy()` at the top of Drop are correctly **outside** the allocator guard — query pools are allocator-independent and should match that pattern.

## Impact
Process-exit-only `VkQueryPool` leak (driver reclaims on process death) plus a validation-layer "destroyed device with live objects" warning if Drop hits the allocator-`None` branch. Bounded impact, but a real Vulkan-object leak on abnormal teardown.

## Suggested Fix
Move the `if let Some(ref mut timers) = self.gpu_timers { timers.destroy(…) }` block **out** of the `Some(alloc)` guard, alongside `egui_pass.destroy()` at the top of Drop.

## Related
#1426 (VKC-005, open — the allocator-`None` early-return that makes this branch reachable), REN-D7-NEW-01 (sibling abnormal-teardown hazard). Best fixed together as "abnormal-teardown hardening."

## Completeness Checks
- [ ] **DROP**: timer destroy runs on every Drop path including allocator-`None`; verify it stays before `device` destruction.
- [ ] **SIBLING**: scan the rest of the allocator-guarded Drop block for other allocator-independent objects that would leak on the `None` path.
- [ ] **TESTS**: N/A practical (teardown path); covered by validation-layer clean-exit check if one exists.
- [ ] **UNSAFE / LOCK_ORDER / FFI / CANONICAL-BOUNDARY**: N/A.
