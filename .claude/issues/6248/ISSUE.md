# #6248: PERF-D5-01: Every grass-blade fragment recomputes the blade's constant root ground colour with up to ~18 bindless fetches, and #6196 roughly doubled it

**Labels**: medium,performance,shaders,terrain-exterior,bug

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: GPU Pipeline · **Location**: `crates/renderer/shaders/groundcover_blade.frag (ground albedo block)`

- **Location**: `crates/renderer/shaders/groundcover_blade.frag:109-160` (ground albedo block), `crates/renderer/shaders/groundcover_blade.vert:419-433` (every input is per-blade constant: `vTerrainUv` comes from the blade `base`, splats from the base terrain sample, and the tile slot is `flat`), `crates/renderer/shaders/include/bindings.glsl:576-612` (`byroTerrainHeightBlend`: one `textureGrad` per lane with a height map and a share above 0, up to 9).
- **Status**: NEW. The root-colour coupling predates the baseline (#4056/#4907/#5174). The added height-blend fetches came in `ea45af067` (#6196) and `5f1a88929` (#6202). No issue was found for "groundcover blade fragment albedo".
- **Description**: the ground colour under a blade is a function of `vTerrainUv`, `vTerrainSplat0/1` and `vTerrainTileSlot`. All of them are the same for every vertex of a blade, so they are the same for every fragment. The fragment stage nevertheless evaluates, per fragment: one base-diffuse fetch, `byroTerrainHeightBlend` (now up to 9 height fetches plus the share and fade ALU), and `byroTerrainSplatAlbedo` (up to 8 layer-diffuse fetches). Because the UV is constant across a blade, `dFdx/dFdy(vTerrainUv)` are ~0 inside it, so the `textureGrad` calls sample near mip 0. These are full-resolution, cache-unfriendly reads, scattered across blades.
- **Evidence**: code read above. Unmeasured: `groundcover_blade_draw_ms` is the bracket. `TerrainTile.heightBlendBand = 0` (the setting at 0) disables only the #6196 half, which gives a ready A/B.
- **Impact**: GPU time scales with blade fragments × painted lanes in every grass exterior. #6196 added up to 9 fetches per fragment on top of the existing ~9.
- **Related**: #6195, #6196, #6202 (the height-map chain), #5959 (Whiterun main-pass reading).
- **Suggested Fix**: resolve the root albedo once per blade. Either compute it in the vertex shader with `textureLod` at a distance-derived LOD and pass it as a `flat` varying, or compute it in the scatter/layout pass into the blade instance. Measure `groundcover_blade_draw_ms` with the band on and off first (Speculative-Vulkan caveat: confidence that it costs is high, the size is unknown).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
