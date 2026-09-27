# EXT-D2-2026-09-27-05: Terrain doc rot

**Issue**: #4918
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,documentation,doc-rot,shaders

**Severity**: LOW
**Dimension**: Terrain, splatting
**Tier Violated**: n/a
**Game Affected**: n/a
**Status**: NEW (sibling of #4337)
**Location**:
`crates/renderer/shaders/include/terrain_sample.glsl:19-22`; `byroredux/src/cell_loader/terrain.rs:57-58,88-92`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- The shader comment still says `GpuTerrainTile` is "24 texture indices and nothing else"; it has been 160 B, carrying affinity rows and more, since #4057.
- `CellSplatLayers` misstates its order: base transitions come first.
- The cap of 8 comes from the 2×RGBA8 packer, not UESP.

## Suggested Fix
Rewrite the three comments.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **SPIR-V**: Edited shaders recompiled and `scripts/check-shader-artifacts.sh` is clean
- [ ] **TESTS**: Doc text matches the code it describes (re-grep the cited symbols)
