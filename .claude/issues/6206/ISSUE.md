# #6206 — INC-2026-10-20-05: Doc rot — `GpuTerrainTile`'s tail is still described as stride padding, and the height blend is undocumented in the engine docs

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6206
- **Labels**: low,documentation,doc-rot,terrain-exterior
- **Filed**: 2026-10-20 by /audit-publish

From `docs/audits/AUDIT_INCREMENTAL_2026-10-20.md` (`/audit-suite --preset quick`, HEAD c4df4c91e).

- **Severity**: LOW
- **Dimension**: tech-debt (doc rot) / renderer
- **Location**: `docs/engine/memory-budget.md:179` ("…layer height handles with its std430 stride padding"); `docs/engine/shader-pipeline.md` and `docs/engine/exal.md` (no mention of `height_blend_band` / `byroTerrainHeightBlend` / `render.terrain_height_blend_band`)
- **Changed in**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (commit ea45af067) without the matching doc edits
- **Status**: NEW (same class as closed #5172 / #4337, `GpuTerrainTile` growth left docs stale; the struct size did not change this time)
- **Description**: The 4-byte tail at @204 is now a live per-tile value broadcast every frame. memory-budget.md still calls it padding. The renderer skill asks that "the rustdoc and shader-pipeline.md carry the new meaning" for semantic lane changes, but only the rustdoc does.
- **Suggested Fix**: Update memory-budget.md:179 to name `height_blend_band`. Add a short height-blend paragraph (setting, broadcast, consumers, band 0 = off) to shader-pipeline.md or exal.md next to the splat description.

## Completeness Checks
- [ ] **SIBLING**: Other docs describing `GpuTerrainTile` lanes (`shader-pipeline.md`, `exal.md`, `exal-groundcover.md`) checked for the same staleness
