# #2719: UI overlay allocates a fresh VkImage and does a blocking one-time submit every frame

- **Severity**: MEDIUM
- **Dimension**: 7 — Worker Threads (blocking work on the main loop)
- **Location**: `byroredux/src/main.rs:662-695`, `crates/ui/src/player.rs:199-227`,
  `crates/renderer/src/texture_registry.rs:1518-1567`, `crates/renderer/src/vulkan/texture.rs:114-172`
- **Status**: NEW
- **Description**: `SwfPlayer::tick` ends with an unconditional `self.dirty = true`, so
  `SwfPlayer::render` never takes its `if !self.dirty { return None; }` early exit while the overlay
  is visible — it re-renders and re-reads-back every frame regardless of whether anything on the
  Flash stage changed. `main.rs` then calls `update_rgba` on every such frame, and `update_rgba`
  does not reuse the existing image: it builds a **brand-new** `Texture` via `Texture::from_rgba` →
  `from_dds_with_mip_chain`, which runs `with_one_time_commands` — allocate a command buffer, create
  a `VkFence`, `queue_submit`, `wait_for_fences(u64::MAX)`, destroy the fence, free the command
  buffer — and pushes the previous image onto the deferred-destroy ring. The registry is created at
  `ctx.swapchain_extent()` (`scene.rs:1139-1140`), so at 1920×1080 that is an 8.3 MB readback plus an
  8.3 MB staging copy plus a fresh `VkImage` + `VkImageView` + allocator slab, per frame.
  This is a full CPU↔GPU serialisation point sitting **ahead of** `draw_frame` in the main loop.
  Dimension 1's checklist names exactly this pattern ("One-time command buffers block the main
  thread on a fence — flag if any such blocking submit runs inside the per-frame hot path rather
  than at load time"); the morning dims-1-3 run audited `with_one_time_commands_inner`'s queue-lock
  scoping but never reached this caller.
- **Evidence**: `player.rs:226` — `self.dirty = true;` at the tail of `tick`, unconditional;
  `texture_registry.rs:1542` — `Texture::from_rgba(...)` inside `update_rgba`;
  `texture.rs:141` — `with_one_time_commands(device, queue, command_pool, |cmd| { … })`;
  `texture.rs:811` — `device.wait_for_fences(&[fence], true, u64::MAX)`.
- **Impact**: a per-frame pipeline bubble plus per-frame GPU image churn whenever a menu is up.
  Not a leak — the deferred-destroy ring drains correctly (§3.6) — but it caps overlay-visible frame
  rate at whatever the round-trip costs, and it is what makes CONC-D7-UI-01 fire every frame instead
  of only on genuine content changes.
- **Related**: CONC-D7-UI-01 (same call site); CONC-D7-UI-05.
- **Suggested Fix**: two independent wins. (a) Only mark dirty when Ruffle actually re-rendered, or
  hash/compare the readback, so a static menu stops re-uploading. (b) Give the UI handle a
  persistent per-frame-in-flight image pair and record the copy into `draw_frame`'s own command
  buffer instead of a private submit+fence — that removes the stall and CONC-D7-UI-01 together.

---

---
**Source**: `docs/audits/AUDIT_CONCURRENCY_UI_2026-08-12.md` (finding `CONC-D7-UI-03`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

