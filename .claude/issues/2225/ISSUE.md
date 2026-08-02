# REN-D16-01: Volumetric height-fog's reference altitude is the camera's own Y, not a world datum

Severity: high
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2225

**Dimension**: 16 (Volumetrics)
**Location**: `crates/renderer/shaders/volumetrics_inject.comp` (`proceduralDensityScale`, line ~284/441); `crates/renderer/shaders/composite.frag` (`heightFogOpticalDepth`, line ~114/522)
**Status**: NEW

**Description**: Both the froxel injector and the beyond-grid analytic continuation evaluate
the exponential height profile against `params.camera_pos.y`. Fog density
is therefore always maximal at eye level and follows the player vertically
instead of thinning with real altitude — climbing a hill doesn't clear the
fog. Worse, it breaks the temporal-reprojection contract: vertical
camera motion changes density at a fixed world point for reasons
reprojection can't model, producing lag/ghost bands on stairs, elevators,
and the fly camera.

**Evidence**: `volumetrics_inject.comp:441` — `float density_scale = proceduralDensityScale(world_pos, params.camera_pos.y);`. `composite.frag:522` — `heightFogOpticalDepth(..., params.camera_pos.y, ...)`. Both sites pass the camera's own Y as the height-fog reference altitude.

**Impact**: Density follows the player vertically instead of a fixed world datum; visible temporal ghosting/lag on any vertical camera motion (stairs, elevators, fly camera), and fog never thins with real altitude.

**Suggested Fix**: anchor the height-fog reference to a per-cell/world datum (ground height for exteriors, cell floor for interiors) instead of the camera.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
