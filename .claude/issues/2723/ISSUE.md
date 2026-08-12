# #2723: UI overlay viewport is pinned to the setup-time swapchain extent and never follows a resize; UiManager::close is dead

- **Severity**: LOW
- **Dimension**: 10 (overlay lifecycle)
- **Location**: [`byroredux/src/scene.rs`](../../byroredux/src/scene.rs):1139-1162 · [`crates/ui/src/lib.rs`](../../crates/ui/src/lib.rs):51-59, 211-216
- **Status**: NEW
- **Description**: `UiManager::new(w, h)` captures `ctx.swapchain_extent()`
  once, and `SwfPlayer::from_movie` fixes both the Ruffle viewport and the
  offscreen `TextureTarget` at that size. Nothing in `byroredux/src/main.rs`
  updates `ui_manager.width/height`, re-registers the UI texture, or resizes the
  Ruffle target when the swapchain is recreated. Separately, `UiManager::close`
  is dead code — nothing calls it, and even if it did, `App::ui_texture_handle`
  would keep the RGBA texture registered.
- **Impact**: Visual only. Explicitly **not** a Vulkan hazard: `update_rgba` is
  called with `ui.width`/`ui.height`, which are the same values the texture was
  registered with and the same values `pixel_buffer` was sized from, so the
  `assert_eq!(pixels.len(), width * height * 4)` in `Texture::from_rgba`
  ([`crates/renderer/src/vulkan/texture.rs`](../../crates/renderer/src/vulkan/texture.rs):79)
  cannot fire and no staging copy can over-read. The overlay simply stretches.
- **Suggested Fix**: On swapchain recreate, drive
  Ruffle's *set_viewport_dimensions* and re-register the UI texture, or document
  the fixed-extent behaviour. Give `close()` a caller or delete it.

---
**Source**: `docs/audits/AUDIT_SAFETY_UI_2026-08-12.md` (finding `SAFEUI-07`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

