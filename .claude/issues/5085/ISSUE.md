# TD9-2026-09-29-01: The issue-traceability CI gate has failed on every push since it was enabled (09-07): `rg: command not found` (regression of #3504)

**Labels**: medium,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

**Regression of #3504** (#3504 is CLOSED; filed as a new issue rather than reopening it).

- **Severity**: MEDIUM. It is the root cause of TD4-01, and #3218 and #3504 were both closed as fixes for
  this class.
- **Dimension**: 9 — Test Hygiene (CI lanes)
- **Location**: `.github/workflows/ci.yml:14-52` (job `issue-traceability`, `runs-on: ubuntu-latest`,
  with no ripgrep install); `scripts/check-issue-traceability.sh:9-11` and every `rg` call after them
- **Status**: Regression of #3504
- **Age**: `798c31651` (2026-09-07, "Fix #3504: run the traceability gate on pushes to main")
- **Effort**: trivial
- **Description**: the job's first step, `scripts/check-issue-traceability.sh --self-test`, dies because
  the runner has no `rg`. The job never reaches "Annotate uncited fixes and uncited closures on the pushed
  range" (`ci.yml:39-43`), which is the step #3504 added for direct pushes to main.
- **Evidence**:
  - HEAD run `36609043348`, job `109545444545`, fails with:
    ```
    scripts/check-issue-traceability.sh: line 9: rg: command not found
    scripts/check-issue-traceability.sh: line 11: rg: command not found
    ##[error]Process completed with exit code 1.
    ```
  - Sampled main runs 09-08 23:17, 09-09, 09-11, 09-12, 09-14, 09-15, 09-16, 09-19, 09-22, 09-24, 09-25,
    09-26, 09-28 and 09-29 are all `failure`. Runs on 09-02 → 09-07 14:28 were `skipped` (the old PR-only
    condition).
  - The only job that installs ripgrep is shader parity (`ci.yml:113`).
  - Locally the self-test passes: `check-issue-traceability: self-test passed`.
- **Impact**:
  - The push-direction traceability signal has not existed for three weeks.
  - Every mis-titled commit in TD4-01 would have been annotated at push time, while its author still had
    the context.
  - The self-test prints nothing of its own on a missing tool, so the red job looked like noise.
- **Related**: #3504, #3218, #3425, #3538 (CLOSED); TD4-2026-09-29-01.
- **Suggested Fix**:
  - Install ripgrep in the job (`sudo apt-get install -y ripgrep`), or fall back to `grep -E` when `rg`
    is absent. Have the script check for its tools with `command -v rg` and a clear message.
  - Once green, run `--push ee6d3fb39 HEAD` once to annotate the window.

**Validated at HEAD 9fcfdc3fc**: `.github/workflows/ci.yml` job `issue-traceability` (`runs-on: ubuntu-latest`) installs no ripgrep; `scripts/check-issue-traceability.sh:9,11` call `rg`; HEAD run 36609043348 job "Issue/commit traceability" concluded `failure` with `line 9: rg: command not found`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
