# TD7-2026-09-29-01: The 8-lane terrain splat budget is a bare `8` in about 17 places; #4056 added a fourth shader loop after #4496 closed

**Labels**: low,renderer,shaders,terrain-exterior,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 7 · **Status**: NEW · **Effort**: small
- **Location**:
  - GLSL:
    - `crates/renderer/shaders/include/bindings.glsl:469-471`
    - `triangle.frag:396/529/662` (`for (uint i = 0u; i < 8u; ++i)`)
    - `groundcover_blade.frag:155` (the same loop, `c14f5361a`, 2026-09-24)
  - Rust: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:13/16/19` (`[u32; 8]` ×3)
  - CPU packer and budget: `byroredux/src/cell_loader/terrain.rs:378-379` (`[Vec<u32>; 8]`), `:468`
    (`8 - base_transition_count.min(8)`), `:935` (`splat1[i - 4]`)
- **Description**:
  - The value comes from `Vertex.splat_weights_0/1` (2 × `[u8; 4]`).
  - #4496 (CLOSED 09-20) pinned the packer's premise but never named the constant. Each new consumer
    re-types `8` and the `i < 4u ? splat0 : splat1[i-4]` split.
  - The GLSL array length and the Rust `[u32; 8]` are tied only through the struct-size pin.
- **Related**: #4496, #4027
- **Suggested Fix**:
  - Add `TERRAIN_SPLAT_LAYERS = 8` and `TERRAIN_SPLAT_LANES_PER_WORD = 4` to `shader_constants_data.rs`,
    emitted to GLSL.
  - Size the `GpuTerrainTile` arrays from it and replace the loops and budget arithmetic.
  - Pin it against `2 * size_of_val(&Vertex.splat_weights_0)`.

**Validated at HEAD 9fcfdc3fc**: bare `for (uint i = 0u; i < 8u; ++i)` at `triangle.frag:396/529/662` and `groundcover_blade.frag:155`; `uint layer*Index[8]` ×3 in `bindings.glsl` `GpuTerrainTile`; `[u32; 8]` ×3 in `scene_buffer/gpu_types.rs`; `[Vec<u32>; 8]` (`terrain.rs:378-379`), `8 - base_transition_count.min(8)` (:468), `splat1[i - 4]` (:935); no `TERRAIN_SPLAT_LAYERS` constant exists.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
