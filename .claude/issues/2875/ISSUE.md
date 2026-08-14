# PHYS-D7-04

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2875

---

Found by `/audit-physics` Dimension 7 (Queries & Diagnostics). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/world.rs:761-817` (`colliders_near_xz` + its doc claim at `:777-778`); `crates/physics/src/sync.rs:335-338` (`SPAWN_CENSUS_DETAIL_CAP = 24`) and `:455`, `:470-475`; `byroredux/src/scene.rs:1121` (`SPAWN_CENSUS_RADIUS_BU = 256.0`)

## Trigger Conditions
A failed door spawn in any column containing more than 24 colliders within +/-256 BU in XZ — i.e. essentially every interior with an upper storey, beams or shelving, and every exterior grid cell.

## Description
`colliders_near_xz` takes `(x, z, radius)` and **no Y**. It sorts descending by AABB centre Y and its doc block claims this is *"so the nearest thing above the probe reads first"*. There is no probe Y in scope, so the sort key is not "nearest above the probe" — it is **"highest in the world column"**. `dump_spawn_collider_census` then prints only the first 24.

The consequence inverts the diagnostic's purpose. The question being asked is *"is there a floor at/below the spawn?"*, whose answer lives at the **low** end of the sort. In a Skyrim inn (two storeys, roof beams, rafters, an upper landing) the 24 shown entries are the roof and the upper floor; the actual spawn-height geometry falls under *"... N further colliders omitted"*.

The very cell shape the doc itself calls out — *"2560 fixed colliders and a hole exactly under the player's spawn"* (`world.rs:769-771`) — is the worst case for this ordering.

## Evidence
- `world.rs:779` signature `colliders_near_xz(&self, x: f32, z: f32, radius: f32)` — no Y parameter exists to sort relative to
- `world.rs:811-815` sort comparator uses `a.aabb_min[1] + a.aabb_max[1]` (absolute centre Y)
- `sync.rs:455` `entries.iter().take(SPAWN_CENSUS_DETAIL_CAP)`
- the unit test that pins the ordering (`world.rs:1136-1153 census_sorts_by_aabb_centre_y_descending`) uses three slabs, so it can never observe the truncation interaction

## Impact
In precisely the dense-interior case the census was written for, it prints a wall of irrelevant ceiling geometry and omits the evidence. Degrades a MEDIUM-value diagnostic to a misleading one; costs a debugging session per occurrence.

## Suggested Fix
Pass the probe origin Y through to `colliders_near_xz` and sort by `|centre_y - probe_y|` (nearest to the probe first, which is what the doc already promises), or keep the descending sort but take 12 from each end. Fix the doc sentence either way.

## Related
- PHYS-D7-03 (same function), PHYS-D7-05
