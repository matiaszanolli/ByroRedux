# TD9-2026-10-19-05: `manual_bench_draw_sort_serial_vs_parallel` is a 58-line print-only benchmark in `#[test]` clothing (sibling of closed #5857)

Labels: low,renderer,test-gap,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `byroredux/src/render/draw_sort_key_tests.rs:726-790` (`#[ignore = "manual timing bench, environment-dependent"]`, body is `eprintln!` timing rows; scan of all `#[test]` bodies finds it as the only assertion-free ignored body besides helper-wrapped false positives)
- **Age**: older than 2026-09; #5857 fixed only the VM census (`vm_cost_idle_burst_polling_runaway` now has one ceiling assertion, `vm/tests.rs:4486`) although its description named both
- **Effort**: trivial
- **Description**: nothing fails if the parallel sort regresses; the sibling `manual_bench_draw_sort_decorate_sort_undecorate` has asserts.
- **Suggested Fix**: move to `byroredux/examples/` or assert serial==parallel output order (the part that is deterministic) and keep timings printed.

Related: #5857 (closed; fixed the VM census half only).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
