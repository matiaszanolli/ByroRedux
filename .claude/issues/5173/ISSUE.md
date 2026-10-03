# #5173: EXT-D2-2026-10-02-02: #5113 says "end to end", but the cover-affinity lane chain still hard-codes 8 lanes

**Labels**: low,terrain-exterior,shaders,tech-debt,bug
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW (latent maintainability)
- **Dimension**: Terrain, splatting
- **Location**: The chain is created by #4903, before #5113 landed the same day.
  - `byroredux/src/cell_loader/terrain.rs:1089` (`[DEFAULT_AFFINITY; 8]`), `:1117-1119` (`[0u32; 8]` ×3).
  - `byroredux/src/components.rs:474,508`.
  - `byroredux/src/render/groundcover.rs:545`, `:401-412`.
  - `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:39-40` and `vulkan/groundcover.rs:94-95` (`[f32; 4]` ×2).
  - `crates/renderer/shaders/include/groundcover_density.glsl:122-138`: eight unrolled `mix` calls.
  - `shader_contract_tests.rs:7087-7089`: asserts `count() == 8`.
- **Status**: Incomplete fix of #5113 (CLOSED)
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description**:
  - Most of these sites fail to compile when `TERRAIN_SPLAT_LAYERS` changes (the index arrays and `authored_grass`).
  - `layer_affinity` does not: `[f32; 8]` is filled by `zip` over `layers`, which would silently drop lanes 8 and up.
  - The GLSL affinity mix and its guard both hard-code 8. A `Vertex` lane bump would leave density running on 8 lanes while the colour chain runs on `TERRAIN_SPLAT_LAYERS`.
- **Suggested Fix**:
  - Size `layer_affinity` / `EntityCell` from `TERRAIN_SPLAT_LAYERS`.
  - Write `byroGcAffinity` as the same `for (i < TERRAIN_SPLAT_LAYERS)` lane loop that `byroTerrainSplatAlbedo` uses (affinities indexed the same way as the weights).
  - Make the guard assert the loop shape rather than a count of 8.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
