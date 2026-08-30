# #3664 — PERF-D4-2026-08-30-02: `upload_terrain_tiles` uploads the full 1024-slot slab and blocks on a queue fence inside `draw_frame`, on every frame a terrain slot changes

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D4-2026-08-30-02`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,memory,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3664

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:771-863`
  (`upload_terrain_tiles`), `crates/renderer/src/vulkan/context/draw.rs:3361-3374` (the call,
  inside `draw_frame` after `begin_command_buffer` at `:1872` and before `record_geometry_pass`
  at `:3642`), `crates/renderer/src/vulkan/context/resources.rs:15-27` (`fill_terrain_tiles`),
  `crates/renderer/src/vulkan/context/init.rs:1471`
  (`terrain_tiles: vec![None; scene_buffer::MAX_TERRAIN_TILES]`),
  `crates/renderer/src/vulkan/texture.rs:826` (`wait_for_fences(&[fence], true, u64::MAX)`)
- **Status**: NEW (sibling of **CLOSED #2719**, the same defect class fixed for the UI overlay)
- **Description**: This is the one upload in the whole scene-buffer path whose byte count is
  `O(MAX_*)` rather than `O(live)`. `terrain_tiles` is a fixed 1024-entry `Vec<Option<_>>`;
  `fill_terrain_tiles` does `dest.extend(tiles.iter().map(|t| t.unwrap_or_default()))`, so the
  scratch is always exactly 1024 × 96 B = 98 304 B no matter how many slots are occupied. One
  tile is allocated per terrain quad (`byroredux/src/cell_loader/terrain.rs:639`), so a
  radius-3 exterior grid holds on the order of 49 live tiles — roughly 95 % of every upload is
  `GpuTerrainTile::default()` padding.

  The bigger cost is the delivery mechanism. Each dirty upload creates a fresh 98 KB staging
  buffer (`create_staging_buffer` → create + allocate + bind + map), memcpys into it, then calls
  `with_one_time_commands`, which submits to `graphics_queue` and does a blocking
  `vkWaitForFences(…, u64::MAX)` — draining everything previously submitted on that queue —
  before destroying the staging buffer and its fence. All of this runs on the render thread in
  the middle of `draw_frame`'s command recording. `terrain_tiles_dirty` is set by **every**
  `allocate_terrain_tile` and `free_terrain_tile` call (`resources.rs:65`, `:49`), so the upload
  fires on every frame in which any terrain slot was touched — i.e. a run of consecutive frames
  during exterior grid streaming and cell unload, not the "few times a minute" the function's own
  comment assumes.
- **Evidence**:
  ```rust
  // context/resources.rs:15-27 — always 1024 entries, live count irrelevant
  dest.clear();
  dest.extend(tiles.iter().map(|t| t.unwrap_or_default()));
  ```
  ```rust
  // upload.rs:805-847 — transient staging per upload, then a blocking submit
  let (staging_buffer, staging_alloc) =
      super::super::buffer::create_staging_buffer(device, allocator, byte_size, "terrain_tile_staging")?;
  …
  let result = super::super::texture::with_one_time_commands(device, queue, command_pool, |cmd| { … });
  ```
  ```rust
  // upload.rs:792-796 — the comment the frequency claim rests on
  // "Terrain tile uploads run at cell-transition frequency (a few times
  //  a minute at most), so skip the StagingPool reuse overhead"
  ```
- **Impact**: A full graphics-queue drain plus a buffer create/allocate/bind/map/destroy cycle,
  on the render thread, on each frame of an exterior cell crossing — precisely the frames whose
  frame time already spikes from streaming. This is the same failure mode as CLOSED #2719 ("UI
  overlay allocates a fresh VkImage and does a blocking one-time submit every frame"), which the
  project treated as worth fixing at a comparable payload size.
- **Related**: #2719 (closed, same class), #497 / #470 (the design this function implements),
  #2463 (the `GpuTerrainTile` 96 B pin), `PERF-D3-2026-08-30-03` (the 32 B doc row).
- **Suggested Fix**: Track a live high-water slot index and upload only `[0 ..= high_water]`;
  take the staging buffer from the existing `StagingPool` rather than creating one per call;
  and record the copy into the frame's own command buffer with a TRANSFER→SHADER_READ barrier
  (as `record_bone_world_copy` already does) instead of a synchronous one-time submit.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
