# PHYS-D5-2026-09-21-01: #3974 made the player's current "plane wins"; the dynamic path it mirrors adds both drags

**Issue**: #4691
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: LOW
**Dimension**: Water / Buoyancy
**Location**: player side: `byroredux/src/systems/character.rs:1093-1120`, `:286-299`; dynamic side: `crates/physics/src/water.rs:843-858`, `:971-980`, `:1040-1081`, `:1008`

## Description
The dynamic path applies plane drag then marker drag and the forces add ("so a co-located water plane's force does not discard the marker's current"). The player sampler consults the marker only when the plane has no `WaterFlow`. #3974's own comment cites a `current_flow.or(plane flow)` resolution on the dynamic side that does not exist in `water.rs`.

## Evidence
River WATR planes get a `WaterFlow`; XWCU markers are separate synthesized volumes — co-location occurs in real content (e.g. Skyrim rapids). Re-grepped at HEAD: `water.rs` has no `.or(` flow resolution.

## Impact
In rapids, a barrel feels plane+marker drag while the swimmer beside it feels only the plane. `water.contacts` shows the same flow for both, hiding the difference. Gameplay parity issue only.

## Related
#3974, #3114, #3268.

## Suggested Fix
Choose one composition for both samplers. Extend the #3974 test with a flowing-plane + marker case asserting the two samplers agree.
