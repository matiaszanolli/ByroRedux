# #7019 — PHYS-D5-02: Stale water-current prose after #5129/#6488 (early-stage access comment and watal.md river paragraph)

**Labels**: low,physics,water,documentation,doc-rot · **Filed from**: `docs/audits/AUDIT_PHYSICS_2026-11-15.md` (2026-11-15)

- **Severity**: LOW
- **Dimension**: Dim 5 — documentation of the two-samplers contract
- **Location**: `byroredux/src/boot/schedule/early.rs:67-69`; `docs/engine/watal.md:671-674`
- **Status**: NEW (doc rot)
- **Description**: The player controller's access row says `// #3974 — player_water_state falls back to a placed WaterCurrentVolume marker when the plane has no flow of its own`. That arm was replaced: since #5129/#5357 the marker is resolved by `placed_current_flow_at` (`character.rs:312`), independent of any plane, and composed additively in `player_current_drift` (`character.rs:1214`). `player_water_state` no longer reads the marker. The `watal.md` river paragraph says `WaterFlow::over_bed` is "the physics current on floating bodies (`apply_buoyancy`) and the `WaterContact` it publishes". It does not name the `local_current` seam or the swimming player, which is what #6488 added and what the two-samplers rule rests on.
- **Evidence**: `character.rs:1272-1279` and `physics/src/water.rs:499-512` already document the current behaviour. The two sites above are the only ones that still describe the old one.
- **Impact**: A reader following the early-stage comment would look for a plane-less fallback inside `player_water_state` that does not exist. The `watal.md` page does not tell the next author that a new reader of a plane's `WaterFlow` must go through `byroredux_physics::local_current`.
- **Related**: #4691, #5129, #5357, #6488.
- **Suggested Fix**: Reword the early.rs comment to "`placed_current_flow_at` resolves the marker, independent of the plane". Add one sentence to the watal.md river paragraph: "`byroredux_physics::local_current` is the one entry point; the dynamic path and `player_water_state` both call it (#6488)".
- **Trigger Conditions**: n/a (documentation).

---

Source: `docs/audits/AUDIT_PHYSICS_2026-11-15.md`

## Completeness Checks
- [ ] **SIBLING**: Every other statement of the same rule (docs, module docs, skill text) checked
- [ ] **TESTS**: n/a (documentation) — or the doc's claim is pinned where a guard already exists
