# #7389 — REN-D2-2026-11-23-04: Two stale comments in the changed shader code

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D2-04). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (doc rot, no behaviour)
- **Dimension**: Ray Queries
- **Location**: `include/lighting.glsl` (the long "Radiance seen by a bounded path that escapes the TLAS ... #2472 ... #3620" comment now sits above `rotateBySkyApertureQuaternion`, not above `pathEnvironmentRadiance`); `include/material_sampling.glsl` header of `terrainParallaxDisplaceUV` ("Geometry is NOT displaced. Ray-traced shadows, reflections and GI hits keep the flat terrain ... a known, accepted mismatch")
- **Status**: NEW
- **Description**: (a) 84104564e inserted `rotateBySkyApertureQuaternion` and `pathCrossesSkyAperture` between the doc comment and its function, orphaning the units/#2472 explanation (and `pathEnvironmentRadiance` keeps only the #7268 note). (b) With TSD-4.1/4.2/4.3 the terrain *is* displaced on patch draws and the patches are in the TLAS (`blas_terrain_patch.rs`), so RT hits do see the relief within the patch range; the paragraph is true only for the base terrain beyond the patches.
- **Impact**: misleads the next reader of the GI-escape units and of the parallax/RT relationship.
- **Related**: `docs/engine/terrain-displacement.md` §7.2.
- **Suggested Fix**: move the comment back above `pathEnvironmentRadiance`; reword the parallax header to "base terrain; patch draws carry `geometryRelief` in geometry".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
