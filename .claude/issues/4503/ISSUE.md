# EXT-D6-2026-09-19-04: terrain_lod docs claim LOD never enters the TLAS while the renderer admits camera-local shadow casters

- **ID**: EXT-D6-2026-09-19-04
- **Labels**: low,terrain-exterior,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4503

**Severity**: LOW (doc rot; behavior deliberate and coherent) · **Dimension**: Distant LOD · **Game Affected**: all with baked/synth LOD rings
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D6-2026-09-19-04)

**Location**: `byroredux/src/cell_loader/terrain_lod.rs:11-13` (module doc: "builds **no BLAS** … keeps it out of the TLAS") and `:811-817` ("never enter the TLAS") vs `byroredux/src/render/static_meshes.rs:45-48,124-143` and `crates/renderer/src/vulkan/context/resources.rs` (#4180 arm)

**Description**
The render side deliberately admits a camera-local subset of `IsLodTerrain` draws into the TLAS as RT shadow casters (`lod_shadow_caster_in_range`, `LOD_SHADOW_CASTER_DISTANCE` = 24000 BU = `SHADOW_FADE_END + DIRECTIONAL_SHADOW_TRACE_DISTANCE`), with BLAS built on demand from the global geometry buffers and a defensive skip for still-missing BLAS. The spawn-module comments predate that policy and now misstate it — the spawn path indeed builds no BLAS, but "the renderer keeps it out of the TLAS" is no longer true for the near subset.

**Impact**
A future editor "simplifying" either side against the wrong comment could break LOD shadow casting or accidentally TLAS every distant block.

**Related**: `docs/engine/shadow-pipeline-tradeoffs.md` is the accurate reference

**Suggested Fix**
Reword both comments to "no BLAS at spawn; the renderer's bounded restore path may admit camera-local blocks as shadow casters."

## Completeness Checks
- [ ] **TESTS**: N/A (docs); the shadow-caster behaviour is pinned renderer-side
