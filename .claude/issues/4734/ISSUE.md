# EXT-D5-2026-09-21-03: FO3/FNV River-by-name creek water gets a physics current along a dead editor default (90 deg)

**Issue**: #4734
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: LOW (small magnitude: `SPEED_MIN` 0.5 BU/s plus a small synthesized scroll)
**Dimension**: Water translation (WATAL)
**Tier Violated**: no-fabrication
**Game Affected**: FNV, FO3
**Location**: `byroredux/src/env_translate.rs:868-884` (`WaterFlow::for_kind(kind, [cos θ, 0, sin θ])` fallback); provenance doc at `crates/plugin/src/esm/records/misc/water.rs:208-218`

## Description
Several creek records classify as River by name and so carry a current. With no `NAM0`, the current axis falls back to the record's `wind_direction` field, which is a dead editor default (90° on every FO3/FNV record that has it). Every such creek flows toward engine +Z (game-world south) regardless of its real course. #2872 already stopped trusting the co-located speed for the same reason (zero variance); #3185 ruled "a name can establish the kind but not the axis" but did not close this specific fallback.

## Evidence
Census of shipped masters: FalloutNV.esm `CreekWater01`, `CreekWater02nv`, `CreekWater02AVGnv`, `CreekWater02nvbetter`, `RockCreekEstatesWater` all River-by-name at 90.0°; Fallout3.esm `CreekWater01`, `RockCreekEstatesWater` same. Oblivion has none.

## Impact
Floating bodies and the water pattern in FO3/FNV creeks drift due south regardless of the creek's real course.

## Suggested Fix
When neither `NAM0` nor `XWCU` is authored and the direction is the dead default, emit no `WaterFlow`, mirroring #3185's ruling.

## Related
#2872 (closed), #3185 (closed), #3144, #4727 (EXT-D5-01)

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D5-2026-09-21-03)
