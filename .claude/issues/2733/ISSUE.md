# #2733: Each SwfPlayer creates its own wgpu instance, adapter and device under block_on

- **Severity**: LOW
- **Dimension**: 7 — Worker Threads (blocking work on the main loop)
- **Location**: `crates/ui/src/player.rs:136-155`, `crates/ui/src/lib.rs:101-113`
- **Status**: NEW
- **Description**: `SwfPlayer::from_movie` calls `create_wgpu_instance(wgpu::Backends::VULKAN, …)`
  and `futures::executor::block_on(request_adapter_and_device(…))` per player, producing a **second
  live Vulkan device** alongside the engine's `VulkanContext`. Device creation is synchronous on the
  winit main-loop thread. `UiManager::install_player` assigns `self.player = Some(player)` only after
  the new player is fully built, so a menu swap transiently holds two Ruffle devices plus the
  engine's.
- **Impact**: a visible hitch on menu load, plus steady-state driver/VRAM overhead for a duplicate
  logical device. Bounded and released on `UiManager::close()` — not a leak.
- **Related**: CONC-D7-UI-03.
- **Suggested Fix**: hoist the `Descriptors` bundle to a lazily-created `UiManager`-owned singleton
  shared by successive players, so device creation happens once per process rather than once per menu.

---

---
**Source**: `docs/audits/AUDIT_CONCURRENCY_UI_2026-08-12.md` (finding `CONC-D7-UI-05`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

