# OBL-D4-2026-10-14-01: Oblivion's 55 `XCCM` realm interiors never apply their authored climate

**Labels**: low,game:oblivion,legacy-compat,terrain-exterior,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D4-2026-10-14-01`. Re-verified against current code before filing.

- **Severity**: LOW
- **Dimension**: 4 (mechanism owned by `/audit-exterior`, interior sky)
- **Location**: `CellData::climate_override` (written by `CellSubrecordFields::absorb`, `crates/plugin/src/esm/cell/helpers.rs`); sole consumer `apply_cell_climate_override` (`byroredux/src/scene/world_setup.rs`), which reads it from `exterior_cells` only; the interior Show-Sky path (`byroredux/src/render/sky.rs`, `outdoor_sky_params` over the installed `SkyParamsRes`) never looks at it
- **Status**: NEW (Oblivion-visible; #5463 left the Oblivion decode correct and unchanged)
- **Description**: Oblivion's 55 `XCCM` cells (Oblivion-plane towers `OblivionRD00x*`, `OblivionMqKvatch*`, sigil / MQ14 / MQ16 realms) are interior cells with Show Sky set. They point at CLMTs such as `Obliviondefaultclimate` (26), `ClimateSigil` (12), `MQ14OblivionClimate` (6) and `DefaultClimate` (4). `3b28dca71` correctly keeps TES4 `XCCM` as a CLMT, so `climate_override` is populated, but nothing applies it to an interior.
- **Evidence**: census of `Oblivion.esm`; `rg climate_override` finds consumers only in `world_setup.rs` (exterior streaming) and `climate.rs`.
- **Impact**: the sky seen from the Oblivion-realm Show-Sky interiors is whatever outdoor environment happens to be installed, not the authored realm climate. Not verified on-device. Same forward-scope gap #5463 records for Skyrim's `sky_region`. No open issue covers the TES4 interior-CLMT leg.
- **Related**: #5463 (closed), #2451 / #5529 (exterior per-cell hook), #4908 / #4839 (interior Show-Sky sky lane).
- **Suggested Fix**: let the interior load resolve `climate_override` through `resolve_cell_climate` and install the CLMT's default weather for Show-Sky interiors, sharing the exterior hook's re-apply path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
