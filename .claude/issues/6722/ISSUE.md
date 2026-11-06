# SPT-2026-11-06-D3-01 (#6722)

Snapshot of the issue as filed; GitHub is authoritative for state.

**Severity**: LOW · **Dimension**: TREE→Billboard Wiring · **Status**: NEW
**Location**: `byroredux/src/systems/billboard.rs:11-35`, `:56-66`, `:92-101`, `:126-133`, `:159`, `:194`; test `active_weather_direction_change_rebends_stationary_speedtree` (`:458`)
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-11-06.md`

## Description
`69a3b0b30` (local gusts) made `let wind_active = true;` unconditional so local gusts move trees in every weather. That makes the early-out `if !camera_changed && !wind_active && !wind_state_changed { return; }` unreachable. Left behind:
- `wind_state_changed`, `last_wind` and the per-frame `WindField` comparison feed only that dead condition.
- `BillboardPass.wind_active` is a constant `true`, and `if let (true, Some(tree_wind)) = (pass.wind_active, tree_wind)` tests it.
- The module doc (lines 11-17) still says that when neither pose moved "the entire billboard loop is skipped". Lines 23-27 still say "wind reopens that gate". Lines 126-133 still say the wind-only pass is the steady state because `wind` "is non-zero for most vanilla weathers".
- The new comment (lines 56-60) says the cost is "the handful of `SpeedTreeWind` trees". That count is not measured anywhere; a TREE-dense exterior spawns one marked placeholder per `.spt` REFR in the loaded grid.
- `active_weather_direction_change_rebends_stationary_speedtree` no longer tests change detection: the tree is re-bent every frame, so it passes with `wind_state_changed` deleted.

## Evidence
`billboard.rs:61` `let wind_active = true;` → `:99` `!wind_active` is always false. The test at `:458` asserts only `before != after` after a direction change, which a free-running re-bend also satisfies.

## Impact
No wrong output. The risk is maintenance: a later reader trusts the #1374 / #3192 prose and "re-adds" or "removes" a gate that does not exist. In calm exterior weather, and in any parked-camera interior that has marked trees, the marked set now dirties `GlobalTransform` every frame where before it stayed clean, so the incremental world-bound path (#1374) is defeated for those entities. That cost is **unmeasured**.

## Related
#1374, #3192, `69a3b0b30`.

## Suggested Fix
Delete `wind_state_changed` / `last_wind` / `BillboardPass.wind_active` and the vestigial early-out, or restore a real gate if a calm-weather cost is measured. Rewrite the module doc around "camera moved → all billboards; parked → `SpeedTreeWind` set". Replace the direction-change test with one that fails if the stationary pass stops re-bending (or drop it).

## Completeness Checks
- [ ] **SIBLING**: Same dead-gate / stale-doc pattern checked for other `WindField` consumers (water, grass)
- [ ] **LOCK_ORDER**: If the `GlobalTransform` / `Billboard` / `SpeedTreeWind` query scopes change, TypeId-sorted acquisition and the `Access` declaration in `boot/schedule/late/camera.rs` stay in step
- [ ] **TESTS**: A regression test pins the stationary-camera wind pass (and a calm-weather cost measurement, if a gate is restored)
