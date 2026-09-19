---
name: source-command-audit-regression-skill
description: Verify that closed ByroRedux bug fixes haven't regressed by dynamically discovering closed GitHub issues, locating each fix and its guard test, and reporting any fix that has gone missing as a regression. Use when the user requests a regression verification or closed-fix recheck audit.
---

# Regression Verification Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-regression/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's churn-weighted issue discovery, fix and guard-test location steps, PASS/PARTIAL/FAIL/UNVERIFIABLE status rules, unconditional fragile-area guards, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only when a fragile-area guard's deep checklist lives there.
- Prefer the repository's codebase-memory graph tools for locating fix symbols, fix sites, and guard tests; use text search for issue-number citations in tests and source, `Fix #N` commit subjects, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise work the fix → guard-test chain one issue at a time.
- Reading existing GitHub issues via `gh` stays in scope for discovery and verification; creating issues remains reserved for a separate audit-publish action. Preserve the canonical output path.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
