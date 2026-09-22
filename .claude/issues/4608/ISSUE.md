# PERF-D1-2026-09-21-03: The MenuXml HUD re-rasterizes, copies and synchronously uploads the whole swapchain-sized frame on every changed tick (≤ 30 Hz while the camera turns); its budget was set at 720p

**Labels**: bug, renderer, medium, performance, game:fnv, game:fo3, game:oblivion, ui

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: MEDIUM (a periodic main-thread spike on the playable HUD path; opt-in `--hud`) · **Dimension**: 1 — CPU Hot Paths
**Location**:
- `byroredux/src/app_frame.rs:873-893`: `tick_hud_overlay`, with `.map(<[u8]>::to_vec)` at `:886`
- `byroredux/src/hud.rs:423`: the overlay and its three textures are sized to `ctx.swapchain_extent()`
- `byroredux/src/hud.rs:553-627` (`MenuXmlHud::render`) and `:642-667` (`upload_frame`)
- `crates/menuxml/src/menu.rs:342-356`: `render_frame` runs a full `EvalState::resolve_all` and a full `frame.clear` on every call
- `crates/renderer/src/texture_registry/mod.rs:899` (`write_rgba_inplace`) → `crates/renderer/src/vulkan/texture.rs:137-232` (`Texture::overwrite_rgba_pixels`) → `with_one_time_commands` (`texture.rs:808`, body `:839-990`)

**Status**: NEW. A sibling of open #3429, but a different route. Introduced by `ffda4ea95` / `dc306a6a0` (2026-09-18).
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

Any camera yaw changes the HUD signature: heading is quantised to 0.1° (`hud.rs:565-573`). The rate limit caps refreshes at one per 33 ms (`HUD_REFRESH_INTERVAL`). Each changed tick does this on the main thread, before `draw_frame`:

1. menu evaluation and layout, then a full clear and raster at swapchain resolution (`render_frame`);
2. `.to_vec()` of the whole RGBA frame into a fresh allocation (`app_frame.rs:886`);
3. a staging memcpy of the same size (`overwrite_rgba_pixels`);
4. a one-time command buffer plus fence create → submit → wait → destroy. This is `with_one_time_commands`, not the `with_one_time_commands_reuse_fence` variant.

The component's own docs size this policy at 720p:
- the raster costs "~16 ms" in the debug profile and "pinned a machine" before the rate limit existed (`hud.rs:297-303`);
- "30 Hz keeps the compass visually continuous at a bounded ~3.5 MB/33 ms transfer budget";
- the `.to_vec()` comment (`app_frame.rs:883-885`) prices the copy at "one 3.5 MB memcpy per *changed* frame" and calls it noise.

The buffer actually tracks the output extent: 8.3 MB at 1080p and 14.7 MB at 1440p, ×2.25 and ×4 the documented figure.

The blocking fence costs less than it looks, because `draw_frame` already waits on every in-flight fence (#4606). It is still redundant. The 3-texture rotation makes the in-place write race-free, so the copy could be recorded into the frame's own command buffer.

## Evidence

- The code sites above, read at the symbols.
- **Confirmed at publish time**, from the safety leg's "note, not a finding" (`docs/audits/AUDIT_SAFETY_2026-09-21.md`, Dimension 3). `overwrite_rgba_pixels` takes its staging buffer from the registry's `StagingPool` (`pool.acquire(image_size)`) but never calls `StagingGuard::release_to`. `StagingGuard::drop` → `cleanup()` therefore destroys the `VkBuffer` and frees the allocation (`crates/renderer/src/vulkan/buffer.rs:637-658`). The closing comment at `texture.rs:229-230` says the buffer "can go back to the pool now (StagingGuard::drop)"; it does not. Once any pooled buffer large enough has been used up, every HUD refresh also pays a fresh swapchain-sized host-visible staging create + allocate + destroy + free.

## Impact

While the player turns with `--hud` on, every second frame at 60 fps pays:
- the raster;
- about three full-frame memory passes (the `Vec` copy, the staging memcpy, the transfer);
- the staging allocation churn.

All of it scales with output resolution. The release-profile magnitude has not been measured. Confidence is high on the mechanism and low on the release-build magnitude. The route serves the MenuXml titles: Oblivion, FO3 and FNV (`HudGameProfile`).

## Related

- #3429 (open, MEDIUM) covers the FO4/Skyrim Scaleform `--hud` path through `TextureRegistry::update_rgba` (recreate plus deferred destroy). Its scope is `update_rgba`. The MenuXml driver moved off that path to `overwrite_rgba_pixels` (see #3429's latest comment, from `AUDIT_CONCURRENCY_2026-09-21.md`), so this cost is not covered there. One shared in-frame upload design would fix both.
- #4606 (PERF-D5-2026-09-21-01): the all-slots fence wait that makes this upload's own fence partly redundant.
- #4601 (CONC-D1-2026-09-21-01): the rotation's safety rests on that all-slots wait, which #4601 asks to pin.
- #4515, #4516 and #4526 (closed) covered this rotation's hazard contract, frames-in-flight tripwire and memory ledger. None of them covers the per-refresh cost.
- #4593 (SAFE-D2-2026-09-21-01): the staging-pool `release_to` capacity rule that a `release_to` fix here must follow (pass the *requested* size, per #4512).

## Suggested Fix

- Raster only the damaged rects (bars, compass). Alternatively, raster at a fixed HUD-native resolution and let the overlay quad scale it.
- Remove the `.to_vec()` by split-borrowing the renderer's frame, or by swapping an owned buffer.
- Upload only the damaged rects, recorded into the frame command buffer. This pairs with #3429's in-frame upload restructure.
- Independently of the above, return the staging buffer to the pool in `overwrite_rgba_pixels` (`staging.release_to(pool, image_size)`) and fix its "goes back to the pool" comment.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D1-2026-09-21-03)

## Completeness Checks
- [ ] **UNSAFE**: If the copy moves into the frame command buffer or `unsafe` changes, the SAFETY comment states the upheld invariant (rotation vs frames in flight)
- [ ] **SIBLING**: The Scaleform `update_rgba` path (#3429) and the other `overwrite_rgba_pixels` / `with_one_time_commands` callers are checked for the same per-refresh pattern
- [ ] **DROP**: If Vulkan objects change (a persistent staging ring), the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins the fix, for example that the staging buffer returns to the pool after `overwrite_rgba_pixels`, or that there is no per-refresh `Vec` copy

