# TD4-2026-10-25-03: No gate sees a closed issue described as open; the class has recurred five times

Labels: low, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6383
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 4 · **Effort**: small
- **Location**: `.claude/commands/_audit-validate.sh` (path/symbol gate only); `.claude/commands/audit-sync/prep.sh` (closed-since-BASELINE only)
- **Status**: NEW (root cause of TD4-01; prior trackers #4369, #5606, #5742, #6173 each fixed instances, not the class)
- **Description**: `audit-sync` only finds issues closed since its BASELINE, so anything closed after the sync (today: 8 issues in 12 hours, 6 via one close-sweep) re-stales the skills immediately and nothing flags it until the next Dim 4 pass. `_audit-validate.sh` cannot see numbers or issue state.
- **Suggested Fix**: add an advisory (non-fatal, skipped when `gh` is unavailable or offline, like `SKIP_SYMBOL_CHECK`) to `_audit-validate.sh`: for each `#N` within ~160 chars of `known[- ]open|still open|stays open|open #N|#N[^.;]{0,25}open|do not re-file|don't re-file` in skill files, `gh issue list --state all --json number,state` once and print `file:line #N CLOSED` for closed ones. A working prototype (regex + window logic, 66 raw hits triaged to the 11 true sites above) is `/tmp/audit/tech-debt/dim4_issues3.py`; false-positive control: skip windows where "open" is a verb (`open conversation`, `fail-open`, `half-open`).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD4-2026-10-25-03`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
