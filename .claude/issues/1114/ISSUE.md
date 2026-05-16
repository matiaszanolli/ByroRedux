# Issue #1114 — TD7-050: Audit-skill path-drift structural fix

**Source**: AUDIT_TECH_DEBT_2026-05-16
**Severity**: MEDIUM
**Status**: CLOSED in 457b9914

## Resolution

Installed `.claude/commands/_audit-validate.sh` — backticked-path gate over audit-*.md files. Fails non-zero on stale refs. Convention documented in `_audit-common.md` and audit-tech-debt.md Dim 7.

Inline closeouts (gate's test data): TD7-045..049 stale refs all fixed during the commit.
