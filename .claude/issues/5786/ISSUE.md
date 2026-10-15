# OBL-D4-2026-10-14-02: #5505's LOD half is already consumed since #5496; no test composes the Oblivion parse stamp with `lod_source_worldspace`

**Labels**: low,game:oblivion,legacy-compat,terrain-exterior,test-gap,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D4-2026-10-14-02`. Re-verified against current code before filing.

- **Severity**: LOW (doc and coverage gap, not a behaviour bug)
- **Dimension**: 4
- **Location**: `lod_source_worldspace` / `ExteriorWorldContext::lod_worldspace_key` (`byroredux/src/env_translate/lod.rs`, `byroredux/src/cell_loader/exterior.rs`); skill Dim 4 bullet "nothing consumes the land / LOD bits yet (#5505)"
- **Status**: NEW (the staleness and the missing pin); the underlying gap is Existing: #5505, state re-verified (still OPEN, no comments, nothing in the delta touches it)
- **Description**: `2ca57c4bc` (#5496, 2026-10-10) keys the terrain-LOD quad index, `.btr` / generated-LOD paths, worldspace form id and band bounds on the nearest `WNAM` ancestor that does not set `INHERIT_LOD`. The #5374 parse stamp sets that bit on all 30 Oblivion children, so for BravilWorld, AnvilWorld and the others the terrain-LOD ring should already draw Tamriel's `landscapelod\generated` quads, holed against the child's own cells. The `INHERIT_LAND` bit is still read only for `DNAM` water height. The residual of #5505 is the full-detail parent LAND for LAND-less ring cells (1 043 cells, 1 040 of which have parent LAND).
- **Evidence**: `rg INHERIT_LOD` finds `translate_lod_water` and `lod_source_worldspace`; `terrain_lod.rs` takes `worldspace_key = wctx.lod_worldspace_key()`; `placement_lod.rs` still uses the child's own key, which is right (children ship their own `distantlod\<child>_*.lod`). Existing tests use synthetic Wasteland / Whiterun chains and an installed-FO3 pin.
- **Impact**: #5505's description overstates the missing LOD. Without a pin, an Oblivion-specific break (the `OblivionLegacy` layout branch, `legacy_lod_index`) would be invisible.
- **Related**: #5505, #5496, #5374.
- **Suggested Fix**: add a plugin + `env_translate` pin that parses an Oblivion-shaped child (`WNAM`, no `PNAM`) and asserts `lod_source_worldspace == root`; re-scope #5505 to the parent-LAND ring after an on-device check of BravilWorld.

### Dim 5 — Gameplay & UI data slice

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
