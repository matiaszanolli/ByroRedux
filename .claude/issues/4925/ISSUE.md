# EXT-D4-2026-09-27-03: `88c23887b` hand-copied the SkyDome packing into `build_sky_cube_params`' interior arm, with no equality test

**Issue**: #4925
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,renderer,tech-debt

**Severity**: LOW
**Dimension**: Sky, weather, sun
**Tier Violated**: n/a
**Game Affected**: all interiors
**Status**: NEW
**Location**:
`crates/renderer/src/vulkan/context/draw.rs:1033-1150` vs `:883-1023`; `crates/renderer/shaders/sky_cube.comp:36-39` (comment now false)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- It is a ~105-line second packer. The two packers match field for field today.
- `sky_cube_params_mirrors_the_sky_dome_struct` checks layout only.
- This is the drift pattern of the #4733 promotion copy.

## Suggested Fix
Extract one `pack_sky_dome(...)`, or add a packer-equality test with `sky_lower.w` masked. Fix the shader comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
