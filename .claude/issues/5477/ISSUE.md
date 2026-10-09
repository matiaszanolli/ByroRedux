# #5477: TOOL-CI-2026-10-08-01: The workspace clippy step is red at HEAD on a `type_complexity` error, separate from the red registry-count test

**Labels**: low,tech-debt,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5477

**Source**: `docs/audits/AUDIT_TOOLING_2026-10-08.md` — `TOOL-CI-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Tool CLIs / CI gate (`.github/` belongs to tech-debt and runtime in `_audit-owners.md`. The offending code is in `byroredux/src/systems/`, owned by ecs and performance, and no audit in this suite filed it.)
- **Exposure**: developers only (CI signal)
- **Location**: `byroredux/src/systems/eat_sleep.rs:54`; `.github/workflows/ci.yml:185-190`
- **Status**: NEW. It is distinct from CONC-D3-2026-10-08-04, which is the `debug_cli_component_counts_match_the_registry` test failure in the same job's `cargo test` step.
- **Description**: 00f580e09 added `let mut actors: Vec<(EntityId, EatOrSleep, Option<f32>, Option<u32>, u32)> = Vec::new();`. Clippy 1.96.0 (the pinned toolchain CI actually uses, see TOOL-CI-02) rejects it under `-D warnings` with `clippy::type_complexity`.
- **Evidence**: CI run 37848932617, job 113556725530, step "cargo clippy":
  - `error: very complex type used … --> byroredux/src/systems/eat_sleep.rs:54:21`
  - `error: could not compile byroredux (bin "byroredux") due to 1 previous error`

  This is the only clippy error in the log, and every other crate passed. The dedicated renderer unsafe-block gate step (`ci.yml:199-201`) was green.
- **Impact**: The workspace clippy gate is red. Because of `--keep-going` and because `byroredux` is the leaf crate, no other crate goes unlinted. But while the step stays red, the next real lint in the `byroredux` crate lands without a new signal. #5308 and #4595 show this repo has paid for multi-day red clippy boards before. Fixing CONC-D3-04 alone will not turn the job green.
- **Related**: CONC-D3-2026-10-08-04, #5308, #5121, #4595
- **Suggested Fix**: Name the tuple as a type alias or a small struct (for example, `type EatSleepCandidate = (…)`) in `eat_sleep.rs`, then re-run `cargo clippy -p byroredux --no-deps -- -D warnings` on 1.96.0.

## Completeness Checks
- [ ] **SIBLING**: `cargo clippy --workspace --keep-going -- -D warnings` on 1.96.0 is green workspace-wide after the alias lands
