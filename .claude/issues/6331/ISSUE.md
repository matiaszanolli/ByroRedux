# #6331: REG-2026-10-21-02: The release workflow has failed on every tag pushed since it was added, so no release exists

**Labels**: low,bug,tech-debt

**Source**: docs/audits/AUDIT_REGRESSION_2026-10-21.md · **Severity**: LOW · **Dimension**: CI gates · **Location**: `.github/workflows/release.yml` (CI-success gate step), `docs/releasing.md:28`

- **Description**: The gate reads the commit's CI conclusion once and fails if it is not `success`. In all four tag-push runs, CI was still queued or running:

  | Run | Time (UTC) | Tag | Commit |
  |---|---|---|---|
  | 39140525596 | 10-18 15:26 | `v0.101` | `d4b095576` |
  | 39140689177 | 10-18 15:29 | `v0.101` | `d4b095576` |
  | 39355513091 | 10-20 01:49 | `v0.102` | `12677d718` |
  | 39418367607 | 10-20 11:54 | `v0.102` | `28e1f413e` |

  In each case the log reads "CI has not succeeded on … (none)". CI on `d4b095576` finished green at 15:44 and on `28e1f413e` at 12:10. The workflow was never re-run, and `gh release view v0.101` / `v0.102` both return "release not found".
- **Evidence**: `gh run list --workflow release.yml` and `gh run view <id> --log-failed`; the CI run timestamps come from `gh run list --workflow ci.yml --commit <sha>`.
- **Impact**: Both tagged alphas exist as tags with no published binaries. The fail-fast gate turns "tag right after push", the normal workflow, into a silent no-release unless someone remembers to re-run it.
- **Related**: TOOL-D4-2026-10-21-02 (release-doc rot, same pipeline, different defect)
- **Suggested Fix**: Make the gate wait for CI: poll `gh run watch`, or loop on `gh run list` until the run completes, with a timeout. Or trigger the release from a `workflow_run` of CI on the tag's commit. Then re-run release for `v0.102`.


Validated 2026-10-21: the gate at `.github/workflows/release.yml:66-72` reads a single completed-run conclusion and exits 1 otherwise; `gh release list` is empty.

## Completeness Checks
- [ ] **SIBLING**: Other workflows gating on another workflow's conclusion checked for the same race
- [ ] **TESTS**: A tag pushed while CI is still running produces a release once CI goes green
