# EXT-D3-2026-09-27-02: §12.3 ground-colour coupling blends toward the renormalized average of the ATXT overlays only — the BTXT base the terrain shows is ignored

**Issue**: #4907
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: medium,terrain-exterior,bug,shaders

**Severity**: MEDIUM (visual)
**Dimension**: Ground-cover pipeline
**Tier Violated**: no-leak
**Game Affected**: all games with splat terrain
**Status**: NEW (introduced by `c14f5361a`)
**Location**:
- `crates/renderer/shaders/groundcover_blade.frag:145-169`.
- Compare `crates/renderer/shaders/triangle.frag:389-409`.
- `byroredux/src/cell_loader/terrain.rs:149-150`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- The terrain builds its colour as the BTXT base followed by an ordered `mix(prev, layer, w)`.
- The blade uses Σw·layer/Σw over the ATXT layers only. On BTXT dirt with a 0.05 grass overlay, the terrain shows about 95% dirt, but the blade roots take 100% of the overlay colour.
- The response is discontinuous at Σw = 0.
- The comment's premise ("the blade's base has no BTXT") is false: BTXT is the terrain entity's `TextureHandle`, just not carried into `GroundCoverCell`.

## Evidence
Main-context re-read of both loops.

## Impact
Blade roots take the colour of faint overlays, with hard seams along overlay edges.

## Suggested Fix
- Carry the BTXT diffuse index into the cell data.
- Share one GLSL mix-chain helper with `triangle.frag`.
- Pin that the blade's ground colour equals the terrain's.

## Related
#4056, EXT-D2-02 (same missing base lane)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **SPIR-V**: Edited shaders recompiled and `scripts/check-shader-artifacts.sh` is clean
- [ ] **TESTS**: A regression test pins this specific fix
