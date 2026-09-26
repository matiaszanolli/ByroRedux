# REN-D5-2026-09-26-14: memory-budget.md has no row for the new `dynamic_rgba` staging arenas, and its Scaleform and MenuXml sections describe a retired mechanism

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4892

**Labels**: low,renderer,memory,documentation,doc-rot

- **Severity**: LOW (a resource owner with no ledger row; close to MEDIUM at 4K, where the page's own native total already exceeds its < 4 GB target)
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md` ("Scaleform UI (Ruffle / wgpu)" table and prose, "MenuXml HUD overlay textures", "Not yet ledgered"), `crates/renderer/src/texture_registry/dynamic_rgba.rs` (`DynamicRgbaUploads::staging`), `crates/renderer/src/vulkan/texture.rs` (`Texture::overwrite_rgba_pixels`)
- **Status**: NEW (window commit `e2f99ad55`; the closed #4526 row still exists but the mechanism under it changed; #4872 covers other rows)
- **Description / Evidence**:
  1. Arena. `DynamicRgbaUploads.staging` is `[Option<GpuBuffer>; MAX_FRAMES_IN_FLIGHT]`, `CpuToGpu`, grow-only, sized to the sum of dirty updates per frame. One overlay frame is 8,294,400 B at 1080p (×2 slots = 16.6 MB) and 33,177,600 B at 4K (×2 = 66.4 MB), doubling if both overlays are dirty. It is not a `*_scratch` field, so the #4610 guard cannot see it.
  2. Scaleform section. It says an animating HUD "cycles a fresh full-viewport `VkImage` every frame (#3429)". `update_rgba` now returns `dynamic_rgba.queue(..)` whenever `can_update_rgba(w, h)`, so only an extent or format change recreates the image. `docs/engine/ui.md` was updated in `e2f99ad55`, memory-budget.md was not.
  3. MenuXml section. It still cites the 3-buffer rotation as the hazard contract of `overwrite_rgba_pixels`. `write_rgba_inplace` no longer reaches it, and it has no production caller. Synchronisation is now barriers in the frame command buffer. Two of the three swapchain-extent textures (16.6 MB at 1080p, 66 MB at 4K) are avoidable residency that the ledger still justifies by a contract that is gone.
- **Impact**: The ledger understates resident memory by 16–66 MB (more with both overlays) and misdescribes two sections.
- **Related**: #4526, #4516, #4515 (closed), #4872 (open), #4608.
- **Suggested Fix**: Add a "dynamic RGBA staging arenas + pending pixel copies" row (formula above), rewrite the two sections to the queued-copy model, decide whether the 3-texture rotation collapses to 1, and delete or `#[cfg(test)]`-gate `overwrite_rgba_pixels`. Fix the stale `update_rgba` sentence in `crates/ui/src/player.rs` (`SwfPlayer::render`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
