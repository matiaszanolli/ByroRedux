# Issue #4903

**Title:** EXT-D2-2026-09-27-02: The canonical base LTEX is never translated into the cover inputs — the density field fabricates 0.15 for unpainted base and ignores the base under partial paint
**State:** OPEN
**Labels:** bug, medium, terrain-exterior, shaders

**Severity**: MEDIUM (visual)
**Dimension**: Terrain, splatting
**Tier Violated**: no-render-time-fallback (and no-fabrication)
**Game Affected**: all LAND games
**Status**: NEW. It shares its root cause (no base lane in `TerrainCoverInputs`) with EXT-D3-02.
**Location**:
- `byroredux/src/cell_loader/terrain.rs:1036-1043`.
- `crates/renderer/shaders/include/groundcover_density.glsl:119-129` (`byroGcAffinity`).
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `coverAffinity0/1`, `layer_affinity` and `authored_grass` describe only the 8 splat lanes. The first-quadrant BTXT base has a real LTEX, but neither its affinity nor its GNAM is emitted.
- With every lane weight at 0, the shader substitutes 0.15. Its comment says the base "has no LTEX record", which is true only when BTXT is 0.
- The shader normalises by painted weight, so 30% dirt over tundra reads as pure dirt, while `triangle.frag`'s ordered `mix` shows 70% tundra.
- The same LTEX gets its true affinity as a non-canonical quadrant base and 0.15 as the canonical one.

## Evidence
- Vertices sitting unpainted on the canonical base: Skyrim 43.8%, FNV 34.5%, FO4 24.8%.
- Partially painted vertices: Skyrim 25.6%, FNV and FO4 about 41%.
- `LSnow01` (true affinity 0.02) gets 0.15, 7.5× too much, across 2.9 M vertices.

## Impact
- Sparse grass grows on snowfields, beaches and rock.
- Grass-textured bases are under-vegetated.
- Density depends on which quadrant was picked as canonical.

## Suggested Fix
- Emit the base as an extra lane, with base affinity and base GNAM.
- Compose affinity in the diffuse loop's order: `a = base; a = mix(a, aff[i], w[i])`.
- Drop both the substitution and the normalisation, and pin the formula with a host-side test.

## Related
EXT-D3-02, EXT-D2-01, EXT-D2-03, #4054

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **SPIR-V**: Edited shaders recompiled and `scripts/check-shader-artifacts.sh` is clean
- [ ] **TESTS**: A regression test pins this specific fix

