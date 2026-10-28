# PHYS-D5-2026-10-27-01: The swimming player's river current ignores the bed; dynamic bodies and their `WaterContact` use `WaterFlow::over_bed`

Labels: low, physics, water, bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-27.md

# PHYS-D5-2026-10-27-01: The swimming player's river current ignores the bed; dynamic bodies and their `WaterContact` use `WaterFlow::over_bed`

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-27.md` finding `PHYS-D5-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `byroredux/src/systems/character.rs` - `player_water_state` (~:1351, takes the plane `WaterFlow` verbatim), `player_current_drift` (~:1205-1226), stale `PlayerWaterState` doc (~:1260-1265); vs `crates/physics/src/water.rs` - `local_flow` (~:1236-1243), current force (~:1335), published `flow: local_flow` (~:1378).

## Description
8647c1128 (2026-10-26) made the dynamic path evaluate a River/Rapids current at the body's position over the plane's `WaterBedField` (`flow.over_bed(sample)`: down to `WATER_RIVER_SPEED_MIN` 0.35 at the waterline, turned along the bank), using it for both the current force and the published `WaterContact.flow`. The player sampler was not changed: `player_water_state` still copies the plane's `WaterFlow` and `player_current_drift` drifts the swimmer at the plane's full speed and authored heading; the `PlayerWaterState` doc still says the dynamic contact write publishes `flow: s.flow`. This breaks the two-samplers rule (every water input added to one must exist in the other); same class as #4691 and #5357.

## Evidence
`water.rs:1236` `let local_flow = s.flow.map(|flow| match (s.kind, s.bed.as_ref()) { (WaterKind::River | WaterKind::Rapids, Some(bed)) => bed.sample(..).map_or(flow, |sample| flow.over_bed(sample)), _ => flow })`. Player side: `character.rs:1351` `let flow = flow_q.as_ref().and_then(|q| q.get(entity).copied());` - no bed lookup, although `bed_q` is already in scope ~two dozen lines up for the wave height (`authored_wave_height_over_bed` at :1321).

## Impact
In a river's shallows or near a bank a swimming player is pushed up to ~3x harder than the barrel beside it, in the authored direction rather than along the bank. The published player `WaterContact.flow` disagrees with every other wet body's in the same column (FX/audio consumers). Small: the player drift is `speed * 0.35 * fraction`.

## Related
#4691, #5357, #5354 (same rule for wave inputs).

## Suggested Fix
In `player_water_state` apply the same `(River | Rapids, Some(bed)) => flow.over_bed(bed.sample(pos.x, pos.z))` to the stored `flow`. Better: factor one `local_current(kind, flow, bed, point)` helper into `crates/physics/src/water.rs` and call it from both sites. Fix the struct doc. Add a parity test over a bed field.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

