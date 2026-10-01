# Issue #4905

**Title:** EXT-D2-2026-09-27-04: The BTXT feather covers only the quadrant edges inside a cell; the same base disagreement across a cell edge stays a hard cut
**State:** OPEN
**Labels:** bug, medium, terrain-exterior

**Severity**: MEDIUM (visual)
**Dimension**: Terrain, splatting
**Tier Violated**: n/a
**Game Affected**: all LAND games
**Status**: NEW
**Location**:
`byroredux/src/cell_loader/terrain.rs:216-240` (`base_transition_alpha`); the doc claim is at `:151-158`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- Internal quadrant edges whose bases differ get a 0.5 weight on the shared vertex.
- The cell's outer edges get nothing, so neighbouring cells with different facing bases meet along a hard line: the seam the comment says is removed.

## Evidence
FNV census.
- Internal quadrant pairs with different bases: 4655/15302 (30%).
- Cross-cell facing pairs with different bases: 4829/15010 (32%).
- On those cross-cell pairs, 32,985 of 82,093 edge vertices are under 0.99 ATXT coverage on both sides.

## Impact
Hard base-texture lines along cell boundaries. This matches vanilla, so it is an inconsistency in the project's own improvement rather than a regression.

## Suggested Fix
Choose one:
- Feed each neighbour's facing quadrant bases into `base_transition_layers_for_bases`. This needs a lane-budget change, because the cap's at-most-4-transitions premise moves.
- Document that cell edges are kept at vanilla parity.

## Related
#470

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix

