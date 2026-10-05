# #5357: PHYS-D5-2026-10-05-02: The swimming player's drift scales the placed current by submerged fraction; the dynamic path applies it at 1.0

**Labels**: low,physics,water,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5357

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-05.md` — `PHYS-D5-2026-10-05-02` (HEAD `a2c24b16e`)

- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `byroredux/src/systems/character.rs:1120-1137` (`player_current_drift`), `:1255-1275` (#4691
  vector composition); compare `crates/physics/src/water.rs:1050-1079` (`current_force(…, 1.0, …)`)
- **Status**: NEW. This is the remainder of the #4691 / #5129 parity rule. The marker-only case is fixed.
- **Trigger Conditions**: the player swims, meaning depth is past `SWIM_HEIGHT_SCALE`, inside a `WaterCurrentVolume`
  box.
- **Description**: the two paths scale the marker differently:
  - The dynamic path applies plane drag × fraction plus marker drag × **1.0**.
  - While not swimming, the player applies the marker × 1.0, which matches.
  - Once swimming, `player_water_state` composes plane + marker into one vector, and `player_current_drift`
    scales *the whole vector* by `state.fraction`. At the swimlevel threshold the fraction is about 0.675
    ((0.35h + h) / 2h), so stepping into the swim state drops the marker's drift by about 32%. It returns to
    full strength only when the player is fully submerged.
  - The guard `swimming_drift_uses_the_composed_column_flow_only` pins the composite × fraction form.

  The player ignoring plane flow while wading is a different matter: it is documented design (the doc at
  `:1111-1113`) and is not filed.
- **Impact**: a small, discontinuous change in drift at the walk→swim boundary inside rapids. A barrel next to the
  swimmer feels the full marker current.
- **Related**: #4691, #5129 (closed); PHYS-D5-2026-10-05-01.
- **Suggested Fix**: compose `plane × fraction + marker × 1.0` in the swim arm, mirroring the dynamic path. Or
  record the intended scaling in `docs/engine/watal.md` and keep the test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
