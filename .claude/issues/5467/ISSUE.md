# #5467: TD1-2026-10-08-01: `byroredux/src/components.rs` crossed 2000 production LOC (1997 → 2035)

**Labels**: low,tech-debt,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5467

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-08.md` — `TD1-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/components.rs` (2597 total lines, 2035 production)
- **Status**: NEW (first crossing; it was on the 10-05 watch list at 1997)
- **Age**: growth came from `c8c0fe868` (#5306, `LoadingCoverClock`), `c2f28e06c` (#4277), `1162236fc` and `63bf3347f`
  (#3817, `CinematicReAdoption`), all 2026-10-05/06.
- **Effort**: medium
- **Description**: the file was meant to hold "marker components + app resources". It now holds six unrelated domains, in
  contiguous blocks:
  - render and material markers plus texture handles (25–460);
  - terrain components (460–570);
  - cell lighting / weather / sky / cloud resources and their three test modules (570–1640);
  - entity indices (`NameIndex`, `SubtreeCache`, `CellRootIndex`, `CinematicReAdoption`, 1640–1720);
  - input, footstep and water-audio components and resources (1724–1935);
  - tuning resources, then the AI / animation / combat-clip / navmesh block (1940–2360+).
- **Evidence**: `prod_loc byroredux/src/components.rs` → 2035. On the `a2c24b16e` tree it was 1997.
- **Impact**: every gameplay, render or audio change touches one file, which taxes merges.
- **Related**: `byroredux/src/components/game_time.rs` shows the directory already exists.
- **Suggested Fix**:
  - Move the domains into `components/{render,terrain,environment,indices,audio,ai_anim,navmesh}.rs` and re-export them
    from `components.rs`.
  - First update the two self-scans: `components.rs:2555` reads `include_str!("components.rs")` and
    `commands/env_health_tests.rs:466` reads `"../components.rs"` (*feedback_file_split_include_str*).

## Completeness Checks
- [ ] **SIBLING**: Both self-scans (`components.rs` `include_str!("components.rs")`, `commands/env_health_tests.rs` `"../components.rs"`) re-pointed before the split, so they do not go vacuous
