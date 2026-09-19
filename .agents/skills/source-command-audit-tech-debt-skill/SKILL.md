---
name: source-command-audit-tech-debt-skill
description: Audit ByroRedux accumulated technical debt — stale markers, dead code, logic duplication, magic numbers, stub implementations, doc rot, audit-finding rot, oversized files, and test hygiene. Use when the user requests a tech-debt, code-debt, or cleanup-candidate audit.
---

# Tech-Debt Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-tech-debt/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's scope selection, baseline snapshot, per-dimension discovery recipes and triage, extra finding fields and TD ID convention, cross-dimension dedup, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and keep cross-referenced out-of-scope work (e.g. NIFAL translation correctness) as routing, not findings.
- Prefer the repository's codebase-memory graph tools for caller and consumer checks (dead code, duplication consolidation sites); use text search for the marker, stub, size, and literal greps the dimensions prescribe, plus `git log`/`git blame` aging.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise execute the routed dimensions sequentially in the canonical's impact order.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
