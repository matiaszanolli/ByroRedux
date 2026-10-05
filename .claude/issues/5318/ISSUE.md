# #5318: TD3-2026-10-05-04: game-loop.md's live-schedule table, refreshed by #5110 on 10-01, already misses the LSCR turntable system, and its footstep row describes the pre-#5146 camera emitter

Labels: low,tech-debt,documentation,doc-rot
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD3-2026-10-05-04) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - `docs/engine/game-loop.md:138-165` (the schedule table)
  - `:160` (the footstep row)
  - The registration is at `byroredux/src/boot/schedule/update.rs:599`
    (`scheduler.add_exclusive(Stage::Update, loading_model_turntable_system)`, `e60911864`, 10-02).
- **Status**: NEW
- **Effort**: trivial
- **Description**:
  - The table says it lists "the live schedule built in `App::new`".
  - `loading_model_turntable_system` is the one system added since the refresh (diffed against the baseline
    `boot/schedule/*.rs`) that appears in no row.
  - The Late `footstep_system` row still says it "reads the propagated camera `GlobalTransform` (#848)". Since
    #5146 the character-mode emitter is on the body.
- **Related**:
  - AUD-2026-10-05-D5-03: the same footstep staleness in `late.rs`, `scene.rs` and `components.rs`.
  - ECS-2026-10-05-D5-01: that turntable never turns.
  - #5110 (CLOSED).
- **Suggested Fix**: add an Update row for the turntable, and re-word the footstep row to name the body emitter
  in character mode and the camera emitter in FlyCam boots.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
