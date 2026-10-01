# 4911: EXT-D5-2026-09-27-03: A REFR XWCU current replaces the `WaterFlow` but not the scroll composed from the WATR flow — pattern and physics current diverge

labels: bug, medium, water, terrain-exterior, physics
state: OPEN

**Severity**: MEDIUM (visual)
**Dimension**: Water translation (WATAL)
**Tier Violated**: single-boundary
**Game Affected**: Skyrim placed mesh water via WNAM; any game with REFR XWCU
**Status**: NEW
**Location**:
- `byroredux/src/cell_loader/water.rs:684-715` (`merge_placed_water`: `reference_flow.or(watr_flow)`; material untouched).
- `byroredux/src/env_translate.rs:936-963`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `scroll_a`/`scroll_b` are composed from the WATR flow direction.
- When XWCU swaps the flow, physics, foam streaks and wave B follow XWCU, while the normal-map flow term follows the WATR.
- The code comment "in vanilla the two are equal" is false.

## Evidence
Across 128 Skyrim.esm REFRs with XWCU, one `RiverWaterFlowSE` REFR is 78.7° off its WATR and one `CreekWaterFlow` REFR is 52° off. The rest agree within 3.8°.

## Impact
On those placements, ripples run up to about 79° across the current carrying floating bodies.

## Suggested Fix
- Extract a translate helper that composes scroll from a given flow, and call it from `merge_placed_water` when XWCU wins.
- Pin it with differing axes.

## Related
#3974, #4728

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix

