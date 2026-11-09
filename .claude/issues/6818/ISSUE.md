# NIFAL-2026-11-08-03: `NEAR_MIRROR_NEUTRAL_ROUGHNESS = 0.5` has no measurement and its open NIFAL call is untracked

GitHub: #6818 (snapshot as filed; GitHub is authoritative for state)
Labels: low,nifal,renderer,bug,game:fo4

- **Severity**: LOW
- **Dimension**: Material (Dim 8)
- **Tier Violated**: no-fabrication
- **Game Affected**: FO4 (BGSM near-mirror materials, ~425 per #3639)
- **Location**: `byroredux/src/material_translate.rs:1366-1380`; `crates/core/src/ecs/components/material.rs:894-897` (`NO_SIGNAL_NEUTRAL` 0.85)
- **Status**: NEW (the doc-rot half closed as #6746; the 0.5 vs 0.85 question it "leaves open for NIFAL" has no issue)
- **Description**: After 80c0d01fc the constant's doc calls it "a chosen mid roughness" equal to `Material::default()`, explicitly not the classifier's 0.85 terminal. The value has no measurement or source behind it.
- **Failure scenario**: A near-mirror BGSM with an unresolvable gloss map lands at 0.5, under the 0.6 reflection gate, so it still traces RT reflections. At 0.85 it would not. Which is right is unmeasured.
- **Suggested fix**: Measure what the affected FO4 materials' `Gloss` maps resolve to elsewhere and either cite the result or switch to the classifier neutral. Link a tracker.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test or guard pins this specific fix


_Source: `docs/audits/AUDIT_NIFAL_2026-11-08.md` (HEAD 51ce1c2f9)._
