# TD9-2026-10-15-05: The ignored VM timing census has 99 lines of `eprintln!` and no assertion, and two more environment-dependent bench tests share the shape

Labels: low,test-gap,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD9-2026-10-15-05`).

- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `crates/scripting/src/fragment/vm/tests.rs:4358-4457` (`vm_cost_idle_burst_polling_runaway`, `#[ignore = "timing census; run with --release --nocapture"]`); `byroredux/src/render/draw_sort_key_tests.rs` (2 x `manual timing bench, environment-dependent`)
- **Status**: NEW
- **Age**: `vm_cost_idle_burst_polling_runaway` 2026-10-13
- **Effort**: trivial
- **Description**: the only assertion-free `#[test]` in the tree that is not a helper-wrapper false positive (a whole-repo scan of 10 507 tests for bodies lacking any assert/panic/expect/`?` found 24; 23 are false positives through helper functions such as `approx(..)` or `.expect(..)` source scans). It exists to print µs-per-call tables for the VM systems, i.e. a benchmark in test clothing, and it is the only place the VM's per-frame cost is measured. Nothing fails if a system regresses 100x.
- **Evidence**: `eprintln!("\nA. idle frame (no events), µs per call of each VM system");` ... no `assert`.
- **Impact**: none today; the PERF gate the table suggests does not exist.
- **Related**: `crates/scripting/src/fragment/vm/tests.rs:4601` (`budgeted(&mut world, 100_000)`).
- **Suggested Fix**: either move it to a `crates/scripting/examples/vm_cost.rs` (an example, not an `#[ignore]` test) or add one generous ceiling assertion (e.g. idle-frame cost at 20 000 refs under a named budget).


## Per-dimension notes (no finding)

- **Dim 1:** `#5586` (`mq101_conformance.rs::run`, 1454 lines) unchanged; `build_pipelines_and_finish` stays dismissed (every phase-3 value feeds the terminal struct literal). `shader_constants_data.rs` is a data table: do not split, generate (`TD1-10`).
- **Dim 3:** `ecs.md` stage list, `memory-budget.md` ground-cover figures, `shader-pipeline.md` binding tables, `game-compatibility.md` vs the ROADMAP matrix and the `docs/engine/index.md` index were sampled and are current.
- **Dim 4:** validator 0 STALE; `groups.txt` covers all 34 `audit-*` dirs exactly once; ~100 `/audit-x Dim N` cross-references valid; the 15 CRITICAL/HIGH findings from 2026-07-13..16 all trace to closed issues. Reports before 2026-06-07 were not re-verified.
- **Dim 5/6:** zero `unimplemented!`/`todo!()` in production; every console command and scheduler system checked is registered; `ScriptTimer`/`TimerExpired` has no producer or consumer but #5608 closed docs-only and the docs now say so. Production-unreachable components are the honestly documented "forward-latent" rows in `registry_completeness_tests.rs` (#5609, #5610 open).
- **Dim 7:** NIF version literals are guarded by `version_literal_tests.rs`; every shader `#define` outside the generated header is a guard, a push-constant alias or a named/cited constant.
- **Dim 8:** no `#[deprecated]` items, `// removed:` breadcrumbs or compatibility re-exports; every workspace dependency has a consuming member; every feature has a cfg consumer; `#[allow(clippy::…)]` without a reason comment is 32 (28 tracked in #5622).
- **Dim 9:** the `workspace_hygiene_tests` guards are not `#[ignore]`d; the core `--no-default-features` lane only compiles, which is proportionate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
