# REN-D5-2026-09-26-11: A failed dynamic-RGBA staging step is now fatal to `draw_frame`; before `e2f99ad55` the same failure was a logged, skipped HUD frame

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4889

**Labels**: medium,renderer,memory,ui,bug

- **Severity**: MEDIUM (a transient allocation failure in a non-essential overlay path terminates the process)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/texture_registry/dynamic_rgba.rs` (`TextureRegistry::record_pending_rgba_uploads`: `GpuBuffer::create_host_visible`, mapped write, the two `anyhow::ensure!`), `crates/renderer/src/vulkan/context/begin_frame_recording.rs` (`return Err(e)` after replacing the acquire semaphore), `byroredux/src/app_frame.rs` (`Err(e) => { log::error!("Draw failed: …"); event_loop.exit(); }`), `byroredux/src/hud.rs` (`MenuXmlHud::upload_frame`)
- **Status**: NEW
- **Description / Evidence**:
  - `update_rgba` / `write_rgba_inplace` now only queue pixels. The copy is recorded at the top of the next frame. Growing the per-slot arena (`create_host_visible`) or a mapped-write or flush error returns `Err` from `record_pending_rgba_uploads`. I confirmed that `begin_frame_recording` propagates it and that the `draw_frame` caller ends with `event_loop.exit()`.
  - The HUD and Scaleform drivers were written for the opposite contract: their `Err` arms only log.
  - The failure is handled cleanly (the semaphore is replaced, the command buffer is freed later) but fatally.
- **Impact**: Under host-visible / BAR pressure (the arena needs 8.3 MB at 1080p and 33 MB at 4K per overlay frame), a cosmetic HUD update kills the session instead of dropping one HUD frame.
- **Related**: #4608, #3429.
- **Suggested Fix**: On `Err`, log once, drop or retain the dirty updates for the next frame, and continue recording without the copy. Keep the fence-idle `ensure!` fatal only if it indicates a sequencing bug.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
