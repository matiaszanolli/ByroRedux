# #7379 — REN-D4-2026-11-23-02: the new frame order (tile upload → patch compute → patch BLAS → TLAS) is not position-pinned, and the tile-upload settle guard is not function-scoped

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D4-02). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (test-gap)
- **Dimension**: Sync/Barriers
- **Location**: `draw_frame` (calls `upload_terrain_tiles_if_dirty`, `record_terrain_patches`, `dispatch_skin_and_cluster`); `record_terrain_patches` → `TerrainPatchPass::record` → `record_terrain_patch_blas`; test `the_tile_upload_is_settled_by_the_skip_clear_latch_hooks` (`context/resources.rs`).
- **Status**: NEW (the order text is documented in shader-pipeline.md rows 1e/1f/1g, #7244 closed; #7361/#7360 closed and unaffected)
- **Description**: (1) No test anchors `self.upload_terrain_tiles_if_dirty(` < `self.record_terrain_patches(` < `self.dispatch_skin_and_cluster(` inside `draw_frame_body()`, nor `pass.record(` < `record_terrain_patch_blas` inside `record_terrain_patches`. `every_frame_recorder_is_documented` only checks that each `record_*` name appears somewhere in shader-pipeline.md. The patch pass's entry barrier (`TRANSFER_WRITE -> SHADER_READ|WRITE`) and `upload_terrain_tiles`' own TRANSFER -> FRAGMENT|COMPUTE barrier order a tile copy only if the copy is recorded *before* the compute; the patch BLAS builds are valid only if recorded after the compute's exit barrier. A swap is invisible to `cargo test`. (2) `the_tile_upload_is_settled_by_the_skip_clear_latch_hooks` asserts `post.contains("discard_terrain_tile_upload(") && post.contains("commit_terrain_tile_upload(")` over all of `post_passes.rs`; its siblings (`the_linear_capture_copy_*`) split the two hook bodies. An additional record-time `commit_terrain_tile_upload` call (which would make `discard` never re-arm) leaves this guard green as long as the hook calls stay.
- **Evidence**: grep for `record_terrain_patches` / `upload_terrain_tiles_if_dirty` across `crates/renderer/src` shows production calls only in `draw.rs`/`dispatch_skin_and_cluster.rs`, doc text, and the one `contains` test above.
- **Impact**: no live defect; both are silent-regression classes for the exact area three fixes (#7360, #7361, #7364) touched today.
- **Related**: #7361, #7360, #7221 (open), Needs-RenderDoc N1.
- **Suggested Fix**: add one `draw_frame_body()` position test for the four calls and one `record_terrain_patches` position test; scope the tile guard to the two hook bodies the way the linear-capture guard does and assert `note_terrain_tile_upload(` is the only record-side touch of `terrain_tiles_upload_staged`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
