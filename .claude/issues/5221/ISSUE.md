# #5221 — REN-D12-2026-10-03-02: `gpu_timers.rs` module header has two inaccuracies that the #4981 refresh missed

**Labels**: low,renderer,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs`, the module doc: the bracket-history paragraph and the "When the driver lacks timestamp support" section.
- **Status**: NEW. Neither item was in #4981's (a)–(e) list, which f4f280c9a closed in full.
- **Description**:
  - (a) "The original four brackets (skin dispatch / skin palette / BLAS refit / TAA) shipped with the #1194 perf-bisect work." The skin-palette bracket was added by #3676 (`6a605e7fc`, slot 28 / `BIT_SKIN_PALETTE = 0x4000`). That commit rewrote the sentence from "The original three brackets (skin / BLAS refit / TAA)" and moved its own bracket into the #1194 history.
  - (b) The no-timestamp section says "`DeviceCapabilities::timestamp_supported == false` skips creation entirely; `last_snapshot()` returns zeroed values." The real gate is `DeviceCapabilities::gpu_timers_supported()`, which is `timestamp_supported && timestamp_valid_bits > 0 && host_query_reset_supported` (#1478). `GpuPerFrameTimers::new` then returns `Ok(None)`. No timer object exists, so `last_snapshot()` cannot be called. The zeroing happens in `fill_skin_coverage_stats`'s `else` arm (`telemetry.rs`), which also clears every `_active` flag.
- **Evidence**: `git show 6a605e7fc -- crates/renderer/src/vulkan/gpu_timers.rs` shows `-//! The original three brackets (skin / BLAS refit / TAA)` replaced by `+//! The original four brackets (skin dispatch / skin palette / BLAS refit / TAA)`. `fn gpu_timers_supported` is in `crates/renderer/src/vulkan/device.rs`. `GpuPerFrameTimers::new` opens with `if !caps.gpu_timers_supported() { return Ok(None); }`.
- **Impact**: Doc only. (b) is the more useful fix. A timestamp-capable GPU without `hostQueryReset` gets no timers, and the header points a reader at the wrong predicate. That is the same misreading #1478 fixed in code.
- **Related**: #4981, #4811 (CLOSED, verified below); #1478.
- **Suggested Fix**: Restore "three original brackets (skin dispatch / BLAS refit / TAA)" and credit skin palette to #3676. In the no-timestamp section, name `gpu_timers_supported()` and say that consumers zero the stats and clear every `_active` flag.


#### Existing #5172: addendum (sites the issue does not list)
#5172 (OPEN, `GpuTerrainTile` 160 → 176 B doc drift) does not list these sites:
- The SAFETY comment in the `SceneBuffers` terrain-tile upload (`crates/renderer/src/vulkan/scene_buffer/upload.rs`) cites the dead `gpu_terrain_tile_is_160_bytes`; the live test is `gpu_terrain_tile_is_176_bytes`.
- `docs/engine/shader-pipeline.md` Scene Buffer Capacity table: "`MAX_TERRAIN_TILES` | 1 024 | 160 B each".
- `docs/engine/exal-groundcover.md`, two passages ("brought it to its current 160 B …", "is now 160 B with the Tier-3 …").
- `crates/renderer/shaders/include/terrain_sample.glsl`: "`GpuTerrainTile` (160 B)". This is routed by Dim 2 and owned by `/audit-exterior`.

Fold these into #5172 rather than filing them separately.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
