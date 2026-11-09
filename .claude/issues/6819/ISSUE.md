# NIFAL-D5-2026-11-08-01: Particle force-field Z-up→Y-up conversion has no value assertion

GitHub: #6819 (snapshot as filed; GitHub is authoritative for state)
Labels: low,nifal,test-gap,bug

- **Severity**: LOW
- **Dimension**: Particles
- **Tier Violated**: harness-gap
- **Game Affected**: all particle-bearing titles (Oblivion → Skyrim)
- **Location**: `byroredux/src/systems/particle.rs:120` (conversion), `:801` and `:988-989` (tests)
- **Status**: NEW
- **Description**: `apply_emitter_overlays` converts force fields once through `convert_force_fields_zup_to_yup`, and the code is correct today. The overlay tests carry a comment saying fields are "converted Z-up→Y-up" but assert only `force_fields.len() == 1`. `convert_force_fields_zup_to_yup` is referenced nowhere else in a test.
- **Failure scenario**: Dropping or doubling the axis swap passes CI. Wind and gravity fields then point along the wrong axis in every particle effect.
- **Suggested fix**: Author a non-symmetric Z-up force vector and assert the Y-up result component by component in `every_overlay_parameter_reaches_the_preset`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test or guard pins this specific fix


_Source: `docs/audits/AUDIT_NIFAL_2026-11-08.md` (HEAD 51ce1c2f9)._
