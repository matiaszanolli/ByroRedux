# NIF-D3-2026-09-21-01: The nightly real-data corpus lane (#3919) has never executed

**Issue**: #4619
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: MEDIUM. A defence-in-depth gap over the HIGH-floor "NIF parse failure" class; same shape as SAFE-D4-2026-09-21-01 (#4595) and SAFE-D5-2026-09-21-01 (#4596).
**Dimension**: Block Dispatch Coverage (baseline harness)
**Game Affected**: all (the gate covers every title)
**Location**:
- `.github/workflows/real-data-gates.yml:36-38`: the job-level `if: … inputs.game == matrix.game`.
- `:39`: `runs-on: [self-hosted, linux, x64, byroredux-game-data]`.
- `:44-74`: the matrix filters.

## Description
- GitHub evaluates `jobs.<id>.if` before it expands the matrix, and the `matrix` context is not available at that point. The workflow file therefore fails to parse.
- GitHub registers it under its path instead of its `name:` (workflow id 351792019), and every push since `bd40e91af` (2026-09-06) creates a failed run with 0 jobs.
- Separately, the repo has **no self-hosted runner** at all (`gh api repos/…/actions/runners` → `total_count: 0`).
- Even once it runs, the per-title filter substrings miss two gates: `fallout_4` does not match `parse_rate_fo4_all_meshes`, and `fallout_76` does not match `parse_rate_fo76_all_meshes`.
- The matrix has no `skyrim_le` row for the Skyrim LE gates added this window.

## Evidence
- `gh api …/workflows/351792019/runs`: as of 2026-09-22 (publish time), 256 runs, all `event=push`, all `conclusion=failure`. There are 0 `schedule` runs and 0 `workflow_dispatch` runs.
- For run 35725531352 (HEAD area, 2026-09-22), `jobs.total_count` is 0 and `gh run view` reports "This run likely failed because of a workflow file issue".
- `gh api repos/matiaszanolli/ByroRedux/actions/runners --jq '.total_count'` → `0`.
- Re-verified against the live workflow file at publish time: the `if:` at line 34 still reads `github.event_name == 'schedule' || inputs.game == 'all' || inputs.game == matrix.game`.

## Impact
- Vanilla-content parse regressions are caught only by manual audits. That is the latency #3919 existed to remove; #3918's FO3 clean-rate collapse to 98.29% sat on `main` for two days.
- It is already costing signal: two per-block baselines are red on the local installs with nobody notified (see the sibling LOW finding NIF-D3-2026-09-21-02, filed separately).

## Related
#3919, #3918, #3850 (closed — the fix this lane was supposed to complete); SAFE-D4-2026-09-21-01 (#4595) and SAFE-D5-2026-09-21-01 (#4596) — other inert CI gates found in the same audit window; #4603 (the `lock-order-check` CI lane, also red since at least 09-14 — same root-cause class of "a CI gate stopped signaling and nobody noticed").

## Suggested Fix
- Move per-game selection into a step-level `if:`, or build the matrix from `inputs.game` with `fromJSON`.
- Register a runner that carries the `byroredux-game-data` label, or point `runs-on` at an existing one.
- Extend the filters to cover `fo4_all_meshes` and `fo76_all_meshes`, and add a `skyrim_le` row.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D3-2026-09-21-01)

## Completeness Checks
- [ ] **TESTS**: A regression test / CI self-check pins this specific fix (e.g. a workflow-lint step or a scheduled dry run that asserts `jobs.total_count > 0`)
