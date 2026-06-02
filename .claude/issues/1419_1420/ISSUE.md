# Issues 1419 + 1420

## #1419 NCPS-02: Volumetrics tlas_written latch never resets per frame
**File:** `crates/renderer/src/vulkan/volumetrics.rs:764`
**Fix:** Reset `self.tlas_written[frame] = false` at start of `dispatch()` after the `debug_assert!`.
The latch is set to `true` by `write_tlas()` but never cleared, so the assert passes even when
`write_tlas()` is not called in subsequent frames.

## #1420 EGUI-01: egui set_textures uses main draw command pool
**File:** `crates/renderer/src/vulkan/context/draw.rs:2973`
**Fix:** Pass `self.transfer_pool` instead of `self.command_pool` to `EguiPass::dispatch`.
All other one-shot uploads use `transfer_pool`; egui was the sole exception.
