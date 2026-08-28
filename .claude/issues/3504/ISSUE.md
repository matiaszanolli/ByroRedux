# Issue #3504: REG-2026-08-27-02: regression of #3218 — the traceability fix shipped an advisory tool, the CI gate is still PR-only, and the citation gap is unchanged (31%)

- **Severity**: MEDIUM
- **Dimension**: Regression / audit infrastructure
- **Labels**: medium, tech-debt, bug
- **Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-27.md`
- **Filed**: 2026-08-28

---

## Description

**Regression of #3218** (CLOSED 2026-08-26, `medium`/`bug`/`tech-debt`) — the fix shipped an advisory tool; the gap it measured is unchanged.

#3218 diagnosed the mechanism precisely — the CI gate is `if: github.event_name == 'pull_request'`, and *"this repo's history is overwhelmingly direct commits to main, so for the dominant workflow it never fires."* The fix added a `--window` mode to `scripts/check-issue-traceability.sh` and a call to it in the `session-close` ritual. **The gate's trigger condition was not changed**, and the `--window` mode is a report, not an enforcement — nothing fails, nothing blocks, and nothing back-fills the citation.

The measured gap has therefore not moved. #3218 was filed against *43 of 134 (32%)* uncited in the 2026-08-16..20 window. Measured 2026-08-27 over the 2026-08-18..28 window: **123 of 400 (31%)**. The rate on 2026-08-26 — the day #3218 itself closed — was **36 of 76 (47%)**.

## Location

`.github/workflows/ci.yml:13-25` · `scripts/check-issue-traceability.sh:34-52` · `.claude/commands/session-close/SKILL.md:80-88`

## Evidence

```yaml
# .github/workflows/ci.yml:13-17 — unchanged trigger
jobs:
  issue-traceability:
    name: Issue/commit traceability
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-latest
```

The workflow itself does fire `on: push: branches: [main]`, so the runner is present — it is the job-level `if:` that excludes every direct-to-main commit.

```
per-day uncited (closing-keyword commit on main, live gh state, measured 2026-08-27):
  2026-08-18: 24/58 (41%)     2026-08-24:  0/3  ( 0%)
  2026-08-19:  2/37 ( 5%)     2026-08-25:  6/19 (32%)
  2026-08-20:  2/11 (18%)     2026-08-26: 36/76 (47%)  <- #3218 closed
  2026-08-21:  2/61 ( 3%)     2026-08-27:  5/47 (11%)
  2026-08-22: 44/64 (69%)     2026-08-28:  2/17 (12%)
  2026-08-23:  0/7  ( 0%)
```

```
# commits on main since 2026-08-20 touching *.rs
246 total — 122 (50%) carry no closing keyword
```

The reverse direction — *commit → issue* — is not checked at all: the script's `closing_issue_numbers` reads a PR body, and `--window` iterates closed issues. A commit that fixes something with no issue attached is invisible to both modes.

## Impact

This is the mechanism that produced the eight-unguarded-walker regression filed alongside this issue. #3237's partial fix was buried in a mega-commit body under a `refactor(...)` heading with no closing keyword; the issue was closed by hand; no gate compared the fix's reach to the issue's stated scope.

As #3218's own script comment says, the degradation is self-concealing: *"a regression audit that cannot find fixes gets quieter, not louder."* At a 31% uncited rate, `/audit-regression`'s Step 2 (`git log --grep="#<N>"`) is a coin flip, and every future sweep pays the cost of re-deriving fix presence by hand.

## Related

- #3218 — the partially-applied fix (CLOSED)
- REG-2026-08-27-01 (filed from the same report) — the concrete regression this gap concealed

## Suggested Fix

Change the gate to run on `push` to `main` over the pushed range (`github.event.before..github.event.after`) rather than only on `pull_request`, so the dominant workflow is actually covered. Separately, add the missing direction — flag pushed commits that touch `*.rs` and cite no issue at all — as a warning-level annotation, so the fix→issue link is recorded while the context is fresh rather than reconstructed by an auditor weeks later.

## Source

`docs/audits/AUDIT_REGRESSION_2026-08-27.md` — REG-2026-08-27-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the other CI jobs that are PR-gated but describe main-branch invariants
- [ ] **TESTS**: A regression test pins this specific fix (extend the script's `--self-test` to cover the push-range mode)
