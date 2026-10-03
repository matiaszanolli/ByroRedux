# #5183: EXT-D5-2026-10-02-03: An XWCU current on a Calm WATR reaches physics as a `WaterFlow` with no pattern term (8 FO4 REFRs). #4911's "ripples and current agree" does not hold on the calm arm

**Labels**: low,terrain-exterior,water,physics,bug,game:fo4
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `byroredux/src/cell_loader/water.rs:712-737`: `reference_flow` is built unconditionally, the compose is gated at `:722`, and `:737` returns `reference_flow.or(watr_flow)`.
  - `crates/core/src/ecs/components/water.rs:493`: the doc says "`Calm` waters do not carry this component".
  - Contrast `byroredux/src/env_translate.rs:950-962`: an authored NAM0 promotes Calm → River.
- **Status**: NEW. The physics override predates #4911; #4911's calm gate is what makes the disagreement explicit.
- **Tier Violated**: single-boundary (two arms apply different rules to an authored current)
- **Game Affected**: FO4
- **Description**:
  - In the WATR arm, an authored current never coexists with `Calm`. `has_authored_linear_flow` promotes the kind to River (or Rapids), so both the pattern term and the physics flow follow. watal.md §2 states the rule: authored flow "cannot silently fall back to calm-water physics".
  - The merge arm does not mirror this. It keeps `watr_kind` (Calm) and skips the compose because of `has_directional_flow()`, but it still returns the XWCU current as the entity's `WaterFlow`.
  - The result is a Calm plane that carries a `WaterFlow`, against the component's own invariant. Floating bodies drift along the XWCU vector while the surface pattern is the WATR's unrelated authored layers.
- **Evidence**: Census, each XWCU REFR traced → base ACTI `WNAM` → WATR, classified with `env_translate`'s kind rules:
  - Skyrim.esm: 128/128 River, unaffected.
  - Fallout4.esm: 163 River and **8 Calm**:
    - 7 interior pond REFRs (`IntPondDarkWaterCalm`, `IntPondDarkWaterCalm_NoFalloff`, `IntPondWaterCalmInteriorLitRed`), at 0.08–0.25 BU/s.
    - `ExtOldGulletWater` (REFR 0x1B273E), XWCU (−3.41, −3.08), 4.6 BU/s.
  - The test `xwcu_current_recomposes_the_pattern_scroll` covers only a River WATR. The calm-gate test pins "no synthesized term", not "no physics flow".
- **Impact**: On those 8 placements the physics current and the visible water disagree, which #4911 set out to remove. Any consumer that trusts "Calm ⇒ no `WaterFlow`" is wrong for them.
- **Suggested Fix**: Make the merge follow the WATR arm's rule. Either let an XWCU current promote Calm → River (and Rapids at `SPEED_RAPIDS`), then compose, or drop the XWCU flow when the kind stays Calm. Pin the chosen rule with a Calm-WATR + XWCU test.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
