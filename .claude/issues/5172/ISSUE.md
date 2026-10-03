# #5172: EXT-D2-2026-10-02-01: `GpuTerrainTile` grew from 160 to 176 B (#4903), but six docs and comments still say 160 B and one cites a test that no longer exists

**Labels**: low,terrain-exterior,documentation,doc-rot
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW (doc rot)
- **Dimension**: Terrain, splatting
- **Location**:
  - `crates/renderer/shaders/include/terrain_sample.glsl:20`. #4918 rewrote this exact comment to "160 B" on 09-30, and #4903 made it stale the next day.
  - `docs/engine/memory-budget.md:161`: row says "160 B … pinned by `gpu_terrain_tile_is_160_bytes`", "~160 KB". The test is now `gpu_terrain_tile_is_176_bytes`; the real size is 176 B and ~176 KB, and the row omits `base_cover_affinity` and `base_diffuse_index`.
  - `crates/renderer/src/vulkan/scene_buffer/constants.rs:260` ("1024 × 160 B = 160 KB").
  - `crates/renderer/src/vulkan/scene_buffer/buffers.rs:552`.
  - `crates/renderer/src/vulkan/context/shrink_frame_scratch.rs:112` (rationale string).
  - Skills: `.claude/commands/audit-exterior/SKILL.md:42`, `.claude/commands/audit-safety/SKILL.md:188`.
- **Status**: Regression of #4918 at the `terrain_sample.glsl` site; the other sites are NEW.
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description**: The layout pin `gpu_terrain_tile_is_176_bytes` is correct. No guard scans the memory-budget terrain row, which is how it drifted.
- **Suggested Fix**:
  - Update each site to 176 B and name the live test.
  - Optionally extend the `memory_budget_ledgers…` guard to cover the terrain-tile row (`size_of::<GpuTerrainTile>()`).

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it
