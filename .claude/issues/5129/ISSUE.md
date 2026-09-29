# PHYS-D5-2026-09-29-01: the two water samplers still disagree on marker-only currents — the player feels a WaterCurrentVolume only inside a submerged WaterPlane column

**Labels**: low,bug,physics,water,test-gap

**Source**: `docs/audits/AUDIT_PHYSICS_2026-09-29.md`
**Severity**: LOW
**Dimension**: Water / Buoyancy
**Location**: `byroredux/src/systems/character.rs` (`player_water_state`); compare `crates/physics/src/water.rs` (dynamic-body `current_flow` resolution)

**Trigger Conditions**: a placed current box (XWCU/XPRM → `WaterCurrentVolume`) that extends beyond its plane's XZ footprint or surface-mesh coverage. Placed-water surface meshes are partial strips (17c01a4e5). Shipped-content occurrence not censused.

## Description
Remainder of the #4691 parity rule (#4691 fixed the co-located case). The dynamic path resolves `current_flow` independently of `surface`: it applies the marker drag at `frac = 1.0` whenever the AABB centre is inside the marker box and the union XZ footprint, including a body "in a marker that overlaps no plane in XZ" (the #3114/#3268 comments name this case). `player_water_state` looks the marker up only inside `for (entity, plane) in wq.iter()`, after three `continue`s:
- `surface_y_at` returns `None`;
- the column is outside `[volume.min.y, surface_y]`;
- `fraction <= 0`.

So in the marker-only region a barrel drifts and the swimmer beside it does not. The WATAL rule is "every water input added to one must exist in the other".

## Evidence
The guard `player_water_state_falls_back_to_a_placed_current_volume` places its marker entirely inside the lake, so the marker-only branch is never exercised.

## Impact
The player gets no drift in rapids whose authored current box outruns the water surface mesh, while clutter there drifts.

## Related
#4691, #3974 (closed), #4911 (XWCU scroll/physics divergence, EXAL).

## Suggested Fix
Resolve the marker before the plane loop, as the dynamic path does (a swim state needs a plane; a current does not). Alternatively, record the asymmetry as intended in `watal.md` and pin it with a marker-outside-plane test.

Validated at HEAD 9fcfdc3fc: in `player_water_state` the `WaterCurrentVolume` marker lookup (`marker_flow`) sits inside `for (entity, plane) in wq.iter()` after the `surface_y_at` / column / fraction `continue`s.

## Completeness Checks
- [ ] **SIBLING**: other player-side water inputs (buoyancy, splash) checked against the dynamic sampler
- [ ] **LOCK_ORDER**: moving the `current_q` query out of the plane loop keeps TypeId-sorted acquisition
- [ ] **TESTS**: A marker-outside-plane fixture pins the chosen behaviour
