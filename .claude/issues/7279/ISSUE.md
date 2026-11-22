# #7279: PHYS-D5-01: The player's own `WaterContact` keeps the per-body buoyancy rescan armed

- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:792-815` (`waves_require_contact_rescan`); `byroredux/src/systems/character.rs:1461-1471` (`sync_player_water_contact`)
- **Status**: NEW
- **Description**: `waves_require_contact_rescan` returns true when any `WaterContact` has a surface and is submerged or within the wave band. The kinematic player writes such a row while wading or swimming, so in an otherwise settled cell the full per-dynamic-body buoyancy scan runs every frame for as long as the player is in water. The player's water state comes from `player_water_state`, not that scan.
- **Evidence**: `contacts.iter().any(|(_, contact)| contact.surface_entity.is_some() && (contact.submerged_fraction > 0.0 || contact.depth.abs() <= max_amplitude + WATERLINE_HYSTERESIS))`, with no filter that excludes the player's row.
- **Trigger Conditions**: the player stands in, or swims in, wave-carrying water in a cell whose dynamic bodies have all settled.
- **Impact**: A per-frame cost that scales with the cell's dynamic bodies, paid only because of the player. Not measured.
- **Related**: #6489, #7018
- **Suggested Fix**: Skip contacts whose entity has no dynamic `RapierHandles` (or skip the player entity) in the rescan gate, and measure the scan cost before and after on a wading exterior.

**Found by**: `docs/audits/AUDIT_PHYSICS_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: the player arm of every water consumer checked
- [ ] **TESTS**: a wading player in a settled cell does not arm the rescan
