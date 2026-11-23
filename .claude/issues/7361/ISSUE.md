# #7361 — CONC-2026-11-23-02

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-11-23.md` (finding CON-02).

- **Severity**: LOW
- **Dimension**: Compute -> AS -> Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/resources.rs:24-39` (`fill_terrain_tiles` sets `*dirty = false`), `context/build_and_upload_instances.rs:1010-1018` (`upload_terrain_tiles_if_dirty`), `scene_buffer/upload.rs:1103-1190` (`upload_terrain_tiles`, `?` on `staging_pool.acquire`)
- **Status**: NEW as a finding (the flag predates the delta: #496 / #497 / #3664); it matters more now because `terrain_patch.comp` and the ground-cover compute passes read the tile SSBO. No open issue found (searched "terrain tile dirty upload": only closed perf issues #496 / #497 / #3664 / #4790).
- **Description**: `fill_terrain_tiles` returns `true` and clears `terrain_tiles_dirty` at record time. `upload_terrain_tiles` then records the staging copy into `cmd`. If that call fails (`acquire(byte_size)?` or `mapped_slice_mut()?`; the caller only logs `Failed to upload terrain tiles`) or if the frame's command buffer is discarded at one of the three tail `Err` sites, the SSBO keeps its old contents and nothing re-arms the flag: the next frame sees `dirty == false`. The tile table (layer texture indices, height-map index, cell origin) then stays stale until the next cell load or unload mutates a tile. `draw_frame` has no terrain-tile rollback beside `rollback_skin_frame_state`.
- **Evidence**: `if !*dirty { return false; } *dirty = false;` (`resources.rs:29-32`); the call site ignores the copy's result beyond a warn (`build_and_upload_instances.rs:1014-1016`).
- **Trigger Conditions**: a frame that has a pending tile change (cell transition) and fails the staging acquire or loses its command buffer.
- **Impact**: wrong / missing terrain layers on the affected cells, and wrong height-map lookups in the patch displacement and ground-cover placement, persisting until the next tile change. Not a GPU hazard.
- **Verification Path**: `cargo test` (a unit test on `fill_terrain_tiles` + the caller shape); no capture needed.
- **Related**: #3991 / #5956 / #6291 (same class), CON-D2-01.
- **Suggested Fix**: re-arm `terrain_tiles_dirty` on the failure arm of `upload_terrain_tiles_if_dirty`, and on the tail-Err rollback path; or commit the clear in `promote_skip_clear_latches`.
- **Dimension agent ID**: CON-D2-02
- **Also found by**: CON-D1-02 (Dims 1+6), independently.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other record-time latches, other temp stems, other exclusive rows)
- [ ] **TESTS**: A regression test pins this specific fix
