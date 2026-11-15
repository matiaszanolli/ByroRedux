# #6992: TD9-2026-11-15-03: the `parallel-scheduler`-off lane only compiles the tests; the serial scheduler arm is run by no job

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6992
- **Labels**: low,ecs,test-gap,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 9 (feature gates vs CI lanes)
- **Location**: `.github/workflows/ci.yml:205-206` (`cargo check -p byroredux-core --no-default-features --lib --tests`); the serial arm is `crates/core/src/ecs/scheduler.rs:790`
- **Age**: lane added by #5624 (closed)
- **Effort**: trivial
- **Labels**: `tech-debt`, `test-gap`
- **Status**: NEW (narrower than closed #5624, which asked for the arm to be compiled)
- **Description**: every workspace consumer takes `byroredux-core` with default features, so `rayon` is always on and the `#[cfg(not(feature = "parallel-scheduler"))]` arm of `Scheduler::run` (`scheduler.rs:790`) exists only in the lane's build. The lane type-checks it and its tests but does not execute them. `docs/engine/ecs.md` documents the flag as a runtime-disable knob, so the serial arm is a supported mode.
- **Evidence**: `cargo test -p byroredux-core --no-default-features --lib` on a clean HEAD worktree (`CARGO_TARGET_DIR=/mnt/data/tmp/td-target-7`): `test result: ok. 916 passed; 0 failed; 1 ignored`; `ecs::scheduler` alone: 38 passed. So the lane can run today and would be green; it takes no GPU and no game data.
- **Impact**: a regression in the serial arm (ordering, stage barriers, timing resources) passes CI until someone disables the feature.
- **Related**: #5624, `docs/engine/ecs.md`.
- **Suggested Fix**: change the step to `cargo test -p byroredux-core --no-default-features --lib` (keep `--tests` for the compile of integration targets if wanted).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
