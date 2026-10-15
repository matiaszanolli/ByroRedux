# TD7-2026-10-15-05: `hkx` clip and skeleton decoders gate hostile-input counts on five bare `4096`s and `degree > 8` while `behavior.rs` names the same class

Labels: low,import-pipeline,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD7-2026-10-15-05`).

- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants
- **Location**: `crates/hkx/src/animation.rs:332` (bones), `:414` (transforms), `:415` (floats), `:419` (blocks), `:981` (spline control points; `degree > 8` on the same line); named counterparts `crates/hkx/src/behavior.rs:25-29,60` (`MAX_EVENTS`, `MAX_STATES`, `MAX_BONES = 1024`, ...) and the named `MAX_TRANSFORM_SAMPLES` / `MAX_SAMPLES` / `MAX_TRACKS` in the same file
- **Status**: NEW
- **Age**: `02c24e4f1` (2026-08-01)
- **Effort**: trivial
- **Description**: the same file already names the census-derived sample ceilings (#5316, #5006: "set from the census at 8x the measured maximum") but leaves the plausibility ceilings for four different quantities as one anonymous `4096`. They disagree with the behavior-graph reader: a skeleton may declare up to 4096 bones in `animation.rs` while a bone-weights array in `behavior.rs:1063` is rejected above `MAX_BONES = 1024`, so a skeleton the clip reader accepts can be unusable by the graph reader with only a generic error. The values carry no provenance (neither a vanilla maximum nor a format limit).
- **Evidence**: `|| bone_count > 4096`, `|| transform_count > 4096`, `|| float_count > 4096`, `|| num_blocks > 4096`, `control_count > 4096`.
- **Impact**: low; the gates are correct as hostile-input backstops. The cost is that a future Starfield/FO76 rig over a limit gets an unexplained `InvalidData`, and the census tests (`skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings`) cannot cite the constants they measure against.
- **Related**: #5316, #5006, #3011.
- **Suggested Fix**: `MAX_SKELETON_BONES`, `MAX_TRANSFORM_TRACKS`, `MAX_FLOAT_TRACKS`, `MAX_SPLINE_BLOCKS`, `MAX_SPLINE_CONTROL_POINTS` with the measured vanilla maximum in each doc comment; reconcile `MAX_BONES` in `behavior.rs` with the skeleton ceiling.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
