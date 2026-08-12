# #2722: SwfPlayer::render clears `dirty` and returns the buffer even when no frame was captured

- **Severity**: LOW
- **Dimension**: 3 (error handling on the readback path)
- **Location**: [`crates/ui/src/player.rs`](../../crates/ui/src/player.rs):247-283
- **Status**: NEW
- **Description**: Three failure paths — the `downcast_mut` returning `None`,
  `capture_frame()` returning `None`, and the `rgba.len() != pixel_buffer.len()`
  mismatch — all fall through to `self.dirty = false; Some(&self.pixel_buffer)`.
  The caller in `byroredux/src/main.rs`:675 treats a `Some` as a fresh frame and
  uploads it, so a failed capture is published as a real frame (stale content,
  or all-zero on the first frame) and, because `dirty` was cleared, is never
  retried until the next `tick`.
- **Evidence**: the early-return-free structure at
  [`crates/ui/src/player.rs`](../../crates/ui/src/player.rs):263-282 — the `if let
  Some(image)` has no `else`, and `self.dirty = false` sits outside every branch.
- **Impact**: **LOW because all three paths are currently unreachable**, and I
  confirmed each: the renderer is always the concrete
  `WgpuRenderBackend<TextureTarget>` by construction; *TextureTarget::new*
  always populates `buffer: Some(..)`, so *capture_frame* never returns `None`
  for this target type; and `pixel_buffer` and the target share one immutable
  `(width, height)` fixed at construction, so the length can never mismatch.
  The finding is that the code takes three branches whose stated purpose is to
  handle failure and then behaves identically to success.
- **Suggested Fix**: Return `None` (and leave `dirty` set) on any of the three
  paths, so a real failure re-tries rather than publishing a stale frame.

---
**Source**: `docs/audits/AUDIT_SAFETY_UI_2026-08-12.md` (finding `SAFEUI-05`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

