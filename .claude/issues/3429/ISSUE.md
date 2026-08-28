# #3429: UI-D6-2026-08-27-04: an animating menu allocates a fresh full-viewport `VkImage` and blocks on a fence every frame, ahead of `draw_frame`

- **Severity**: MEDIUM
- **Dimension**: Render Path & Device Lifecycle
- **Profile**: both
- **Location**: `crates/renderer/src/texture_registry.rs:1596-1645` -> `crates/renderer/src/vulkan/texture.rs:71-99` -> `:114-131` · `byroredux/src/app_frame.rs:316-333`
- **Source**: `docs/audits/AUDIT_UI_2026-08-27.md` (UI-D6-2026-08-27-04)

## Description

`TextureRegistry::update_rgba` does not update an image in place. It builds a **new** `Texture` via `Texture::from_rgba` -> `from_dds_with_mip_chain`, which records the upload into a one-time command buffer, submits and fence-waits once, then releases the staging buffer; the previous image is swapped out and pushed onto a `pending_destroy` ring drained after `MAX_FRAMES_IN_FLIGHT` (2) frames.

## Evidence

```rust
// crates/renderer/src/texture_registry.rs:1620-1631
let new_texture = Texture::from_rgba(ctx, width, height, pixels, ...)?;
if let Some(prev) = entry.texture.replace(new_texture) {
    entry.pending_destroy.push_back((current_frame_id, prev));
}
```

#2719 fixed the *unconditional* dirty flag, so a static menu no longer pays this. But `SwfPlayer::render` returns `Some` whenever the captured pixels differ from the last upload (`player.rs:470-484`), and a real Bethesda HUD (compass, health/AP bars, radiation ticks) differs every single frame. `app_frame.rs:316-333` calls `update_rgba` on every `UiFrame::Fresh`, ahead of `draw_frame`.

## Impact

Per frame, per animating menu: one full-viewport `VkImage` + `VkImageView` create/destroy pair (8.3 MB at 1920x1080, three live copies under the 2-frame destroy ring), a staging upload, and a **blocking `vkWaitForFences` on the main thread ahead of `draw_frame`**. This is allocator churn and a serialisation point in the frame, not a leak — the ring does drain — but it is the single most expensive thing the UI does, and `/audit-performance` has no dimension that would surface it.

## Related

#2719 (fixed the always-dirty half of the same cost), sibling findings UI-D6-2026-08-27-01 and UI-D6-2026-08-27-06.

## Suggested Fix

Give the registry a genuine in-place path for dynamic RGBA entries — persistent image + per-FIF staging ring + `cmd_copy_buffer_to_image` recorded into the frame's own command buffer — instead of recreate-and-defer-destroy. Pairs naturally with UI-D6-2026-08-27-01's move of the overlay out of the geometry pass.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other dynamic-texture callers of `update_rgba`, the bindless descriptor-write path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
