# #4027 — REN-2026-09-06-D3-03: the terrain-tile shift/mask is the last `GpuInstance.flags` bitfield hand-written shader-side, with no generated `#define` and no lockstep pin

**Labels**: low, renderer, shaders, test-gap, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D3-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs` (`INSTANCE_TERRAIN_TILE_SHIFT`, `INSTANCE_TERRAIN_TILE_MASK`), `crates/renderer/shaders/triangle.frag`, `crates/renderer/src/shader_constants_data.rs`
- **Status**: NEW
- **Description**: Every other packed field in `GpuInstance.flags` reaches GLSL through the generated `include/shader_constants.glsl` header: `INSTANCE_FLAG_NON_UNIFORM_SCALE`/`ALPHA_BLEND`/`CAUSTIC_SOURCE`/`TERRAIN_SPLAT`/`FLAT_SHADING`/`DIFFUSE_ALPHA` plus `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK`. The terrain-tile window does not. The CPU packs it with the named constants (`f |= (tile_idx & INSTANCE_TERRAIN_TILE_MASK) << INSTANCE_TERRAIN_TILE_SHIFT` in `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`), while `triangle.frag` unpacks it with the literals `(inst.flags >> 16) & 0xFFFFu`. No `#define` is emitted and no test pins the two halves equal.
- **Evidence**:
  - `grep -n "TERRAIN" crates/renderer/shaders/include/shader_constants.glsl` returns only `#define INSTANCE_FLAG_TERRAIN_SPLAT 8u` — no shift, no mask.
  - `shader_constants_data.rs`'s own header comment *names* the two constants in prose ("the upper 16 bits pack the terrain-tile slot per `INSTANCE_TERRAIN_TILE_SHIFT/MASK`") while not mirroring them.
  - The identical defect for the render-layer bits was fixed by #2045 / TD7-101, whose comment reads: *"Previously hand-written as `INST_RENDER_LAYER_SHIFT`/`_MASK` directly in `triangle.frag` with no lockstep test, unlike every other `INSTANCE_FLAG_*` bit"*.
- **Impact**: No live drift — the values are 16 and `0xFFFF` on both sides today, and `instance_flag_bits_unique_and_outside_packed_windows` guards the CPU side against collisions. The gap is one-directional: a future widening of the tile window (`MAX_TERRAIN_TILES` is capped at 65535 *by this encoding*) would move the Rust constants and leave `triangle.frag` reading a stale window, indexing `terrainTiles[nonuniformEXT(…)]` with a truncated slot — wrong diffuse/normal/specular layers on every exterior cell, no test failure, no validation error. Same failure mode `gpu_terrain_tile_is_96_bytes`' doc describes for the sibling stride hazard.
- **Related**: #2045 / TD7-101 (the same fix for the render-layer bits); #470 (the encoding).
- **Suggested Fix**: Mirror `INSTANCE_TERRAIN_TILE_SHIFT` / `INSTANCE_TERRAIN_TILE_MASK` into `shader_constants_data.rs`, add them to `build.rs`'s emit and to `generated_header_contains_all_defines`, add an *instance_terrain_tile_bits_match_scene_buffer_consts* pin alongside the existing render-layer one, and replace the two literals in `triangle.frag`.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
