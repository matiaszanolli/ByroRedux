# REN-EXT-L-01: ground-cover interaction field's "previous half is valid" flag is latched at `prepare`, not when the dispatch is recorded

Labels: low,renderer,vulkan,sync,bug
Source: docs/audits/AUDIT_RENDERER_2026-10-18.md

- **Severity**: LOW
- **Dimension**: Sync/Barriers (state machine) · Memory/Lifecycle
- **Location**: `GroundCoverPipeline::prepare` → `advance_field_state` (`groundcover/frame.rs`); early-out in `GroundCoverPipeline::record_scatter` when `tlas` is `None`; field allocated in `groundcover/construct.rs` (`create_device_local_uninit(… STORAGE_BUFFER | TRANSFER_DST)`); `groundcover_interaction.comp`
- **Status**: NEW
- **Description**: `advance_field_state` flips `field_write_half` and sets `field_previous = Some(origin)` inside `prepare`, which runs before `draw_frame`. The dispatch that actually writes that half is recorded later in `record_scatter`. That function returns before `record_interaction` whenever `ray_query_tlas(frame)` is `None`, i.e. the frame's TLAS build failed or was skipped. The same early-out follows an upload failure inside `prepare`, which advances the state and then returns `false`. So the host's `previous.w = 1.0` ("the previous half holds a field") can be true for a half the GPU never wrote.
  - The field buffer is created uninitialised. `TRANSFER_DST` is requested but nothing ever fills it.
  - `groundcover_interaction.comp` does not sanitise what it reads: a NaN in `value` fails `dot(push, push) > dot(value, value)` and persists, then is copied forward by reprojection until the camera leaves the field extent.
- **Evidence**: `advance_field_state` call at `prepare`; `let Some(tlas) = tlas else { … return; }` before `self.record_interaction(device, cmd, frame)`; no `cmd_fill_buffer` on `field_buffer` anywhere.
- **Impact**: Mid-session a skipped frame only resurrects a stale trail for a frame. The bad case needs the very first live ground-cover frame to be skipped with a never-written half and non-zero uninitialised VRAM (fresh device-local allocations are usually zero-filled by drivers, but the spec does not promise it). Then blades near those texels would be displaced by garbage or NaN until the camera moves about one field extent. Not reproduced; severity reflects low likelihood.
- **Suggested Fix**: advance `field_previous` / `field_write_half` at record time (in `record_interaction`), or zero both halves once at creation with a fenced one-time fill (the buffer already has `TRANSFER_DST`). Neither changes any barrier or layout.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_RENDERER_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
